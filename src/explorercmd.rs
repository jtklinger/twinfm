//! The packaged apps' context menu commands and the Open with apps, for the Windows 11 style menu
//! (P7-M2). Everything here that touches COM runs on the verb worker (`crate::verbs`), never on
//! the UI thread: the catalog of commands, the objects made for one menu, and their invokes.
//!
//! A packaged app declares its commands in its manifest (`AppxManifest.xml`, an `Extension` of
//! category `windows.fileExplorerContextMenus`): per `ItemType` (`*` any file, `.ext`, `Directory`,
//! `Directory\Background`, `Drive`) the `Verb`s, each the class id of a packaged COM class that
//! implements `IExplorerCommand`. The installed packages that register classes are the subkeys of
//! `HKCR\PackagedCom\Package`; the catalog is read once, and again when that key's last-write time
//! changes (a package installed, updated or removed), checked when a menu opens.
//!
//! For a menu the worker creates each class whose types fit every target, asks it for its state,
//! title and icon for an `IShellItemArray` of the targets (a command that says hidden is left out),
//! and keeps the objects until the next menu, so a pick invokes the object that was asked. The
//! Open with apps are the shell's recommended handlers for the file's extension
//! (`SHAssocEnumHandlers`), kept the same way.

use crate::shellmenu::{Com, Pidl, check, parse_items, ui_data_object, wide};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CoCreateInstance, CoTaskMemFree};
use windows_sys::Win32::System::Registry::{HKEY, HKEY_CLASSES_ROOT, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryInfoKeyW};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{ASSOC_FILTER_RECOMMENDED, SHAssocEnumHandlers, SHCreateShellItemArray, SHCreateShellItemArrayFromIDLists};
use windows_sys::core::{GUID, HRESULT, PWSTR};

/// The packaged COM catalog's list of installed packages (under HKEY_CLASSES_ROOT).
const PACKAGES_KEY: &str = r"PackagedCom\Package";

/// `IExplorerCommand::GetState`'s answers (shobjidl_core.h `EXPCMDSTATE`; 1 is disabled).
const ECS_ENABLED: u32 = 0;
const ECS_HIDDEN: u32 = 2;
/// `GetFlags`: the command opens a submenu of its own commands (`EnumSubCommands`).
const ECF_HASSUBCOMMANDS: u32 = 0x1;
/// `GetFlags`: a separator, in a submenu.
const ECF_ISSEPARATOR: u32 = 0x8;

const IID_IEXPLORERCOMMAND: GUID = GUID::from_u128(0xa08ce4d0_fa25_44ab_b57c_c7b1c323e0b9);

/// One packaged class and the item types it was declared for, as the manifest has them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decl {
    pub package: String,
    pub clsid: String,
    /// Lowercase: `*`, `.txt`, `directory`, `directory\background`, `drive`.
    pub types: Vec<String>,
}

/// What the menu is for, one per target, as the manifests' item types name them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A file, with its extension in lowercase with the dot (empty when it has none).
    File(String),
    Dir,
    Drive,
    /// The folder's background menu.
    Background,
}

/// A manifest's context menu commands: every `Verb` under an `ItemType` of an `Extension` of
/// category `windows.fileExplorerContextMenus`, merged per class in the order they first appear.
/// Namespace prefixes (`desktop4:`, `desktop5:`) are ignored.
pub fn parse_manifest(xml: &str, package: &str) -> Vec<Decl> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let mut out: Vec<Decl> = Vec::new();
    let menus = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Extension" && n.attribute("Category") == Some("windows.fileExplorerContextMenus"));
    for ext in menus {
        for it in ext.descendants().filter(|n| n.tag_name().name() == "ItemType") {
            let Some(ty) = it.attribute("Type") else { continue };
            let ty = ty.trim().to_ascii_lowercase();
            for verb in it.children().filter(|n| n.tag_name().name() == "Verb") {
                let Some(clsid) = verb.attribute("Clsid") else { continue };
                let clsid = clsid.trim().trim_start_matches('{').trim_end_matches('}').to_ascii_lowercase();
                match out.iter_mut().find(|d| d.clsid == clsid) {
                    Some(d) if !d.types.contains(&ty) => d.types.push(ty.clone()),
                    Some(_) => {}
                    None => out.push(Decl { package: package.to_string(), clsid, types: vec![ty.clone()] }),
                }
            }
        }
    }
    out
}

/// Whether a declared item type covers one target: `*` any file, `.ext` a file with that
/// extension, `Directory` a folder or a drive, `Directory\Background` the background, `Drive` a
/// drive.
pub fn type_covers(ty: &str, t: &Target) -> bool {
    match (ty, t) {
        ("*", Target::File(_)) => true,
        ("directory", Target::Dir | Target::Drive) => true,
        ("directory\\background", Target::Background) => true,
        ("drive", Target::Drive) => true,
        (e, Target::File(x)) if e.starts_with('.') => e == x,
        _ => false,
    }
}

/// A class applies to the menu when each target is covered by one of its types.
pub fn applies(types: &[String], targets: &[Target]) -> bool {
    !targets.is_empty() && targets.iter().all(|t| types.iter().any(|ty| type_covers(ty, t)))
}

/// A GUID from its text form, braces optional.
pub fn parse_guid(s: &str) -> Option<GUID> {
    let hex: String = s.trim().trim_start_matches('{').trim_end_matches('}').chars().filter(|&c| c != '-').collect();
    if hex.len() != 32 || s.trim().trim_start_matches('{').trim_end_matches('}').len() != 36 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok().map(GUID::from_u128)
}

/// A title without its access-key markers: `&` goes, `&&` is a literal `&`.
pub fn strip_access(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                out.push('&');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// What the worker asks for one menu.
#[derive(Clone, Debug)]
pub struct Query {
    pub seq: u64,
    /// The folder and the names in it, as `shellmenu::parse_items` takes them (no names: the
    /// background).
    pub folder: PathBuf,
    pub names: Vec<String>,
    pub targets: Vec<Target>,
    /// The icons' size in physical px.
    pub icon_px: u32,
    /// The Open with apps for this extension (one file only).
    pub open_with: Option<String>,
}

pub type Pixels = SharedPixelBuffer<Rgba8Pixel>;

/// A packaged command as the menu shows it.
#[derive(Clone, Debug)]
pub struct Command {
    pub title: String,
    pub icon: Option<Pixels>,
    pub enabled: bool,
    /// Its own commands, for a command with a submenu.
    pub subs: Vec<Command>,
}

/// An Open with app.
#[derive(Clone, Debug)]
pub struct App {
    pub name: String,
    pub icon: Option<Pixels>,
}

/// The worker's answer for one menu.
#[derive(Clone, Debug, Default)]
pub struct Answer {
    pub seq: u64,
    pub commands: Vec<Command>,
    pub apps: Vec<App>,
    pub ms: f64,
}

/// Which object a pick invokes: a command, one of a command's own commands, an Open with app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Command(usize),
    Sub(usize, usize),
    App(usize),
}

/// A pick in a menu the worker answered.
#[derive(Clone, Debug)]
pub struct Run {
    pub seq: u64,
    pub pick: Pick,
    pub folder: PathBuf,
    pub names: Vec<String>,
    pub title: String,
}

/// The catalog and the last menu's objects, on the verb worker only.
struct Held {
    catalog: Vec<Decl>,
    /// `PACKAGES_KEY`'s subkey count and last-write time when the catalog was read.
    stamp: Option<(u32, u64)>,
    seq: u64,
    commands: Vec<(Com, Vec<Com>)>,
    apps: Vec<Com>,
}

thread_local! {
    static HELD: RefCell<Held> = const { RefCell::new(Held { catalog: Vec::new(), stamp: None, seq: 0, commands: Vec::new(), apps: Vec::new() }) };
}

fn open_key(parent: HKEY, sub: &str) -> Option<HKEY> {
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `sub` is NUL-terminated and outlives the call; `key` receives an open key on success.
    let r = unsafe { RegOpenKeyExW(parent, wide(std::ffi::OsStr::new(sub)).as_ptr(), 0, KEY_READ, &mut key) };
    (r == ERROR_SUCCESS).then_some(key)
}

/// The packages key's subkey count and last-write time: a package change moves one of them.
fn packages_stamp() -> Option<(u32, u64)> {
    let key = open_key(HKEY_CLASSES_ROOT, PACKAGES_KEY)?;
    let (mut n, mut ft) = (0u32, FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 });
    // SAFETY: `key` is open; only the subkey count and the last-write time are asked for.
    let r = unsafe {
        RegQueryInfoKeyW(
            key,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            &mut n,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut ft,
        )
    };
    // SAFETY: opened above, closed once.
    unsafe { RegCloseKey(key) };
    (r == ERROR_SUCCESS).then(|| (n, (ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64))
}

/// The installed packages that register COM classes, by full name.
fn packages() -> Vec<String> {
    let Some(key) = open_key(HKEY_CLASSES_ROOT, PACKAGES_KEY) else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0.. {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        // SAFETY: `key` is open; `name` is writable for `len` characters.
        let r = unsafe { RegEnumKeyExW(key, i, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    // SAFETY: opened above, closed once.
    unsafe { RegCloseKey(key) };
    out
}

/// A package's folder (`GetPackagePathByFullName`).
fn package_path(full_name: &str) -> Option<PathBuf> {
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagePathByFullName;
    let name = wide(std::ffi::OsStr::new(full_name));
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `name` is NUL-terminated; `buf` is writable for `len` characters.
    let r = unsafe { GetPackagePathByFullName(name.as_ptr(), &mut len, buf.as_mut_ptr()) };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    (n > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Read every installed package's manifest for its context menu commands.
fn read_catalog() -> Vec<Decl> {
    let t0 = std::time::Instant::now();
    let all = packages();
    let mut out = Vec::new();
    let mut with = 0;
    for p in &all {
        let Some(dir) = package_path(p) else { continue };
        let Ok(xml) = std::fs::read_to_string(dir.join("AppxManifest.xml")) else { continue };
        if !xml.contains("windows.fileExplorerContextMenus") {
            continue;
        }
        let decls = parse_manifest(&xml, p);
        if !decls.is_empty() {
            with += 1;
        }
        out.extend(decls);
    }
    eprintln!(
        "[winmenu] catalog: {with} of {} packages declare context menu commands, {} classes, read in {:.0} ms",
        all.len(),
        out.len(),
        crate::perf::elapsed_ms(t0)
    );
    out
}

/// The catalog, read again when the packages key has changed since it was read.
fn refresh_catalog(h: &mut Held) {
    let stamp = packages_stamp();
    if h.stamp.is_some() && h.stamp == stamp {
        return;
    }
    if h.stamp.is_some() {
        eprintln!("[winmenu] packages changed: the catalog is read again");
    }
    h.catalog = read_catalog();
    h.stamp = stamp;
}

/// The targets as an `IShellItemArray`: the items of `names` in `folder`, or the folder itself for
/// the background. Also returns the folder and its children for a data object.
fn item_array(folder: &Path, names: &[String]) -> Result<(Com, Pidl, Com, Vec<Pidl>), String> {
    let (folder_pidl, shell_folder, children) = parse_items(std::ptr::null_mut(), folder, names)?;
    let mut p: *mut c_void = std::ptr::null_mut();
    let hr = if children.is_empty() {
        let one = [folder_pidl.0 as *const ITEMIDLIST];
        // SAFETY: one absolute PIDL, alive for the call.
        unsafe { SHCreateShellItemArrayFromIDLists(1, one.as_ptr(), &mut p) }
    } else {
        let apidl: Vec<*const ITEMIDLIST> = children.iter().map(|c| c.0 as *const ITEMIDLIST).collect();
        // SAFETY: child PIDLs relative to `shell_folder` (an IShellFolder), alive for the call.
        unsafe { SHCreateShellItemArray(std::ptr::null(), shell_folder.0, apidl.len() as u32, apidl.as_ptr(), &mut p) }
    };
    check(hr, "SHCreateShellItemArray")?;
    if p.is_null() {
        return Err("no item array".into());
    }
    Ok((Com(p), folder_pidl, shell_folder, children))
}

/// IExplorerCommand (shobjidl_core.h), in declaration order.
#[repr(C)]
struct ExplorerCommandVtbl {
    _query_interface: usize,
    _add_ref: usize,
    _release: usize,
    get_title: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut PWSTR) -> HRESULT,
    get_icon: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut PWSTR) -> HRESULT,
    _get_tool_tip: usize,
    _get_canonical_name: usize,
    get_state: unsafe extern "system" fn(*mut c_void, *mut c_void, i32, *mut u32) -> HRESULT,
    invoke: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT,
    get_flags: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    enum_sub_commands: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

/// IEnumExplorerCommand and IEnumAssocHandlers: IUnknown, then Next.
#[repr(C)]
struct EnumVtbl {
    _query_interface: usize,
    _add_ref: usize,
    _release: usize,
    next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> HRESULT,
}

/// IAssocHandler (shobjidl_core.h), in declaration order.
#[repr(C)]
struct AssocHandlerVtbl {
    _query_interface: usize,
    _add_ref: usize,
    _release: usize,
    _get_name: usize,
    get_ui_name: unsafe extern "system" fn(*mut c_void, *mut PWSTR) -> HRESULT,
    get_icon_location: unsafe extern "system" fn(*mut c_void, *mut PWSTR, *mut i32) -> HRESULT,
    _is_recommended: usize,
    _make_default: usize,
    invoke: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

/// A string the callee allocated with the COM task allocator, freed here.
fn take_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: a NUL-terminated wide string from the callee, freed once with CoTaskMemFree.
    unsafe {
        let n = (0..).take_while(|&i| *p.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        CoTaskMemFree(p as *const c_void);
        s
    }
}

/// The items of an enumerator (IEnumExplorerCommand, IEnumAssocHandlers), at most `max`.
fn drain(e: &Com, max: usize) -> Vec<Com> {
    let mut out = Vec::new();
    while out.len() < max {
        let (mut p, mut got): (*mut c_void, u32) = (std::ptr::null_mut(), 0);
        // SAFETY: `e` is one of the two enumerators, whose Next takes (1, out, fetched).
        let hr = unsafe { (e.vtbl::<EnumVtbl>().next)(e.0, 1, &mut p, &mut got) };
        if hr != 0 || got == 0 || p.is_null() {
            break;
        }
        out.push(Com(p));
    }
    out
}

/// A command's state, title, icon and flags for `psia`.
fn describe(c: &Com, psia: &Com, icon_px: u32) -> (u32, String, Option<Pixels>, u32) {
    // SAFETY: `c` is an IExplorerCommand; the out-pointers are valid; strings are ours to free.
    unsafe {
        let v = c.vtbl::<ExplorerCommandVtbl>();
        let mut state = ECS_HIDDEN;
        if (v.get_state)(c.0, psia.0, 1, &mut state) < 0 {
            state = ECS_HIDDEN;
        }
        let mut flags = 0u32;
        if (v.get_flags)(c.0, &mut flags) < 0 {
            flags = 0;
        }
        if state == ECS_HIDDEN {
            return (state, String::new(), None, flags);
        }
        let mut t: PWSTR = std::ptr::null_mut();
        let title = if (v.get_title)(c.0, psia.0, &mut t) >= 0 { strip_access(&take_string(t)) } else { String::new() };
        let mut i: PWSTR = std::ptr::null_mut();
        let icon = if (v.get_icon)(c.0, psia.0, &mut i) >= 0 { crate::icons::location_pixels(&take_string(i), icon_px) } else { None };
        (state, title, icon, flags)
    }
}

/// Answer a menu's query (on the verb worker): the packaged commands that apply, and the Open with
/// apps. The objects stay held for the picks of this menu.
pub fn query(q: &Query) -> Answer {
    let t0 = std::time::Instant::now();
    HELD.with(|h| {
        let h = &mut *h.borrow_mut();
        refresh_catalog(h);
        h.seq = q.seq;
        h.commands.clear();
        h.apps.clear();
        let mut ans = Answer { seq: q.seq, ..Answer::default() };
        let psia = match item_array(&q.folder, &q.names) {
            Ok((a, ..)) => a,
            Err(e) => {
                eprintln!("[winmenu] query id={}: {e}", q.seq);
                return ans;
            }
        };
        let (mut hidden, mut asked) = (0, 0);
        for d in h.catalog.iter().filter(|d| applies(&d.types, &q.targets)) {
            let Some(clsid) = parse_guid(&d.clsid) else { continue };
            asked += 1;
            let t1 = std::time::Instant::now();
            let mut p: *mut c_void = std::ptr::null_mut();
            // SAFETY: plain COM creation on this STA; `p` receives an IExplorerCommand or stays null.
            let hr = unsafe { CoCreateInstance(&clsid, std::ptr::null_mut(), CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER, &IID_IEXPLORERCOMMAND, &mut p) };
            if hr < 0 || p.is_null() {
                eprintln!("[winmenu]   {{{}}} ({}): CoCreateInstance = 0x{:08X}", d.clsid, d.package, hr as u32);
                continue;
            }
            let c = Com(p);
            let (state, title, icon, flags) = describe(&c, &psia, q.icon_px);
            eprintln!(
                "[winmenu]   {{{}}} \"{title}\" state {state} flags 0x{flags:X} in {:.0} ms ({})",
                d.clsid,
                crate::perf::elapsed_ms(t1),
                d.package.split('_').next().unwrap_or(&d.package)
            );
            if state == ECS_HIDDEN || title.is_empty() {
                hidden += 1;
                continue;
            }
            let mut subs = Vec::new();
            let mut sub_objs = Vec::new();
            if flags & ECF_HASSUBCOMMANDS != 0 {
                let mut e: *mut c_void = std::ptr::null_mut();
                // SAFETY: `c` is an IExplorerCommand; `e` receives an IEnumExplorerCommand or stays null.
                if unsafe { (c.vtbl::<ExplorerCommandVtbl>().enum_sub_commands)(c.0, &mut e) } >= 0 && !e.is_null() {
                    for s in drain(&Com(e), 32) {
                        let (st, t, i, f) = describe(&s, &psia, q.icon_px);
                        if st == ECS_HIDDEN || f & ECF_ISSEPARATOR != 0 || t.is_empty() {
                            continue;
                        }
                        subs.push(Command { title: t, icon: i, enabled: st == ECS_ENABLED, subs: Vec::new() });
                        sub_objs.push(s);
                    }
                }
                if subs.is_empty() {
                    hidden += 1;
                    continue;
                }
            }
            ans.commands.push(Command { title, icon, enabled: state == ECS_ENABLED, subs });
            h.commands.push((c, sub_objs));
        }
        if let Some(ext) = &q.open_with {
            let w = wide(std::ffi::OsStr::new(ext));
            let mut e: *mut c_void = std::ptr::null_mut();
            // SAFETY: NUL-terminated extension; `e` receives an IEnumAssocHandlers or stays null.
            if unsafe { SHAssocEnumHandlers(w.as_ptr(), ASSOC_FILTER_RECOMMENDED, &mut e) } >= 0 && !e.is_null() {
                for a in drain(&Com(e), 16) {
                    // SAFETY: `a` is an IAssocHandler; strings are ours to free.
                    let (name, icon) = unsafe {
                        let v = a.vtbl::<AssocHandlerVtbl>();
                        let mut n: PWSTR = std::ptr::null_mut();
                        let name = if (v.get_ui_name)(a.0, &mut n) >= 0 { take_string(n) } else { String::new() };
                        let (mut path, mut index): (PWSTR, i32) = (std::ptr::null_mut(), 0);
                        let loc = if (v.get_icon_location)(a.0, &mut path, &mut index) >= 0 { format!("{},{index}", take_string(path)) } else { String::new() };
                        let icon = crate::icons::location_pixels(&loc, q.icon_px);
                        if icon.is_none() {
                            eprintln!("[winmenu]   open-with app {name:?}: no icon from {loc:?}");
                        }
                        (name, icon)
                    };
                    if name.is_empty() {
                        continue;
                    }
                    ans.apps.push(App { name, icon });
                    h.apps.push(a);
                }
            }
        }
        ans.ms = crate::perf::elapsed_ms(t0);
        eprintln!(
            "[winmenu] query id={}: {} of {asked} commands shown ({hidden} hidden), {} open-with app(s), in {:.0} ms (verb worker)",
            q.seq,
            ans.commands.len(),
            ans.apps.len(),
            ans.ms
        );
        ans
    })
}

/// Release the last menu's objects (on the verb worker, before it leaves its apartment: the
/// thread-local would drop them after `CoUninitialize`).
pub fn release_all() {
    HELD.with(|h| {
        let h = &mut *h.borrow_mut();
        h.commands.clear();
        h.apps.clear();
    });
}

/// Invoke a pick (on the verb worker) with the object the menu's query made: a packaged command
/// with an `IShellItemArray` of the targets, an Open with app with their data object. Err when the
/// menu it came from is not the one the worker holds, or the targets cannot be parsed.
pub fn run(r: &Run) -> Result<HRESULT, String> {
    HELD.with(|h| {
        let h = &*h.borrow();
        if h.seq != r.seq {
            return Err(format!("the menu was replaced (id {} held, {} picked)", h.seq, r.seq));
        }
        let (psia, _folder_pidl, folder, children) = item_array(&r.folder, &r.names)?;
        let invoke_command = |c: &Com| {
            // MODAL LOOP: a command may show a window on this thread; no app state lives here.
            // SAFETY: `c` is an IExplorerCommand held since the query; no bind context.
            unsafe { (c.vtbl::<ExplorerCommandVtbl>().invoke)(c.0, psia.0, std::ptr::null_mut()) }
        };
        match r.pick {
            Pick::Command(i) => h.commands.get(i).map(|(c, _)| invoke_command(c)).ok_or_else(|| "no such command".into()),
            Pick::Sub(i, j) => h.commands.get(i).and_then(|(_, s)| s.get(j)).map(invoke_command).ok_or_else(|| "no such command".into()),
            Pick::App(k) => {
                let a = h.apps.get(k).ok_or("no such app")?;
                let data = ui_data_object(std::ptr::null_mut(), &folder, &children)?;
                // SAFETY: `a` is an IAssocHandler held since the query; `data` the items' data object.
                Ok(unsafe { (a.vtbl::<AssocHandlerVtbl>().invoke)(a.0, data.0) })
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
         xmlns:desktop5="http://schemas.microsoft.com/appx/manifest/desktop/windows10/5"
         xmlns:com="http://schemas.microsoft.com/appx/manifest/com/windows10">
  <!-- a comment with windows.fileExplorerContextMenus in it -->
  <Applications><Application Id="App"><Extensions>
    <desktop4:Extension Category="windows.fileExplorerContextMenus">
      <desktop4:FileExplorerContextMenus>
        <desktop5:ItemType Type="Directory">
          <desktop5:Verb Id="OpenHere" Clsid="9f156763-7844-4dc4-b2b1-901f640f5155" />
        </desktop5:ItemType>
        <desktop5:ItemType Type="Directory\Background">
          <desktop5:Verb Id="OpenHere" Clsid="{9F156763-7844-4DC4-B2B1-901F640F5155}" />
        </desktop5:ItemType>
        <desktop4:ItemType Type=".PY">
          <desktop4:Verb Id="Edit" Clsid="C7E29CB0-9691-4DE8-B72B-6719DDC0B4A1" />
          <desktop4:Verb Id="Run" Clsid="11111111-2222-3333-4444-555555555555" />
        </desktop4:ItemType>
      </desktop4:FileExplorerContextMenus>
    </desktop4:Extension>
    <com:Extension Category="windows.comServer"><com:ComServer /></com:Extension>
    <desktop4:Extension Category="windows.somethingElse">
      <desktop4:ItemType Type="*"><desktop4:Verb Id="X" Clsid="99999999-2222-3333-4444-555555555555" /></desktop4:ItemType>
    </desktop4:Extension>
  </Extensions></Application></Applications>
</Package>"#;

    #[test]
    fn manifest_commands_merge_per_class() {
        let d = parse_manifest(MANIFEST, "Pkg_1.0_x64__abc");
        assert_eq!(d.len(), 3, "{d:?}");
        assert_eq!(d[0].clsid, "9f156763-7844-4dc4-b2b1-901f640f5155", "braces and case go");
        assert_eq!(d[0].types, vec!["directory".to_string(), "directory\\background".to_string()]);
        assert_eq!(d[1].types, vec![".py".to_string()]);
        assert_eq!(d[2].clsid, "11111111-2222-3333-4444-555555555555");
        assert!(d.iter().all(|x| x.package == "Pkg_1.0_x64__abc"));
        assert!(parse_manifest("<not xml", "p").is_empty());
        assert!(parse_manifest("<Package/>", "p").is_empty());
    }

    #[test]
    fn item_types_cover_their_targets() {
        let f = |e: &str| Target::File(e.to_string());
        assert!(type_covers("*", &f(".txt")));
        assert!(type_covers("*", &f("")), "a file with no extension");
        assert!(!type_covers("*", &Target::Dir), "any file, not a folder");
        assert!(type_covers(".png", &f(".png")));
        assert!(!type_covers(".png", &f(".jpg")));
        assert!(type_covers("directory", &Target::Dir));
        assert!(type_covers("directory", &Target::Drive), "a drive is a folder");
        assert!(!type_covers("directory", &Target::Background));
        assert!(type_covers("directory\\background", &Target::Background));
        assert!(type_covers("drive", &Target::Drive));
        assert!(!type_covers("drive", &Target::Dir));
        let star = vec!["*".to_string()];
        let both = vec!["*".to_string(), "directory".to_string()];
        assert!(applies(&star, &[f(".txt"), f(".png")]));
        assert!(!applies(&star, &[f(".txt"), Target::Dir]), "every target must be covered");
        assert!(applies(&both, &[f(".txt"), Target::Dir]));
        assert!(!applies(&both, &[]), "nothing to act on");
    }

    #[test]
    fn guids_and_titles() {
        let g = parse_guid("{9F156763-7844-4DC4-B2B1-901F640F5155}").unwrap();
        assert_eq!(g.data1, 0x9f156763);
        assert_eq!(g.data2, 0x7844);
        assert_eq!(g.data4, [0xb2, 0xb1, 0x90, 0x1f, 0x64, 0x0f, 0x51, 0x55]);
        assert_eq!(parse_guid("9f156763-7844-4dc4-b2b1-901f640f5155").map(|g| g.data1), Some(0x9f156763));
        assert!(parse_guid("9f156763784Ldc4b2b1901f640f5155").is_none());
        assert!(parse_guid("not a guid").is_none());
        assert_eq!(strip_access("Open in &Terminal"), "Open in Terminal");
        assert_eq!(strip_access("&Move to OneDrive"), "Move to OneDrive");
        assert_eq!(strip_access("Tom && Jerry"), "Tom & Jerry");
        assert_eq!(strip_access("plain"), "plain");
    }
}
