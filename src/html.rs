//! HTML for the preview pane (P9-M2): a hand-written scanner (no parser crate pinned cleanly, see
//! `docs/phase-9-plan.md` § P9-M2 Settled first) that reads a page into the Markdown preview's
//! block list. The tokenizer knows comments, quoted attributes and the raw-text elements (a `<` in
//! a script is not a tag); the mapping keeps headings, paragraphs, lists, quotes, `pre`, rules,
//! tables, pictures and links, drops scripts, styles and the page's head, and flows every other
//! element's text into the block around it. Pure functions only: the preview worker reads the file
//! and makes the styled text (preview.rs). Nothing is executed and nothing remote is fetched.

use crate::markdown::{self, Block, Cell, Doc, Inline, Kind, Row};
use std::path::{Path, PathBuf};

/// Elements whose content is raw text up to their end tag (the HTML tokenizer's rule).
const RAW: &[&str] = &["script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes", "noscript"];
/// Elements dropped with everything inside them.
const DROPPED: &[&str] = &["script", "style", "head", "nav", "noscript", "title", "template", "svg", "iframe", "noembed", "noframes"];
/// Block containers: each ends the paragraph being built, at its start and at its end.
const BREAKS: &[&str] = &[
    "p", "div", "section", "article", "main", "header", "footer", "aside", "figure", "figcaption", "address", "details", "summary", "dl", "dt",
    "dd", "form", "fieldset", "center", "body", "html",
];
/// A `colspan` past this counts as this (the HTML standard's own limit is 1,000).
const MAX_SPAN: usize = 64;
/// Elements that never have an end tag.
const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// One token of the page.
#[derive(Debug, PartialEq)]
enum Tok<'a> {
    /// Text as written (entities not decoded yet).
    Text(&'a str),
    /// A start tag: its lower-case name and the text of its attributes.
    Start(String, &'a str),
    End(String),
}

/// The page as tokens. Comments, `<!DOCTYPE>`, `<?...?>` and CDATA sections are dropped; a raw-text
/// element's content is one text token whatever it holds; a `<` that starts no tag is text.
fn tokens(html: &str) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let b = html.as_bytes();
    let (mut i, mut text_from) = (0, 0);
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        let next = b.get(i + 1).copied().unwrap_or(0);
        // Where the markup ends (None: it is text after all).
        let end = if rest.starts_with("<!--") {
            Some(rest[4..].find("-->").map_or(html.len(), |j| i + 4 + j + 3))
        } else if rest.starts_with("<![CDATA[") {
            Some(rest.find("]]>").map_or(html.len(), |j| i + j + 3))
        } else if next == b'!' || next == b'?' {
            Some(rest.find('>').map_or(html.len(), |j| i + j + 1))
        } else if next == b'/' && b.get(i + 2).is_some_and(u8::is_ascii_alphabetic) || next.is_ascii_alphabetic() {
            Some(tag_end(html, i))
        } else {
            None
        };
        let Some(end) = end else {
            i += 1;
            continue;
        };
        if text_from < i {
            out.push(Tok::Text(&html[text_from..i]));
        }
        let inner = &html[i + 1..end.saturating_sub(1).max(i + 1)];
        i = end;
        text_from = end;
        if !(next == b'/' || next.is_ascii_alphabetic()) {
            continue;
        }
        let closing = next == b'/';
        let body = if closing { &inner[1..] } else { inner };
        let name_end = body.find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').unwrap_or(body.len());
        let name = body[..name_end].to_ascii_lowercase();
        if closing {
            out.push(Tok::End(name));
            continue;
        }
        let raw = RAW.contains(&name.as_str());
        out.push(Tok::Start(name.clone(), &body[name_end..]));
        if raw && !body.trim_end().ends_with('/') {
            // Up to `</name`, any case.
            let close = format!("</{name}");
            let lower_rest = html[i..].to_ascii_lowercase();
            let j = lower_rest.find(&close).map_or(html.len(), |j| i + j);
            if i < j {
                out.push(Tok::Text(&html[i..j]));
            }
            i = if j < html.len() { tag_end(html, j) } else { j };
            text_from = i;
            out.push(Tok::End(name));
        }
    }
    if text_from < b.len() {
        out.push(Tok::Text(&html[text_from..]));
    }
    out
}

/// The end of the tag starting at `i` (one past its `>`), quotes in attributes honoured.
fn tag_end(html: &str, i: usize) -> usize {
    let mut quote = None;
    for (k, c) in html[i..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return i + k + 1,
            _ => {}
        }
    }
    html.len()
}

/// An attribute's value (entities decoded) from a start tag's attribute text; Some("") for one
/// written without a value.
fn attr(attrs: &str, want: &str) -> Option<String> {
    let mut s = attrs;
    loop {
        s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if s.is_empty() {
            return None;
        }
        let n = s.find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/' || c == '>').unwrap_or(s.len()).max(1);
        let name = &s[..n];
        s = s[n..].trim_start();
        let mut value = String::new();
        if let Some(r) = s.strip_prefix('=') {
            let r = r.trim_start();
            let (v, after) = match r.chars().next() {
                Some(q @ ('"' | '\'')) => match r[1..].find(q) {
                    Some(j) => (&r[1..1 + j], &r[2 + j..]),
                    None => (&r[1..], ""),
                },
                _ => {
                    let j = r.find(|c: char| c.is_ascii_whitespace()).unwrap_or(r.len());
                    (&r[..j], &r[j..])
                }
            };
            value = markdown::entities(v);
            s = after;
        }
        if name.eq_ignore_ascii_case(want) {
            return Some(value);
        }
    }
}

/// What comes before the next content written into a block.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Gap {
    Space,
    Break,
}

/// One block's inline content being built (a paragraph, a heading, a table cell): whitespace
/// collapsed as a browser does it, emphasis runs written only around content (so `<b> x </b>` is
/// ` **x** `, which Markdown reads as bold, and `<b></b>` is nothing), a link likewise.
#[derive(Default)]
struct Ink {
    i: Inline,
    gap: Option<Gap>,
    /// The open emphasis runs ("**", "*", or "" inside a bold heading), and whether each is written
    /// in this block.
    runs: Vec<(&'static str, bool)>,
    /// The open link: its destination, whether its `[` is written in this block (and where), and how
    /// many runs were open outside it.
    link: Option<(String, Option<usize>, usize)>,
    /// Inside a `code` element: its text, written as one code span at its end.
    code: Option<String>,
}

impl Ink {
    fn empty(&self) -> bool {
        self.i.plain.trim().is_empty() && !self.i.md.contains("](<")
    }

    /// Before content: the pending space or line break, the link's `[` and the runs opened.
    fn start(&mut self) {
        match self.gap.take() {
            Some(Gap::Break) if !self.i.plain.is_empty() => self.i.hard_break(),
            Some(Gap::Space) if !self.i.plain.is_empty() && !self.i.plain.ends_with([' ', '\n']) => self.i.text(" "),
            _ => {}
        }
        let outer = self.link.as_ref().map_or(self.runs.len(), |l| l.2);
        for k in 0..outer {
            self.open_run(k);
        }
        if let Some(l) = self.link.as_mut() {
            if l.1.is_none() {
                l.1 = Some(self.i.md.len());
                self.i.link_open();
            }
        }
        for k in outer..self.runs.len() {
            self.open_run(k);
        }
    }

    fn open_run(&mut self, k: usize) {
        let (run, written) = self.runs[k];
        if !written {
            if !run.is_empty() {
                self.i.delim(run, true);
            }
            self.runs[k].1 = true;
        }
    }

    /// Text as written in the page (entities decoded already).
    fn text(&mut self, s: &str) {
        if let Some(c) = self.code.as_mut() {
            c.push_str(s);
            return;
        }
        let mut word = String::new();
        for ch in s.chars() {
            if ch.is_ascii_whitespace() {
                if !word.is_empty() {
                    self.start();
                    self.i.text(&std::mem::take(&mut word));
                }
                if self.gap.is_none() {
                    self.gap = Some(Gap::Space);
                }
            } else {
                word.push(ch);
            }
        }
        if !word.is_empty() {
            self.start();
            self.i.text(&word);
        }
    }

    fn brk(&mut self) {
        if self.code.is_none() {
            self.gap = Some(Gap::Break);
        }
    }

    fn open(&mut self, run: &'static str) {
        self.runs.push((run, false));
    }

    /// Close the runs from the top down to the newest `run` (those above it close with it).
    fn close(&mut self, run: &'static str) {
        let Some(k) = self.runs.iter().rposition(|r| r.0 == run) else { return };
        self.close_from(k);
    }

    fn close_from(&mut self, k: usize) {
        while self.runs.len() > k {
            if let Some((run, true)) = self.runs.pop() {
                if !run.is_empty() {
                    self.i.delim(run, false);
                }
            }
        }
    }

    fn link_open(&mut self, dest: String) {
        if self.link.is_none() {
            self.link = Some((dest, None, self.runs.len()));
        }
    }

    fn link_close(&mut self) {
        let Some((_, _, outer)) = self.link else { return };
        self.close_from(outer);
        self.end_link_here();
        self.link = None;
    }

    /// The link's `](<dest>)` if its `[` is written, or the `[` taken back if nothing followed it.
    fn end_link_here(&mut self) {
        let Some((dest, Some(at), _)) = self.link.as_mut() else { return };
        if self.i.md.len() == *at + 1 {
            self.i.md.truncate(*at);
        } else {
            let dest = dest.clone();
            self.i.link_close(&dest);
        }
        if let Some(l) = self.link.as_mut() {
            l.1 = None;
        }
    }

    fn code_open(&mut self) {
        if self.code.is_none() {
            self.code = Some(String::new());
        }
    }

    fn code_close(&mut self) {
        let Some(c) = self.code.take() else { return };
        let c = c.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        if c.is_empty() {
            return;
        }
        self.start();
        self.i.code(&c);
    }

    /// The block's content so far, every open run and link closed (they open again in the next
    /// block, as a `<b>` around two paragraphs makes both bold).
    fn take(&mut self) -> Inline {
        self.code_close();
        let outer = self.link.as_ref().map_or(self.runs.len(), |l| l.2);
        for k in (outer..self.runs.len()).rev() {
            self.end_run(k);
        }
        self.end_link_here();
        for k in (0..outer).rev() {
            self.end_run(k);
        }
        self.gap = None;
        std::mem::take(&mut self.i)
    }

    fn end_run(&mut self, k: usize) {
        if let (run, true) = self.runs[k] {
            if !run.is_empty() {
                self.i.delim(run, false);
            }
            self.runs[k].1 = false;
        }
    }
}

/// The table being read (only the outermost: a table inside a cell flows into that cell).
#[derive(Default)]
struct Table {
    rows: Vec<Row>,
    row: Option<(Vec<Cell>, bool)>,
    /// The open cell, whether it is a `th`, and its `colspan` (P11-M4).
    cell: Option<(Ink, bool, usize)>,
    thead: bool,
    /// Tables open inside the cells.
    inner: usize,
}

impl Table {
    fn end_cell(&mut self) {
        let Some((mut ink, th, span)) = self.cell.take() else { return };
        let i = ink.take();
        let md = if th && !i.md.is_empty() { format!("**{}**", i.md) } else { i.md };
        let (cells, all_th) = self.row.get_or_insert_with(|| (Vec::new(), true));
        cells.push(Cell { md, plain: i.plain.trim().to_string() });
        // A cell spanning columns is followed by empty ones, so the cells after it line up.
        cells.extend((1..span).map(|_| Cell::default()));
        *all_th &= th;
    }

    fn end_row(&mut self) {
        self.end_cell();
        if let Some((cells, all_th)) = self.row.take() {
            if !cells.is_empty() {
                self.rows.push(Row { head: self.thead || all_th, cells });
            }
        }
    }
}

struct Builder {
    base: Option<PathBuf>,
    out: Vec<Block>,
    para: Ink,
    heading: Option<u8>,
    quote: u8,
    lists: Vec<Option<u64>>,
    marker: Option<String>,
    /// Ids for the next block.
    anchor: Vec<String>,
    /// Local pictures inside the current block, placed after it.
    later: Vec<Block>,
    /// Inside a `pre`: its text so far, and how many are open.
    pre: Option<(String, usize)>,
    table: Option<Table>,
    /// The dropped element being skipped, and how many of its name are open.
    skip: Option<(String, usize)>,
    remote: Vec<String>,
}

impl Builder {
    /// The inline content text goes into now: the open table cell's, else the paragraph's.
    fn ink(&mut self) -> &mut Ink {
        match self.table.as_mut().and_then(|t| t.cell.as_mut()) {
            Some((ink, ..)) => ink,
            None => &mut self.para,
        }
    }

    fn in_cell(&self) -> bool {
        self.table.as_ref().is_some_and(|t| t.cell.is_some())
    }

    /// Place a block at the current list and quote depth, with the item's marker if it is the
    /// item's first, and the ids waiting for it.
    fn push(&mut self, mut b: Block) {
        b.indent = self.lists.len().min(255) as u8;
        b.quote = self.quote;
        if let Some(m) = self.marker.take() {
            b.marker = m;
        }
        if !self.anchor.is_empty() {
            b.anchor = std::mem::take(&mut self.anchor).join(" ");
        }
        self.out.push(b);
    }

    /// End the paragraph or heading being built (if it has anything in it), then place the
    /// pictures that sat in it.
    fn flush(&mut self) {
        if !self.para.empty() {
            let i = self.para.take();
            let b = match self.heading {
                Some(level) => Block { kind: Kind::Heading, level, md: format!("**{}**", i.md), plain: i.plain.trim().to_string(), ..Default::default() },
                None => Block { kind: Kind::Para, md: i.md, plain: i.plain.trim().to_string(), ..Default::default() },
            };
            self.push(b);
        } else {
            self.para.take();
        }
        for b in std::mem::take(&mut self.later) {
            self.push(b);
        }
    }

    fn picture(&mut self, attrs: &str) {
        let src = attr(attrs, "src").unwrap_or_default();
        let alt = attr(attrs, "alt").unwrap_or_default();
        let local = !self.in_cell() && self.base.as_ref().is_some_and(|b| markdown::image_path(&src, b).is_some());
        if local {
            self.later.push(Block { kind: Kind::Image, src, plain: alt, ..Default::default() });
        } else {
            if !alt.trim().is_empty() {
                self.ink().text(&alt);
            }
            if markdown::image_path(&src, Path::new("")).is_none() && !src.is_empty() {
                self.remote.push(src);
            }
        }
    }

    fn start(&mut self, name: &str, attrs: &str) {
        if let Some((skipped, n)) = self.skip.as_mut() {
            if skipped == name && !VOID.contains(&name) {
                *n += 1;
            }
            if !(skipped == "head" && name == "body") {
                return;
            }
            self.skip = None;
        }
        if DROPPED.contains(&name) {
            if !attrs.trim_end().ends_with('/') {
                self.skip = Some((name.to_string(), 1));
            }
            return;
        }
        // A block element ends the paragraph before it, so its id goes to the block it starts.
        let blockish = BREAKS.contains(&name) || matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote" | "pre" | "hr" | "table");
        if blockish && self.pre.is_none() && !self.in_cell() {
            self.flush();
        }
        if let Some(id) = attr(attrs, "id").or_else(|| (name == "a").then(|| attr(attrs, "name")).flatten()) {
            if !id.is_empty() && !self.in_cell() {
                self.anchor.push(id.to_lowercase());
            }
        }
        if let Some((_, n)) = self.pre.as_mut() {
            match name {
                "pre" => *n += 1,
                "br" => self.pre.as_mut().map_or((), |p| p.0.push('\n')),
                _ => {}
            }
            return;
        }
        // Tables first: they work inside cells too.
        match name {
            "table" => {
                match self.table.as_mut() {
                    Some(t) => {
                        t.inner += 1;
                        self.ink().brk();
                    }
                    None => {
                        self.flush();
                        self.table = Some(Table::default());
                    }
                }
                return;
            }
            "tr" | "td" | "th" | "thead" | "tbody" | "tfoot" | "caption" if self.table.is_some() => {
                let t = self.table.as_mut().expect("checked");
                if t.inner > 0 {
                    if name == "tr" {
                        self.ink().brk();
                    } else if name != "caption" {
                        self.ink().text(" ");
                    }
                    return;
                }
                match name {
                    "tr" => t.end_row(),
                    "td" | "th" => {
                        t.end_cell();
                        let span = attr(attrs, "colspan").and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(1).clamp(1, MAX_SPAN);
                        t.cell = Some((Ink::default(), name == "th", span));
                    }
                    "thead" => {
                        t.end_row();
                        t.thead = true;
                    }
                    "tbody" | "tfoot" => {
                        t.end_row();
                        t.thead = false;
                    }
                    _ => {
                        // A caption: a paragraph before the table.
                        t.end_row();
                        self.flush();
                    }
                }
                return;
            }
            _ => {}
        }
        let block = BREAKS.contains(&name) || matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote" | "pre" | "hr" | "br");
        if block && self.in_cell() {
            self.ink().brk();
            return;
        }
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.heading = Some(name.as_bytes()[1] - b'0');
            }
            "ul" | "ol" => {
                self.flush();
                let start = attr(attrs, "start").and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(1);
                self.lists.push((name == "ol").then_some(start));
            }
            "li" => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                self.marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        Some(format!("{}.", *n - 1))
                    }
                    Some(None) => Some(["\u{2022}", "\u{25e6}", "\u{25aa}"][depth % 3].to_string()),
                    None => None,
                };
            }
            "blockquote" => {
                self.flush();
                self.quote = self.quote.saturating_add(1);
            }
            "pre" => {
                self.flush();
                self.pre = Some((String::new(), 1));
            }
            "hr" => {
                self.flush();
                self.push(Block { kind: Kind::Rule, ..Default::default() });
            }
            "br" => self.ink().brk(),
            "img" => self.picture(attrs),
            "a" => {
                if let Some(href) = attr(attrs, "href").filter(|h| !h.trim().is_empty()) {
                    self.ink().link_open(href.trim().to_string());
                }
            }
            "strong" | "b" => {
                let run = if self.bold() { "" } else { "**" };
                self.ink().open(run);
            }
            "em" | "i" => self.ink().open("*"),
            "code" | "kbd" | "samp" | "tt" => self.ink().code_open(),
            _ if BREAKS.contains(&name) => self.flush(),
            _ => {}
        }
    }

    /// Inside a heading or a table's head cell: the whole is bold, so a bold run is not written.
    fn bold(&self) -> bool {
        match self.table.as_ref().and_then(|t| t.cell.as_ref()) {
            Some((_, th, _)) => *th,
            None => self.heading.is_some(),
        }
    }

    fn end(&mut self, name: &str) {
        if let Some((skipped, n)) = self.skip.as_mut() {
            if skipped == name {
                *n -= 1;
                if *n == 0 {
                    self.skip = None;
                }
            }
            return;
        }
        if let Some((text, n)) = self.pre.as_mut() {
            if name != "pre" {
                return;
            }
            *n -= 1;
            if *n > 0 {
                return;
            }
            // A line break right after `<pre>` is not part of its text.
            let text = std::mem::take(text);
            let body = text.strip_prefix("\r\n").or_else(|| text.strip_prefix('\n')).unwrap_or(&text);
            for b in markdown::text_blocks(&markdown::entities(body)).0 {
                self.push(b);
            }
            self.pre = None;
            return;
        }
        match name {
            "table" if self.table.is_some() => {
                let t = self.table.as_mut().expect("checked");
                if t.inner > 0 {
                    t.inner -= 1;
                    return;
                }
                t.end_row();
                let rows = std::mem::take(&mut t.rows);
                self.table = None;
                // Text outside the cells goes before the table, as a browser places it.
                self.flush();
                for b in markdown::table_chunks(rows, Vec::new()) {
                    self.push(b);
                }
                return;
            }
            "tr" | "td" | "th" | "thead" | "tbody" | "tfoot" | "caption" if self.table.is_some() => {
                let t = self.table.as_mut().expect("checked");
                if t.inner > 0 {
                    return;
                }
                match name {
                    "tr" => t.end_row(),
                    "td" | "th" => t.end_cell(),
                    "thead" => {
                        t.end_row();
                        t.thead = false;
                    }
                    "caption" => self.flush(),
                    _ => {}
                }
                return;
            }
            _ => {}
        }
        let block = BREAKS.contains(&name) || matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote");
        if block && self.in_cell() {
            self.ink().brk();
            return;
        }
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.heading = None;
            }
            "ul" | "ol" => {
                self.flush();
                self.lists.pop();
                self.marker = None;
            }
            "li" => {
                self.flush();
                self.marker = None;
            }
            "blockquote" => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            "a" => self.ink().link_close(),
            "strong" | "b" => {
                let run = if self.bold() { "" } else { "**" };
                self.ink().close(run);
            }
            "em" | "i" => self.ink().close("*"),
            "code" | "kbd" | "samp" | "tt" => self.ink().code_close(),
            _ if BREAKS.contains(&name) => self.flush(),
            _ => {}
        }
    }

    fn text(&mut self, raw: &str) {
        if self.skip.is_some() {
            return;
        }
        if let Some((text, _)) = self.pre.as_mut() {
            text.push_str(raw);
            return;
        }
        let t = markdown::entities(raw);
        if let Some(tb) = self.table.as_ref() {
            // Whitespace between a table's cells is nothing.
            if tb.cell.is_none() && t.trim().is_empty() {
                return;
            }
        }
        self.ink().text(&t);
    }

    fn finish(mut self) -> Doc {
        if let Some((text, _)) = self.pre.take() {
            for b in markdown::text_blocks(&markdown::entities(&text)).0 {
                self.push(b);
            }
        }
        if let Some(mut t) = self.table.take() {
            t.end_row();
            self.flush();
            for b in markdown::table_chunks(t.rows, Vec::new()) {
                self.push(b);
            }
        }
        self.flush();
        Doc { blocks: self.out, remote: self.remote }
    }
}

/// Windows-1252's characters for the bytes 0x80 to 0x9F (the rest of its upper half is Latin-1);
/// the five bytes it leaves undefined stand for themselves, as the HTML standard maps them.
const CP1252: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090,
    0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];
/// ISO-8859-15's eight changes to Latin-1.
const ISO8859_15: [(u8, u16); 8] = [(0xA4, 0x20AC), (0xA6, 0x0160), (0xA8, 0x0161), (0xB4, 0x017D), (0xB8, 0x017E), (0xBC, 0x0152), (0xBD, 0x0153), (0xBE, 0x0178)];

/// The single-byte code pages TwinFM reads a page in (P11-M4). The HTML standard reads a page
/// labelled ISO-8859-1 or ASCII as Windows-1252, a superset, and so does this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodePage {
    Cp1252,
    Iso8859_15,
}

impl CodePage {
    fn char(self, b: u8) -> char {
        let u = match (self, b) {
            (_, 0..=0x7F) => u16::from(b),
            (CodePage::Cp1252, 0x80..=0x9F) => CP1252[usize::from(b - 0x80)],
            (CodePage::Iso8859_15, _) => ISO8859_15.iter().find(|(x, _)| *x == b).map_or(u16::from(b), |&(_, u)| u),
            _ => u16::from(b),
        };
        char::from_u32(u32::from(u)).unwrap_or(char::REPLACEMENT_CHARACTER)
    }

    fn name(self) -> &'static str {
        match self {
            CodePage::Cp1252 => "windows-1252",
            CodePage::Iso8859_15 => "ISO-8859-15",
        }
    }
}

/// What a page's `<meta>` says it is written in.
#[derive(Debug, PartialEq)]
pub enum Charset {
    /// None given, or UTF-8; also UTF-16 named in a `<meta>`, which the HTML standard reads as
    /// UTF-8 (a page that is really UTF-16 starts with its byte order mark, read before this).
    Utf8,
    Single(CodePage),
    /// A label TwinFM has no table for (read as UTF-8, invalid bytes replaced, and named).
    Unknown(String),
}

/// The charset a page's first 1,024 bytes declare (the HTML standard's prescan, simplified): a
/// `<meta charset="...">`, or a `<meta http-equiv="Content-Type" content="text/html; charset=...">`.
pub fn meta_charset(bytes: &[u8]) -> Charset {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let lower = head.to_ascii_lowercase();
    let mut from = 0;
    let label = loop {
        let Some(i) = lower[from..].find("<meta") else { break None };
        let start = from + i + 5;
        let end = lower[start..].find('>').map_or(lower.len(), |j| start + j);
        from = end;
        let attrs = &lower[start..end];
        if let Some(c) = attr(attrs, "charset") {
            break Some(c);
        }
        if let Some(c) = attr(attrs, "content").and_then(|c| c.split_once("charset=").map(|(_, r)| r.to_string())) {
            break Some(c.trim_matches(['"', '\'', ' ']).split([';', ' ']).next().unwrap_or("").to_string());
        }
    };
    let Some(label) = label.map(|l| l.trim().to_string()) else { return Charset::Utf8 };
    match label.as_str() {
        "" | "utf-8" | "utf8" | "unicode-1-1-utf-8" | "utf-16" | "utf-16le" | "utf-16be" | "unicode" | "ucs-2" => Charset::Utf8,
        "windows-1252" | "cp1252" | "x-cp1252" | "iso-8859-1" | "iso8859-1" | "iso_8859-1" | "latin1" | "l1" | "us-ascii" | "ascii" | "cp819"
        | "ibm819" | "iso-ir-100" | "csisolatin1" | "ansi_x3.4-1968" => Charset::Single(CodePage::Cp1252),
        "iso-8859-15" | "iso8859-15" | "iso_8859-15" | "iso885915" | "latin9" | "l9" | "csisolatin9" | "latin-9" => Charset::Single(CodePage::Iso8859_15),
        _ => Charset::Unknown(label),
    }
}

/// A page's text from its first bytes (`MAX_BYTES` + 1 read): a byte order mark first, then the
/// `<meta>` charset, else UTF-8, as `markdown::decode` cuts it. Also whether it was cut and the
/// encoding's name for the `[preview]` line.
pub fn decode(bytes: &[u8]) -> (String, bool, String) {
    let bom = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let utf8 = |note: String| {
        let (text, cut) = markdown::decode(bytes);
        (text, cut, format!("{}{note}", markdown::encoding(bytes)))
    };
    match if bom { Charset::Utf8 } else { meta_charset(bytes) } {
        Charset::Utf8 => utf8(String::new()),
        Charset::Unknown(label) => utf8(format!(", charset \"{label}\" not known")),
        Charset::Single(cp) => {
            let cut = bytes.len() > markdown::MAX_BYTES;
            let mut head = &bytes[..bytes.len().min(markdown::MAX_BYTES)];
            if cut {
                if let Some(i) = head.iter().rposition(|&b| b == b'\n') {
                    head = &head[..=i];
                }
            }
            (head.iter().map(|&b| cp.char(b)).collect(), cut, cp.name().to_string())
        }
    }
}

/// Read a page into the preview's blocks. `base` is its folder, for telling local pictures from
/// remote ones (None: every picture is shown as its alt text).
pub fn parse(html: &str, base: Option<&Path>) -> Doc {
    let mut b = Builder {
        base: base.map(Path::to_path_buf),
        out: Vec::new(),
        para: Ink::default(),
        heading: None,
        quote: 0,
        lists: Vec::new(),
        marker: None,
        anchor: Vec::new(),
        later: Vec::new(),
        pre: None,
        table: None,
        skip: None,
        remote: Vec::new(),
    };
    for t in tokens(html) {
        match t {
            Tok::Text(s) => b.text(s),
            Tok::Start(name, attrs) => b.start(&name, attrs),
            Tok::End(name) => b.end(&name),
        }
    }
    b.finish()
}


#[cfg(test)]
mod tests {
    use super::*;
    use slint::StyledText;

    /// The serialized inline Markdown reads in Slint as `expected` (a hand-written Markdown line) does.
    fn reads_as(md: &str, expected: &str) {
        let ours = StyledText::from_markdown(md).unwrap_or_else(|e| panic!("{md:?}: {e}"));
        assert_eq!(ours, StyledText::from_markdown(expected).unwrap(), "{md:?}");
    }

    #[test]
    fn the_tokenizer() {
        let t = tokens("a<!-- <b> -->b<!DOCTYPE html><p class=\"x>y\" id=q>c</P><script>if (a<b) x='</p>';</SCRIPT >d < e<br/>f");
        assert_eq!(
            t,
            vec![
                Tok::Text("a"),
                Tok::Text("b"),
                Tok::Start("p".into(), " class=\"x>y\" id=q"),
                Tok::Text("c"),
                Tok::End("p".into()),
                Tok::Start("script".into(), ""),
                Tok::Text("if (a<b) x='</p>';"),
                Tok::End("script".into()),
                Tok::Text("d < e"),
                Tok::Start("br".into(), "/"),
                Tok::Text("f"),
            ]
        );
        assert_eq!(tokens("x<style>never closed"), vec![Tok::Text("x"), Tok::Start("style".into(), ""), Tok::Text("never closed"), Tok::End("style".into())]);
        assert_eq!(tokens("<a href='x'"), vec![Tok::Start("a".into(), " href='x")], "a tag cut off at the end");
        assert_eq!(attr(" class=\"x>y\" id=q", "id"), Some("q".into()));
        assert_eq!(attr(" data-src=\"y\" SRC='z&amp;w'", "src"), Some("z&w".into()));
        assert_eq!(attr(" hidden alt=", "hidden"), Some(String::new()));
        assert_eq!(attr(" alt=x", "src"), None);
    }

    /// A page with every mapped element, a script, a style and the rest of what is dropped.
    const PAGE: &str = r#"<!DOCTYPE html>
<html><head><title>Page title</title><style>p { color: red }</style>
<script>document.write("<p>injected</p>")</script></head>
<body>
<nav><a href="/">Home</a> | <a href="/x">X</a></nav>
<h1 id="Top">Main &amp; title</h1>
<p>First <b>bold</b>, <strong> strong </strong>, <em>em</em> and <i>i</i>, <code>a &lt; b</code>, a <a href="https://example.com/a">link</a>.</p>
<div>Div text<br>second line</div>
<div>  </div>
<ul><li>one<li>two<ul><li>nested</ul></li></ul>
<ol start="3"><li>three</li><li>four</li></ol>
<blockquote><p>quoted</p><blockquote>twice</blockquote></blockquote>
<pre>
fn main() {
	x &amp; y
}
</pre>
<hr>
<table><caption>Cap</caption><thead><tr><th>Name</th><th>Size</th></tr></thead>
<tbody><tr><td>a <b>b</b></td><td>1</td></tr><tr><td>c<br>d</td><td>2</td></table>
<p>Pic <img src="img/dot.png" alt="Dot"> and <img src="https://x/y.png" alt="Remote">.</p>
<noscript>enable js</noscript>
<span>loose</span> text <!-- comment --> here
<h2>Second</h2>
</body></html>"#;

    #[test]
    fn a_page_becomes_blocks() {
        let d = parse(PAGE, Some(Path::new(r"C:\site")));
        let b = &d.blocks;
        use Kind::*;
        let kinds: Vec<Kind> = b.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec![Heading, Para, Para, Para, Para, Para, Para, Para, Para, Para, Code, Rule, Para, Table, Para, Image, Para, Heading], "{b:#?}");
        let all: String = b.iter().map(|x| format!("{} {} ", x.plain, x.md)).collect();
        for gone in ["Page title", "color", "injected", "Home", "enable js", "comment"] {
            assert!(!all.contains(gone), "{gone:?} dropped");
        }
        assert_eq!((b[0].level, b[0].plain.as_str(), b[0].anchor.as_str()), (1, "Main & title", "top"));
        reads_as(&b[0].md, "**Main & title**");
        reads_as(&b[1].md, "First **bold**, **strong** , *em* and *i*, `a < b`, a [link](https://example.com/a).");
        assert_eq!(b[2].plain, "Div text\nsecond line");
        let items: Vec<(&str, &str, u8)> = b[3..8].iter().map(|x| (x.plain.as_str(), x.marker.as_str(), x.indent)).collect();
        assert_eq!(items, vec![("one", "\u{2022}", 1), ("two", "\u{2022}", 1), ("nested", "\u{25e6}", 2), ("three", "3.", 1), ("four", "4.", 1)]);
        assert_eq!((b[8].plain.as_str(), b[8].quote), ("quoted", 1));
        assert_eq!((b[9].plain.as_str(), b[9].quote), ("twice", 2));
        assert_eq!(b[10].plain, "fn main() {\n    x & y\n}");
        assert_eq!(b[12].plain, "Cap", "the caption before its table");
        let t = &b[13];
        assert_eq!(t.rows.len(), 3);
        assert!(t.rows[0].head && !t.rows[1].head && !t.rows[2].head);
        assert_eq!(t.rows[0].cells.iter().map(|c| c.md.as_str()).collect::<Vec<_>>(), vec!["**Name**", "**Size**"]);
        reads_as(&t.rows[1].cells[0].md, "a **b**");
        assert_eq!(t.rows[2].cells[0].plain, "c\nd");
        assert_eq!(t.aligns, vec![0, 0]);
        assert_eq!(b[14].plain, "Pic and Remote.");
        assert_eq!((b[15].src.as_str(), b[15].plain.as_str()), ("img/dot.png", "Dot"));
        assert_eq!(d.remote, vec!["https://x/y.png"]);
        assert_eq!(b[16].plain, "loose text here");
        assert_eq!((b[17].level, b[17].plain.as_str()), (2, "Second"));
        for x in b {
            StyledText::from_markdown(&x.md).unwrap();
            for r in &x.rows {
                for c in &r.cells {
                    StyledText::from_markdown(&c.md).unwrap();
                }
            }
        }
    }

    #[test]
    fn runs_and_links_across_blocks() {
        let b = parse("<b><p>one</p><p>two</p></b><a href='x.html'><div>card</div><div>text</div></a><b></b><i> </i>end", None).blocks;
        let md: Vec<&str> = b.iter().map(|x| x.md.as_str()).collect();
        reads_as(md[0], "**one**");
        reads_as(md[1], "**two**");
        reads_as(md[2], "[card](x.html)");
        reads_as(md[3], "[text](x.html)");
        reads_as(md[4], "end");
        // Emphasis left open at the end, and nested wrongly.
        reads_as(&parse("<p>a <b>b <i>c</b> d", None).blocks[0].md, "a **b *c*** d");
        // Bold inside a heading is not written twice; an empty link is nothing.
        let b = parse("<h3>A <b>B</b></h3><p><a href='y'></a>z</p>", None).blocks;
        reads_as(&b[0].md, "**A B**");
        assert_eq!(b[1].md, "z");
    }

    #[test]
    fn odd_pages() {
        assert!(parse("<script>alert('<b>hi</b>')</script>", None).blocks.is_empty(), "a page of nothing but a script");
        assert!(parse("", None).blocks.is_empty());
        // No head element: the title still dropped; a stray li is a paragraph; a nested table flows.
        let b = parse("<title>T</title><li>stray</li><table><tr><td>x<table><tr><td>in</td><td>ner</td></tr></table></td><td>y</td></tr></table>", None).blocks;
        assert_eq!(b.len(), 2, "{b:#?}");
        assert_eq!((b[0].plain.as_str(), b[0].marker.as_str()), ("stray", ""));
        assert_eq!(b[1].rows[0].cells.iter().map(|c| c.plain.as_str()).collect::<Vec<_>>(), vec!["x\nin ner", "y"]);
        // An unclosed pre at the end, and a newline right after <pre> dropped.
        assert_eq!(parse("<pre>\na  b\n</pre><pre>open", None).blocks.iter().map(|x| x.plain.as_str()).collect::<Vec<_>>(), vec!["a  b", "open"]);
        // Text outside the cells goes before the table.
        let b = parse("<table>stray<tr><td>c</td></tr></table>", None).blocks;
        assert_eq!((b[0].kind, b[0].plain.as_str(), b[1].kind), (Kind::Para, "stray", Kind::Table));
        // <a name> and an id on an inline element anchor the block they sit in.
        let b = parse("<p>x <a name=Here>y</a></p><section id=S2><p>z</p></section>", None).blocks;
        assert_eq!((b[0].anchor.as_str(), b[1].anchor.as_str()), ("here", "s2"));
        // A local picture with no base is its alt text.
        assert_eq!(parse("<img src=a.png alt=A>", None).blocks[0].plain, "A");
    }

    #[test]
    fn colspan_repeats_empty_cells() {
        let b = parse("<table><tr><th colspan=3>Head</th></tr><tr><td>a<td colspan='2'>b<tr><td colspan=0>c<td>d<td colspan=x>e</table>", None).blocks;
        let rows: Vec<Vec<&str>> = b[0].rows.iter().map(|r| r.cells.iter().map(|c| c.plain.as_str()).collect()).collect();
        assert_eq!(rows, vec![vec!["Head", "", ""], vec!["a", "b", ""], vec!["c", "d", "e"]]);
        assert_eq!(parse("<table><tr><td colspan=999>x</table>", None).blocks[0].rows[0].cells.len(), MAX_SPAN);
    }

    #[test]
    fn the_meta_charset() {
        assert_eq!(meta_charset(b"<html><meta charset=\"Windows-1252\">"), Charset::Single(CodePage::Cp1252));
        assert_eq!(meta_charset(b"<META http-equiv=Content-Type content='text/html; charset=iso-8859-1'>"), Charset::Single(CodePage::Cp1252));
        assert_eq!(meta_charset(b"<meta content=\"text/html;charset=ISO-8859-15\" http-equiv=\"content-type\">"), Charset::Single(CodePage::Iso8859_15));
        assert_eq!(meta_charset(b"<meta charset=utf-16>"), Charset::Utf8);
        assert_eq!(meta_charset(b"<meta name=viewport content=\"width=device-width\"><p>x"), Charset::Utf8);
        assert_eq!(meta_charset(b"<meta charset=shift_jis>"), Charset::Unknown("shift_jis".into()));
        // Only the first 1,024 bytes are looked at.
        let mut late = vec![b' '; 1100];
        late.extend_from_slice(b"<meta charset=windows-1252>");
        assert_eq!(meta_charset(&late), Charset::Utf8);
        // Windows-1252's 0x80 to 0x9F, Latin-1's upper half, and ISO-8859-15's euro.
        let page = b"<meta charset=windows-1252><p>\x93caf\xe9\x94 \x80 \x81\xff";
        let (text, cut, name) = decode(page);
        assert_eq!((text.ends_with("\u{201C}caf\u{e9}\u{201D} \u{20AC} \u{81}\u{ff}"), cut, name.as_str()), (true, false, "windows-1252"));
        assert_eq!(parse(&text, None).blocks[0].plain, "\u{201C}caf\u{e9}\u{201D} \u{20AC} \u{81}\u{ff}");
        assert!(decode(b"<meta charset=latin9>\xa4\xbd").0.ends_with("\u{20AC}\u{153}"));
        // A byte order mark wins over the meta; an unknown label is named.
        assert_eq!(decode(b"\xef\xbb\xbf<meta charset=windows-1252>\xc3\xa9").0, "<meta charset=windows-1252>\u{e9}");
        assert_eq!(decode(b"<meta charset=koi8-r>\xc1").2, "UTF-8 (invalid bytes replaced), charset \"koi8-r\" not known");
    }
}
