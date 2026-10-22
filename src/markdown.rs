//! Markdown for the preview pane (P8-M2): the file's text cut to its first 2 MB, parsed by
//! pulldown-cmark into a flat list of blocks (headings, paragraphs, list items, code, quotes,
//! tables, rules, pictures), each paragraph's or heading's inline content written back as the
//! inline Markdown Slint's `StyledText::from_markdown` takes (which refuses a whole file with any
//! heading, code block, table or quote). Pure functions only: the preview worker reads the file and
//! builds the `StyledText`s, the UI thread shows the blocks (preview.rs), and the link clicked in
//! one is classified here.

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Only this much of a file is read and shown, with a note (Jeremy's cap, 2026-10-20).
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
/// A code block is shown as rows of at most this many lines and a table as rows of at most this
/// many table rows, so the ListView builds only what is on screen even for a long one.
pub const CODE_CHUNK: usize = 40;
pub const TABLE_CHUNK: usize = 40;
/// Plain text's code blocks also end before they pass this many bytes (P9-M2).
pub const TEXT_CHUNK_BYTES: usize = 8192;

/// Slint's placeholder for interpolation in `from_markdown`, which fails on it (removed).
const PLACEHOLDER: char = '\u{E541}';
/// Written between a closing and an opening emphasis run (`*a*` then `**b**`), which would
/// otherwise read as one run of three.
const ZWSP: char = '\u{200B}';

/// Programs and scripts: a link to one shows it in the pane, never runs it (Jeremy, 2026-10-20).
const PROGRAM_EXTS: &[&str] = &[
    "exe", "bat", "cmd", "ps1", "lnk", "vbs", "msi", "com", "scr", "pif", "js", "jse", "wsf", "wsh", "hta", "cpl", "msc",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Para,
    Heading,
    Code,
    Rule,
    Table,
    Image,
    /// The 2 MB note.
    Note,
}

/// One table cell: its inline Markdown, and the same without markup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub md: String,
    pub plain: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub head: bool,
    pub cells: Vec<Cell>,
}

/// One block of the flat list the column shows, one ListView row each.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub kind: Kind,
    /// A heading's level, 1 to 6.
    pub level: u8,
    /// List nesting depth, and quote nesting depth.
    pub indent: u8,
    pub quote: u8,
    /// A list item's bullet or number ("•", "3."), on its first block only.
    pub marker: String,
    /// A task list item: 1 open, 2 done (the box replaces the bullet).
    pub task: u8,
    /// Paragraphs and headings: the inline Markdown for `StyledText::from_markdown`.
    pub md: String,
    /// The text without markup: a paragraph's or heading's fallback (and a heading's slug), a code
    /// block's lines, a picture's alt text, the note.
    pub plain: String,
    /// A table's rows (this chunk's), its column alignments (0 left, 1 centre, 2 right) and the
    /// columns' shares of the width (the same in every chunk of one table).
    pub rows: Vec<Row>,
    pub aligns: Vec<u8>,
    pub weights: Vec<f32>,
    /// A picture's source as written.
    pub src: String,
    /// A code block's or table's first and last chunk (a chunk between draws no ends).
    pub first: bool,
    pub last: bool,
    /// HTML (P9-M2): the ids of the elements that start this block, lower case, space separated,
    /// for `#id` links (a heading's slug is added by the worker).
    pub anchor: String,
}

/// The text to parse from the file's first bytes (`MAX_BYTES` + 1 read, so `cut` can tell): a
/// UTF-16 file by its byte order mark, else UTF-8 (invalid bytes replaced), without a BOM. Over
/// the cap the text ends at the last line break before `MAX_BYTES`, else at the last character
/// boundary.
pub fn decode(bytes: &[u8]) -> (String, bool) {
    let cut = bytes.len() > MAX_BYTES;
    let head = &bytes[..bytes.len().min(MAX_BYTES)];
    let utf16 = match head {
        [0xFF, 0xFE, ..] => Some(false),
        [0xFE, 0xFF, ..] => Some(true),
        _ => None,
    };
    if let Some(be) = utf16 {
        let units: Vec<u16> = head[2..].chunks_exact(2).map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) }).collect();
        let mut s = String::from_utf16_lossy(&units);
        if cut {
            if let Some(i) = s.rfind('\n') {
                s.truncate(i + 1);
            }
        }
        return (s, cut);
    }
    let bom = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let raw_end = head.len();
    let end = if !cut {
        raw_end
    } else if let Some(i) = bytes[bom..raw_end].iter().rposition(|&b| b == b'\n') {
        bom + i + 1
    } else {
        // `bytes[raw_end]` is the first byte left out: back off to the start of its character.
        let mut e = raw_end;
        while e > bom && bytes[e] & 0xC0 == 0x80 {
            e -= 1;
        }
        e
    };
    (String::from_utf8_lossy(&bytes[bom..end.max(bom)]).into_owned(), cut)
}

/// GitHub's heading anchors: lower case, letters, digits, `-` and `_` kept, spaces made `-`,
/// everything else dropped; a repeated slug gets `-1`, `-2` (github-slugger).
#[derive(Default)]
pub struct Slugger {
    seen: HashMap<String, usize>,
}

impl Slugger {
    pub fn slug(&mut self, text: &str) -> String {
        let base: String = text
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                ' ' => Some('-'),
                c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                _ => None,
            })
            .collect();
        let mut s = base.clone();
        while self.seen.contains_key(&s) {
            let n = self.seen.entry(base.clone()).or_insert(0);
            *n += 1;
            s = format!("{base}-{n}");
        }
        self.seen.insert(s.clone(), 0);
        s
    }
}

/// What a link clicked in the preview does.
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// `http`, `https` or `mailto`: the default browser or mail program.
    Url(String),
    /// `#slug`: the heading with that slug (percent-decoded, lower case).
    Anchor(String),
    /// A path, resolved against the Markdown file's folder.
    Local(PathBuf),
    Refused(String),
}

/// What a local link points at, once the file system has been asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Local {
    /// Navigate the active pane there.
    Folder,
    /// Open as a double-click does.
    Document,
    /// Show it in the pane (its folder, the cursor on it); never run.
    Program,
}

/// `s`'s URL scheme (`https` in `https://x`), if it has one: two or more characters (so `C:\` is a
/// drive), a letter first.
fn scheme(s: &str) -> Option<&str> {
    let (head, _) = s.split_once(':')?;
    let mut c = head.chars();
    (head.len() >= 2 && c.next().is_some_and(|c| c.is_ascii_alphabetic()) && c.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))).then_some(head)
}

/// `%XX` escapes decoded (invalid UTF-8 replaced).
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A relative or absolute path written in a link or a picture's source, resolved against `base`
/// (the Markdown file's folder): `?` and `#` and what follows dropped, `%XX` decoded, `/` made `\`,
/// `.` and `..` folded.
fn local_path(dest: &str, base: &Path) -> PathBuf {
    let end = dest.find(['?', '#']).unwrap_or(dest.len());
    let rel = percent_decode(&dest[..end]).replace('/', "\\");
    let mut out = PathBuf::new();
    for c in base.join(rel).components() {
        match c {
            Component::ParentDir => {
                // Never above the root.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

pub fn classify_link(dest: &str, base: &Path) -> Link {
    let d = dest.trim();
    if d.is_empty() {
        return Link::Refused("an empty link".into());
    }
    if let Some(frag) = d.strip_prefix('#') {
        return Link::Anchor(percent_decode(frag).to_lowercase());
    }
    if let Some(s) = scheme(d) {
        return match s.to_ascii_lowercase().as_str() {
            "http" | "https" | "mailto" => Link::Url(d.to_string()),
            other => Link::Refused(format!("{other}: links are not opened")),
        };
    }
    if d.starts_with("//") {
        return Link::Refused("a link without a scheme".into());
    }
    Link::Local(local_path(d, base))
}

/// A local link's target by what it is: a folder, a program or script (by its extension), or any
/// other file.
pub fn local_kind(path: &Path, is_dir: bool) -> Local {
    let program = path.extension().and_then(|e| e.to_str()).is_some_and(|e| PROGRAM_EXTS.iter().any(|p| p.eq_ignore_ascii_case(e)));
    if is_dir {
        Local::Folder
    } else if program {
        Local::Program
    } else {
        Local::Document
    }
}

/// A picture's file, or None for a remote one (a scheme, `data:` included, or `//host`).
pub fn image_path(src: &str, base: &Path) -> Option<PathBuf> {
    let s = src.trim();
    (!s.is_empty() && scheme(s).is_none() && !s.starts_with("//")).then(|| local_path(s, base))
}

/// One block's inline content, written as the inline Markdown `StyledText::from_markdown` takes,
/// and as plain text beside it.
#[derive(Default)]
pub(crate) struct Inline {
    pub(crate) md: String,
    pub(crate) plain: String,
    /// The last thing written was a closing emphasis run.
    closed: bool,
}

impl Inline {
    /// Text: every ASCII punctuation character escaped (so nothing in it reads as markup), line
    /// breaks made spaces, Slint's placeholder removed.
    pub(crate) fn text(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                PLACEHOLDER => {}
                '\n' | '\r' => {
                    self.md.push(' ');
                    self.plain.push(' ');
                }
                c => {
                    if c.is_ascii_punctuation() {
                        self.md.push('\\');
                    }
                    self.md.push(c);
                    self.plain.push(c);
                }
            }
        }
        self.closed = false;
    }

    /// An emphasis, strong or strikethrough run, opening or closing.
    pub(crate) fn delim(&mut self, run: &str, open: bool) {
        if open && self.closed {
            self.md.push(ZWSP);
        }
        self.md.push_str(run);
        self.closed = !open;
    }

    /// A code span, fenced one backtick longer than any run inside, padded with a space either
    /// side when it starts or ends with a backtick or with a space at both ends (which a code span
    /// strips again).
    pub(crate) fn code(&mut self, s: &str) {
        let s: String = s.chars().filter(|&c| c != PLACEHOLDER).map(|c| if c == '\n' { ' ' } else { c }).collect();
        if s.is_empty() {
            return;
        }
        let mut longest = 0;
        let mut run = 0;
        for c in s.chars() {
            run = if c == '`' { run + 1 } else { 0 };
            longest = longest.max(run);
        }
        let fence = "`".repeat(longest + 1);
        let pad = s.starts_with('`') || s.ends_with('`') || (s.starts_with(' ') && s.ends_with(' ') && s.trim() != "");
        let sp = if pad { " " } else { "" };
        self.md.push_str(&format!("{fence}{sp}{s}{sp}{fence}"));
        self.plain.push_str(&s);
        self.closed = false;
    }

    pub(crate) fn link_open(&mut self) {
        self.md.push('[');
        self.closed = false;
    }

    /// `](<dest>)`: in angle brackets so spaces and parentheses need nothing; `<`, `>` and `\`
    /// escaped.
    pub(crate) fn link_close(&mut self, dest: &str) {
        self.md.push_str("](<");
        for c in dest.chars() {
            match c {
                PLACEHOLDER | '\n' | '\r' => {}
                '<' | '>' | '\\' => {
                    self.md.push('\\');
                    self.md.push(c);
                }
                c => self.md.push(c),
            }
        }
        self.md.push_str(">)");
        self.closed = false;
    }

    pub(crate) fn hard_break(&mut self) {
        self.md.push_str("\\\n");
        self.plain.push('\n');
        self.closed = false;
    }
}

/// The inline Markdown of `source` re-serialized (what a paragraph of it becomes), for the tests.
#[cfg(test)]
pub fn inline_md(source: &str) -> String {
    let blocks = parse(source);
    blocks.first().map(|b| b.md.clone()).unwrap_or_default()
}

struct Table {
    aligns: Vec<u8>,
    rows: Vec<Row>,
    row: Vec<Cell>,
    head: bool,
}

/// One piece of an HTML block: its text, a line break, or a picture.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Break,
    Img { src: String, alt: String },
}

/// `name="value"` (or 'value', or unquoted) in a tag's text.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        if !lower[..at].ends_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        return Some(match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or("").to_string(),
            _ => rest.split(|c: char| c.is_ascii_whitespace() || c == '>').next().unwrap_or("").trim_end_matches('/').to_string(),
        });
    }
    None
}

/// The text a named entity (`mdash`, without `&` and `;`) stands for, from the full HTML5 table
/// (P11-M4: 2,125 names, a few of them two characters). CommonMark decodes every HTML5 entity, so
/// the table pulldown-cmark already carries for Markdown is asked: a parse of `&name;` comes back
/// changed exactly when the name is one. Nothing is added to the program.
fn entity(name: &str) -> Option<String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let src = format!("&{name};");
    let text: String = Parser::new(&src).filter_map(|e| if let Event::Text(t) = e { Some(t.into_string()) } else { None }).collect();
    (text != src && !text.is_empty()).then_some(text)
}

/// Entities decoded: the named ones (`entity`) and numeric ones; any other is left as written.
pub(crate) fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // The longest name is 31 characters (`CounterClockwiseContourIntegral`).
        let semi = rest[1..].find(['&', ';']).map(|j| j + 1).filter(|&j| j <= 33 && rest.as_bytes()[j] == b';');
        let decoded = semi.and_then(|j| {
            let name = &rest[1..j];
            let c = entity(name).or_else(|| {
                name.strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32)
                    .map(String::from)
            });
            c.map(|c| (c, j + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push_str(&c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An HTML block's text with its tags stripped (Jeremy, 2026-10-20: no centring), in order with
/// its pictures; block-level tags and `<br>` break the text. Comments are dropped.
fn html_pieces(html: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Piece>| {
        let t = entities(&text.split_whitespace().collect::<Vec<_>>().join(" "));
        if !t.trim().is_empty() {
            out.push(Piece::Text(t));
        }
        text.clear();
    };
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        text.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |j| &after[j + 3..]);
            continue;
        }
        let Some(j) = rest.find('>') else {
            text.push_str(rest);
            rest = "";
            break;
        };
        let tag = &rest[1..j];
        rest = &rest[j + 1..];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_ascii_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        match name.as_str() {
            "img" => {
                flush(&mut text, &mut out);
                out.push(Piece::Img { src: entities(&attr(tag, "src").unwrap_or_default()), alt: entities(&attr(tag, "alt").unwrap_or_default()) });
            }
            "br" | "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr" | "ul" | "ol" | "table" | "details" | "summary" | "hr" => {
                flush(&mut text, &mut out);
                if out.last().is_some_and(|p| *p != Piece::Break) {
                    out.push(Piece::Break);
                }
            }
            _ => {}
        }
    }
    text.push_str(rest);
    flush(&mut text, &mut out);
    out.retain(|p| *p != Piece::Break);
    out
}

/// A table as blocks of at most `TABLE_CHUNK` rows, every row as wide as the widest, the columns'
/// shares from their longest cell (3 to 40 characters).
pub(crate) fn table_chunks(mut rows: Vec<Row>, mut aligns: Vec<u8>) -> Vec<Block> {
    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(aligns.len());
    if cols == 0 {
        return Vec::new();
    }
    for r in &mut rows {
        r.cells.resize(cols, Cell::default());
    }
    let weights: Vec<f32> = (0..cols).map(|c| rows.iter().map(|r| r.cells[c].plain.chars().count()).max().unwrap_or(0).clamp(3, 40) as f32).collect();
    aligns.resize(cols, 0);
    let n = rows.len().div_ceil(TABLE_CHUNK);
    let chunks = rows.chunks(TABLE_CHUNK).enumerate();
    chunks.map(|(i, chunk)| Block { kind: Kind::Table, rows: chunk.to_vec(), aligns: aligns.clone(), weights: weights.clone(), first: i == 0, last: i + 1 == n, ..Default::default() }).collect()
}

/// Plain text (P9-M2), and an HTML `pre`: tabs as four spaces, `\r\n` and `\r` as line ends, shown
/// as code blocks of at most `CODE_CHUNK` lines and `TEXT_CHUNK_BYTES` bytes, a longer line split at
/// a character boundary into pieces of that size (so one 2 MB line is not one `Text` shaped whole).
/// Returns the blocks and the number of lines.
pub fn text_blocks(text: &str) -> (Vec<Block>, usize) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ");
    let body = text.strip_suffix('\n').unwrap_or(&text);
    if body.is_empty() {
        return (Vec::new(), 0);
    }
    let lines: Vec<&str> = body.split('\n').collect();
    let mut chunks: Vec<String> = Vec::new();
    let (mut cur, mut n) = (String::new(), 0);
    for line in &lines {
        let mut rest: &str = line;
        loop {
            let mut end = rest.len().min(TEXT_CHUNK_BYTES);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            let (piece, more) = rest.split_at(end);
            if n > 0 && (n == CODE_CHUNK || cur.len() + 1 + piece.len() > TEXT_CHUNK_BYTES) {
                chunks.push(std::mem::take(&mut cur));
                n = 0;
            }
            if n > 0 {
                cur.push('\n');
            }
            cur.push_str(piece);
            n += 1;
            if more.is_empty() {
                break;
            }
            rest = more;
        }
    }
    chunks.push(cur);
    let k = chunks.len();
    let blocks = chunks.into_iter().enumerate().map(|(i, plain)| Block { kind: Kind::Code, plain, first: i == 0, last: i + 1 == k, ..Default::default() });
    (blocks.collect(), lines.len())
}

/// The encoding `decode` reads a file's first bytes in, for the `[preview]` line.
pub fn encoding(bytes: &[u8]) -> &'static str {
    let head = &bytes[..bytes.len().min(MAX_BYTES)];
    // A character cut by the cap at the end is not an invalid byte.
    let valid = |b: &[u8]| std::str::from_utf8(b).map_or_else(|e| e.error_len().is_none(), |_| true);
    match head {
        [0xFF, 0xFE, ..] => "UTF-16LE",
        [0xFE, 0xFF, ..] => "UTF-16BE",
        [0xEF, 0xBB, 0xBF, rest @ ..] if valid(rest) => "UTF-8 BOM",
        [0xEF, 0xBB, 0xBF, ..] => "UTF-8 BOM (invalid bytes replaced)",
        _ if valid(head) => "UTF-8",
        _ => "UTF-8 (invalid bytes replaced)",
    }
}

/// What the parse found besides the blocks: the remote pictures shown as their alt text.
#[derive(Debug, Default)]
pub struct Doc {
    pub blocks: Vec<Block>,
    pub remote: Vec<String>,
}

struct Builder {
    base: Option<PathBuf>,
    out: Vec<Block>,
    inline: Option<Inline>,
    /// A heading or a table's head: the whole is bold, so a strong run inside is not written.
    bold: bool,
    heading: Option<u8>,
    quote: u8,
    /// The open lists, each with its next number (None: bulleted).
    lists: Vec<Option<u64>>,
    /// The current item's bullet or number and task box, for its first block.
    marker: Option<(String, u8)>,
    code: Option<String>,
    html: Option<String>,
    /// Inside a picture: its source and alt text so far.
    image: Option<(String, String)>,
    links: Vec<String>,
    /// Local pictures inside the current block, placed after it.
    later: Vec<Block>,
    table: Option<Table>,
    /// The footnote being defined (its label) and where its blocks start.
    footnote: Option<(String, usize)>,
    notes: Vec<(String, Vec<Block>)>,
    numbers: HashMap<String, usize>,
    /// Inside a metadata block (YAML front matter): skipped.
    skip: usize,
    remote: Vec<String>,
}

impl Builder {
    fn inline(&mut self) -> &mut Inline {
        self.inline.get_or_insert_with(Inline::default)
    }

    /// Place a block at the current list and quote depth, giving it the item's marker if it is the
    /// item's first.
    fn push(&mut self, mut b: Block) {
        b.indent = self.lists.len().min(255) as u8;
        b.quote = self.quote;
        if let Some((m, t)) = self.marker.take() {
            (b.marker, b.task) = (m, t);
        }
        self.out.push(b);
    }

    /// End the current paragraph or heading (if it has anything in it), then place the pictures
    /// that sat in it.
    fn flush(&mut self) {
        if let Some(i) = self.inline.take() {
            if !i.plain.trim().is_empty() || i.md.contains("](<") {
                let b = match self.heading {
                    Some(level) => Block { kind: Kind::Heading, level, md: format!("**{}**", i.md), plain: i.plain, ..Default::default() },
                    None => Block { kind: Kind::Para, md: i.md, plain: i.plain, ..Default::default() },
                };
                self.push(b);
            }
        }
        for b in std::mem::take(&mut self.later) {
            self.push(b);
        }
    }

    /// A picture: a local one becomes its own block after the current one; a remote one is shown
    /// as its alt text where it stands.
    fn picture(&mut self, src: String, alt: String) {
        let local = self.base.as_ref().is_some_and(|b| image_path(&src, b).is_some());
        if local {
            self.later.push(Block { kind: Kind::Image, src, plain: alt, ..Default::default() });
        } else {
            if !alt.is_empty() {
                self.inline().text(&alt);
            }
            self.remote.push(src);
        }
    }

    fn number(&mut self, label: &str) -> usize {
        let n = self.numbers.len() + 1;
        *self.numbers.entry(label.to_string()).or_insert(n)
    }

    fn event(&mut self, ev: Event) {
        if self.skip > 0 {
            match ev {
                Event::Start(Tag::MetadataBlock(_)) => self.skip += 1,
                Event::End(TagEnd::MetadataBlock(_)) => self.skip -= 1,
                _ => {}
            }
            return;
        }
        match ev {
            Event::Start(Tag::MetadataBlock(_)) => self.skip += 1,
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => self.flush(),
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush();
                self.heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                });
                self.bold = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                self.flush();
                self.heading = None;
                self.bold = false;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush();
                self.quote = self.quote.saturating_add(1);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                self.flush();
                self.code = Some(String::new());
            }
            Event::End(TagEnd::CodeBlock) => {
                let text = self.code.take().unwrap_or_default().replace('\t', "    ");
                let lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(&text).split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
                let n = lines.len().div_ceil(CODE_CHUNK);
                for (i, chunk) in lines.chunks(CODE_CHUNK).enumerate() {
                    self.push(Block { kind: Kind::Code, plain: chunk.join("\n"), first: i == 0, last: i + 1 == n, ..Default::default() });
                }
            }
            Event::Start(Tag::HtmlBlock) => {
                self.flush();
                self.html = Some(String::new());
            }
            Event::End(TagEnd::HtmlBlock) => {
                for p in html_pieces(&self.html.take().unwrap_or_default()) {
                    match p {
                        Piece::Text(t) => {
                            self.inline().text(&t);
                            self.flush();
                        }
                        Piece::Img { src, alt } => {
                            self.picture(src, alt);
                            self.flush();
                        }
                        Piece::Break => {}
                    }
                }
            }
            Event::Start(Tag::List(first)) => {
                self.flush();
                self.lists.push(first);
            }
            Event::End(TagEnd::List(_)) => {
                self.flush();
                self.lists.pop();
            }
            Event::Start(Tag::Item) => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let m = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}.", *n - 1)
                    }
                    _ => ["\u{2022}", "\u{25e6}", "\u{25aa}"][depth % 3].to_string(),
                };
                self.marker = Some((m, 0));
            }
            Event::End(TagEnd::Item) => {
                self.flush();
                self.marker = None;
            }
            Event::TaskListMarker(done) => {
                if let Some(m) = self.marker.as_mut() {
                    m.1 = if done { 2 } else { 1 };
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                self.flush();
                self.footnote = Some((label.to_string(), self.out.len()));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                self.flush();
                if let Some((label, start)) = self.footnote.take() {
                    let blocks = self.out.split_off(start.min(self.out.len()));
                    self.notes.push((label, blocks));
                }
            }
            Event::FootnoteReference(label) => {
                let n = self.number(&label);
                self.inline().text(&format!("[{n}]"));
            }
            Event::Start(Tag::Table(aligns)) => {
                self.flush();
                let aligns = aligns.iter().map(|a| match a {
                    Alignment::Center => 1,
                    Alignment::Right => 2,
                    _ => 0,
                });
                self.table = Some(Table { aligns: aligns.collect(), rows: Vec::new(), row: Vec::new(), head: false });
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = self.table.as_mut() {
                    t.head = true;
                }
            }
            Event::Start(Tag::TableCell) => {
                self.bold = self.table.as_ref().is_some_and(|t| t.head);
                self.inline = Some(Inline::default());
            }
            Event::End(TagEnd::TableCell) => {
                let i = self.inline.take().unwrap_or_default();
                let head = self.bold && !i.md.is_empty();
                self.bold = false;
                if let Some(t) = self.table.as_mut() {
                    t.row.push(Cell { md: if head { format!("**{}**", i.md) } else { i.md }, plain: i.plain });
                }
            }
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if let Some(t) = self.table.as_mut() {
                    let cells = std::mem::take(&mut t.row);
                    t.rows.push(Row { head: t.head, cells });
                    t.head = false;
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = self.table.take() {
                    self.table_blocks(t);
                }
                self.flush();
            }
            Event::Start(Tag::Emphasis) => self.inline().delim("*", true),
            Event::End(TagEnd::Emphasis) => self.inline().delim("*", false),
            Event::Start(Tag::Strong) if !self.bold => self.inline().delim("**", true),
            Event::End(TagEnd::Strong) if !self.bold => self.inline().delim("**", false),
            Event::Start(Tag::Strikethrough) => self.inline().delim("~~", true),
            Event::End(TagEnd::Strikethrough) => self.inline().delim("~~", false),
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.links.push(dest_url.to_string());
                self.inline().link_open();
            }
            Event::End(TagEnd::Link) => {
                let dest = self.links.pop().unwrap_or_default();
                self.inline().link_close(&dest);
            }
            Event::Start(Tag::Image { dest_url, .. }) => self.image = Some((dest_url.to_string(), String::new())),
            Event::End(TagEnd::Image) => {
                if let Some((src, alt)) = self.image.take() {
                    self.picture(src, alt);
                }
            }
            Event::Text(t) | Event::Code(t) if self.image.is_some() => {
                if let Some((_, alt)) = self.image.as_mut() {
                    alt.push_str(&t);
                }
            }
            Event::Text(t) => {
                if let Some(c) = self.code.as_mut() {
                    c.push_str(&t);
                } else {
                    self.inline().text(&t);
                }
            }
            Event::Code(t) => self.inline().code(&t),
            Event::Html(t) => {
                if let Some(h) = self.html.as_mut() {
                    h.push_str(&t);
                }
            }
            Event::InlineHtml(t) => {
                let lower = t.to_ascii_lowercase();
                if lower.starts_with("<img") {
                    if let Some(Piece::Img { src, alt }) = html_pieces(&t).into_iter().next() {
                        self.picture(src, alt);
                    }
                } else if lower.starts_with("<br") {
                    self.inline().hard_break();
                }
            }
            Event::SoftBreak => self.inline().text(" "),
            Event::HardBreak => self.inline().hard_break(),
            Event::Rule => {
                self.flush();
                self.push(Block { kind: Kind::Rule, ..Default::default() });
            }
            _ => {}
        }
    }

    fn table_blocks(&mut self, t: Table) {
        for b in table_chunks(t.rows, t.aligns) {
            self.push(b);
        }
    }

    /// The footnotes at the end, after a rule, numbered as they were first referred to (GitHub's
    /// order), each one's first block carrying its number.
    fn finish(mut self) -> Doc {
        self.flush();
        if !self.notes.is_empty() {
            self.lists.clear();
            self.quote = 0;
            self.push(Block { kind: Kind::Rule, ..Default::default() });
            let mut notes = std::mem::take(&mut self.notes);
            for (label, _) in &notes {
                self.number(label);
            }
            notes.sort_by_key(|(l, _)| self.numbers.get(l).copied().unwrap_or(usize::MAX));
            for (label, blocks) in notes {
                let n = self.numbers.get(&label).copied().unwrap_or(0);
                for (i, mut b) in blocks.into_iter().enumerate() {
                    b.indent = b.indent.saturating_add(1);
                    if i == 0 {
                        b.marker = format!("{n}.");
                    }
                    self.out.push(b);
                }
            }
        }
        Doc { blocks: self.out, remote: self.remote }
    }
}

/// Parse a Markdown file's text into its blocks. `base` is the file's folder, for telling local
/// pictures from remote ones (None: every picture is shown as its alt text).
pub fn parse_doc(text: &str, base: Option<&Path>) -> Doc {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut b = Builder {
        base: base.map(Path::to_path_buf),
        out: Vec::new(),
        inline: None,
        bold: false,
        heading: None,
        quote: 0,
        lists: Vec::new(),
        marker: None,
        code: None,
        html: None,
        image: None,
        links: Vec::new(),
        later: Vec::new(),
        table: None,
        footnote: None,
        notes: Vec::new(),
        numbers: HashMap::new(),
        skip: 0,
        remote: Vec::new(),
    };
    for ev in Parser::new_ext(text, opts) {
        b.event(ev);
    }
    b.finish()
}

/// The blocks of `text` with every picture treated as local (for the tests).
#[cfg(test)]
pub fn parse(text: &str) -> Vec<Block> {
    parse_doc(text, Some(Path::new(r"C:\docs"))).blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::StyledText;

    /// The serialized inline Markdown parses in Slint and gives what the source gives (for sources
    /// Slint takes as they are).
    fn same(source: &str) {
        let md = inline_md(source);
        let ours = StyledText::from_markdown(&md).unwrap_or_else(|e| panic!("{source:?} -> {md:?}: {e}"));
        let theirs = StyledText::from_markdown(source).unwrap_or_else(|e| panic!("source {source:?}: {e}"));
        assert_eq!(ours, theirs, "{source:?} -> {md:?}");
    }

    /// The serialized inline Markdown parses in Slint (the source alone may not).
    fn parses(source: &str) -> String {
        let md = inline_md(source);
        StyledText::from_markdown(&md).unwrap_or_else(|e| panic!("{source:?} -> {md:?}: {e}"));
        md
    }

    #[test]
    fn inline_markdown_round_trips_through_styled_text() {
        same("plain words");
        same("a *b* **c** ~~d~~ `e`");
        same(r"literal \* and \_ and \< and \` here");
        same("*a**b*");
        same("**bold *and italic* inside**");
        same("x `` a`b `` y");
        same("[the site](https://example.com/a b) after");
        same("[x](<https://example.com/(paren)>)");
        same("a_b_c and 2*3*4");
        same("[`code` link](https://x.y)");
        assert_eq!(inline_md("a\u{E541}b"), "ab", "Slint's placeholder removed");
        assert_eq!(inline_md("`` ` ``"), "`` ` ``");
        assert_eq!(inline_md("1. not a list").chars().filter(|&c| c == '\\').count(), 0, "a list in the source stays a list");
        // Text that would read as markup at the start of a line, after a hard break.
        let mut i = Inline::default();
        for line in ["one", "# two", "- three", "1. four", "> five", "=====", "    six", "<div>"] {
            i.text(line);
            i.hard_break();
        }
        assert!(i.md.contains("\\\n\\#"), "{:?}", i.md);
        assert_eq!(StyledText::from_markdown(&i.md).map(|_| ()), Ok(()), "{:?}", i.md);
        // A closing run then an opening one.
        let md = parses("*a***b**");
        assert!(md.contains('*'));
        let md = parses("*em*__strong__");
        assert!(md.contains("*\u{200B}**"), "{md:?}");
        parses("<u>under</u> and <b>bold</b> stripped");
        parses("[a link with *emphasis* and `code`](./doc%20one.md#part \"title\")");
        parses("a footnote[^1]\n\n[^1]: the note");
        parses("trailing backslash \\");
        parses("a & b &amp; c &lt;tag&gt;");
        parses("![remote](https://x/y.png) badge");
    }

    #[test]
    fn headings_are_bold_and_slugged() {
        let b = parse("# Title *one*\n\n## **Bold** part\n\n###### six");
        assert_eq!((b[0].kind, b[0].level), (Kind::Heading, 1));
        assert_eq!(b[0].md, "**Title *one***");
        assert_eq!(b[1].md, "**Bold part**", "strong inside a heading is not written twice");
        assert_eq!(b[2].level, 6);
        for x in &b {
            StyledText::from_markdown(&x.md).unwrap();
        }
        let mut s = Slugger::default();
        assert_eq!(s.slug("P8-M2: Markdown"), "p8-m2-markdown");
        assert_eq!(s.slug("What landed"), "what-landed");
        assert_eq!(s.slug("What landed"), "what-landed-1");
        assert_eq!(s.slug("What landed"), "what-landed-2");
        assert_eq!(s.slug("What landed-1"), "what-landed-1-1", "a slug taken by a duplicate");
        assert_eq!(s.slug("Über `code` & more!"), "über-code--more");
        assert_eq!(s.slug("snake_case"), "snake_case");
    }

    #[test]
    fn the_block_list() {
        let src = "---\ntitle: front matter\n---\n\n# Head\n\nPara one\nstill one.\n\n\
                   - a\n- b\n  - nested\n    1. deep\n- [ ] open\n- [x] done\n\n\
                   3. three\n4. four\n\n> quoted\n> > twice\n\n```rust\nfn main() {}\n\tx\n```\n\n\
                   | L | C | R |\n|:--|:-:|--:|\n| 1 | **2** | 3 |\n| four |  | six |\n\n---\n\n\
                   <p align=\"center\"><img src=\"logo.png\" alt=\"Logo\"> Centred <b>text</b></p>\n\n\
                   Note[^n] and ![pic](img/a.png) inline.\n\n[^n]: The footnote.\n";
        let b = parse(src);
        let kinds: Vec<Kind> = b.iter().map(|x| x.kind).collect();
        use Kind::*;
        assert_eq!(kinds, vec![Heading, Para, Para, Para, Para, Para, Para, Para, Para, Para, Para, Para, Code, Table, Rule, Image, Para, Para, Image, Rule, Para], "{b:#?}");
        assert!(!b.iter().any(|x| x.plain.contains("front matter")), "front matter skipped");
        assert_eq!(b[1].plain, "Para one still one.");
        let items: Vec<(&str, u8, u8)> = b[2..8].iter().map(|x| (x.marker.as_str(), x.indent, x.task)).collect();
        assert_eq!(items, vec![("\u{2022}", 1, 0), ("\u{2022}", 1, 0), ("\u{25e6}", 2, 0), ("1.", 3, 0), ("\u{2022}", 1, 1), ("\u{2022}", 1, 2)]);
        assert_eq!((b[8].marker.as_str(), b[9].marker.as_str()), ("3.", "4."));
        assert_eq!((b[10].quote, b[10].plain.as_str()), (1, "quoted"));
        assert_eq!((b[11].quote, b[11].plain.as_str()), (2, "twice"));
        assert_eq!(b[12].plain, "fn main() {}\n    x");
        assert!(b[12].first && b[12].last);
        let t = &b[13];
        assert_eq!(t.aligns, vec![0, 1, 2]);
        assert_eq!(t.rows.len(), 3);
        assert!(t.rows[0].head && !t.rows[1].head);
        assert_eq!(t.rows[0].cells[0].md, "**L**");
        assert_eq!(t.rows[1].cells[1].md, "**2**");
        assert_eq!(t.rows[2].cells[1].md, "", "an empty cell");
        assert_eq!(t.weights, vec![4.0, 3.0, 3.0]);
        assert_eq!((b[15].src.as_str(), b[15].plain.as_str()), ("logo.png", "Logo"));
        assert_eq!(b[16].plain, "Centred text");
        assert_eq!(b[17].plain, "Note[1] and  inline.");
        assert_eq!(b[18].src, "img/a.png", "a local picture after its paragraph");
        assert_eq!((b[20].marker.as_str(), b[20].indent, b[20].plain.as_str()), ("1.", 1, "The footnote."));
        for x in &b {
            StyledText::from_markdown(&x.md).unwrap();
            for r in &x.rows {
                for c in &r.cells {
                    StyledText::from_markdown(&c.md).unwrap();
                }
            }
        }
    }

    #[test]
    fn long_code_and_tables_are_chunked() {
        let code = format!("```\n{}```\n", (0..95).map(|i| format!("line {i}\n")).collect::<String>());
        let b = parse(&code);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].plain.lines().count(), CODE_CHUNK);
        assert_eq!(b[2].plain.lines().last(), Some("line 94"));
        assert_eq!(b.iter().map(|x| (x.first, x.last)).collect::<Vec<_>>(), vec![(true, false), (false, false), (false, true)]);
        let table = format!("| a | b |\n|---|---|\n{}", (0..50).map(|i| format!("| {i} | x |\n")).collect::<String>());
        let b = parse(&table);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].rows.len(), TABLE_CHUNK);
        assert_eq!(b[1].rows.len(), 51 - TABLE_CHUNK);
        assert_eq!(b[0].weights, b[1].weights, "every chunk has the same columns");
    }

    #[test]
    fn remote_pictures_are_alt_text() {
        let d = parse_doc("[![CI](https://x/badge.svg)](https://x) ![](//cdn/y.png) ![d](data:image/png;base64,AA)", Some(Path::new(r"C:\d")));
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].plain, "CI  d");
        assert_eq!(d.remote, vec!["https://x/badge.svg", "//cdn/y.png", "data:image/png;base64,AA"]);
        assert_eq!(image_path("a b%20c.png?raw=1", Path::new(r"C:\d")), Some(PathBuf::from(r"C:\d\a b c.png")));
        assert_eq!(image_path("../img/x.png", Path::new(r"C:\d\sub")), Some(PathBuf::from(r"C:\d\img\x.png")));
        assert_eq!(image_path("https://x/y.png", Path::new(r"C:\d")), None);
    }

    #[test]
    fn the_2_mb_cut() {
        let small = b"# hi\n".to_vec();
        assert_eq!(decode(&small), ("# hi\n".to_string(), false));
        assert_eq!(decode(b"\xEF\xBB\xBFbom").0, "bom", "the UTF-8 BOM dropped");
        assert_eq!(decode(b"bad \xFF byte").0, "bad \u{FFFD} byte");
        // Over the cap: at the last line break before it.
        let mut big = "x".repeat(MAX_BYTES - 10).into_bytes();
        big.extend(b"\nlast line that is cut off\n");
        let (s, cut) = decode(&big[..MAX_BYTES + 1]);
        assert!(cut);
        assert_eq!(s.len(), MAX_BYTES - 9);
        assert!(s.ends_with("x\n"));
        // No line break: at the last character boundary (a 3-byte character across the cap).
        let mut one = "y".repeat(MAX_BYTES - 1).into_bytes();
        one.extend("€€".as_bytes());
        let (s, cut) = decode(&one[..MAX_BYTES + 1]);
        assert!(cut);
        assert_eq!(s.len(), MAX_BYTES - 1, "the euro sign across the cap is left out whole");
        assert!(!s.contains('\u{FFFD}'));
        // UTF-16 by its BOM, little and big endian.
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain("# Ü\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(decode(&le), ("# Ü\n".to_string(), false));
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain("é".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(decode(&be).0, "é");
        let mut long16: Vec<u8> = vec![0xFF, 0xFE];
        while long16.len() <= MAX_BYTES {
            long16.extend("ab\n".encode_utf16().flat_map(u16::to_le_bytes));
        }
        let (s, cut) = decode(&long16[..MAX_BYTES + 1]);
        assert!(cut && s.ends_with("ab\n"));
    }

    #[test]
    fn links_are_classified() {
        let base = Path::new(r"C:\repo\docs");
        assert_eq!(classify_link("https://slint.dev", base), Link::Url("https://slint.dev".into()));
        assert_eq!(classify_link("MAILTO:a@b.c", base), Link::Url("MAILTO:a@b.c".into()));
        assert_eq!(classify_link("#P8-M2%3A-markdown", base), Link::Anchor("p8-m2:-markdown".into()));
        assert_eq!(classify_link("../README.md#install", base), Link::Local(PathBuf::from(r"C:\repo\README.md")));
        assert_eq!(classify_link("sub%20dir/x.md?plain=1", base), Link::Local(PathBuf::from(r"C:\repo\docs\sub dir\x.md")));
        assert_eq!(classify_link(r"D:\other\file.txt", base), Link::Local(PathBuf::from(r"D:\other\file.txt")));
        assert!(matches!(classify_link("javascript:alert(1)", base), Link::Refused(_)));
        assert!(matches!(classify_link("file:///C:/x", base), Link::Refused(_)));
        assert!(matches!(classify_link("", base), Link::Refused(_)));
        assert_eq!(local_kind(Path::new(r"C:\repo\docs"), true), Local::Folder);
        assert_eq!(local_kind(Path::new(r"C:\repo\setup.EXE"), false), Local::Program);
        assert_eq!(local_kind(Path::new(r"C:\repo\run.ps1"), false), Local::Program);
        assert_eq!(local_kind(Path::new(r"C:\repo\a.lnk"), false), Local::Program);
        assert_eq!(local_kind(Path::new(r"C:\repo\notes.md"), false), Local::Document);
        assert_eq!(local_kind(Path::new(r"C:\repo\Makefile"), false), Local::Document);
    }

    #[test]
    fn html_blocks_keep_text_and_pictures() {
        let p = html_pieces("<!-- a comment <img src=\"no.png\"> -->\n<div align=center>\n<IMG SRC='a.png' ALT=\"A &amp; B\" width=100/>\n<h1>Name &lt;1&gt;</h1>caption</div>");
        assert_eq!(p, vec![Piece::Img { src: "a.png".into(), alt: "A & B".into() }, Piece::Text("Name <1>".into()), Piece::Text("caption".into())]);
        assert_eq!(attr(" src=x.png>", "src"), Some("x.png".into()));
        assert_eq!(attr(" data-src=\"y\" src=\"z\"", "src"), Some("z".into()));
        assert_eq!(entities("&#65;&#x42;&bogus;&"), "AB&bogus;&");
    }

    #[test]
    fn named_entities() {
        assert_eq!(entities("&nbsp;&yuml;&Auml;&eacute;&times;"), "\u{a0}ÿÄé×");
        assert_eq!(entities("a&mdash;b&hellip; &ldquo;q&rdquo; &euro;5 &rarr; &trade;"), "a—b… “q” €5 → ™");
        assert_eq!(entities("&Amp; &Eacute; &eacute;"), "&Amp; É é", "names are case-sensitive");
        assert_eq!(entity("copy").as_deref(), Some("©"));
        assert_eq!(entity("frac12").as_deref(), Some("½"));
        // P11-M4: the full HTML5 table, the longest name, two-character ones, and no Markdown run.
        assert_eq!(entities("&AMP;&notin;&Afr;&nsubE;&CounterClockwiseContourIntegral;"), "&∉\u{1D504}\u{2AC5}\u{338}∳");
        assert_eq!(entities("&ThickSpace;"), "\u{205F}\u{200A}");
        assert_eq!(entities("&a*b; &x; &; &&lt;"), "&a*b; &x; &; &<");
        assert_eq!(entity("lt").as_deref(), Some("<"));
    }

    #[test]
    fn plain_text_is_chunked() {
        let (b, lines) = text_blocks("one\r\ntwo\rthree\tx\n");
        assert_eq!(lines, 3);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].kind, b[0].plain.as_str(), b[0].first, b[0].last), (Kind::Code, "one\ntwo\nthree    x", true, true));
        assert_eq!(text_blocks(""), (vec![], 0));
        assert_eq!(text_blocks("\n").1, 0, "a lone line end is an empty file");
        assert_eq!(text_blocks("a\n\nb").0[0].plain, "a\n\nb", "blank lines kept");
        // 95 lines: chunks of 40, 40 and 15.
        let src: String = (0..95).map(|i| format!("line {i}\n")).collect();
        let (b, lines) = text_blocks(&src);
        assert_eq!(lines, 95);
        assert_eq!(b.iter().map(|x| x.plain.lines().count()).collect::<Vec<_>>(), vec![40, 40, 15]);
        assert_eq!(b.iter().map(|x| (x.first, x.last)).collect::<Vec<_>>(), vec![(true, false), (false, false), (false, true)]);
        // Lines of 3,000 bytes: two to a chunk (three would pass 8 KB).
        let src = format!("{}\n", "x".repeat(3000)).repeat(5);
        let b = text_blocks(&src).0;
        assert_eq!(b.iter().map(|x| x.plain.lines().count()).collect::<Vec<_>>(), vec![2, 2, 1]);
        assert!(b.iter().all(|x| x.plain.len() <= TEXT_CHUNK_BYTES));
        // One line of 20,000 bytes (a minified JSON), with a 3-byte character across the cut.
        let long = format!("{}€{}", "a".repeat(TEXT_CHUNK_BYTES - 1), "b".repeat(20_000 - TEXT_CHUNK_BYTES - 2));
        let (b, lines) = text_blocks(&long);
        assert_eq!(lines, 1);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].plain.len(), TEXT_CHUNK_BYTES - 1, "the euro sign not cut");
        assert!(b[1].plain.starts_with('€'));
        assert_eq!(b.iter().map(|x| x.plain.as_str()).collect::<String>(), long, "nothing lost");
    }

    #[test]
    fn encodings_are_named() {
        assert_eq!(encoding(b"plain"), "UTF-8");
        assert_eq!(encoding("é".as_bytes()), "UTF-8");
        assert_eq!(encoding(b"\xEF\xBB\xBFbom"), "UTF-8 BOM");
        assert_eq!(encoding(b"bad \xFF"), "UTF-8 (invalid bytes replaced)");
        assert_eq!(encoding(b"\xEF\xBB\xBFbad \xFF"), "UTF-8 BOM (invalid bytes replaced)");
        assert_eq!(encoding(b"\xFF\xFEa\0"), "UTF-16LE");
        assert_eq!(encoding(b"\xFE\xFF\0a"), "UTF-16BE");
        // A character cut by the 2 MB cap is not invalid.
        let mut big = "y".repeat(MAX_BYTES - 1).into_bytes();
        big.extend("€".as_bytes());
        assert_eq!(encoding(&big), "UTF-8");
    }
}
