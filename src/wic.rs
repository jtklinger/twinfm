//! HEIC, HEIF and AVIF pictures through Windows' own image decoders (P11-M4): the Windows Imaging
//! Component, whose HEIF decoder comes with the "HEIF Image Extensions" from the Store (HEVC for
//! HEIC, AV1 for AVIF). No crate decodes these in pure Rust; without the extension the decoder is
//! not found and the column says so (`NO_DECODER`). Called on the preview worker, which joins the
//! multithreaded apartment once; every COM object is made and released on that thread.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows_sys::core::{GUID, HRESULT};

/// The extensions decoded here, any case.
pub const EXTS: &[&str] = &["heic", "heif", "hif", "avif"];
/// Why a picture is not shown when no decoder takes it (the extension is not installed).
pub const NO_DECODER: &str = "no decoder";

const CLSID_WIC_IMAGING_FACTORY: GUID = GUID::from_u128(0xcacaf262_9370_4615_a13b_9f5539da4c0a);
const IID_IWIC_IMAGING_FACTORY: GUID = GUID::from_u128(0xec5ec8a9_c395_4314_9c77_54d7a935ff70);
const GUID_WIC_PIXEL_FORMAT_32BPP_RGBA: GUID = GUID::from_u128(0xf5c7ad2d_6a8d_43dd_a7a8_a29935261ae9);
/// The HEIF container, whose decoder (the Store extension's) also reads AVIF.
const GUID_CONTAINER_FORMAT_HEIF: GUID = GUID::from_u128(0xe1e62521_6787_405b_a339_500715b5763f);
/// WINCODEC_ERR_COMPONENTNOTFOUND: no decoder installed takes the file.
const COMPONENT_NOT_FOUND: HRESULT = 0x88982F50_u32 as HRESULT;
const GENERIC_READ: u32 = 0x8000_0000;
const VT_UI2: u16 = 18;

#[repr(C)]
struct UnknownVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IWICImagingFactory (wincodec.h): CreateDecoderFromFilename, CreateDecoder and
/// CreateFormatConverter used.
#[repr(C)]
struct FactoryVtbl {
    _unknown: UnknownVtbl,
    create_decoder_from_filename: unsafe extern "system" fn(*mut c_void, *const u16, *const GUID, u32, u32, *mut *mut c_void) -> HRESULT,
    _from_stream_to_create_component_info: [usize; 3],
    create_decoder: unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> HRESULT,
    _create_encoder_and_create_palette: [usize; 2],
    create_format_converter: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

/// IWICBitmapDecoder: GetFrame used.
#[repr(C)]
struct DecoderVtbl {
    _unknown: UnknownVtbl,
    _query_capability_to_get_frame_count: [usize; 10],
    get_frame: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
}

/// IWICBitmapSource, then IWICBitmapFrameDecode's GetMetadataQueryReader and IWICFormatConverter's
/// Initialize, both in slot 8.
#[repr(C)]
struct SourceVtbl {
    _unknown: UnknownVtbl,
    get_size: unsafe extern "system" fn(*mut c_void, *mut u32, *mut u32) -> HRESULT,
    _get_pixel_format_to_copy_palette: [usize; 3],
    copy_pixels: unsafe extern "system" fn(*mut c_void, *const c_void, u32, u32, *mut u8) -> HRESULT,
    slot8: usize,
}

type InitializeConverter = unsafe extern "system" fn(*mut c_void, *mut c_void, *const GUID, u32, *mut c_void, f64, u32) -> HRESULT;
type GetMetadataQueryReader = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;

/// IWICMetadataQueryReader: GetMetadataByName used.
#[repr(C)]
struct QueryReaderVtbl {
    _unknown: UnknownVtbl,
    _get_container_format_and_location: [usize; 2],
    get_metadata_by_name: unsafe extern "system" fn(*mut c_void, *const u16, *mut PROPVARIANT) -> HRESULT,
}

/// A COM pointer released once when it goes.
struct Com(*mut c_void);

impl Com {
    fn vt<T>(&self) -> &T {
        // SAFETY: a live object whose vtable starts as T says (each caller names the interface).
        unsafe { &**(self.0 as *const *const T) }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: made on this thread and released exactly once.
            unsafe { (self.vt::<UnknownVtbl>().release)(self.0) };
        }
    }
}

thread_local! {
    static JOINED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn hr(what: &str, h: HRESULT) -> String {
    format!("failed: {what} 0x{:08X}", h as u32)
}

/// The picture decoded to straight RGBA, upright (its orientation read from the metadata as
/// `System.Photo.Orientation`, 1 to 8, as EXIF's), and its format's name from the extension. Err:
/// `NO_DECODER` (no HEIF decoder is installed), `over the budget` past `max` bytes of pixels, or
/// `failed: ...` (a file no installed decoder takes is `failed: not a picture Windows can read`).
pub fn decode(path: &Path, max: u64) -> Result<(image::RgbaImage, &'static str), String> {
    if !JOINED.with(|j| j.replace(true)) {
        // SAFETY: once per thread; never uninitialised (the worker lives as long as the program).
        unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    }
    let format = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("avif") => "AVIF",
        Some("heic") => "HEIC",
        _ => "HEIF",
    };
    let mut p = std::ptr::null_mut();
    // SAFETY: the out pointer receives an IWICImagingFactory or stays null.
    let h = unsafe { CoCreateInstance(&CLSID_WIC_IMAGING_FACTORY, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_IWIC_IMAGING_FACTORY, &mut p) };
    let factory = Com(p);
    if h < 0 || factory.0.is_null() {
        return Err(hr("CoCreateInstance(WICImagingFactory)", h));
    }
    let name = wide(path.as_os_str());
    let mut p = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and outlives the call; metadata cached on demand (0).
    let h = unsafe { (factory.vt::<FactoryVtbl>().create_decoder_from_filename)(factory.0, name.as_ptr(), std::ptr::null(), GENERIC_READ, 0, &mut p) };
    let decoder = Com(p);
    if h == COMPONENT_NOT_FOUND {
        // No decoder took the file: none is installed, or the file is not what its name says.
        let mut p = std::ptr::null_mut();
        // SAFETY: a live factory; the out pointer receives a decoder or stays null.
        let installed = unsafe { (factory.vt::<FactoryVtbl>().create_decoder)(factory.0, &GUID_CONTAINER_FORMAT_HEIF, std::ptr::null(), &mut p) } >= 0;
        drop(Com(p));
        return Err(if installed { "failed: not a picture Windows can read".into() } else { NO_DECODER.into() });
    }
    if h < 0 || decoder.0.is_null() {
        return Err(hr("CreateDecoderFromFilename", h));
    }
    let mut p = std::ptr::null_mut();
    // SAFETY: a live decoder; frame 0 is the picture (a HEIF's primary image).
    let h = unsafe { (decoder.vt::<DecoderVtbl>().get_frame)(decoder.0, 0, &mut p) };
    let frame = Com(p);
    if h < 0 || frame.0.is_null() {
        return Err(hr("GetFrame", h));
    }
    let (mut w, mut h_px) = (0u32, 0u32);
    // SAFETY: a live frame.
    let h = unsafe { (frame.vt::<SourceVtbl>().get_size)(frame.0, &mut w, &mut h_px) };
    if h < 0 || w == 0 || h_px == 0 {
        return Err(hr("GetSize", h));
    }
    let bytes = u64::from(w) * u64::from(h_px) * 4;
    if bytes > max {
        return Err("over the budget".into());
    }
    let orientation = orientation(&frame);
    let mut p = std::ptr::null_mut();
    // SAFETY: a live factory.
    let h = unsafe { (factory.vt::<FactoryVtbl>().create_format_converter)(factory.0, &mut p) };
    let conv = Com(p);
    if h < 0 || conv.0.is_null() {
        return Err(hr("CreateFormatConverter", h));
    }
    // SAFETY: slot 8 of IWICFormatConverter is Initialize; no dither (0), no palette, custom (0).
    let h = unsafe {
        let init: InitializeConverter = std::mem::transmute(conv.vt::<SourceVtbl>().slot8);
        init(conv.0, frame.0, &GUID_WIC_PIXEL_FORMAT_32BPP_RGBA, 0, std::ptr::null_mut(), 0.0, 0)
    };
    if h < 0 {
        return Err(hr("the conversion to RGBA", h));
    }
    let mut buf = vec![0u8; bytes as usize];
    // SAFETY: the whole picture (no rectangle) into a buffer of exactly stride times height bytes.
    let h = unsafe { (conv.vt::<SourceVtbl>().copy_pixels)(conv.0, std::ptr::null(), w * 4, buf.len() as u32, buf.as_mut_ptr()) };
    if h < 0 {
        return Err(hr("CopyPixels", h));
    }
    let img = image::RgbaImage::from_raw(w, h_px, buf).ok_or_else(|| "failed: a bad size".to_string())?;
    let mut img = image::DynamicImage::ImageRgba8(img);
    img.apply_orientation(orientation);
    Ok((img.into_rgba8(), format))
}

/// The frame's orientation (`System.Photo.Orientation`), none when the decoder offers no metadata.
fn orientation(frame: &Com) -> image::metadata::Orientation {
    let none = image::metadata::Orientation::NoTransforms;
    let mut p = std::ptr::null_mut();
    // SAFETY: slot 8 of IWICBitmapFrameDecode is GetMetadataQueryReader.
    let h = unsafe {
        let get: GetMetadataQueryReader = std::mem::transmute(frame.vt::<SourceVtbl>().slot8);
        get(frame.0, &mut p)
    };
    let reader = Com(p);
    if h < 0 || reader.0.is_null() {
        return none;
    }
    let name = wide(std::ffi::OsStr::new("System.Photo.Orientation"));
    // SAFETY: a zeroed PROPVARIANT is VT_EMPTY; it is cleared once after the read.
    unsafe {
        let mut v: PROPVARIANT = std::mem::zeroed();
        let got = (reader.vt::<QueryReaderVtbl>().get_metadata_by_name)(reader.0, name.as_ptr(), &mut v);
        let o = (got >= 0 && v.Anonymous.Anonymous.vt == VT_UI2).then(|| v.Anonymous.Anonymous.Anonymous.uiVal);
        PropVariantClear(&mut v);
        o.and_then(|o| image::metadata::Orientation::from_exif(o as u8)).unwrap_or(none)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUID_WIC_PIXEL_FORMAT_32BPP_BGRA: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90f);

    /// Method `slot` of a COM object, as a function of type F.
    unsafe fn method<F: Copy>(p: *mut c_void, slot: usize) -> F {
        // SAFETY: the caller names the interface's slot and its signature.
        unsafe {
            let vt = *(p as *const *const usize);
            std::mem::transmute_copy(&*vt.add(slot))
        }
    }

    type Out = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;

    /// A HEIC written by Windows' own HEIF encoder (the Store extension's), `w` x `h`, the left half
    /// red and the right half blue; false when no encoder is installed.
    fn write_heic(path: &Path, w: u32, h: u32) -> bool {
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32);
            let mut f = std::ptr::null_mut();
            assert!(CoCreateInstance(&CLSID_WIC_IMAGING_FACTORY, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_IWIC_IMAGING_FACTORY, &mut f) >= 0);
            let factory = Com(f);
            let mut e = std::ptr::null_mut();
            let create_encoder: unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> HRESULT = method(factory.0, 8);
            let h0 = create_encoder(factory.0, &GUID_CONTAINER_FORMAT_HEIF, std::ptr::null(), &mut e);
            let encoder = Com(e);
            if h0 < 0 {
                eprintln!("PROBE CreateEncoder(HEIF) = 0x{:08X}", h0 as u32);
                return false;
            }
            let mut st = std::ptr::null_mut();
            assert!(method::<Out>(factory.0, 14)(factory.0, &mut st) >= 0);
            let stream = Com(st);
            let name = wide(path.as_os_str());
            let init_file: unsafe extern "system" fn(*mut c_void, *const u16, u32) -> HRESULT = method(stream.0, 15);
            assert!(init_file(stream.0, name.as_ptr(), 0x4000_0000) >= 0);
            let init_enc: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) -> HRESULT = method(encoder.0, 3);
            assert!(init_enc(encoder.0, stream.0, 2) >= 0);
            let (mut fr, mut bag) = (std::ptr::null_mut(), std::ptr::null_mut());
            let new_frame: unsafe extern "system" fn(*mut c_void, *mut *mut c_void, *mut *mut c_void) -> HRESULT = method(encoder.0, 10);
            assert!(new_frame(encoder.0, &mut fr, &mut bag) >= 0);
            let (frame, bag) = (Com(fr), Com(bag));
            let init_frame: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT = method(frame.0, 3);
            assert!(init_frame(frame.0, bag.0) >= 0);
            let set_size: unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT = method(frame.0, 4);
            assert!(set_size(frame.0, w, h) >= 0);
            let mut fmt = GUID_WIC_PIXEL_FORMAT_32BPP_BGRA;
            let set_format: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT = method(frame.0, 6);
            assert!(set_format(frame.0, &mut fmt) >= 0);
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            for _ in 0..h {
                for x in 0..w {
                    px.extend_from_slice(if x < w / 2 { &[0, 0, 255, 255] } else { &[255, 0, 0, 255] });
                }
            }
            let write: unsafe extern "system" fn(*mut c_void, u32, u32, u32, *const u8) -> HRESULT = method(frame.0, 10);
            assert!(write(frame.0, h, w * 4, px.len() as u32, px.as_ptr()) >= 0, "WritePixels (format {:08x})", fmt.data1);
            assert!(method::<unsafe extern "system" fn(*mut c_void) -> HRESULT>(frame.0, 12)(frame.0) >= 0);
            assert!(method::<unsafe extern "system" fn(*mut c_void) -> HRESULT>(encoder.0, 11)(encoder.0) >= 0);
            true
        }
    }

    #[test]
    fn heic_through_windows_decoders() {
        let dir = std::env::temp_dir().join(format!("twinfm-p11m4-heic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let heic = dir.join("a.HEIC");
        if !write_heic(&heic, 64, 32) {
            eprintln!("no HEIF encoder on this machine: the HEIC check is skipped");
            assert_eq!(decode(&heic, 512 << 20).map(|_| ()), Err(NO_DECODER.into()));
        } else {
            let (img, format) = decode(&heic, 512 << 20).unwrap();
            assert_eq!((img.dimensions(), format), ((64, 32), "HEIC"));
            // Lossy: within a few levels of the red and blue halves written.
            let near = |p: image::Rgba<u8>, want: [u8; 4]| p.0.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 8);
            assert!(near(*img.get_pixel(4, 16), [255, 0, 0, 255]) && near(*img.get_pixel(60, 16), [0, 0, 255, 255]));
            assert_eq!(decode(&heic, 64 * 32 * 4 - 1).map(|_| ()), Err("over the budget".into()));
            let bogus = dir.join("b.heic");
            std::fs::write(&bogus, b"not a picture").unwrap();
            assert_eq!(decode(&bogus, 512 << 20).map(|_| ()), Err("failed: not a picture Windows can read".into()));
        }
        assert!(decode(&dir.join("missing.avif"), 512 << 20).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
