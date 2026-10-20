//! A zip's records as bytes (P8-L2). An edit of a zip copies each kept entry's local record, its
//! data and its data descriptor, and its central directory record, exactly as they are, patching
//! only the name (a rename) and the local record's offset. The `zip` crate's raw copy rebuilt every
//! entry from its options instead and lost what it does not model: the encrypted and data
//! descriptor flags (which left a password-protected entry unreadable, P7-M3's console check) and
//! Info-ZIP's extended timestamp field.
//!
//! The central directory is read here, not through the crate, so nothing of an entry is
//! interpreted beyond the fields an edit needs. Zip64 is read and written (an offset past 4 GiB
//! goes into the zip64 extra field; an archive of 65,535 entries or more, or past 4 GiB, gets the
//! zip64 end records). Leading bytes before the first entry (a self-extractor's stub) are kept out:
//! the offsets read are shifted by them, and the archive written has none.

use std::io::{BufWriter, Read, Seek, SeekFrom, Write};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const EOCD: u32 = 0x0605_4b50;
const EOCD64: u32 = 0x0606_4b50;
const LOCATOR64: u32 = 0x0706_4b50;
const DESCRIPTOR: u32 = 0x0807_4b50;
const MAX32: u64 = 0xFFFF_FFFF;
const MAX16: u64 = 0xFFFF;
/// The zip64 extended information extra field.
const ZIP64: u16 = 0x0001;
/// Info-ZIP's Unicode path extra field: a name given in it wins over the entry's own, so a rename
/// drops it.
const UNICODE_PATH: u16 = 0x7075;
/// WinZip's AES extra field.
const AES: u16 = 0x9901;
/// General-purpose flags.
pub const FLAG_ENCRYPTED: u16 = 0x0001;
pub const FLAG_DESCRIPTOR: u16 = 0x0008;
pub const FLAG_STRONG: u16 = 0x0040;
pub const FLAG_UTF8: u16 = 0x0800;
/// The compression method WinZip's AES entries name.
pub const METHOD_AES: u16 = 99;

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().expect("eight bytes"))
}

fn put16(b: &mut [u8], i: usize, v: u16) {
    b[i..i + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], i: usize, v: u32) {
    b[i..i + 4].copy_from_slice(&v.to_le_bytes());
}

/// The extra fields of a record as (id, data); a field that runs past the end ends the list.
pub fn fields(extra: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= extra.len() {
        let (id, len) = (u16_at(extra, i), u16_at(extra, i + 2) as usize);
        if i + 4 + len > extra.len() {
            break;
        }
        out.push((id, &extra[i + 4..i + 4 + len]));
        i += 4 + len;
    }
    out
}

/// `extra` without the fields `drop` names (the rest byte for byte, a damaged tail included).
fn without(extra: &[u8], drop: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= extra.len() {
        let (id, len) = (u16_at(extra, i), u16_at(extra, i + 2) as usize);
        let end = (i + 4 + len).min(extra.len());
        if !drop.contains(&id) {
            out.extend_from_slice(&extra[i..end]);
        }
        i = end;
    }
    out.extend_from_slice(&extra[i..]);
    out
}

/// One entry of the central directory.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The central record's 46 fixed bytes, signature included.
    fixed: [u8; 46],
    pub name: Vec<u8>,
    extra: Vec<u8>,
    comment: Vec<u8>,
    pub flags: u16,
    pub method: u16,
    pub csize: u64,
    pub usize: u64,
    /// Where its local record starts in the file read (leading bytes counted).
    pub offset: u64,
}

impl Entry {
    pub fn encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }

    pub fn is_dir(&self) -> bool {
        self.name.last() == Some(&b'/') || self.name.last() == Some(&b'\\')
    }

    /// The name Info-ZIP's Unicode path field gives it (version 1, the name's CRC, UTF-8).
    pub fn unicode_path(&self) -> Option<String> {
        fields(&self.extra).into_iter().find(|(id, d)| *id == UNICODE_PATH && d.len() > 5 && d[0] == 1).and_then(|(_, d)| String::from_utf8(d[5..].to_vec()).ok())
    }

    /// Its AES strength (1: 128, 2: 192, 3: 256 bits) when it is a WinZip AES entry.
    pub fn aes_strength(&self) -> Option<u8> {
        if self.method != METHOD_AES {
            return None;
        }
        fields(&self.extra).into_iter().find(|(id, d)| *id == AES && d.len() >= 7).map(|(_, d)| d[4])
    }

    /// The central record with the local record at `offset` and the name `name` (`utf8`: set the
    /// UTF-8 flag; `renamed`: drop the Unicode path field), its zip64 field rebuilt for the new
    /// offset.
    fn central(&self, offset: u64, name: &[u8], utf8: bool, renamed: bool) -> Vec<u8> {
        let mut fixed = self.fixed;
        if utf8 {
            put16(&mut fixed, 8, self.flags | FLAG_UTF8);
        }
        let drop: &[u16] = if renamed { &[ZIP64, UNICODE_PATH] } else { &[ZIP64] };
        let mut extra = without(&self.extra, drop);
        // The zip64 field holds, in this order, each of these whose fixed field reads all ones.
        let (u32f, c32f, d16f) = (u32_at(&fixed, 24) as u64, u32_at(&fixed, 20) as u64, u16_at(&fixed, 34) as u64);
        let mut z = Vec::new();
        if u32f == MAX32 {
            z.extend_from_slice(&self.usize.to_le_bytes());
        }
        if c32f == MAX32 {
            z.extend_from_slice(&self.csize.to_le_bytes());
        }
        if offset >= MAX32 {
            z.extend_from_slice(&offset.to_le_bytes());
            put32(&mut fixed, 42, MAX32 as u32);
        } else {
            put32(&mut fixed, 42, offset as u32);
        }
        if d16f == MAX16 {
            z.extend_from_slice(&0u32.to_le_bytes());
        }
        if !z.is_empty() {
            let mut f = Vec::with_capacity(4 + z.len());
            f.extend_from_slice(&ZIP64.to_le_bytes());
            f.extend_from_slice(&(z.len() as u16).to_le_bytes());
            f.extend_from_slice(&z);
            f.extend_from_slice(&extra);
            extra = f;
        }
        put16(&mut fixed, 28, name.len() as u16);
        put16(&mut fixed, 30, extra.len() as u16);
        put16(&mut fixed, 32, self.comment.len() as u16);
        let mut out = Vec::with_capacity(46 + name.len() + extra.len() + self.comment.len());
        out.extend_from_slice(&fixed);
        out.extend_from_slice(name);
        out.extend_from_slice(&extra);
        out.extend_from_slice(&self.comment);
        out
    }
}

/// An archive's central directory: its entries in order, and the archive's comment.
#[derive(Clone, Debug, Default)]
pub struct Directory {
    pub entries: Vec<Entry>,
    pub comment: Vec<u8>,
    /// Where the central directory starts in the file read.
    pub start: u64,
}

fn read_at<R: Read + Seek>(f: &mut R, at: u64, len: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; len];
    f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    f.read_exact(&mut b).map_err(|e| format!("the archive ends early: {e}"))?;
    Ok(b)
}

/// Read the central directory of the zip in `f`.
pub fn read<R: Read + Seek>(f: &mut R) -> Result<Directory, String> {
    let bad = |why: &str| format!("not a readable zip archive: {why}");
    let len = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let tail = len.min(65_535 + 22);
    let buf = read_at(f, len - tail, tail as usize)?;
    // The end record whose comment reaches the end of the file; else the last one found.
    let found: Vec<usize> = (0..buf.len().saturating_sub(21)).rev().filter(|&i| u32_at(&buf, i) == EOCD).collect();
    let i = found
        .iter()
        .copied()
        .find(|&i| i + 22 + u16_at(&buf, i + 20) as usize == buf.len())
        .or_else(|| found.first().copied())
        .ok_or_else(|| bad("no end of central directory"))?;
    let eocd_pos = len - tail + i as u64;
    let clen = (u16_at(&buf, i + 20) as usize).min(buf.len() - i - 22);
    let comment = buf[i + 22..i + 22 + clen].to_vec();
    let (mut count, mut size, mut start) = (u16_at(&buf, i + 10) as u64, u32_at(&buf, i + 12) as u64, u32_at(&buf, i + 16) as u64);
    let mut end = eocd_pos;
    if (count == MAX16 || size == MAX32 || start == MAX32) && eocd_pos >= 20 {
        let loc = read_at(f, eocd_pos - 20, 20)?;
        if u32_at(&loc, 0) == LOCATOR64 {
            let at = u64_at(&loc, 8);
            let z = read_at(f, at, 56)?;
            if u32_at(&z, 0) != EOCD64 {
                return Err(bad("no zip64 end of central directory where its locator points"));
            }
            (count, size, start) = (u64_at(&z, 32), u64_at(&z, 40), u64_at(&z, 48));
            end = at;
        }
    }
    let shift = end.checked_sub(start + size).ok_or_else(|| bad("the central directory runs past its end record"))?;
    let cd = read_at(f, start + shift, usize::try_from(size).map_err(|_| bad("central directory too large"))?)?;
    let mut entries = Vec::new();
    let mut p = 0usize;
    for _ in 0..count {
        if p + 46 > cd.len() || u32_at(&cd, p) != CENTRAL {
            return Err(bad("a damaged central directory"));
        }
        let fixed: [u8; 46] = cd[p..p + 46].try_into().expect("46 bytes");
        let (n, x, c) = (u16_at(&fixed, 28) as usize, u16_at(&fixed, 30) as usize, u16_at(&fixed, 32) as usize);
        if p + 46 + n + x + c > cd.len() {
            return Err(bad("a damaged central directory"));
        }
        let name = cd[p + 46..p + 46 + n].to_vec();
        let extra = cd[p + 46 + n..p + 46 + n + x].to_vec();
        let comment = cd[p + 46 + n + x..p + 46 + n + x + c].to_vec();
        let (mut usize_, mut csize, mut offset) = (u32_at(&fixed, 24) as u64, u32_at(&fixed, 20) as u64, u32_at(&fixed, 42) as u64);
        if let Some((_, z)) = fields(&extra).into_iter().find(|(id, _)| *id == ZIP64) {
            let mut k = 0;
            let mut next = |v: &mut u64| {
                if *v == MAX32 && k + 8 <= z.len() {
                    *v = u64_at(z, k);
                    k += 8;
                }
            };
            next(&mut usize_);
            next(&mut csize);
            next(&mut offset);
        }
        entries.push(Entry {
            fixed,
            name,
            extra,
            comment,
            flags: u16_at(&fixed, 8),
            method: u16_at(&fixed, 10),
            csize,
            usize: usize_,
            offset: offset + shift,
        });
        p += 46 + n + x + c;
    }
    Ok(Directory { entries, comment, start: start + shift })
}

/// An entry's local record as found: its 30 fixed bytes, name and extra field, where its data
/// starts, and the length of its data with the data descriptor after it.
struct Local {
    fixed: [u8; 30],
    extra: Vec<u8>,
    data_at: u64,
    body: u64,
}

fn local<R: Read + Seek>(f: &mut R, e: &Entry) -> Result<Local, String> {
    let h = read_at(f, e.offset, 30)?;
    if u32_at(&h, 0) != LOCAL {
        return Err("not a readable zip archive: an entry's local record is missing".into());
    }
    let fixed: [u8; 30] = h[..30].try_into().expect("30 bytes");
    let (n, x) = (u16_at(&fixed, 26) as u64, u16_at(&fixed, 28) as u64);
    let extra = read_at(f, e.offset + 30 + n, x as usize)?;
    let data_at = e.offset + 30 + n + x;
    let mut body = e.csize;
    if e.flags & FLAG_DESCRIPTOR != 0 {
        let wide = e.csize >= MAX32 || e.usize >= MAX32 || fields(&extra).iter().any(|(id, _)| *id == ZIP64);
        let sig = read_at(f, data_at + e.csize, 4).map(|b| u32_at(&b, 0) == DESCRIPTOR).unwrap_or(false);
        body += if sig { 4 } else { 0 } + 4 + if wide { 16 } else { 8 };
    }
    Ok(Local { fixed, extra, data_at, body })
}

/// A zip being written: entries copied as bytes, the crate's new ones adopted, then the central
/// directory and the end records.
pub struct Out {
    w: BufWriter<std::fs::File>,
    pos: u64,
    central: Vec<Vec<u8>>,
}

impl Out {
    pub fn create(path: &std::path::Path) -> Result<Out, String> {
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).map_err(|e| format!("cannot write beside the archive: {e}"))?;
        Ok(Out { w: BufWriter::with_capacity(256 * 1024, f), pos: 0, central: Vec::new() })
    }

    /// Copy entry `e` of the zip in `src` as it is, under `name` when given (a rename: `\` read as
    /// `/`, the UTF-8 flag set for a name that is not ASCII, the Unicode path field dropped).
    /// `progress` gets the bytes copied so far for this entry.
    pub fn copy<R: Read + Seek>(&mut self, src: &mut R, e: &Entry, name: Option<&str>, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
        let l = local(src, e)?;
        let offset = self.pos;
        let renamed = name.is_some();
        let new_name: Vec<u8> = match name {
            Some(n) => n.replace('\\', "/").into_bytes(),
            None => e.name.clone(),
        };
        let utf8 = renamed && !new_name.is_ascii();
        let mut fixed = l.fixed;
        if utf8 {
            let f = u16_at(&fixed, 6) | FLAG_UTF8;
            put16(&mut fixed, 6, f);
        }
        let extra = if renamed { without(&l.extra, &[UNICODE_PATH]) } else { l.extra.clone() };
        put16(&mut fixed, 26, new_name.len() as u16);
        put16(&mut fixed, 28, extra.len() as u16);
        let io = |e: std::io::Error| e.to_string();
        self.w.write_all(&fixed).map_err(io)?;
        self.w.write_all(&new_name).map_err(io)?;
        self.w.write_all(&extra).map_err(io)?;
        src.seek(SeekFrom::Start(l.data_at)).map_err(io)?;
        let mut left = l.body;
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        while left > 0 {
            let k = buf.len().min(left as usize);
            src.read_exact(&mut buf[..k]).map_err(|e| format!("the archive ends early: {e}"))?;
            self.w.write_all(&buf[..k]).map_err(io)?;
            left -= k as u64;
            done += k as u64;
            progress(done);
        }
        self.pos += 30 + new_name.len() as u64 + extra.len() as u64 + l.body;
        self.central.push(e.central(offset, &new_name, utf8, renamed));
        Ok(())
    }

    /// The writer the `zip` crate appends new entries through (it records their offsets from the
    /// stream's position, so they land after the copied ones). `adopt` must follow its `finish`.
    pub fn writer(&mut self) -> &mut BufWriter<std::fs::File> {
        &mut self.w
    }

    /// Take the entries the crate appended since the copies: read the central directory it wrote
    /// after them, cut it off, and keep its records for ours. Returns how many there were.
    pub fn adopt(&mut self) -> Result<usize, String> {
        let io = |e: std::io::Error| e.to_string();
        self.w.flush().map_err(io)?;
        let f = self.w.get_mut();
        let dir = read(f)?;
        if dir.entries.iter().any(|e| e.offset < self.pos) || dir.start < self.pos {
            return Err("the new entries landed among the copied ones".into());
        }
        f.set_len(dir.start).map_err(io)?;
        f.seek(SeekFrom::Start(dir.start)).map_err(io)?;
        self.pos = dir.start;
        let n = dir.entries.len();
        for e in dir.entries {
            let name = e.name.clone();
            self.central.push(e.central(e.offset, &name, false, false));
        }
        Ok(n)
    }

    /// Write the central directory and the end records (zip64's when needed), with `comment`.
    pub fn finish(mut self, comment: &[u8]) -> Result<std::fs::File, String> {
        let io = |e: std::io::Error| e.to_string();
        let start = self.pos;
        let mut size = 0u64;
        for c in &self.central {
            self.w.write_all(c).map_err(io)?;
            size += c.len() as u64;
        }
        let count = self.central.len() as u64;
        let end64 = start + size;
        if count >= MAX16 || start >= MAX32 || size >= MAX32 {
            let mut z = Vec::with_capacity(56 + 20);
            z.extend_from_slice(&EOCD64.to_le_bytes());
            z.extend_from_slice(&44u64.to_le_bytes());
            z.extend_from_slice(&45u16.to_le_bytes());
            z.extend_from_slice(&45u16.to_le_bytes());
            z.extend_from_slice(&0u32.to_le_bytes());
            z.extend_from_slice(&0u32.to_le_bytes());
            z.extend_from_slice(&count.to_le_bytes());
            z.extend_from_slice(&count.to_le_bytes());
            z.extend_from_slice(&size.to_le_bytes());
            z.extend_from_slice(&start.to_le_bytes());
            z.extend_from_slice(&LOCATOR64.to_le_bytes());
            z.extend_from_slice(&0u32.to_le_bytes());
            z.extend_from_slice(&end64.to_le_bytes());
            z.extend_from_slice(&1u32.to_le_bytes());
            self.w.write_all(&z).map_err(io)?;
        }
        let mut e = Vec::with_capacity(22 + comment.len());
        e.extend_from_slice(&EOCD.to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes());
        e.extend_from_slice(&(count.min(MAX16) as u16).to_le_bytes());
        e.extend_from_slice(&(count.min(MAX16) as u16).to_le_bytes());
        e.extend_from_slice(&(size.min(MAX32) as u32).to_le_bytes());
        e.extend_from_slice(&(start.min(MAX32) as u32).to_le_bytes());
        let comment = &comment[..comment.len().min(MAX16 as usize)];
        e.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        e.extend_from_slice(comment);
        self.w.write_all(&e).map_err(io)?;
        self.w.into_inner().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_with(fixed_sizes: (u32, u32, u16), extra: Vec<u8>) -> Entry {
        let mut fixed = [0u8; 46];
        put32(&mut fixed, 0, CENTRAL);
        put32(&mut fixed, 20, fixed_sizes.1);
        put32(&mut fixed, 24, fixed_sizes.0);
        put16(&mut fixed, 34, fixed_sizes.2);
        Entry { fixed, name: b"a.txt".to_vec(), extra, comment: Vec::new(), flags: 0, method: 8, csize: 5, usize: 7, offset: 0 }
    }

    #[test]
    fn extra_fields_are_kept_dropped_and_rebuilt() {
        // A timestamp field (0x5455) and a Unicode path field (0x7075).
        let mut extra = vec![0x55, 0x54, 5, 0, 1, 2, 3, 4, 5];
        extra.extend_from_slice(&[0x75, 0x70, 2, 0, 9, 9]);
        assert_eq!(fields(&extra), vec![(0x5455, &[1u8, 2, 3, 4, 5][..]), (0x7075, &[9u8, 9][..])]);
        assert_eq!(without(&extra, &[UNICODE_PATH]), extra[..9].to_vec());
        assert_eq!(without(&[1, 0, 9, 0, 1], &[ZIP64]), Vec::<u8>::new(), "a field cut short is dropped whole");
        // An entry under 4 GiB at an offset under 4 GiB: no zip64 field, the offset in place.
        let e = entry_with((7, 5, 0), extra.clone());
        let c = e.central(1234, b"b.txt", false, false);
        assert_eq!((u32_at(&c, 42), u16_at(&c, 28), u16_at(&c, 30)), (1234, 5, extra.len() as u16));
        assert_eq!(&c[46..51], b"b.txt");
        // A rename drops the Unicode path field, keeps the timestamp.
        let r = e.central(0, "café.txt".as_bytes(), true, true);
        assert_eq!(&r[46 + 9..], &extra[..9]);
        assert_eq!(u16_at(&r, 8) & FLAG_UTF8, FLAG_UTF8);
        // An offset past 4 GiB goes into a zip64 field, the fixed field all ones.
        let far = 5u64 << 30;
        let z = e.central(far, b"a.txt", false, false);
        assert_eq!(u32_at(&z, 42) as u64, MAX32);
        assert_eq!(fields(&z[51..]), vec![(ZIP64, &far.to_le_bytes()[..]), (0x5455, &[1u8, 2, 3, 4, 5][..]), (0x7075, &[9u8, 9][..])]);
        // Sizes already in a zip64 field stay there, in order, with the new offset after them.
        let big = entry_with((MAX32 as u32, MAX32 as u32, 0), vec![1, 0, 16, 0, 7, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let b = big.central(far, b"a.txt", false, false);
        let f = fields(&b[51..]);
        assert_eq!(f.len(), 1);
        assert_eq!((u64_at(f[0].1, 0), u64_at(f[0].1, 8), u64_at(f[0].1, 16)), (7, 5, far));
    }

    #[test]
    fn copies_keep_every_byte_but_the_name_and_offset() {
        use std::io::Cursor;
        // A zip from the crate: a stored entry with a comment, a deflated one, a folder.
        let mut src = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut src);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("a.txt", o.compression_method(zip::CompressionMethod::Stored)).unwrap();
            w.write_all(b"alpha").unwrap();
            w.add_directory("d/", o).unwrap();
            w.start_file("d/b.txt", o.compression_method(zip::CompressionMethod::Deflated)).unwrap();
            w.write_all(&b"beta ".repeat(100)).unwrap();
            w.set_comment("the archive's comment").unwrap();
            w.finish().unwrap();
        }
        let dir = read(&mut src).unwrap();
        assert_eq!(dir.entries.iter().map(|e| String::from_utf8_lossy(&e.name).into_owned()).collect::<Vec<_>>(), ["a.txt", "d/", "d/b.txt"]);
        assert_eq!(dir.comment, b"the archive's comment");
        let path = std::env::temp_dir().join(format!("twinfm-p8l2-raw-{}.zip", std::process::id()));
        let mut out = Out::create(&path).unwrap();
        // b.txt first, renamed to a non-ASCII name; a.txt as it is; the folder left out.
        out.copy(&mut src, &dir.entries[2], Some("d\\bêta.txt"), &mut |_| {}).unwrap();
        out.copy(&mut src, &dir.entries[0], None, &mut |_| {}).unwrap();
        // The crate appends one more after them.
        {
            let mut w = zip::ZipWriter::new(out.writer());
            w.start_file("new.txt", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"fresh").unwrap();
            w.finish().unwrap();
        }
        assert_eq!(out.adopt().unwrap(), 1);
        out.finish(&dir.comment).unwrap();
        let mut za = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut read_name = |n: &str| {
            let mut s = String::new();
            za.by_name(n).unwrap().read_to_string(&mut s).unwrap();
            s
        };
        assert_eq!(read_name("d/bêta.txt"), "beta ".repeat(100));
        assert_eq!(read_name("a.txt"), "alpha");
        assert_eq!(read_name("new.txt"), "fresh");
        assert_eq!(za.comment(), b"the archive's comment");
        assert_eq!(za.len(), 3);
        drop(za);
        // Our own reader agrees, and the copied entry's compressed bytes are the source's.
        let mut f = std::fs::File::open(&path).unwrap();
        let back = read(&mut f).unwrap();
        assert_eq!(back.entries[0].csize, dir.entries[2].csize);
        assert_eq!(back.entries[0].flags & FLAG_UTF8, FLAG_UTF8);
        let _ = std::fs::remove_file(&path);
    }
}
