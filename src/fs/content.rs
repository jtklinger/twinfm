//! Content search (P11-M1): what the recursive search reads inside one file. The walker
//! (`enumerate::walk`) hands every plain file of the tree here; a file over `CONTENT_MAX_BYTES` is
//! never opened, a binary one is closed after its first `SNIFF_BYTES`, and a text one is decoded by
//! the preview's rule (a UTF-16 byte order mark, else UTF-8 with invalid bytes replaced) and searched
//! case-insensitively for a plain substring; the first matching line is kept for the result's row.
//! A cloud-only file (by the listing's attributes) is never opened: reading it would download it.

use std::io::Read;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN};

/// Files larger than this are skipped (counted as over the cap), never opened.
pub const CONTENT_MAX_BYTES: u64 = 16 << 20;
/// Binary is decided by the file's first bytes.
pub const SNIFF_BYTES: usize = 8 << 10;
/// The matching line on the row is cut to this many characters.
pub const LINE_MAX_CHARS: usize = 200;
/// A match further into a long line than fits is shown from this many characters before it.
const LINE_LEAD_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// What reading one file gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The first matching line, cut for the row.
    Found(String),
    Missed,
    OverCap,
    Binary,
    /// It could not be opened or read (in use, access denied, gone).
    Unreadable,
}

/// The listing's attributes say the file's data is not on this machine (the preview's rule).
pub fn cloud_only(attrs: u32) -> bool {
    attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE) != 0
}

/// Lower case, one character at a time, a final sigma `ς` read as `σ` (so "ΣΊΣΥΦΟΣ" and "σίσυφος"
/// fold alike; `char::to_lowercase` never gives `ς`).
pub fn fold(s: &str) -> String {
    s.chars().flat_map(lower).collect()
}

fn lower(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|l| if l == 'ς' { 'σ' } else { l })
}

/// Text or binary from the file's first bytes: a UTF-16 byte order mark is text; else a NUL byte,
/// or over 10% of the characters invalid UTF-8 or control characters (other than tab, line breaks,
/// form feed and escape), in the first `SNIFF_BYTES` is binary (None).
pub fn sniff(head: &[u8]) -> Option<Encoding> {
    match head {
        [0xFF, 0xFE, ..] => return Some(Encoding::Utf16Le),
        [0xFE, 0xFF, ..] => return Some(Encoding::Utf16Be),
        _ => {}
    }
    let head = &head[..head.len().min(SNIFF_BYTES)];
    if head.contains(&0) {
        return None;
    }
    let (mut all, mut odd) = (0usize, 0usize);
    for c in String::from_utf8_lossy(head).chars() {
        all += 1;
        if c == '\u{FFFD}' || (c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\x0C' | '\x1B')) {
            odd += 1;
        }
    }
    (odd * 10 <= all).then_some(Encoding::Utf8)
}

/// The whole text, without its byte order mark.
pub fn decode(bytes: &[u8], enc: Encoding) -> String {
    match enc {
        Encoding::Utf8 => String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let be = enc == Encoding::Utf16Be;
            let units: Vec<u16> = bytes[2.min(bytes.len())..]
                .chunks_exact(2)
                .map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) })
                .collect();
            String::from_utf16_lossy(&units)
        }
    }
}

/// The first line of `text` holding `needle` (already `fold`ed, not empty), as the row shows it.
pub fn first_match(text: &str, needle: &str) -> Option<String> {
    // Fold the whole text once; when no character changed its length the offsets are the text's.
    let mut folded = String::with_capacity(text.len());
    let mut same = true;
    for c in text.chars() {
        let before = folded.len();
        folded.extend(lower(c));
        same &= folded.len() - before == c.len_utf8();
    }
    if same {
        let at = folded.find(needle)?;
        let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        return Some(cut_line(&text[start..end], text[start..at].chars().count()));
    }
    // A character that folds to another length (rare): line by line, the line shown from its start.
    text.lines().find(|l| fold(l).contains(needle)).map(|l| cut_line(l, 0))
}

/// The line for the row: control characters as spaces, trimmed, at most `LINE_MAX_CHARS`
/// characters; a match at character `at` that would fall past the cut is shown from a little
/// before it, behind an ellipsis.
fn cut_line(line: &str, at: usize) -> String {
    let chars: Vec<char> = line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let lead = chars.iter().take_while(|c| c.is_whitespace()).count();
    let end = chars.len() - chars.iter().rev().take_while(|c| c.is_whitespace()).count();
    if lead >= end {
        return String::new();
    }
    let at = at.clamp(lead, end);
    let from = if at + LINE_LEAD_CHARS < lead + LINE_MAX_CHARS { lead } else { at - LINE_LEAD_CHARS };
    let to = end.min(from + LINE_MAX_CHARS);
    let mut s = String::new();
    if from > lead {
        s.push('…');
    }
    s.extend(&chars[from..to]);
    if to < end {
        s.push('…');
    }
    s
}

/// Read the file at `path` (`size` from the listing) and look for `needle` (`fold`ed).
pub fn search_file(path: &Path, size: u64, needle: &str) -> Outcome {
    if size > CONTENT_MAX_BYTES {
        return Outcome::OverCap;
    }
    let Ok(mut f) = std::fs::File::open(path) else { return Outcome::Unreadable };
    let mut bytes = Vec::with_capacity(size as usize);
    if (&mut f).take(SNIFF_BYTES as u64).read_to_end(&mut bytes).is_err() {
        return Outcome::Unreadable;
    }
    let Some(enc) = sniff(&bytes) else { return Outcome::Binary };
    // The listing's size may be stale: never past the cap.
    if f.take(CONTENT_MAX_BYTES - bytes.len() as u64).read_to_end(&mut bytes).is_err() {
        return Outcome::Unreadable;
    }
    match first_match(&decode(&bytes, enc), needle) {
        Some(line) => Outcome::Found(line),
        None => Outcome::Missed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_is_told_by_a_nul_or_the_share_of_control_characters() {
        assert_eq!(sniff(b"plain text\r\n\tindented\x0C"), Some(Encoding::Utf8));
        assert_eq!(sniff(b"MZ\x90\x00\x03"), None, "a NUL");
        assert_eq!(sniff(&[0xFF, 0xFE, b'a', 0, b'b', 0]), Some(Encoding::Utf16Le), "UTF-16's NULs are text behind its mark");
        assert_eq!(sniff(&[0xFE, 0xFF, 0, b'a']), Some(Encoding::Utf16Be));
        // Nine control bytes in 100 characters is text; eleven is not.
        let mut nine = vec![b'a'; 91];
        nine.extend([1u8; 9]);
        assert_eq!(sniff(&nine), Some(Encoding::Utf8));
        let mut eleven = vec![b'a'; 89];
        eleven.extend([1u8; 11]);
        assert_eq!(sniff(&eleven), None);
        // Invalid UTF-8 counts as odd; accented UTF-8 does not.
        assert_eq!(sniff("café crème brûlée".as_bytes()), Some(Encoding::Utf8));
        assert_eq!(sniff(&[0xC3, 0x28, 0xA0, 0xA1, b'x']), None);
        // Only the first 8 KB is looked at: a NUL after it does not make a file binary.
        let mut late = vec![b'a'; SNIFF_BYTES];
        late.push(0);
        assert_eq!(sniff(&late), Some(Encoding::Utf8));
        assert_eq!(sniff(b""), Some(Encoding::Utf8), "an empty file is text (and matches nothing)");
    }

    #[test]
    fn encodings_decode_as_the_preview_reads_them() {
        assert_eq!(decode(b"\xEF\xBB\xBFhello", Encoding::Utf8), "hello", "the BOM goes");
        assert_eq!(decode(b"a\xFFb", Encoding::Utf8), "a\u{FFFD}b");
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain("Grüße\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(decode(&le, Encoding::Utf16Le), "Grüße\n");
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain("Ω".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(decode(&be, Encoding::Utf16Be), "Ω");
        assert_eq!(decode(&[0xFF, 0xFE, b'a'], Encoding::Utf16Le), "", "an odd last byte is dropped");
    }

    #[test]
    fn the_first_matching_line_is_kept_and_cut() {
        let text = "first line\r\n  The Needle is here\t(end)  \nanother needle\n";
        assert_eq!(first_match(text, "needle").as_deref(), Some("The Needle is here (end)"), "the first, trimmed, a tab as a space");
        assert_eq!(first_match(text, "NEEDLE"), None, "the needle comes folded");
        assert_eq!(first_match(text, &fold("NEEDLE")).as_deref(), Some("The Needle is here (end)"));
        assert_eq!(first_match("no match", "needle"), None);
        assert_eq!(first_match("needle", "needle").as_deref(), Some("needle"), "no line break at all");
        assert_eq!(first_match("ΣΊΣΥΦΟΣ rolls", &fold("σίσυφος")).as_deref(), Some("ΣΊΣΥΦΟΣ rolls"), "Greek folds both ways");
        // 'İ' folds to two characters: the line-by-line path.
        assert_eq!(first_match("x\nİstanbul Needle\n", "needle").as_deref(), Some("İstanbul Needle"));
        // A short match in a long line: the first 200 characters and an ellipsis.
        let long = format!("needle {}", "x".repeat(300));
        let row = first_match(&long, "needle").unwrap();
        assert_eq!(row.chars().count(), LINE_MAX_CHARS + 1);
        assert!(row.starts_with("needle ") && row.ends_with('…'));
        // A match deep in a long line: shown from 40 characters before it.
        let deep = format!("{}needle{}", "a".repeat(5000), "b".repeat(5000));
        let row = first_match(&deep, "needle").unwrap();
        assert!(row.starts_with('…') && row.ends_with('…'), "{row}");
        assert_eq!(row.chars().count(), LINE_MAX_CHARS + 2);
        assert_eq!(row.find("needle"), Some('…'.len_utf8() + LINE_LEAD_CHARS));
        // A match near the end of a long line: the line's tail, no ellipsis after it.
        let tail = format!("{}needle", "a".repeat(500));
        assert!(first_match(&tail, "needle").unwrap().ends_with("needle"));
    }

    #[test]
    fn files_are_read_up_to_the_cap() {
        let dir = std::env::temp_dir().join(format!("twinfm-p11m1-content-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("a.txt");
        std::fs::write(&text, "alpha\nbeta Gamma\n").unwrap();
        let wide = dir.join("w.txt");
        std::fs::write(&wide, [0xFF, 0xFE].into_iter().chain("one\nzwei Drei\n".encode_utf16().flat_map(u16::to_le_bytes)).collect::<Vec<u8>>()).unwrap();
        let bin = dir.join("b.bin");
        std::fs::write(&bin, b"gamma\x00gamma").unwrap();
        let size = |p: &Path| std::fs::metadata(p).unwrap().len();
        let found = search_file(&text, size(&text), "gamma");
        let missed = search_file(&text, size(&text), "delta");
        let utf16 = search_file(&wide, size(&wide), "drei");
        let binary = search_file(&bin, size(&bin), "gamma");
        let over = search_file(&text, CONTENT_MAX_BYTES + 1, "gamma");
        let gone = search_file(&dir.join("nope.txt"), 1, "gamma");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, Outcome::Found("beta Gamma".into()));
        assert_eq!(missed, Outcome::Missed);
        assert_eq!(utf16, Outcome::Found("zwei Drei".into()));
        assert_eq!(binary, Outcome::Binary);
        assert_eq!(over, Outcome::OverCap, "by the listing's size, never opened");
        assert_eq!(gone, Outcome::Unreadable);
        assert!(cloud_only(FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) && cloud_only(FILE_ATTRIBUTE_OFFLINE) && !cloud_only(0x20));
    }
}
