//! Content search (P11-M1): what the recursive search reads inside one file. The walker
//! (`enumerate::walk`) hands every plain file of the tree here; a file over `CONTENT_MAX_BYTES` is
//! never opened, a binary one is closed after its first `SNIFF_BYTES`, and a text one is decoded by
//! the preview's rule (a UTF-16 byte order mark, else UTF-8 with invalid bytes replaced) and searched
//! with a regular expression: a typed one (P15-M4), or since P16-M4 the plain text escaped (`plain`),
//! case-insensitive either way; the line the match starts in is kept for the result's row.
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

/// A plain text as the expression that finds it (P16-M4): every character itself, case-insensitive
/// by the engine's rule (Unicode simple case folding: `σ`, `ς` and `Σ` are one letter, as are `s`
/// and `ſ`; `İ` is found only by `İ`), on the engine a typed `re:` expression uses
/// (`model::compile`), so no file's text is copied to be searched. Until P16-M4 the text and each
/// file were lowercased into new strings and compared.
pub fn plain(text: &str) -> Result<regex::Regex, String> {
    crate::model::compile(&regex::escape(text))
}

/// The listing's attributes say the file's data is not on this machine (the preview's rule).
pub fn cloud_only(attrs: u32) -> bool {
    attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE) != 0
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

/// The line of `text` where `re` first matches, as the row shows it: the line the match starts in,
/// taken from the match's own place (no second pass), cut around it.
pub fn first_match(text: &str, re: &regex::Regex) -> Option<String> {
    let at = re.find(text)?.start();
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    Some(cut_line(&text[start..end], text[start..at].chars().count()))
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

/// Read the file at `path` (`size` from the listing) and look for `needle`.
pub fn search_file(path: &Path, size: u64, needle: &regex::Regex) -> Outcome {
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
    let text = decode(&bytes, enc);
    match first_match(&text, needle) {
        Some(line) => Outcome::Found(line),
        None => Outcome::Missed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expression_shows_the_line_it_matched_in() {
        // P15-M4: `model::compile`'s expression against a file's text.
        let re = |e: &str| crate::model::compile(e).unwrap();
        let text = "first line\r\n  The Needle is here\t(end)  \nneedle at the start\r\nlast";
        let m = |e: &str| first_match(text, &re(e));
        assert_eq!(m("needle").as_deref(), Some("The Needle is here (end)"), "no letter case, the line trimmed");
        assert_eq!(m("(?-i)needle").as_deref(), Some("needle at the start"), "case-sensitive when asked");
        assert_eq!(m("^needle").as_deref(), Some("needle at the start"), "^ at a line's start");
        assert_eq!(m(r"line$").as_deref(), Some("first line"), "$ before a CRLF");
        assert_eq!(m(r"start$").as_deref(), Some("needle at the start"));
        assert_eq!(m("^last$").as_deref(), Some("last"), "the last line, no line break after it");
        assert_eq!(m(r"\d+"), None);
        assert_eq!(m("TODO|here").as_deref(), Some("The Needle is here (end)"));
        // A dot stays inside its line; an expression that spans lines shows the one it starts in.
        assert_eq!(m("first.*needle"), None);
        assert_eq!(m(r"line\s+the").as_deref(), Some("first line"));
        // A match deep in a long line is shown from a little before it.
        let deep = format!("{}needle 42{}", "a".repeat(5000), "b".repeat(5000));
        let row = first_match(&deep, &re(r"needle \d+")).unwrap();
        assert_eq!(row.chars().count(), LINE_MAX_CHARS + 2);
        assert_eq!(row.find("needle 42"), Some('…'.len_utf8() + LINE_LEAD_CHARS));
        // Characters of several bytes before the match: the cut counts characters.
        assert_eq!(first_match("x\nİstanbul Needle\n", &re("needle")).as_deref(), Some("İstanbul Needle"));
    }

    /// The P16-M4 probe (not part of the gates: `cargo test --release -- --ignored
    /// probe_plain_against_the_engine --nocapture`): where a plain text searched as an escaped,
    /// case-insensitive expression answers differently from `fold` and a substring. Every
    /// character with a case mapping as the needle, against every character there is as the
    /// text; then some words.
    #[test]
    #[ignore]
    fn probe_plain_against_the_engine() {
        // The comparison used until P16-M4: lowercase one character at a time, a final sigma as
        // a sigma, the needle a substring of the text.
        fn fold(s: &str) -> String {
            s.chars().flat_map(|c| c.to_lowercase().map(|l| if l == 'ς' { 'σ' } else { l })).collect()
        }
        let all: Vec<char> = (0..=0x10FFFFu32).filter_map(char::from_u32).collect();
        let hay: String = all.iter().collect();
        let cased: Vec<char> = all.iter().copied().filter(|c| c.to_lowercase().ne([*c]) || c.to_uppercase().ne([*c])).collect();
        // Characters whose folded form is not themselves, with it.
        let changed: Vec<(char, String)> = all.iter().filter_map(|&c| Some((c, fold(&c.to_string()))).filter(|(c, f)| f.chars().ne([*c]))).collect();
        eprintln!("[probe] {} characters, {} with a case mapping, {} that fold to something else", all.len(), cased.len(), changed.len());
        let name = |c: char| format!("{} U+{:04X}", if c.is_control() || c == '\u{345}' { ' ' } else { c }, c as u32);
        let (mut only_old, mut only_new) = (Vec::new(), Vec::new());
        for &n in &cased {
            let needle = fold(&n.to_string());
            let mut old: Vec<char> = changed.iter().filter(|(_, f)| f.contains(&needle)).map(|(c, _)| *c).collect();
            let mut own = needle.chars();
            if let (Some(x), None) = (own.next(), own.next()) {
                if fold(&x.to_string()).chars().eq([x]) {
                    old.push(x);
                }
            }
            old.sort_unstable();
            let re = plain(&n.to_string()).unwrap();
            let new: Vec<char> = re.find_iter(&hay).flat_map(|m| m.as_str().chars().collect::<Vec<_>>()).collect();
            for &c in old.iter().filter(|c| !new.contains(c)) {
                only_old.push(format!("{} in {}", name(n), name(c)));
            }
            for &c in new.iter().filter(|c| !old.contains(c)) {
                only_new.push(format!("{} in {}", name(n), name(c)));
            }
        }
        eprintln!("[probe] found today, not by the engine: {}", only_old.len());
        only_old.iter().for_each(|l| eprintln!("[probe]   {l}"));
        eprintln!("[probe] found by the engine, not today: {}", only_new.len());
        only_new.iter().for_each(|l| eprintln!("[probe]   {l}"));
        let words = [
            ("i", "İ"), ("ali", "ALİ"), ("istanbul", "İSTANBUL"), ("İstanbul", "istanbul"), ("İstanbul", "İSTANBUL"), ("İstanbul", "i\u{307}stanbul"),
            ("istanbul", "ISTANBUL"), ("ı", "I"), ("straße", "STRASSE"), ("strasse", "Straße"), ("straße", "STRAẞE"), ("ﬁle", "FILE"), ("file", "ﬁle"),
            ("σίσυφος", "ΣΊΣΥΦΟΣ"), ("ΣΊΣΥΦΟΣ", "σίσυφος"), ("ς", "Σ"), ("s", "ſ"), ("ſ", "S"), ("μ", "µ"), ("k", "\u{212A}"), ("å", "\u{212B}"),
            ("é", "É"), ("é", "e\u{301}"), ("ǆ", "ǅ"), ("ǌ", "Ǌ"), ("ა", "Ა"), ("ꭰ", "Ꭰ"), ("ԥ", "Ԥ"), ("a.b", "A.B"), ("a.b", "AxB"), ("(x)", "(X)"), ("[a]", "[A]"), ("a\\b", "A\\B"),
        ];
        for (n, text) in words {
            let old = fold(text).contains(&fold(n));
            let new = first_match(text, &plain(n).unwrap()).is_some();
            eprintln!("[probe] {:9} \"{n}\" in \"{text}\": today {old}, the engine {new}", if old == new { "same" } else { "DIFFERENT" });
        }
    }

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
        let m = |text: &str, needle: &str| first_match(text, &plain(needle).unwrap());
        let text = "first line\r\n  The Needle is here\t(end)  \nanother needle\n";
        assert_eq!(m(text, "needle").as_deref(), Some("The Needle is here (end)"), "the first, trimmed, a tab as a space");
        assert_eq!(m(text, "NEEDLE").as_deref(), Some("The Needle is here (end)"), "no letter case, either way");
        assert_eq!(m("no match", "needle"), None);
        assert_eq!(m("needle", "needle").as_deref(), Some("needle"), "no line break at all");
        assert_eq!(m("ΣΊΣΥΦΟΣ rolls", "σίσυφος").as_deref(), Some("ΣΊΣΥΦΟΣ rolls"), "Greek, the final sigma as any sigma");
        assert_eq!(m("σίσυφος rolls", "ΣΊΣΥΦΟΣ").as_deref(), Some("σίσυφος rolls"), "and the other way");
        // 'İ' before the match: the line is shown around the match as any other.
        assert_eq!(m("x\nİstanbul Needle\n", "needle").as_deref(), Some("İstanbul Needle"));
        // A short match in a long line: the first 200 characters and an ellipsis.
        let long = format!("needle {}", "x".repeat(300));
        let row = m(&long, "needle").unwrap();
        assert_eq!(row.chars().count(), LINE_MAX_CHARS + 1);
        assert!(row.starts_with("needle ") && row.ends_with('…'));
        // A match deep in a long line: shown from 40 characters before it.
        let deep = format!("{}needle{}", "a".repeat(5000), "b".repeat(5000));
        let row = m(&deep, "needle").unwrap();
        assert!(row.starts_with('…') && row.ends_with('…'), "{row}");
        assert_eq!(row.chars().count(), LINE_MAX_CHARS + 2);
        assert_eq!(row.find("needle"), Some('…'.len_utf8() + LINE_LEAD_CHARS));
        // A match near the end of a long line: the line's tail, no ellipsis after it.
        let tail = format!("{}needle", "a".repeat(500));
        assert!(m(&tail, "needle").unwrap().ends_with("needle"));
    }

    /// P16-M4: a plain text is searched for as itself, whatever it holds, and by the engine's
    /// letter cases; where those differ from the lowercase comparison used until then, the answer
    /// chosen is pinned (the probe below lists every such letter).
    #[test]
    fn plain_text_is_an_escaped_expression() {
        let m = |text: &str, needle: &str| first_match(text, &plain(needle).unwrap()).is_some();
        // Every character is itself: nothing typed is an operator.
        assert!(m("see A.B here", "a.b") && !m("see AxB here", "a.b"));
        assert!(m("f(X) [Y] {z} a|b c*d e+f g?h ^i$ j\\k", "f(x) [y] {z} a|b c*d e+f g?h ^i$ j\\k"));
        assert!(!m("fx", "f(x)") && !m("ab", "a|b") && !m("cd", "c*d"));
        assert!(m("price: $5.00 (USD)", "$5.00 (usd)"));
        // The same as before: accents, sharp s, ligatures and separate accents are not unfolded.
        assert!(m("CAFÉ", "café") && m("STRAẞE", "straße"));
        assert!(!m("STRASSE", "straße") && !m("ﬁle", "file") && !m("cafe\u{301}", "café"));
        // Found by the engine and not before: other shapes of the same letter.
        assert!(m("ſ", "s") && m("S", "ſ"), "the long s");
        assert!(m("5 µm", "μm") && m("5 μm", "µm"), "the micro sign and the Greek mu");
        assert!(m("ϑ", "θ") && m("ϕ", "φ"), "the Greek symbol forms");
        // Found before and not by the engine: the dotted capital I is found only by itself.
        assert!(!m("İ", "i") && !m("ALİ", "ali"), "it was found by a plain i");
        assert!(m("İSTANBUL", "İstanbul") && !m("İSTANBUL", "istanbul") && !m("istanbul", "İstanbul"));
        assert!(m("ISTANBUL", "istanbul"));
        // Nothing to look for is no expression.
        assert!(plain("").is_err());
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
        let t = |s: &str| plain(s).unwrap();
        let found = search_file(&text, size(&text), &t("gamma"));
        let missed = search_file(&text, size(&text), &t("delta"));
        let utf16 = search_file(&wide, size(&wide), &t("drei"));
        let binary = search_file(&bin, size(&bin), &t("gamma"));
        let over = search_file(&text, CONTENT_MAX_BYTES + 1, &t("gamma"));
        let gone = search_file(&dir.join("nope.txt"), 1, &t("gamma"));
        // P15-M4: a typed expression reads the same file.
        let by_re = search_file(&text, size(&text), &crate::model::compile("^BETA g").unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, Outcome::Found("beta Gamma".into()));
        assert_eq!(by_re, Outcome::Found("beta Gamma".into()));
        assert_eq!(missed, Outcome::Missed);
        assert_eq!(utf16, Outcome::Found("zwei Drei".into()));
        assert_eq!(binary, Outcome::Binary);
        assert_eq!(over, Outcome::OverCap, "by the listing's size, never opened");
        assert_eq!(gone, Outcome::Unreadable);
        assert!(cloud_only(FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) && cloud_only(FILE_ATTRIBUTE_OFFLINE) && !cloud_only(0x20));
    }
}
