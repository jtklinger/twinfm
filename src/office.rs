//! Word, Excel and PowerPoint for the preview pane (P9-M3): an Office Open XML file is a zip of XML
//! parts, read here with a streaming reader (quick-xml) into the Markdown preview's block list, so
//! reading can stop at 500 rows of a sheet or 64 MB of any one part and still hand back what it has.
//! Layout, fonts, colours and charts are not kept: a Word file's headings, lists, tables, links and
//! pictures; one Excel sheet as a table; a PowerPoint file's slide text, tables and pictures. Pure but
//! for the zip it is given: the preview worker opens the file and decodes the pictures (preview.rs).

use crate::markdown::{self, Block, Cell, Inline, Kind, Row};
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::io::{BufReader, Read, Seek};
use zip::ZipArchive;

/// No more than this is read of any one part (Settled first: the cap a partial result is kept at).
pub const OFFICE_MAX_XML: u64 = 64 << 20;
/// A sheet shows at most this many rows (from its first row with a value) and columns.
pub const XLSX_MAX_ROWS: u32 = 500;
pub const XLSX_MAX_COLS: u32 = 50;
/// The note put first when a part was cut at `OFFICE_MAX_XML`.
pub const CUT_NOTE: &str = "Only the first part of this document is shown.";
/// Fonts whose runs are shown as code.
const MONO: &[&str] = &["consolas", "courier new", "courier", "lucida console", "cascadia code", "cascadia mono"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeKind {
    Docx,
    Xlsx,
    Pptx,
}

impl OfficeKind {
    /// By the file's extension (any case); a macro-enabled file reads as its plain form.
    pub fn of(ext: &str) -> Option<OfficeKind> {
        match ext.to_ascii_lowercase().as_str() {
            "docx" | "docm" => Some(OfficeKind::Docx),
            "xlsx" | "xlsm" => Some(OfficeKind::Xlsx),
            "pptx" | "pptm" => Some(OfficeKind::Pptx),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OfficeKind::Docx => "docx",
            OfficeKind::Xlsx => "xlsx",
            OfficeKind::Pptx => "pptx",
        }
    }
}

/// What a document became: its blocks (pictures as `Image` blocks whose `src` is the part's name in
/// the zip), the `[preview]` line's middle (`docx, 12 blocks (...)` without the time), and whether a
/// part was cut at `OFFICE_MAX_XML` (the caller puts the note first).
#[derive(Debug, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    pub summary: String,
    pub cut: bool,
}

/// A password-protected Office file is an OLE compound file holding an `EncryptionInfo` stream, not a
/// zip: told from its first bytes (at most 64 KB).
pub fn password_protected(head: &[u8]) -> bool {
    const OLE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    let name: Vec<u8> = "EncryptionInfo".encode_utf16().flat_map(u16::to_le_bytes).collect();
    head.starts_with(&OLE) && head.windows(name.len()).any(|w| w == name.as_slice())
}

/// One XML event, simplified: an element opened (its local name and attributes by local name), an
/// element closed, or text (entities resolved). An empty element comes as an open and a close.
#[derive(Debug, PartialEq)]
enum X {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

fn attrs(e: &BytesStart) -> Vec<(String, String)> {
    e.attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .map(|a| {
            let k = String::from_utf8_lossy(a.key.local_name().as_ref()).into_owned();
            let v = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            (k, v)
        })
        .collect()
}

fn get<'a>(a: &'a [(String, String)], key: &str) -> Option<&'a str> {
    a.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// A part being read, one event at a time; `None` at its end, at an error, or where the cap cut it.
struct Part<'a> {
    r: quick_xml::Reader<BufReader<Box<dyn Read + 'a>>>,
    buf: Vec<u8>,
    closing: Option<String>,
    done: bool,
}

impl Part<'_> {
    fn next(&mut self) -> Option<X> {
        if let Some(name) = self.closing.take() {
            return Some(X::Close(name));
        }
        loop {
            if self.done {
                return None;
            }
            self.buf.clear();
            let name = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            let ev = match self.r.read_event_into(&mut self.buf) {
                Ok(Event::Start(e)) => X::Open(name(e.local_name().as_ref()), attrs(&e)),
                Ok(Event::Empty(e)) => {
                    let n = name(e.local_name().as_ref());
                    self.closing = Some(n.clone());
                    X::Open(n, attrs(&e))
                }
                Ok(Event::End(e)) => X::Close(name(e.local_name().as_ref())),
                Ok(Event::Text(t)) => X::Text(t.xml10_content().map(|c| c.into_owned()).unwrap_or_default()),
                Ok(Event::CData(t)) => X::Text(String::from_utf8_lossy(&t).into_owned()),
                Ok(Event::GeneralRef(g)) => {
                    let c = match g.resolve_char_ref() {
                        Ok(Some(c)) => Some(c),
                        _ => match g.decode().as_deref() {
                            Ok("amp") => Some('&'),
                            Ok("lt") => Some('<'),
                            Ok("gt") => Some('>'),
                            Ok("quot") => Some('"'),
                            Ok("apos") => Some('\''),
                            _ => None,
                        },
                    };
                    X::Text(c.map(String::from).unwrap_or_default())
                }
                Ok(Event::Eof) | Err(_) => {
                    self.done = true;
                    return None;
                }
                Ok(_) => continue,
            };
            return Some(ev);
        }
    }
}

/// Read part `name` with `f`; None when the zip has no such part. The bool: the part is longer than
/// `OFFICE_MAX_XML` and was cut.
fn with_part<R: Read + Seek, T>(za: &mut ZipArchive<R>, name: &str, f: impl FnOnce(&mut Part) -> T) -> Option<(T, bool)> {
    let file = za.by_name(name).ok()?;
    let cut = file.size() > OFFICE_MAX_XML;
    let reader: Box<dyn Read + '_> = Box::new(file.take(OFFICE_MAX_XML));
    let mut part = Part { r: quick_xml::Reader::from_reader(BufReader::new(reader)), buf: Vec::new(), closing: None, done: false };
    Some((f(&mut part), cut))
}

/// A relationship target resolved against the folder of the part that names it (`word/document.xml`
/// and `media/a.png` give `word/media/a.png`; `/xl/x.xml` is from the root).
fn resolve(from_part: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if let Some(abs) = target.strip_prefix('/') {
        return normalize(abs.split('/').collect());
    } else {
        from_part.rsplit_once('/').map_or(vec![], |(dir, _)| dir.split('/').collect())
    };
    parts.extend(target.split('/'));
    normalize(parts)
}

fn normalize(parts: Vec<&str>) -> String {
    let mut out: Vec<&str> = Vec::new();
    for p in parts {
        match p {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

/// A part's relationships: id -> (target resolved, or the URL as written when external; external).
fn rels<R: Read + Seek>(za: &mut ZipArchive<R>, part: &str) -> HashMap<String, (String, bool)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let mut out = HashMap::new();
    with_part(za, &path, |p| {
        while let Some(x) = p.next() {
            if let X::Open(n, a) = x {
                if n == "Relationship" {
                    if let (Some(id), Some(t)) = (get(&a, "Id"), get(&a, "Target")) {
                        let external = get(&a, "TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External"));
                        out.insert(id.to_string(), (if external { t.to_string() } else { resolve(part, t) }, external));
                    }
                }
            }
        }
    });
    out
}

/// One run of text with its formatting, or a line break.
#[derive(Debug, Clone, PartialEq)]
enum Run {
    Text { text: String, bold: bool, italic: bool, code: bool, link: Option<String> },
    Break,
}

/// Write runs into `i` as inline Markdown: neighbouring runs with the same formatting and link as one,
/// spaces kept outside the emphasis delimiters, links around their runs. `bold`: the whole is bold (a
/// heading, a head cell), so bold runs are not written.
fn write_runs(runs: &[Run], i: &mut Inline, bold: bool) {
    // Merge first: Word splits a word into several runs for spell-check marks and the like.
    let mut merged: Vec<Run> = Vec::new();
    for r in runs {
        match (merged.last_mut(), r) {
            (Some(Run::Text { text, bold: b, italic: it, code: c, link: l }), Run::Text { text: t2, bold: b2, italic: i2, code: c2, link: l2 })
                if (*b || bold) == (*b2 || bold) && it == i2 && c == c2 && l == l2 =>
            {
                text.push_str(t2)
            }
            _ => merged.push(r.clone()),
        }
    }
    let mut open_link: Option<String> = None;
    for r in &merged {
        let link = match r {
            Run::Text { link, .. } => link.clone(),
            Run::Break => open_link.clone(),
        };
        if link != open_link {
            if let Some(d) = open_link.take() {
                i.link_close(&d);
            }
            if link.is_some() {
                i.link_open();
            }
            open_link = link;
        }
        match r {
            Run::Break => i.hard_break(),
            Run::Text { text, code: true, .. } => i.code(text),
            Run::Text { text, bold: b, italic, .. } => {
                let core = text.trim();
                let lead = &text[..text.len() - text.trim_start().len()];
                let trail = &text[text.trim_end().len()..];
                i.text(lead);
                if !core.is_empty() {
                    let runs: Vec<&str> = [(*b && !bold).then_some("**"), italic.then_some("*")].into_iter().flatten().collect();
                    for d in &runs {
                        i.delim(d, true);
                    }
                    i.text(core);
                    for d in runs.iter().rev() {
                        i.delim(d, false);
                    }
                }
                i.text(trail);
            }
        }
    }
    if let Some(d) = open_link {
        i.link_close(&d);
    }
}

/// A paragraph's runs as a block of `kind`.
fn para_block(runs: &[Run], heading: Option<u8>) -> Option<Block> {
    let mut i = Inline::default();
    write_runs(runs, &mut i, heading.is_some());
    if i.plain.trim().is_empty() && !i.md.contains("](<") {
        return None;
    }
    let plain = i.plain.trim().to_string();
    Some(match heading {
        Some(level) => Block { kind: Kind::Heading, level, md: format!("**{}**", i.md), plain, ..Default::default() },
        None => Block { kind: Kind::Para, md: i.md, plain, ..Default::default() },
    })
}

/// A table being read: its rows, the open row (cells, head) and the open cell's text, and how many
/// tables are open inside its cells (their text flows into the cell).
#[derive(Default)]
struct Table {
    rows: Vec<Row>,
    row: Option<(Vec<Cell>, bool)>,
    cell: Option<Inline>,
    inner: usize,
}

impl Table {
    /// A paragraph's runs into the open cell, after a line break when it holds something already.
    fn add(&mut self, runs: &[Run]) {
        let cell = self.cell.get_or_insert_with(Inline::default);
        if runs.iter().all(|r| matches!(r, Run::Text { text, .. } if text.trim().is_empty())) {
            return;
        }
        if !cell.plain.trim().is_empty() {
            cell.hard_break();
        }
        write_runs(runs, cell, false);
    }

    fn end_cell(&mut self) {
        if let Some(i) = self.cell.take() {
            let row = self.row.get_or_insert_with(|| (Vec::new(), false));
            row.0.push(Cell { md: i.md, plain: i.plain.trim().to_string() });
        }
    }

    fn end_row(&mut self) {
        self.end_cell();
        if let Some((cells, head)) = self.row.take() {
            let head = head && cells.iter().any(|c| !c.md.is_empty());
            let cells = if head { cells.into_iter().map(|c| Cell { md: if c.md.is_empty() { c.md } else { format!("**{}**", c.md) }, plain: c.plain }).collect() } else { cells };
            self.rows.push(Row { head, cells });
        }
    }
}

/// Roman numerals for list markers.
fn roman(mut n: u64) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

fn letters(n: u64) -> String {
    // 1 a, 26 z, 27 aa (Word repeats the letter).
    let c = (b'a' + ((n.max(1) - 1) % 26) as u8) as char;
    c.to_string().repeat(((n.max(1) - 1) / 26 + 1) as usize)
}

const BULLETS: [&str; 3] = ["\u{2022}", "\u{25e6}", "\u{25aa}"];

/// A list marker for item number `n` (from 1) in Word's format `fmt` at depth `level` (0-based).
fn marker(fmt: &str, n: u64, level: usize) -> String {
    match fmt {
        "bullet" => BULLETS[level % 3].to_string(),
        "lowerLetter" => format!("{}.", letters(n)),
        "upperLetter" => format!("{}.", letters(n).to_uppercase()),
        "lowerRoman" => format!("{}.", roman(n)),
        "upperRoman" => format!("{}.", roman(n).to_uppercase()),
        "none" => String::new(),
        _ => format!("{n}."),
    }
}

/// Counts list items per list and level: a deeper level starts again when a shallower item comes.
#[derive(Default)]
struct Counters(HashMap<String, Vec<u64>>);

impl Counters {
    fn next(&mut self, list: &str, level: usize, start: u64) -> u64 {
        let c = self.0.entry(list.to_string()).or_default();
        if c.len() <= level {
            c.resize(level + 1, 0);
        }
        c[level] = if c[level] == 0 { start.max(1) } else { c[level] + 1 };
        c.truncate(level + 1);
        c[level]
    }
}

/// Read an Office file into blocks. `sheet`: the Excel sheet to show (0-based; one past the end shows
/// the first). `go_on` is asked between parts (slides, the sheet and its strings): false stops with
/// what is read so far (a newer file was asked for).
pub fn parse<R: Read + Seek>(za: &mut ZipArchive<R>, kind: OfficeKind, sheet: usize, go_on: &dyn Fn() -> bool) -> Result<Parsed, String> {
    match kind {
        OfficeKind::Docx => docx(za),
        OfficeKind::Xlsx => xlsx(za, sheet, go_on),
        OfficeKind::Pptx => pptx(za, go_on),
    }
}

/// The counts for the `[preview]` line.
fn count(blocks: &[Block], k: Kind) -> usize {
    blocks.iter().filter(|b| b.kind == k && (b.first || k != Kind::Table)).count()
}

// ---- Word -------------------------------------------------------------------------------------

/// `styles.xml`: style id -> heading level (1 to 6), for headings and the title.
fn word_styles<R: Read + Seek>(za: &mut ZipArchive<R>) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    with_part(za, "word/styles.xml", |p| {
        let (mut id, mut name, mut outline) = (None::<String>, String::new(), None::<u8>);
        while let Some(x) = p.next() {
            match x {
                X::Open(n, a) if n == "style" => {
                    id = get(&a, "styleId").map(str::to_string);
                    name.clear();
                    outline = None;
                }
                X::Open(n, a) if n == "name" => name = get(&a, "val").unwrap_or("").to_ascii_lowercase(),
                X::Open(n, a) if n == "outlineLvl" => outline = get(&a, "val").and_then(|v| v.parse::<u8>().ok()).filter(|v| *v < 6).map(|v| v + 1),
                X::Close(n) if n == "style" => {
                    let level = if name == "title" {
                        Some(1)
                    } else if let Some(l) = name.strip_prefix("heading ").and_then(|l| l.parse::<u8>().ok()).filter(|l| (1..=6).contains(l)) {
                        Some(l)
                    } else {
                        outline
                    };
                    if let (Some(id), Some(l)) = (id.take(), level) {
                        out.insert(id, l);
                    }
                }
                _ => {}
            }
        }
    });
    out
}

/// `numbering.xml`: numId -> (per level: format, start).
fn word_numbering<R: Read + Seek>(za: &mut ZipArchive<R>) -> HashMap<String, Vec<(String, u64)>> {
    let mut abstracts: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    let mut nums: HashMap<String, String> = HashMap::new();
    with_part(za, "word/numbering.xml", |p| {
        let (mut abs, mut lvl, mut num) = (None::<String>, 0usize, None::<String>);
        while let Some(x) = p.next() {
            match x {
                X::Open(n, a) if n == "abstractNum" => abs = get(&a, "abstractNumId").map(str::to_string),
                X::Open(n, a) if n == "lvl" && abs.is_some() => lvl = get(&a, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0).min(8),
                X::Open(n, a) if (n == "numFmt" || n == "start") && abs.is_some() => {
                    let levels = abstracts.entry(abs.clone().unwrap_or_default()).or_default();
                    if levels.len() <= lvl {
                        levels.resize(lvl + 1, ("decimal".into(), 1));
                    }
                    let v = get(&a, "val").unwrap_or("");
                    if n == "numFmt" {
                        levels[lvl].0 = v.to_string();
                    } else {
                        levels[lvl].1 = v.parse().unwrap_or(1);
                    }
                }
                X::Close(n) if n == "abstractNum" => abs = None,
                X::Open(n, a) if n == "num" => num = get(&a, "numId").map(str::to_string),
                X::Open(n, a) if n == "abstractNumId" => {
                    if let (Some(id), Some(v)) = (num.clone(), get(&a, "val")) {
                        nums.insert(id, v.to_string());
                    }
                }
                _ => {}
            }
        }
    });
    nums.into_iter().filter_map(|(id, abs)| abstracts.get(&abs).map(|l| (id, l.clone()))).collect()
}

/// A Word paragraph being read.
#[derive(Default)]
struct WPara {
    runs: Vec<Run>,
    style: Option<String>,
    num: Option<(String, usize)>,
    anchors: Vec<String>,
    page_before: bool,
    page_after: bool,
    pictures: Vec<Block>,
}

/// The elements whose content is never shown.
const WORD_SKIP: &[&str] = &["del", "instrText", "delText", "Fallback", "footnoteReference", "endnoteReference", "commentReference", "pPrChange", "rPrChange"];

fn docx<R: Read + Seek>(za: &mut ZipArchive<R>) -> Result<Parsed, String> {
    if za.by_name("word/document.xml").is_err() {
        return Err("no word/document.xml".into());
    }
    let styles = word_styles(za);
    let numbering = word_numbering(za);
    let links = rels(za, "word/document.xml");
    let mut out: Vec<Block> = Vec::new();
    let mut counters = Counters::default();
    let have_styles = !styles.is_empty();
    let (_, cut) = with_part(za, "word/document.xml", |p| {
        let mut paras: Vec<WPara> = Vec::new();
        let mut tables: Vec<Table> = Vec::new();
        let (mut bold, mut italic, mut code, mut in_t, mut in_ppr, mut in_rpr) = (false, false, false, false, false, false);
        let mut link: Option<String> = None;
        let mut alt = String::new();
        let mut skip: Option<(String, usize)> = None;
        while let Some(x) = p.next() {
            if let Some((name, depth)) = skip.as_mut() {
                match &x {
                    X::Open(n, _) if n == name => *depth += 1,
                    X::Close(n) if n == name => {
                        *depth -= 1;
                        if *depth == 0 {
                            skip = None;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            match x {
                X::Open(n, _) if WORD_SKIP.contains(&n.as_str()) => skip = Some((n, 1)),
                X::Open(n, _) if n == "p" => paras.push(WPara::default()),
                X::Open(n, _) if n == "pPr" => in_ppr = true,
                X::Close(n) if n == "pPr" => in_ppr = false,
                X::Open(n, a) if n == "pStyle" => {
                    if let Some(p) = paras.last_mut() {
                        p.style = get(&a, "val").map(str::to_string);
                    }
                }
                X::Open(n, a) if n == "numId" && in_ppr => {
                    if let Some(p) = paras.last_mut() {
                        let id = get(&a, "val").unwrap_or("").to_string();
                        let lvl = p.num.as_ref().map_or(0, |n| n.1);
                        p.num = (id != "0").then_some((id, lvl));
                    }
                }
                X::Open(n, a) if n == "ilvl" && in_ppr => {
                    if let Some(p) = paras.last_mut() {
                        let lvl = get(&a, "val").and_then(|v| v.parse().ok()).unwrap_or(0usize).min(8);
                        p.num = Some((p.num.take().map_or(String::new(), |n| n.0), lvl));
                    }
                }
                X::Open(n, a) if n == "pageBreakBefore" && in_ppr => {
                    if let Some(p) = paras.last_mut() {
                        p.page_before = !matches!(get(&a, "val"), Some("0" | "false"));
                    }
                }
                X::Open(n, a) if n == "bookmarkStart" => {
                    if let (Some(p), Some(name)) = (paras.last_mut(), get(&a, "name")) {
                        if !name.starts_with("_GoBack") {
                            p.anchors.push(name.to_lowercase());
                        }
                    }
                }
                X::Open(n, a) if n == "hyperlink" => {
                    link = get(&a, "id").and_then(|id| links.get(id)).map(|(t, _)| t.clone()).or_else(|| get(&a, "anchor").map(|a| format!("#{}", a.to_lowercase())));
                }
                X::Close(n) if n == "hyperlink" => link = None,
                X::Open(n, _) if n == "r" => (bold, italic, code) = (false, false, false),
                X::Open(n, _) if n == "rPr" => in_rpr = !in_ppr,
                X::Close(n) if n == "rPr" => in_rpr = false,
                X::Open(n, a) if in_rpr && (n == "b" || n == "i") => {
                    let on = !matches!(get(&a, "val"), Some("0" | "false"));
                    if n == "b" {
                        bold = on;
                    } else {
                        italic = on;
                    }
                }
                X::Open(n, a) if in_rpr && n == "rFonts" => {
                    code = get(&a, "ascii").or_else(|| get(&a, "hAnsi")).is_some_and(|f| MONO.contains(&f.to_ascii_lowercase().as_str()));
                }
                X::Open(n, _) if n == "t" => in_t = true,
                X::Close(n) if n == "t" => in_t = false,
                X::Text(t) if in_t => {
                    if let Some(p) = paras.last_mut() {
                        p.runs.push(Run::Text { text: t, bold, italic, code, link: link.clone() });
                    }
                }
                X::Open(n, _) if n == "tab" && !in_ppr => {
                    if let Some(p) = paras.last_mut() {
                        p.runs.push(Run::Text { text: " ".into(), bold, italic, code, link: link.clone() });
                    }
                }
                X::Open(n, a) if n == "br" || n == "cr" => {
                    if let Some(p) = paras.last_mut() {
                        if get(&a, "type") == Some("page") {
                            p.page_after = true;
                        } else {
                            p.runs.push(Run::Break);
                        }
                    }
                }
                X::Open(n, a) if n == "docPr" => alt = get(&a, "descr").filter(|d| !d.is_empty()).or_else(|| get(&a, "name")).unwrap_or("").to_string(),
                X::Open(n, a) if n == "blip" => {
                    if let (Some(p), Some((target, false))) = (paras.last_mut(), get(&a, "embed").and_then(|id| links.get(id))) {
                        let b = Block { kind: Kind::Image, src: target.clone(), plain: alt.clone(), ..Default::default() };
                        if tables.is_empty() {
                            p.pictures.push(b);
                        } else {
                            p.runs.push(Run::Text { text: format!("[{}]", if alt.is_empty() { "picture" } else { &alt }), bold: false, italic: true, code: false, link: None });
                        }
                    }
                }
                X::Open(n, _) if n == "tbl" => match tables.last_mut() {
                    Some(t) if t.cell.is_some() => t.inner += 1,
                    _ => tables.push(Table::default()),
                },
                X::Open(n, _) if n == "tr" => {
                    if let Some(t) = tables.last_mut().filter(|t| t.inner == 0) {
                        t.end_row();
                        t.row = Some((Vec::new(), false));
                    }
                }
                X::Open(n, _) if n == "tblHeader" => {
                    if let Some((_, head)) = tables.last_mut().and_then(|t| t.row.as_mut()) {
                        *head = true;
                    }
                }
                X::Open(n, _) if n == "tc" => {
                    if let Some(t) = tables.last_mut().filter(|t| t.inner == 0) {
                        t.end_cell();
                        t.cell = Some(Inline::default());
                    }
                }
                X::Close(n) if n == "tc" => {
                    if let Some(t) = tables.last_mut().filter(|t| t.inner == 0) {
                        t.end_cell();
                    }
                }
                X::Close(n) if n == "tr" => {
                    if let Some(t) = tables.last_mut().filter(|t| t.inner == 0) {
                        t.end_row();
                    }
                }
                X::Close(n) if n == "tbl" => {
                    if let Some(t) = tables.last_mut() {
                        if t.inner > 0 {
                            t.inner -= 1;
                            continue;
                        }
                    }
                    if let Some(mut t) = tables.pop() {
                        t.end_row();
                        for b in markdown::table_chunks(t.rows, Vec::new()) {
                            out.push(b);
                        }
                    }
                }
                X::Close(n) if n == "p" => {
                    let Some(para) = paras.pop() else { continue };
                    if let Some(t) = tables.last_mut() {
                        t.add(&para.runs);
                        continue;
                    }
                    if para.page_before {
                        out.push(Block { kind: Kind::Rule, ..Default::default() });
                    }
                    let heading = para.style.as_ref().and_then(|s| {
                        if have_styles {
                            styles.get(s).copied()
                        } else if s == "Title" {
                            Some(1)
                        } else {
                            s.strip_prefix("Heading").and_then(|l| l.parse::<u8>().ok()).filter(|l| (1..=6).contains(l))
                        }
                    });
                    let mut list = None;
                    if heading.is_none() {
                        if let Some((id, lvl)) = &para.num {
                            let levels = numbering.get(id);
                            let (fmt, start) = levels.and_then(|l| l.get(*lvl)).cloned().unwrap_or(("bullet".into(), 1));
                            let n = counters.next(id, *lvl, start);
                            list = Some((marker(&fmt, n, *lvl), *lvl as u8 + 1));
                        }
                    }
                    if let Some(mut b) = para_block(&para.runs, heading) {
                        if let Some((m, indent)) = list {
                            (b.marker, b.indent) = (m, indent);
                        }
                        b.anchor = para.anchors.join(" ");
                        out.push(b);
                    }
                    out.extend(para.pictures);
                    if para.page_after {
                        out.push(Block { kind: Kind::Rule, ..Default::default() });
                    }
                }
                _ => {}
            }
        }
        // A part cut short: the tables and paragraphs open at the cut.
        while let Some(mut t) = tables.pop() {
            t.end_row();
            out.extend(markdown::table_chunks(t.rows, Vec::new()));
        }
    })
    .ok_or("no word/document.xml")?;
    let summary = format!(
        "docx, {} blocks ({} headings, {} tables, {} images)",
        out.len(),
        count(&out, Kind::Heading),
        count(&out, Kind::Table),
        count(&out, Kind::Image)
    );
    Ok(Parsed { blocks: out, summary, cut })
}

// ---- Excel ------------------------------------------------------------------------------------

/// A sheet's column letters (`AB`) and row number from a cell reference (`AB12`), both from 1.
fn cell_ref(r: &str) -> Option<(u32, u32)> {
    let split = r.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = r.split_at(split);
    let col = letters.bytes().try_fold(0u32, |acc, b| b.is_ascii_alphabetic().then(|| acc * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)))?;
    Some((col, digits.parse().ok()?))
}

/// What a number format makes of a number.
#[derive(Debug, Clone, Copy, PartialEq)]
enum NumKind {
    Number,
    Date { time: bool },
    Time,
}

/// A number format's kind by its id and code: built-in ids 14 to 22 and 45 to 47, or a code with `d`
/// or `y` (a date) or `h` or `s` alone (a time) outside quotes, brackets and escapes.
fn num_kind(id: u32, code: Option<&str>) -> NumKind {
    match id {
        14..=17 => return NumKind::Date { time: false },
        22 => return NumKind::Date { time: true },
        18..=21 | 45..=47 => return NumKind::Time,
        _ => {}
    }
    let Some(code) = code else { return NumKind::Number };
    let mut plain = String::new();
    let (mut quote, mut bracket, mut esc) = (false, false, false);
    for c in code.chars() {
        match c {
            _ if esc => esc = false,
            '\\' => esc = true,
            '"' => quote = !quote,
            '[' if !quote => bracket = true,
            ']' if !quote => bracket = false,
            c if !quote && !bracket => plain.push(c.to_ascii_lowercase()),
            _ => {}
        }
    }
    let date = plain.contains('d') || plain.contains('y');
    if date {
        NumKind::Date { time: plain.contains('h') }
    } else if plain.contains('h') || plain.contains('s') && plain.contains(':') {
        NumKind::Time
    } else {
        NumKind::Number
    }
}

/// Days since 1970-01-01 as a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A serial number as `yyyy-mm-dd` (with ` hh:mm` when `time` and the value has a fraction), in the
/// 1900 system (with its 29 February 1900) or the 1904 one.
fn excel_date(v: f64, time: bool, date1904: bool) -> String {
    let day = v.floor() as i64;
    let date = if date1904 {
        let (y, m, d) = civil(day - 24_107);
        format!("{y:04}-{m:02}-{d:02}")
    } else if day == 60 {
        "1900-02-29".to_string()
    } else {
        let (y, m, d) = civil(day - if day < 60 { 25_568 } else { 25_569 });
        format!("{y:04}-{m:02}-{d:02}")
    };
    let frac = v - v.floor();
    if time && frac > 0.0 {
        let secs = (frac * 86_400.0).round() as i64;
        format!("{date} {:02}:{:02}", secs / 3600, secs / 60 % 60)
    } else {
        date
    }
}

fn excel_time(v: f64) -> String {
    let secs = ((v - v.floor()) * 86_400.0).round() as i64;
    format!("{:02}:{:02}:{:02}", secs / 3600 % 24, secs / 60 % 60, secs % 60)
}

/// A number as stored, in its shortest form (`3.1400000000000001` reads `3.14`).
fn excel_number(s: &str) -> String {
    match s.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => format!("{v}"),
        _ => s.to_string(),
    }
}

/// One cell as read: its place, its type (`t`), its style (`s`) and its value as written.
#[derive(Debug, Default, Clone)]
struct RawCell {
    row: u32,
    col: u32,
    t: String,
    s: usize,
    v: String,
}

fn xlsx<R: Read + Seek>(za: &mut ZipArchive<R>, want: usize, go_on: &dyn Fn() -> bool) -> Result<Parsed, String> {
    // The sheets: name, relationship id, hidden.
    let mut sheets: Vec<(String, String, bool)> = Vec::new();
    let mut date1904 = false;
    with_part(za, "xl/workbook.xml", |p| {
        while let Some(x) = p.next() {
            match x {
                X::Open(n, a) if n == "sheet" => {
                    let hidden = get(&a, "state").is_some_and(|s| s != "visible");
                    sheets.push((get(&a, "name").unwrap_or("?").to_string(), get(&a, "id").unwrap_or("").to_string(), hidden));
                }
                X::Open(n, a) if n == "workbookPr" => date1904 = matches!(get(&a, "date1904"), Some("1" | "true")),
                _ => {}
            }
        }
    })
    .ok_or("no xl/workbook.xml")?;
    if sheets.is_empty() {
        return Err("no sheets in xl/workbook.xml".into());
    }
    let links = rels(za, "xl/workbook.xml");
    let shown = if want < sheets.len() { want } else { 0 };
    let (name, rid, _) = sheets[shown].clone();
    let target = links.get(&rid).map(|(t, _)| t.clone()).ok_or_else(|| format!("sheet \"{name}\" has no part"))?;
    let mut out = vec![Block { kind: Kind::Heading, level: 1, md: format!("**{}**", esc(&name)), plain: name.clone(), anchor: (1..=sheets.len()).map(|k| format!("sheet={k}")).collect::<Vec<_>>().join(" "), ..Default::default() }];
    if sheets.len() > 1 {
        let mut i = Inline::default();
        for (k, (n, _, hidden)) in sheets.iter().enumerate() {
            if k > 0 {
                i.text(" | ");
            }
            let label = if *hidden { format!("{n} (hidden)") } else { n.clone() };
            if k == shown {
                i.delim("**", true);
                i.text(&label);
                i.delim("**", false);
            } else {
                i.link_open();
                i.text(&label);
                i.link_close(&format!("#sheet={}", k + 1));
            }
        }
        out.push(Block { kind: Kind::Para, md: i.md, plain: i.plain, ..Default::default() });
    }
    let sheet_count = sheets.len();
    if target.contains("chartsheets/") {
        out.push(Block { kind: Kind::Note, plain: "Chart sheets are not previewed.".into(), ..Default::default() });
        return Ok(Parsed { blocks: out, summary: format!("xlsx, {sheet_count} sheets, sheet \"{name}\" chart"), cut: false });
    }
    // The number formats: each cell style's format id, and the custom codes.
    let mut codes: HashMap<u32, String> = HashMap::new();
    let mut xfs: Vec<u32> = Vec::new();
    with_part(za, "xl/styles.xml", |p| {
        let mut in_xfs = false;
        while let Some(x) = p.next() {
            match x {
                X::Open(n, a) if n == "numFmt" => {
                    if let (Some(id), Some(code)) = (get(&a, "numFmtId").and_then(|v| v.parse().ok()), get(&a, "formatCode")) {
                        codes.insert(id, code.to_string());
                    }
                }
                X::Open(n, _) if n == "cellXfs" => in_xfs = true,
                X::Close(n) if n == "cellXfs" => in_xfs = false,
                X::Open(n, a) if n == "xf" && in_xfs => xfs.push(get(&a, "numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0)),
                _ => {}
            }
        }
    });
    if !go_on() {
        return Err("superseded".into());
    }
    // The cells, up to XLSX_MAX_ROWS rows from the first with a value.
    let mut cells: Vec<RawCell> = Vec::new();
    let mut rows_cut = false;
    let (_, part_cut) = with_part(za, &target, |p| {
        let (mut row, mut col) = (0u32, 0u32);
        let mut first: Option<u32> = None;
        let mut cur: Option<RawCell> = None;
        let (mut in_v, mut in_t) = (false, false);
        while let Some(x) = p.next() {
            match x {
                X::Open(n, a) if n == "row" => {
                    row = get(&a, "r").and_then(|v| v.parse().ok()).unwrap_or(row + 1);
                    col = 0;
                    if first.is_some_and(|f| row >= f + XLSX_MAX_ROWS) {
                        rows_cut = true;
                        break;
                    }
                }
                X::Open(n, a) if n == "c" => {
                    let (c, r) = get(&a, "r").and_then(cell_ref).unwrap_or((col + 1, row));
                    col = c;
                    cur = Some(RawCell { row: r, col: c, t: get(&a, "t").unwrap_or("n").to_string(), s: get(&a, "s").and_then(|v| v.parse().ok()).unwrap_or(0), v: String::new() });
                }
                X::Open(n, _) if n == "v" => in_v = true,
                X::Close(n) if n == "v" => in_v = false,
                X::Open(n, _) if n == "t" => in_t = true,
                X::Close(n) if n == "t" => in_t = false,
                X::Open(n, _) if n == "rPh" => in_t = false,
                X::Text(t) if in_v || in_t => {
                    if let Some(c) = cur.as_mut() {
                        c.v.push_str(&t);
                    }
                }
                X::Close(n) if n == "c" => {
                    if let Some(c) = cur.take().filter(|c| !c.v.is_empty()) {
                        first.get_or_insert(c.row);
                        if first.is_some_and(|f| c.row < f + XLSX_MAX_ROWS) {
                            cells.push(c);
                        } else {
                            rows_cut = true;
                        }
                    }
                }
                _ => {}
            }
        }
    })
    .ok_or_else(|| format!("no {target}"))?;
    if !go_on() {
        return Err("superseded".into());
    }
    // The shared strings the cells use, read up to the highest index.
    let wanted = cells.iter().filter(|c| c.t == "s").filter_map(|c| c.v.trim().parse::<usize>().ok()).max();
    let mut strings: Vec<String> = Vec::new();
    if let Some(top) = wanted {
        with_part(za, "xl/sharedStrings.xml", |p| {
            let (mut cur, mut in_t, mut in_ph) = (String::new(), false, false);
            while let Some(x) = p.next() {
                match x {
                    X::Open(n, _) if n == "si" => cur.clear(),
                    X::Open(n, _) if n == "rPh" => in_ph = true,
                    X::Close(n) if n == "rPh" => in_ph = false,
                    X::Open(n, _) if n == "t" => in_t = !in_ph,
                    X::Close(n) if n == "t" => in_t = false,
                    X::Text(t) if in_t => cur.push_str(&t),
                    X::Close(n) if n == "si" => {
                        strings.push(std::mem::take(&mut cur));
                        if strings.len() > top {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        });
    }
    let value = |c: &RawCell| -> (String, bool) {
        match c.t.as_str() {
            "s" => (c.v.trim().parse::<usize>().ok().and_then(|i| strings.get(i)).cloned().unwrap_or_default(), false),
            "b" => ((if c.v.trim() == "1" { "TRUE" } else { "FALSE" }).to_string(), false),
            "str" | "inlineStr" | "e" => (c.v.clone(), false),
            _ => {
                let id = xfs.get(c.s).copied().unwrap_or(0);
                let v = c.v.trim().parse::<f64>().ok();
                match (num_kind(id, codes.get(&id).map(String::as_str)), v) {
                    (NumKind::Date { time }, Some(v)) => (excel_date(v, time, date1904), true),
                    (NumKind::Time, Some(v)) => (excel_time(v), true),
                    _ => (excel_number(&c.v), v.is_some()),
                }
            }
        }
    };
    let min_col = cells.iter().map(|c| c.col).min().unwrap_or(1);
    let min_row = cells.iter().map(|c| c.row).min().unwrap_or(1);
    let cols_cut = cells.iter().any(|c| c.col >= min_col + XLSX_MAX_COLS);
    let cells: Vec<&RawCell> = cells.iter().filter(|c| c.col < min_col + XLSX_MAX_COLS).collect();
    let width = cells.iter().map(|c| c.col - min_col + 1).max().unwrap_or(0) as usize;
    let height = cells.iter().map(|c| c.row - min_row + 1).max().unwrap_or(0) as usize;
    let mut grid = vec![vec![(String::new(), false); width]; height];
    for c in &cells {
        grid[(c.row - min_row) as usize][(c.col - min_col) as usize] = value(c);
    }
    // A column whose values are all numbers is right-aligned.
    let aligns: Vec<u8> = (0..width)
        .map(|k| {
            let vals: Vec<&(String, bool)> = grid.iter().map(|r| &r[k]).filter(|v| !v.0.is_empty()).collect();
            if !vals.is_empty() && vals.iter().all(|v| v.1) { 2 } else { 0 }
        })
        .collect();
    let rows: Vec<Row> = grid
        .into_iter()
        .map(|r| Row { head: false, cells: r.into_iter().map(|(v, _)| Cell { md: esc(&v), plain: v }).collect() })
        .collect();
    let what = match (rows_cut, cols_cut) {
        (true, true) => Some(("500 rows and 50 columns", format!("Only the first {XLSX_MAX_ROWS} rows and {XLSX_MAX_COLS} columns are shown."))),
        (true, false) => Some(("500 rows", format!("Only the first {XLSX_MAX_ROWS} rows are shown."))),
        (false, true) => Some(("50 columns", format!("Only the first {XLSX_MAX_COLS} columns are shown."))),
        (false, false) => None,
    };
    if let Some((_, note)) = &what {
        out.push(Block { kind: Kind::Note, plain: note.clone(), ..Default::default() });
    }
    out.extend(markdown::table_chunks(rows, aligns));
    let summary = format!(
        "xlsx, {sheet_count} sheets, sheet \"{name}\" {height} x {width}{}",
        what.map_or(String::new(), |(w, _)| format!(" (cut at {w})"))
    );
    Ok(Parsed { blocks: out, summary, cut: part_cut })
}

/// Text as inline Markdown with nothing in it read as markup.
fn esc(s: &str) -> String {
    let mut i = Inline::default();
    i.text(s);
    i.md
}

// ---- PowerPoint -------------------------------------------------------------------------------

fn pptx<R: Read + Seek>(za: &mut ZipArchive<R>, go_on: &dyn Fn() -> bool) -> Result<Parsed, String> {
    let mut ids: Vec<String> = Vec::new();
    with_part(za, "ppt/presentation.xml", |p| {
        while let Some(x) = p.next() {
            if let X::Open(n, a) = x {
                // `id` (a number) and `r:id` (the relationship) share the local name.
                if n == "sldId" {
                    ids.extend(a.iter().find(|(k, v)| k == "id" && !v.chars().all(|c| c.is_ascii_digit())).map(|(_, v)| v.clone()));
                }
            }
        }
    })
    .ok_or("no ppt/presentation.xml")?;
    let links = rels(za, "ppt/presentation.xml");
    let slides: Vec<String> = ids.iter().filter_map(|id| links.get(id).map(|(t, _)| t.clone())).collect();
    let mut out: Vec<Block> = Vec::new();
    let mut cut = false;
    for (k, slide) in slides.iter().enumerate() {
        if !go_on() {
            return Err("superseded".into());
        }
        if k > 0 {
            out.push(Block { kind: Kind::Rule, ..Default::default() });
        }
        out.push(Block { kind: Kind::Heading, level: 1, md: format!("**Slide {}**", k + 1), plain: format!("Slide {}", k + 1), anchor: format!("slide-{}", k + 1), ..Default::default() });
        let srels = rels(za, slide);
        if let Some((blocks, c)) = with_part(za, slide, |p| slide_blocks(p, &srels)) {
            out.extend(blocks);
            cut |= c;
        }
    }
    let summary = format!("pptx, {} slides, {} blocks", slides.len(), out.len());
    Ok(Parsed { blocks: out, summary, cut })
}

/// A PowerPoint paragraph being read.
#[derive(Default)]
struct PPara {
    runs: Vec<Run>,
    lvl: usize,
    bullet: Option<bool>,
    auto: bool,
}

/// One slide's shapes in `p:spTree` order.
fn slide_blocks(p: &mut Part, links: &HashMap<String, (String, bool)>) -> Vec<Block> {
    let mut out = Vec::new();
    let (mut title, mut body, mut in_rpr, mut in_t, mut in_ppr) = (false, false, false, false, false);
    let (mut bold, mut italic, mut code) = (false, false, false);
    let mut link: Option<String> = None;
    let mut para: Option<PPara> = None;
    let mut counters = Counters::default();
    let mut table: Option<Table> = None;
    let mut alt = String::new();
    let mut skip: Option<(String, usize)> = None;
    while let Some(x) = p.next() {
        if let Some((name, depth)) = skip.as_mut() {
            match &x {
                X::Open(n, _) if n == name => *depth += 1,
                X::Close(n) if n == name => {
                    *depth -= 1;
                    if *depth == 0 {
                        skip = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        match x {
            X::Open(n, _) if n == "Fallback" => skip = Some((n, 1)),
            X::Open(n, _) if n == "sp" => (title, body) = (false, false),
            X::Open(n, a) if n == "ph" => {
                let t = get(&a, "type");
                title = matches!(t, Some("title" | "ctrTitle"));
                body = matches!(t, None | Some("body" | "obj"));
            }
            X::Open(n, _) if n == "txBody" => counters = Counters::default(),
            X::Open(n, _) if n == "p" => para = Some(PPara::default()),
            X::Open(n, a) if n == "pPr" => {
                in_ppr = true;
                if let Some(pp) = para.as_mut() {
                    pp.lvl = get(&a, "lvl").and_then(|v| v.parse().ok()).unwrap_or(0usize).min(8);
                }
            }
            X::Close(n) if n == "pPr" => in_ppr = false,
            X::Open(n, _) if in_ppr && n == "buNone" => {
                if let Some(pp) = para.as_mut() {
                    pp.bullet = Some(false);
                }
            }
            X::Open(n, _) if in_ppr && (n == "buChar" || n == "buAutoNum") => {
                if let Some(pp) = para.as_mut() {
                    pp.bullet = Some(true);
                    pp.auto = n == "buAutoNum";
                }
            }
            X::Open(n, a) if n == "rPr" || n == "endParaRPr" => {
                in_rpr = n == "rPr";
                bold = matches!(get(&a, "b"), Some("1" | "true"));
                italic = matches!(get(&a, "i"), Some("1" | "true"));
                code = false;
                link = None;
            }
            X::Close(n) if n == "rPr" => in_rpr = false,
            X::Open(n, a) if in_rpr && n == "latin" => code = get(&a, "typeface").is_some_and(|f| MONO.contains(&f.to_ascii_lowercase().as_str())),
            X::Open(n, a) if in_rpr && n == "hlinkClick" => link = get(&a, "id").and_then(|id| links.get(id)).map(|(t, _)| t.clone()),
            X::Open(n, _) if n == "r" || n == "fld" => (bold, italic, code, link) = (false, false, false, None),
            X::Open(n, _) if n == "t" => in_t = true,
            X::Close(n) if n == "t" => in_t = false,
            X::Text(t) if in_t => {
                if let Some(pp) = para.as_mut() {
                    pp.runs.push(Run::Text { text: t, bold, italic, code, link: link.clone() });
                }
            }
            X::Open(n, _) if n == "br" => {
                if let Some(pp) = para.as_mut() {
                    pp.runs.push(Run::Break);
                }
            }
            X::Close(n) if n == "p" => {
                let Some(pp) = para.take() else { continue };
                if let Some(t) = table.as_mut() {
                    t.add(&pp.runs);
                    continue;
                }
                let heading = title.then_some(2);
                if let Some(mut b) = para_block(&pp.runs, heading) {
                    let bullet = !title && pp.bullet.unwrap_or(body);
                    if bullet {
                        b.indent = pp.lvl as u8 + 1;
                        b.marker = if pp.auto { format!("{}.", counters.next("", pp.lvl, 1)) } else { BULLETS[pp.lvl % 3].to_string() };
                    }
                    out.push(b);
                }
            }
            X::Open(n, a) if n == "cNvPr" => alt = get(&a, "descr").filter(|d| !d.is_empty()).or_else(|| get(&a, "name")).unwrap_or("").to_string(),
            X::Open(n, a) if n == "blip" => {
                if let Some((target, false)) = get(&a, "embed").and_then(|id| links.get(id)) {
                    out.push(Block { kind: Kind::Image, src: target.clone(), plain: alt.clone(), ..Default::default() });
                }
            }
            X::Open(n, _) if n == "tbl" => match table.as_mut() {
                Some(t) => t.inner += 1,
                None => table = Some(Table::default()),
            },
            X::Open(n, _) if n == "tr" => {
                if let Some(t) = table.as_mut().filter(|t| t.inner == 0) {
                    t.end_row();
                    t.row = Some((Vec::new(), false));
                }
            }
            X::Open(n, _) if n == "tc" => {
                if let Some(t) = table.as_mut().filter(|t| t.inner == 0) {
                    t.end_cell();
                    t.cell = Some(Inline::default());
                }
            }
            X::Close(n) if n == "tc" => {
                if let Some(t) = table.as_mut().filter(|t| t.inner == 0) {
                    t.end_cell();
                }
            }
            X::Close(n) if n == "tr" => {
                if let Some(t) = table.as_mut().filter(|t| t.inner == 0) {
                    t.end_row();
                }
            }
            X::Close(n) if n == "tbl" => {
                if let Some(t) = table.as_mut().filter(|t| t.inner > 0) {
                    t.inner -= 1;
                    continue;
                }
                if let Some(mut t) = table.take() {
                    t.end_row();
                    out.extend(markdown::table_chunks(t.rows, Vec::new()));
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    /// A zip of hand-written parts, in memory.
    pub fn zip_of(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in parts {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn open(bytes: Vec<u8>) -> ZipArchive<Cursor<Vec<u8>>> {
        ZipArchive::new(Cursor::new(bytes)).unwrap()
    }

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;

    /// A 1 x 1 red PNG.
    pub fn png1() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0])).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// The hand-made `.docx` of Settled first.
    pub fn docx_fixture() -> Vec<u8> {
        let styles = format!(
            r#"<w:styles {W}><w:style w:styleId="berschrift1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Titel"><w:name w:val="Title"/></w:style><w:style w:styleId="Heading2"><w:name w:val="heading 2"/></w:style></w:styles>"#
        );
        let numbering = format!(
            r#"<w:numbering {W}><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#
        );
        let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="h" Target="https://example.com/a" TargetMode="External"/><Relationship Id="rId2" Type="i" Target="media/image1.png"/></Relationships>"#;
        let p = |props: &str, runs: &str| format!("<w:p><w:pPr>{props}</w:pPr>{runs}</w:p>");
        let r = |props: &str, text: &str| format!("<w:r><w:rPr>{props}</w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>");
        let body = [
            p(r#"<w:pStyle w:val="Titel"/>"#, &r("", "The title")),
            p(r#"<w:pStyle w:val="berschrift1"/>"#, &format!(r#"<w:bookmarkStart w:id="0" w:name="Intro"/>{}<w:bookmarkEnd w:id="0"/>"#, r("<w:b/>", "Intro &amp; more"))),
            p("", &format!("{}{}{}{}", r("<w:b/>", "Bold"), r("", " and "), r("<w:i/>", "italic "), r(r#"<w:rFonts w:ascii="Consolas"/>"#, "code()"))),
            p("", &format!(r#"<w:hyperlink r:id="rId1">{}</w:hyperlink>{}<w:hyperlink w:anchor="Intro">{}</w:hyperlink>"#, r("", "a link"), r("", " and "), r("", "back up"))),
            p(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#, &r("", "bullet one")),
            p(r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr>"#, &r("", "bullet nested")),
            p(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr>"#, &r("", "three")),
            p(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr>"#, &r("", "four")),
            format!(
                "<w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr><w:tr><w:tc>{}{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>",
                p("", &r("", "Name")),
                p("", &r("", "Size")),
                p("", &r("", "alpha")),
                p("", &r("", "second line")),
                p("", &r("", "1"))
            ),
            p("", &format!(r#"{}<w:r><w:br w:type="page"/></w:r>"#, r("", "before the break"))),
            p("", &format!(r#"{}<w:del w:id="1"><w:r><w:delText>deleted</w:delText></w:r></w:del><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>PAGE \* MERGEFORMAT</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}<w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:footnoteReference w:id="1"/></w:r>"#, r("", "kept "), r("", "7"))),
            p(
                "",
                &format!(
                    r#"{}<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><w:txbxContent>{}</w:txbxContent></w:drawing></mc:Choice><mc:Fallback><w:pict><w:txbxContent>{}</w:txbxContent></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#,
                    r("", "outer"),
                    p("", &r("", "in a text box")),
                    p("", &r("", "in a text box"))
                ),
            ),
            p("", r#"<w:r><w:drawing><wp:inline><wp:docPr id="1" name="Picture 1" descr="A red dot"/><a:graphic><a:graphicData><pic:pic xmlns:pic="p"><pic:blipFill><a:blip r:embed="rId2"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#),
        ]
        .concat();
        let doc = format!("<w:document {W}><w:body>{body}<w:sectPr/></w:body></w:document>");
        zip_of(&[
            ("[Content_Types].xml", b"<Types/>"),
            ("word/document.xml", doc.as_bytes()),
            ("word/styles.xml", styles.as_bytes()),
            ("word/numbering.xml", numbering.as_bytes()),
            ("word/_rels/document.xml.rels", rels.as_bytes()),
            ("word/media/image1.png", &png1()),
        ])
    }

    fn reads_as(md: &str, expected: &str) {
        let ours = slint::StyledText::from_markdown(md).unwrap_or_else(|e| panic!("{md:?}: {e}"));
        assert_eq!(ours, slint::StyledText::from_markdown(expected).unwrap(), "{md:?}");
    }

    #[test]
    fn a_word_file() {
        let d = parse(&mut open(docx_fixture()), OfficeKind::Docx, 0, &|| true).unwrap();
        let b = &d.blocks;
        let kinds: Vec<Kind> = b.iter().map(|x| x.kind).collect();
        use Kind::*;
        assert_eq!(kinds, vec![Heading, Heading, Para, Para, Para, Para, Para, Para, Table, Para, Rule, Para, Para, Para, Image], "{b:#?}");
        assert_eq!((b[0].level, b[0].plain.as_str()), (1, "The title"));
        assert_eq!((b[1].level, b[1].plain.as_str(), b[1].anchor.as_str()), (1, "Intro & more", "intro"), "a heading by its localized style id");
        reads_as(&b[1].md, "**Intro & more**");
        reads_as(&b[2].md, "**Bold** and *italic* `code()`");
        reads_as(&b[3].md, "[a link](https://example.com/a) and [back up](#intro)");
        let items: Vec<(&str, &str, u8)> = b[4..8].iter().map(|x| (x.plain.as_str(), x.marker.as_str(), x.indent)).collect();
        assert_eq!(items, vec![("bullet one", "\u{2022}", 1), ("bullet nested", "\u{25e6}", 2), ("three", "3.", 1), ("four", "4.", 1)]);
        let t = &b[8];
        assert!(t.rows[0].head && !t.rows[1].head);
        assert_eq!(t.rows[0].cells[0].md, "**Name**");
        assert_eq!(t.rows[1].cells[0].plain, "alpha\nsecond line");
        assert_eq!(b[9].plain, "before the break");
        assert_eq!(b[11].plain, "kept 7", "deleted text, the field code and the footnote mark dropped");
        assert_eq!(b[12].plain, "in a text box", "the text box once, before the paragraph it sits in");
        assert_eq!(b[13].plain, "outer");
        assert_eq!((b[14].src.as_str(), b[14].plain.as_str()), ("word/media/image1.png", "A red dot"));
        assert!(!d.cut);
        assert_eq!(d.summary, "docx, 15 blocks (2 headings, 1 tables, 1 images)");
        for x in b {
            slint::StyledText::from_markdown(&x.md).unwrap();
        }
    }

    /// The hand-made `.xlsx` of Settled first: two sheets, a chart sheet and a big one.
    pub fn xlsx_fixture() -> Vec<u8> {
        let workbook = r#"<workbook xmlns="m" xmlns:r="r"><workbookPr/><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Second" sheetId="2" r:id="rId2"/><sheet name="Chart" sheetId="3" r:id="rId3"/><sheet name="Big" sheetId="4" state="hidden" r:id="rId4"/></sheets></workbook>"#;
        let rels = r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="/xl/worksheets/sheet2.xml"/><Relationship Id="rId3" Target="chartsheets/sheet1.xml"/><Relationship Id="rId4" Target="worksheets/sheet3.xml"/></Relationships>"#;
        let styles = r#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd\ hh:mm"/></numFmts><cellStyleXfs><xf numFmtId="14"/></cellStyleXfs><cellXfs><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/><xf numFmtId="2"/></cellXfs></styleSheet>"#;
        let strings = r#"<sst><si><t>Name</t></si><si><r><t>Va</t></r><r><t>lue</t></r><rPh><t>ignored</t></rPh></si><si><t>never read</t></si></sst>"#;
        // Row 1 and column A empty: the table starts at B2.
        let sheet1 = r#"<worksheet><sheetData><row r="2"><c r="B2" t="s"><v>0</v></c><c r="C2" t="s"><v>1</v></c></row><row r="3"><c r="B3" t="inlineStr"><is><t>inline</t></is></c><c r="C3"><v>0.30000000000000004</v></c></row><row r="4"><c r="B4" t="b"><v>1</v></c><c r="C4" s="3"><f>C3*2</f><v>42</v></c></row><row r="6"><c r="B6" t="e"><v>#DIV/0!</v></c><c r="C6" s="1"><v>45200</v></c></row><row r="7"><c r="C7" s="2"><v>45200.5</v></c></row></sheetData></worksheet>"#;
        let sheet2 = r#"<worksheet><sheetData><row r="1"><c r="A1" t="str"><f>"x"</f><v>from a formula</v></c></row></sheetData></worksheet>"#;
        let mut big = String::from("<worksheet><sheetData>");
        for r in 1..=600 {
            big.push_str(&format!("<row r=\"{r}\">"));
            for c in 0..60u32 {
                let col = if c < 26 { ((b'A' + c as u8) as char).to_string() } else { format!("A{}", (b'A' + (c - 26) as u8) as char) };
                big.push_str(&format!("<c r=\"{col}{r}\"><v>{}</v></c>", r * 100 + c));
            }
            big.push_str("</row>");
        }
        big.push_str("</sheetData></worksheet>");
        zip_of(&[
            ("[Content_Types].xml", b"<Types/>"),
            ("xl/workbook.xml", workbook.as_bytes()),
            ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
            ("xl/styles.xml", styles.as_bytes()),
            ("xl/sharedStrings.xml", strings.as_bytes()),
            ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
            ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
            ("xl/chartsheets/sheet1.xml", b"<chartsheet/>"),
            ("xl/worksheets/sheet3.xml", big.as_bytes()),
        ])
    }

    #[test]
    fn an_excel_file() {
        let bytes = xlsx_fixture();
        let d = parse(&mut open(bytes.clone()), OfficeKind::Xlsx, 0, &|| true).unwrap();
        let b = &d.blocks;
        assert_eq!(d.summary, "xlsx, 4 sheets, sheet \"Data\" 6 x 2");
        assert_eq!((b[0].kind, b[0].plain.as_str(), b[0].anchor.as_str()), (Kind::Heading, "Data", "sheet=1 sheet=2 sheet=3 sheet=4"));
        reads_as(&b[1].md, "**Data** | [Second](#sheet=2) | [Chart](#sheet=3) | [Big (hidden)](#sheet=4)");
        let t = &b[2];
        assert_eq!(t.kind, Kind::Table);
        let rows: Vec<Vec<&str>> = t.rows.iter().map(|r| r.cells.iter().map(|c| c.plain.as_str()).collect()).collect();
        assert_eq!(
            rows,
            vec![
                vec!["Name", "Value"],
                vec!["inline", "0.30000000000000004"],
                vec!["TRUE", "42"],
                vec!["", ""],
                vec!["#DIV/0!", "2023-10-01"],
                vec!["", "2023-10-01 12:00"],
            ]
        );
        assert_eq!(t.aligns, vec![0, 0], "the second column holds a string");
        let d = parse(&mut open(bytes.clone()), OfficeKind::Xlsx, 1, &|| true).unwrap();
        assert_eq!(d.blocks.last().unwrap().rows[0].cells[0].plain, "from a formula");
        reads_as(&d.blocks[1].md, "[Data](#sheet=1) | **Second** | [Chart](#sheet=3) | [Big (hidden)](#sheet=4)");
        let d = parse(&mut open(bytes.clone()), OfficeKind::Xlsx, 2, &|| true).unwrap();
        assert_eq!(d.blocks.last().unwrap().plain, "Chart sheets are not previewed.");
        assert_eq!(d.summary, "xlsx, 4 sheets, sheet \"Chart\" chart");
        let d = parse(&mut open(bytes.clone()), OfficeKind::Xlsx, 3, &|| true).unwrap();
        assert_eq!(d.summary, "xlsx, 4 sheets, sheet \"Big\" 500 x 50 (cut at 500 rows and 50 columns)");
        assert_eq!(d.blocks[2].plain, "Only the first 500 rows and 50 columns are shown.");
        let table_rows: usize = d.blocks.iter().filter(|b| b.kind == Kind::Table).map(|b| b.rows.len()).sum();
        assert_eq!(table_rows, 500);
        assert!(d.blocks.iter().filter(|b| b.kind == Kind::Table).all(|b| b.aligns.iter().all(|a| *a == 2)), "numbers right-aligned");
        let d = parse(&mut open(bytes), OfficeKind::Xlsx, 9, &|| true).unwrap();
        assert!(d.summary.contains("sheet \"Data\""), "a sheet past the end shows the first");
    }

    #[test]
    fn numbers_and_dates() {
        assert_eq!(excel_number("3.1400000000000001"), "3.14");
        assert_eq!(excel_number("42"), "42");
        assert_eq!(excel_number("1E-3"), "0.001");
        assert_eq!(excel_date(1.0, false, false), "1900-01-01");
        assert_eq!(excel_date(59.0, false, false), "1900-02-28");
        assert_eq!(excel_date(60.0, false, false), "1900-02-29");
        assert_eq!(excel_date(61.0, false, false), "1900-03-01");
        assert_eq!(excel_date(45200.75, true, false), "2023-10-01 18:00");
        assert_eq!(excel_date(0.0, false, true), "1904-01-01");
        assert_eq!(excel_time(0.5), "12:00:00");
        assert_eq!(num_kind(14, None), NumKind::Date { time: false });
        assert_eq!(num_kind(20, None), NumKind::Time);
        assert_eq!(num_kind(164, Some("yyyy\\-mm\\-dd hh:mm")), NumKind::Date { time: true });
        assert_eq!(num_kind(165, Some("[Red]#,##0.00;\"days\"")), NumKind::Number, "quoted and bracketed letters ignored");
        assert_eq!(num_kind(166, Some("General")), NumKind::Number);
        assert_eq!(num_kind(167, Some("hh:mm:ss")), NumKind::Time);
        assert_eq!(cell_ref("AB12"), Some((28, 12)));
        assert_eq!(cell_ref("A1"), Some((1, 1)));
        assert_eq!(resolve("ppt/slides/slide1.xml", "../media/image1.png"), "ppt/media/image1.png");
        assert_eq!(resolve("xl/workbook.xml", "/xl/worksheets/s.xml"), "xl/worksheets/s.xml");
        assert_eq!(marker("lowerRoman", 4, 0), "iv.");
        assert_eq!(marker("upperLetter", 28, 0), "BB.");
    }

    /// The hand-made `.pptx` of Settled first.
    pub fn pptx_fixture() -> Vec<u8> {
        let pres = r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/></p:sldIdLst></p:presentation>"#;
        let rels = r#"<Relationships><Relationship Id="rId2" Target="slides/slide1.xml"/><Relationship Id="rId3" Target="slides/slide2.xml"/></Relationships>"#;
        let sp = |ph: &str, paras: &str| format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="s"/><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:txBody>{paras}</p:txBody></p:sp>"#);
        let ap = |ppr: &str, rpr: &str, text: &str| format!("<a:p>{ppr}<a:r><a:rPr {rpr}/><a:t>{text}</a:t></a:r></a:p>");
        let slide1 = format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><p:cSld><p:spTree>{}{}<p:grpSp>{}</p:grpSp><p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr><a:tc><a:txBody>{}</a:txBody></a:tc><a:tc><a:txBody>{}</a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame><p:pic><p:nvPicPr><p:cNvPr id="9" name="Picture 9" descr="Logo"/></p:nvPicPr><p:blipFill><a:blip r:embed="rId1"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#,
            sp(r#"<p:ph type="title"/>"#, &ap("", "", "Quarterly review")),
            sp(r#"<p:ph idx="1"/>"#, &[ap("", r#"b="1""#, "First point"), ap(r#"<a:pPr lvl="1"/>"#, "", "a detail"), ap("<a:pPr><a:buNone/></a:pPr>", "", "no bullet here")].concat()),
            sp("", &[ap(r#"<a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr>"#, "", "step"), ap(r#"<a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr>"#, "", "next step")].concat()),
            ap("", "", "Cell A"),
            ap("", "", "Cell B")
        );
        let slide2 = format!(r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>{}{}</p:spTree></p:cSld></p:sld>"#, sp(r#"<p:ph type="ctrTitle"/>"#, &ap("", "", "Thanks")), sp("", &ap("", "", "plain text box")));
        let srels = r#"<Relationships><Relationship Id="rId1" Target="../media/image1.png"/></Relationships>"#;
        zip_of(&[
            ("[Content_Types].xml", b"<Types/>"),
            ("ppt/presentation.xml", pres.as_bytes()),
            ("ppt/_rels/presentation.xml.rels", rels.as_bytes()),
            ("ppt/slides/slide1.xml", slide1.as_bytes()),
            ("ppt/slides/_rels/slide1.xml.rels", srels.as_bytes()),
            ("ppt/slides/slide2.xml", slide2.as_bytes()),
            ("ppt/media/image1.png", &png1()),
        ])
    }

    #[test]
    fn a_powerpoint_file() {
        let d = parse(&mut open(pptx_fixture()), OfficeKind::Pptx, 0, &|| true).unwrap();
        let b = &d.blocks;
        let got: Vec<(Kind, u8, &str, &str)> = b.iter().map(|x| (x.kind, x.level, x.marker.as_str(), x.plain.as_str())).collect();
        use Kind::*;
        assert_eq!(
            got,
            vec![
                (Heading, 1, "", "Slide 1"),
                (Heading, 2, "", "Quarterly review"),
                (Para, 0, "\u{2022}", "First point"),
                (Para, 0, "\u{25e6}", "a detail"),
                (Para, 0, "", "no bullet here"),
                (Para, 0, "1.", "step"),
                (Para, 0, "2.", "next step"),
                (Table, 0, "", ""),
                (Image, 0, "", "Logo"),
                (Rule, 0, "", ""),
                (Heading, 1, "", "Slide 2"),
                (Heading, 2, "", "Thanks"),
                (Para, 0, "", "plain text box"),
            ]
        );
        reads_as(&b[2].md, "**First point**");
        assert_eq!(b[3].indent, 2);
        assert_eq!(b[7].rows[0].cells.iter().map(|c| c.plain.as_str()).collect::<Vec<_>>(), vec!["Cell A", "Cell B"]);
        assert_eq!(b[8].src, "ppt/media/image1.png");
        assert_eq!(d.summary, "pptx, 2 slides, 13 blocks");
        // Stopped between slides when a newer file is asked for.
        assert!(parse(&mut open(pptx_fixture()), OfficeKind::Pptx, 0, &|| false).is_err());
    }

    #[test]
    fn broken_files() {
        let no_doc = zip_of(&[("[Content_Types].xml", b"<Types/>")]);
        assert_eq!(parse(&mut open(no_doc), OfficeKind::Docx, 0, &|| true).unwrap_err(), "no word/document.xml");
        let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        ole.extend(vec![0u8; 600]);
        assert!(!password_protected(&ole));
        ole.extend("EncryptionInfo".encode_utf16().flat_map(u16::to_le_bytes));
        assert!(password_protected(&ole));
        assert!(!password_protected(b"PK\x03\x04EncryptionInfo"));
        // A document cut short in the middle of an element keeps what it has.
        let cut = format!("<w:document {W}><w:body><w:p><w:r><w:t>whole</w:t></w:r></w:p><w:p><w:r><w:t>half");
        let d = parse(&mut open(zip_of(&[("word/document.xml", cut.as_bytes())])), OfficeKind::Docx, 0, &|| true).unwrap();
        assert_eq!(d.blocks.iter().map(|b| b.plain.as_str()).collect::<Vec<_>>(), vec!["whole"]);
        assert_eq!(OfficeKind::of("DOCM"), Some(OfficeKind::Docx));
        assert_eq!(OfficeKind::of("xls"), None);
    }
}
