//! Syntax colours for text and source in the preview (P11-M5): TwinFM's own tokenizer, one table
//! per language (Jeremy chose it over syntect on 2026-10-23: no new crate, every language he opens,
//! `docs/phase-11-plan.md` § P11-M5). It finds five kinds of token (keywords, strings, comments,
//! numbers, types) in a code chunk, carrying a block comment or a string that is still open across
//! the chunk's end, and writes the chunk as the inline Markdown `StyledText::from_markdown` takes,
//! each token in a `<font color>` run. Pure functions only: the preview worker calls them.

use std::ops::Range;

/// What a token is, by the colour it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Keyword,
    Str,
    Comment,
    Number,
    Type,
}

/// The five colours, RGB, for a light and a dark ground (Tailwind's 600 and 400 steps, the shades
/// the theme's link colour comes from).
pub const LIGHT: [u32; 5] = [0x7C3AED, 0x15803D, 0x737373, 0xC2410C, 0x0E7490];
pub const DARK: [u32; 5] = [0xC4B5FD, 0x86EFAC, 0xA3A3A3, 0xFDBA74, 0x67E8F9];

fn colour(palette: &[u32; 5], t: Tok) -> u32 {
    palette[t as usize]
}

/// How a language's lines read.
#[derive(Debug)]
pub enum Shape {
    /// Comments, strings, keywords, numbers and types by the table's fields.
    Code,
    /// INI, `.reg`, `.inf` and the like: `[section]` a type, `key =` a keyword, `;` or `#` a comment.
    Ini,
    /// A log: levels, the time stamp, `[tags]`, quoted text and numbers.
    Log,
    /// A diff: added and removed lines, hunk heads, file heads.
    Diff,
}

/// One language's table.
#[derive(Debug)]
pub struct Lang {
    pub name: &'static str,
    pub shape: Shape,
    pub line: &'static [&'static str],
    pub block: Option<(&'static str, &'static str)>,
    /// String delimiters (open, close), whether a backslash escapes, and whether one may run past
    /// the end of a line.
    pub strings: &'static [(&'static str, &'static str, bool, bool)],
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    /// An identifier starting with a capital is a type (Rust, C#, Java, TypeScript, Slint, Go).
    pub caps_types: bool,
    /// Keywords match in any case (PowerShell, SQL, batch).
    pub any_case: bool,
    /// `$name` (and `%name%` in batch) is a variable, drawn as a type.
    pub dollar_vars: bool,
}

const C_STRINGS: &[(&str, &str, bool, bool)] = &[("\"", "\"", true, false), ("'", "'", true, false)];

macro_rules! lang {
    ($name:expr, $line:expr, $block:expr, $strings:expr, $kw:expr, $types:expr, $caps:expr) => {
        Lang { name: $name, shape: Shape::Code, line: $line, block: $block, strings: $strings, keywords: $kw, types: $types, caps_types: $caps, any_case: false, dollar_vars: false }
    };
}

pub static RUST: Lang = lang!(
    "rust",
    &["//"],
    Some(("/*", "*/")),
    &[("\"", "\"", true, true), ("'", "'", true, false)],
    &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let",
        "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "yield",
    ],
    &["bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64"],
    true
);
pub static POWERSHELL: Lang = Lang {
    name: "powershell",
    shape: Shape::Code,
    line: &["#"],
    block: Some(("<#", "#>")),
    strings: &[("@\"", "\"@", false, true), ("@'", "'@", false, true), ("\"", "\"", false, false), ("'", "'", false, false)],
    keywords: &[
        "begin", "break", "catch", "class", "continue", "data", "do", "dynamicparam", "else", "elseif", "end", "enum", "exit", "filter", "finally",
        "for", "foreach", "function", "if", "in", "param", "process", "return", "switch", "throw", "trap", "try", "until", "using", "while", "-eq",
        "-ne", "-gt", "-ge", "-lt", "-le", "-like", "-notlike", "-match", "-notmatch", "-and", "-or", "-not", "-is", "-contains", "-replace",
    ],
    types: &[],
    caps_types: false,
    any_case: true,
    dollar_vars: true,
};
pub static PYTHON: Lang = lang!(
    "python",
    &["#"],
    None,
    &[("\"\"\"", "\"\"\"", true, true), ("'''", "'''", true, true), ("\"", "\"", true, false), ("'", "'", true, false)],
    &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for",
        "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while",
        "with", "yield", "self",
    ],
    &["int", "float", "str", "bool", "list", "dict", "tuple", "set", "bytes", "object"],
    false
);
pub static JSON: Lang = lang!("json", &[], None, &[("\"", "\"", true, false)], &["true", "false", "null"], &[], false);
pub static TOML: Lang = Lang {
    name: "toml",
    shape: Shape::Ini,
    line: &["#"],
    block: None,
    strings: &[("\"\"\"", "\"\"\"", true, true), ("\"", "\"", true, false), ("'", "'", false, false)],
    keywords: &["true", "false"],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};
pub static INI: Lang = Lang {
    name: "ini",
    shape: Shape::Ini,
    line: &[";", "#"],
    block: None,
    strings: &[("\"", "\"", false, false)],
    keywords: &[],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};
pub static YAML: Lang = Lang {
    name: "yaml",
    shape: Shape::Ini,
    line: &["#"],
    block: None,
    strings: &[("\"", "\"", true, false), ("'", "'", false, false)],
    keywords: &["true", "false", "null", "yes", "no"],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};
pub static XML: Lang = lang!("xml", &[], Some(("<!--", "-->")), &[("\"", "\"", false, false), ("'", "'", false, false)], &[], &[], false);
pub static CSS: Lang = lang!(
    "css",
    &[],
    Some(("/*", "*/")),
    C_STRINGS,
    &["important", "inherit", "initial", "none", "auto", "media", "import", "keyframes", "font-face"],
    &[],
    false
);
pub static JS: Lang = lang!(
    "javascript",
    &["//"],
    Some(("/*", "*/")),
    &[("`", "`", true, true), ("\"", "\"", true, false), ("'", "'", true, false)],
    &[
        "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "enum",
        "export", "extends", "false", "finally", "for", "from", "function", "if", "implements", "import", "in", "instanceof", "interface", "let",
        "new", "null", "of", "private", "protected", "public", "readonly", "return", "static", "super", "switch", "this", "throw", "true", "try",
        "type", "typeof", "undefined", "var", "void", "while", "with", "yield",
    ],
    &["string", "number", "boolean", "any", "unknown", "never", "object", "bigint", "symbol"],
    true
);
pub static C: Lang = lang!(
    "c",
    &["//"],
    Some(("/*", "*/")),
    C_STRINGS,
    &[
        "auto", "break", "case", "catch", "class", "const", "constexpr", "continue", "default", "delete", "do", "else", "enum", "explicit",
        "extern", "false", "for", "friend", "goto", "if", "inline", "namespace", "new", "nullptr", "operator", "private", "protected", "public",
        "return", "sizeof", "static", "struct", "switch", "template", "this", "throw", "true", "try", "typedef", "typename", "union", "using",
        "virtual", "volatile", "while", "#include", "#define", "#if", "#ifdef", "#ifndef", "#else", "#endif", "#pragma",
    ],
    &["void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "bool", "size_t", "uint8_t", "uint32_t", "int32_t", "uint64_t"],
    false
);
pub static CSHARP: Lang = lang!(
    "c#",
    &["//"],
    Some(("/*", "*/")),
    &[("@\"", "\"", false, true), ("\"", "\"", true, false), ("'", "'", true, false)],
    &[
        "abstract", "as", "async", "await", "base", "break", "case", "catch", "class", "const", "continue", "default", "delegate", "do", "else",
        "enum", "event", "explicit", "extern", "false", "finally", "fixed", "for", "foreach", "get", "if", "implicit", "in", "interface", "internal",
        "is", "lock", "namespace", "new", "null", "operator", "out", "override", "params", "private", "protected", "public", "readonly", "record",
        "ref", "return", "sealed", "set", "static", "struct", "switch", "this", "throw", "true", "try", "typeof", "using", "var", "virtual",
        "void", "while", "yield",
    ],
    &["bool", "byte", "char", "decimal", "double", "float", "int", "long", "object", "sbyte", "short", "string", "uint", "ulong", "ushort"],
    true
);
pub static JAVA: Lang = lang!(
    "java",
    &["//"],
    Some(("/*", "*/")),
    C_STRINGS,
    &[
        "abstract", "assert", "break", "case", "catch", "class", "const", "continue", "default", "do", "else", "enum", "extends", "false", "final",
        "finally", "for", "if", "implements", "import", "instanceof", "interface", "native", "new", "null", "package", "private", "protected",
        "public", "return", "static", "super", "switch", "synchronized", "this", "throw", "throws", "true", "try", "var", "void", "while",
    ],
    &["boolean", "byte", "char", "double", "float", "int", "long", "short"],
    true
);
pub static GO: Lang = lang!(
    "go",
    &["//"],
    Some(("/*", "*/")),
    &[("`", "`", false, true), ("\"", "\"", true, false), ("'", "'", true, false)],
    &[
        "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "false", "for", "func", "go", "goto", "if",
        "import", "interface", "map", "nil", "package", "range", "return", "select", "struct", "switch", "true", "type", "var",
    ],
    &["bool", "byte", "error", "float32", "float64", "int", "int64", "rune", "string", "uint", "uint64", "any"],
    true
);
pub static SHELL: Lang = Lang {
    name: "shell",
    shape: Shape::Code,
    line: &["#"],
    block: None,
    strings: &[("\"", "\"", true, false), ("'", "'", false, false)],
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "function", "return", "exit", "local",
        "export", "set", "unset", "echo", "cd", "source",
    ],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: true,
};
pub static BATCH: Lang = Lang {
    name: "batch",
    shape: Shape::Code,
    line: &["rem ", "::"],
    block: None,
    strings: &[("\"", "\"", false, false)],
    keywords: &[
        "if", "else", "for", "in", "do", "goto", "call", "exit", "set", "setlocal", "endlocal", "echo", "not", "exist", "defined", "errorlevel",
        "equ", "neq", "lss", "leq", "gtr", "geq", "shift", "pause", "start",
    ],
    types: &[],
    caps_types: false,
    any_case: true,
    dollar_vars: true,
};
pub static SLINT: Lang = lang!(
    "slint",
    &["//"],
    Some(("/*", "*/")),
    &[("\"", "\"", true, false)],
    &[
        "import", "from", "export", "component", "inherits", "global", "struct", "enum", "property", "in", "out", "in-out", "private", "callback",
        "function", "public", "pure", "if", "else", "for", "return", "animate", "states", "transitions", "changed", "true", "false", "root",
        "parent", "self",
    ],
    &["int", "float", "bool", "string", "length", "duration", "color", "brush", "image", "percent", "angle", "relative-font-size"],
    true
);
pub static SQL: Lang = Lang {
    name: "sql",
    shape: Shape::Code,
    line: &["--"],
    block: Some(("/*", "*/")),
    strings: &[("'", "'", false, false), ("\"", "\"", false, false)],
    keywords: &[
        "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "index", "join",
        "left", "right", "inner", "outer", "on", "and", "or", "not", "null", "is", "as", "order", "by", "group", "having", "limit", "distinct",
        "union", "all", "primary", "key", "foreign", "references", "default", "begin", "commit", "rollback", "case", "when", "then", "else", "end",
    ],
    types: &["integer", "int", "text", "real", "blob", "varchar", "char", "boolean", "date", "timestamp", "numeric"],
    caps_types: false,
    any_case: true,
    dollar_vars: false,
};
pub static LOG: Lang = Lang {
    name: "log",
    shape: Shape::Log,
    line: &[],
    block: None,
    strings: &[("\"", "\"", false, false)],
    keywords: &[],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};
pub static DIFF: Lang = Lang {
    name: "diff",
    shape: Shape::Diff,
    line: &[],
    block: None,
    strings: &[],
    keywords: &[],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};

/// The language of a file by its extension (any case); None: plain text (`.txt`, `.csv` and the
/// rest), drawn uncoloured.
pub fn lang_of(ext: &str) -> Option<&'static Lang> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => &RUST,
        "ps1" | "psm1" | "psd1" => &POWERSHELL,
        "py" => &PYTHON,
        "json" => &JSON,
        "toml" | "lock" => &TOML,
        "ini" | "cfg" | "conf" | "reg" | "inf" => &INI,
        "yaml" | "yml" => &YAML,
        "xml" => &XML,
        "css" => &CSS,
        "js" | "jsx" | "ts" | "tsx" => &JS,
        "c" | "h" | "cpp" | "hpp" | "cc" => &C,
        "cs" => &CSHARP,
        "java" => &JAVA,
        "go" => &GO,
        "sh" => &SHELL,
        "bat" | "cmd" => &BATCH,
        "slint" => &SLINT,
        "sql" => &SQL,
        "log" => &LOG,
        "diff" | "patch" => &DIFF,
        _ => return None,
    })
}

/// What is still open at a chunk's end: a block comment, or a string (its index in `strings`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub comment: bool,
    pub string: Option<usize>,
}

fn starts(s: &str, at: usize, pat: &str, any_case: bool) -> bool {
    let b = s.as_bytes();
    let p = pat.as_bytes();
    at + p.len() <= b.len() && if any_case { b[at..at + p.len()].eq_ignore_ascii_case(p) } else { &b[at..at + p.len()] == p }
}

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The end of a string opened before `from` (delimiter `k`): the byte after its close, or the end
/// of the line (`multiline` false) or text (true) when it does not close.
fn string_end(lang: &Lang, text: &str, from: usize, k: usize) -> (usize, bool) {
    let (_, close, escape, multiline) = lang.strings[k];
    let b = text.as_bytes();
    let mut i = from;
    while i < b.len() {
        if escape && b[i] == b'\\' {
            i += 2;
            continue;
        }
        if !multiline && b[i] == b'\n' {
            return (i, true);
        }
        if starts(text, i, close, false) {
            return (i + close.len(), true);
        }
        i += 1;
    }
    (b.len(), false)
}

/// The tokens of `text` (a chunk, lines separated by `\n`) as byte ranges, in order and apart,
/// starting in `state` and leaving it as the chunk ends.
pub fn tokens(lang: &Lang, text: &str, state: &mut State) -> Vec<(Range<usize>, Tok)> {
    match lang.shape {
        Shape::Code => code_tokens(lang, text, state),
        Shape::Ini => ini_tokens(lang, text, state),
        Shape::Log => text.split_inclusive('\n').scan(0, |at, l| Some((std::mem::replace(at, *at + l.len()), l))).flat_map(|(at, l)| log_line(l, at)).collect(),
        Shape::Diff => diff_tokens(text),
    }
}

fn code_tokens(lang: &Lang, text: &str, state: &mut State) -> Vec<(Range<usize>, Tok)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    if state.comment {
        let (_, close) = lang.block.expect("a block comment open in a language with none");
        let end = text.find(close).map_or(b.len(), |j| j + close.len());
        state.comment = end == b.len() && !text.ends_with(close);
        out.push((0..end, Tok::Comment));
        i = end;
    } else if let Some(k) = state.string {
        let (end, closed) = string_end(lang, text, 0, k);
        state.string = (!closed).then_some(k);
        out.push((0..end, Tok::Str));
        i = end;
    }
    let mut line_start = i == 0 || b.get(i.wrapping_sub(1)) == Some(&b'\n');
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if c == b' ' {
            i += 1;
            continue;
        }
        let at_start = line_start;
        line_start = false;
        // A line comment (batch's `rem ` only at a line's start).
        if lang.line.iter().any(|p| starts(text, i, p, lang.any_case) && (!p.starts_with("rem") || at_start)) {
            let end = text[i..].find('\n').map_or(b.len(), |j| i + j);
            out.push((i..end, Tok::Comment));
            i = end;
            continue;
        }
        if let Some((open, close)) = lang.block {
            if starts(text, i, open, false) {
                match text[i + open.len()..].find(close) {
                    Some(j) => {
                        let end = i + open.len() + j + close.len();
                        out.push((i..end, Tok::Comment));
                        i = end;
                    }
                    None => {
                        out.push((i..b.len(), Tok::Comment));
                        state.comment = true;
                        i = b.len();
                    }
                }
                continue;
            }
        }
        if let Some(k) = lang.strings.iter().position(|s| starts(text, i, s.0, false)) {
            // A Rust lifetime ('a) is not a char literal: a quote, a name, and no closing quote.
            let lifetime = lang.name == "rust" && c == b'\'' && b.get(i + 2).is_some_and(|&n| n != b'\'') && b.get(i + 1).is_some_and(|&n| n.is_ascii_alphabetic());
            if !lifetime {
                let (end, closed) = string_end(lang, text, i + lang.strings[k].0.len(), k);
                if !closed && lang.strings[k].3 {
                    state.string = Some(k);
                }
                out.push((i..end, Tok::Str));
                i = end;
                continue;
            }
        }
        if lang.dollar_vars && (c == b'$' || (lang.name == "batch" && c == b'%')) {
            let mut j = i + 1;
            while j < b.len() && (ident_byte(b[j]) || b[j] == b':' || b[j] == b'{' || b[j] == b'}') {
                j += 1;
            }
            if lang.name == "batch" && c == b'%' && b.get(j) == Some(&b'%') {
                j += 1;
            }
            if j > i + 1 {
                out.push((i..j, Tok::Type));
                i = j;
                continue;
            }
        }
        if c.is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && (ident_byte(b[j]) || (b[j] == b'.' && b.get(j + 1).is_some_and(u8::is_ascii_digit))) {
                j += 1;
            }
            out.push((i..j, Tok::Number));
            i = j;
            continue;
        }
        // A word: letters, digits, `_`, and `-` inside one for the languages whose names have it
        // (PowerShell's `-eq`, CSS's properties, Slint's `in-out`); `#` before one for C's directives.
        let dash_words = matches!(lang.name, "powershell" | "css" | "slint");
        if ident_byte(c) || (c == b'#' && lang.name == "c") || (c == b'-' && lang.name == "powershell" && b.get(i + 1).is_some_and(u8::is_ascii_alphabetic)) {
            let mut j = i + 1;
            while j < b.len() && (ident_byte(b[j]) || (dash_words && b[j] == b'-' && b.get(j + 1).is_some_and(|&n| ident_byte(n)))) {
                j += 1;
            }
            let w = &text[i..j];
            let kw = if lang.any_case { lang.keywords.iter().any(|k| k.eq_ignore_ascii_case(w)) } else { lang.keywords.contains(&w) };
            let ty = lang.types.contains(&w) || (lang.caps_types && w.as_bytes()[0].is_ascii_uppercase());
            if kw {
                out.push((i..j, Tok::Keyword));
            } else if ty {
                out.push((i..j, Tok::Type));
            }
            i = j;
            continue;
        }
        // Anything else, one character (never inside a multi-byte one).
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// INI, TOML and YAML: per line a `[section]` head (a type), a key before `=` or `:` (a keyword),
/// then the value's strings, numbers, keywords (`true`, `false`) and a trailing comment.
fn ini_tokens(lang: &Lang, text: &str, state: &mut State) -> Vec<(Range<usize>, Tok)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let base = at;
        at += line.len();
        let body = line.trim_end_matches('\n');
        // A TOML multi-line string carried from the line before: the line up to its close is string
        // (and the rest of a closing line is left plain).
        if let Some(k) = state.string {
            let close = lang.strings[k].1;
            let end = body.find(close).map_or(body.len(), |j| {
                state.string = None;
                j + close.len()
            });
            if end > 0 {
                out.push((base..base + end, Tok::Str));
            }
            continue;
        }
        let lead = body.len() - body.trim_start().len();
        let t = body.trim_start();
        if lang.line.iter().any(|p| t.starts_with(p)) {
            out.push((base + lead..base + body.len(), Tok::Comment));
            continue;
        }
        if t.starts_with('[') {
            out.push((base + lead..base + body.len(), Tok::Type));
            continue;
        }
        let sep = if lang.name == "yaml" { t.find(": ").or_else(|| t.ends_with(':').then(|| t.len() - 1)) } else { t.find('=') };
        let value_from = match sep {
            Some(j) if j > 0 && !t.starts_with(['"', '\'', '-']) => {
                out.push((base + lead..base + lead + t[..j].trim_end().len(), Tok::Keyword));
                lead + j + 1
            }
            _ => lead,
        };
        let value = &line[value_from..body.len()];
        let mut s = State::default();
        let sub = if lang.name == "ini" { &INI_VALUE } else { lang };
        for (r, k) in code_tokens(sub, value, &mut s) {
            out.push((base + value_from + r.start..base + value_from + r.end, k));
        }
        if let Some(k) = s.string {
            state.string = Some(k);
        }
    }
    out
}

/// An INI value: its quoted text and numbers only.
static INI_VALUE: Lang = Lang {
    name: "ini-value",
    shape: Shape::Code,
    line: &[],
    block: None,
    strings: &[("\"", "\"", false, false)],
    keywords: &[],
    types: &[],
    caps_types: false,
    any_case: false,
    dollar_vars: false,
};

const LOG_ERRORS: &[&str] = &["ERROR", "ERR", "FATAL", "CRITICAL", "PANIC", "PANICKED", "FAIL", "FAILED", "FAILURE", "EXCEPTION"];
const LOG_WARNINGS: &[&str] = &["WARN", "WARNING"];
const LOG_QUIET: &[&str] = &["INFO", "DEBUG", "TRACE", "VERBOSE", "NOTICE"];

/// One log line (`at` its offset): a leading time stamp as a comment, levels (errors as keywords,
/// warnings as numbers, the quiet ones as comments; any case), `[tags]` as types, quoted text as
/// strings, numbers.
fn log_line(line: &str, at: usize) -> Vec<(Range<usize>, Tok)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let stamp = b.iter().take_while(|&&c| c.is_ascii_digit() || b"-:./TZ+, ".contains(&c)).count();
    let stamp = line[..stamp].trim_end().len();
    let mut i = 0;
    if stamp >= 8 && b[..stamp].iter().filter(|c| c.is_ascii_digit()).count() >= 6 {
        out.push((at..at + stamp, Tok::Comment));
        i = stamp;
    }
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let end = line[i + 1..].find('"').map_or(line.trim_end_matches('\n').len(), |j| i + j + 2);
            out.push((at + i..at + end, Tok::Str));
            i = end;
        } else if c == b'[' {
            match line[i..].find(']') {
                Some(j) if j < 40 && !line[i + 1..i + j].contains(' ') => {
                    out.push((at + i..at + i + j + 1, Tok::Type));
                    i += j + 1;
                }
                _ => i += 1,
            }
        } else if c.is_ascii_digit() && (i == 0 || !ident_byte(b[i - 1])) {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_digit() || (b[j] == b'.' && b.get(j + 1).is_some_and(u8::is_ascii_digit))) {
                j += 1;
            }
            if j < b.len() && ident_byte(b[j]) && !b[j].is_ascii_digit() {
                // Part of a word (`0x1f`, `abc123` was skipped above): leave it.
                while j < b.len() && ident_byte(b[j]) {
                    j += 1;
                }
            } else {
                out.push((at + i..at + j, Tok::Number));
            }
            i = j;
        } else if c.is_ascii_alphabetic() && (i == 0 || !ident_byte(b[i - 1])) {
            let mut j = i + 1;
            while j < b.len() && ident_byte(b[j]) {
                j += 1;
            }
            let w = &line[i..j];
            let is = |list: &[&str]| list.iter().any(|x| x.eq_ignore_ascii_case(w));
            if is(LOG_ERRORS) {
                out.push((at + i..at + j, Tok::Keyword));
            } else if is(LOG_WARNINGS) {
                out.push((at + i..at + j, Tok::Number));
            } else if is(LOG_QUIET) {
                out.push((at + i..at + j, Tok::Comment));
            }
            i = j;
        } else {
            i += line[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// A diff: `+` lines strings, `-` lines numbers, `@@` hunk heads types, file heads keywords.
fn diff_tokens(text: &str) -> Vec<(Range<usize>, Tok)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let kind = if body.starts_with("+++") || body.starts_with("---") || body.starts_with("diff ") || body.starts_with("index ") {
            Some(Tok::Keyword)
        } else if body.starts_with("@@") {
            Some(Tok::Type)
        } else if body.starts_with('+') {
            Some(Tok::Str)
        } else if body.starts_with('-') {
            Some(Tok::Number)
        } else {
            None
        };
        if let (Some(k), false) = (kind, body.is_empty()) {
            out.push((at..at + body.len(), k));
        }
        at += line.len();
    }
    out
}

/// Slint's markup placeholder, never written (as `markdown::Inline::text` drops it).
const PLACEHOLDER: char = '\u{E541}';

/// Text into the markup: ASCII punctuation escaped, spaces at a line's start made no-break spaces
/// (Markdown drops them), so the indentation stays.
fn escape(out: &mut String, s: &str, line_start: &mut bool) {
    for c in s.chars() {
        match c {
            PLACEHOLDER => {}
            ' ' if *line_start => out.push('\u{A0}'),
            c => {
                *line_start = false;
                if c.is_ascii_punctuation() {
                    out.push('\\');
                }
                out.push(c);
            }
        }
    }
}

/// `text` (a chunk) as inline Markdown for `StyledText::from_markdown`: each token in a `<font
/// color>` run of `palette`, every line a hard break, an empty line a no-break space so it keeps
/// its height. A token running over a line's end is closed and opened again around the break.
pub fn markup(text: &str, runs: &[(Range<usize>, Tok)], palette: &[u32; 5]) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut pos = 0;
    let mut line_start = true;
    let piece = |out: &mut String, s: &str, tok: Option<Tok>, line_start: &mut bool| {
        for (n, part) in s.split('\n').enumerate() {
            if n > 0 {
                if *line_start {
                    out.push('\u{A0}');
                }
                out.push_str("\\\n");
                *line_start = true;
            }
            if part.is_empty() {
                continue;
            }
            match tok {
                Some(t) => {
                    out.push_str(&format!("<font color=\"#{:06x}\">", colour(palette, t)));
                    escape(out, part, line_start);
                    out.push_str("</font>");
                }
                None => escape(out, part, line_start),
            }
        }
    };
    for (r, t) in runs {
        if r.start > pos {
            piece(&mut out, &text[pos..r.start], None, &mut line_start);
        }
        piece(&mut out, &text[r.start.max(pos)..r.end], Some(*t), &mut line_start);
        pos = pos.max(r.end);
    }
    if pos < text.len() {
        piece(&mut out, &text[pos..], None, &mut line_start);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tokens of `text` as (text, kind) pairs.
    fn toks<'a>(lang: &Lang, text: &'a str) -> Vec<(&'a str, Tok)> {
        tokens(lang, text, &mut State::default()).into_iter().map(|(r, t)| (&text[r], t)).collect()
    }

    /// The markup without its font tags and escapes.
    fn regex_free_strip(md: &str) -> String {
        let mut out = String::new();
        let mut rest = md;
        while let Some(c) = rest.chars().next() {
            if rest.starts_with("<font") || rest.starts_with("</font") {
                rest = &rest[rest.find('>').unwrap() + 1..];
            } else if c == '\\' {
                let n = rest[1..].chars().next().unwrap();
                out.push(n);
                rest = &rest[1 + n.len_utf8()..];
            } else {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        out
    }

    fn of(lang: &Lang, text: &str, kind: Tok) -> Vec<String> {
        toks(lang, text).into_iter().filter(|(_, k)| *k == kind).map(|(s, _)| s.to_string()).collect()
    }

    #[test]
    fn every_language_on_a_line() {
        use Tok::*;
        assert_eq!(
            toks(&RUST, "pub fn f<'a>(x: &'a str) -> Vec<u8> { let c = 'x'; 0x1F // done\n}"),
            vec![("pub", Keyword), ("fn", Keyword), ("str", Type), ("Vec", Type), ("u8", Type), ("let", Keyword), ("'x'", Str), ("0x1F", Number), ("// done", Comment)]
        );
        assert_eq!(of(&RUST, "let s = \"a \\\" b\";", Str), vec!["\"a \\\" b\""]);
        assert_eq!(
            toks(&POWERSHELL, "if ($x -EQ 5) { Write-Host 'hi' } # note"),
            vec![("if", Keyword), ("$x", Type), ("-EQ", Keyword), ("5", Number), ("'hi'", Str), ("# note", Comment)]
        );
        assert_eq!(toks(&PYTHON, "def f(): return None  # x"), vec![("def", Keyword), ("return", Keyword), ("None", Keyword), ("# x", Comment)]);
        assert_eq!(toks(&JSON, "{\"a\": [1.5, true, null]}"), vec![("\"a\"", Str), ("1.5", Number), ("true", Keyword), ("null", Keyword)]);
        assert_eq!(
            toks(&TOML, "[package]\nname = \"twinfm\" # the crate\nedition = 2021\n"),
            vec![("[package]", Type), ("name", Keyword), ("\"twinfm\"", Str), ("# the crate", Comment), ("edition", Keyword), ("2021", Number)]
        );
        assert_eq!(toks(&YAML, "key: 'v'\n# c\n- item: 3\n"), vec![("key", Keyword), ("'v'", Str), ("# c", Comment), ("3", Number)]);
        assert_eq!(toks(&INI, "; c\n[Sec]\nk=\"v\"\n"), vec![("; c", Comment), ("[Sec]", Type), ("k", Keyword), ("\"v\"", Str)]);
        assert_eq!(toks(&XML, "<a b=\"1\"><!-- c --></a>"), vec![("\"1\"", Str), ("<!-- c -->", Comment)]);
        assert_eq!(toks(&CSS, ".a { color: red !important; } /* c */"), vec![("important", Keyword), ("/* c */", Comment)]);
        assert_eq!(
            toks(&JS, "const t: string = `a ${b}`; // c"),
            vec![("const", Keyword), ("string", Type), ("`a ${b}`", Str), ("// c", Comment)]
        );
        assert_eq!(toks(&C, "#include <x.h>\nint main() { return 0; }"), vec![("#include", Keyword), ("int", Type), ("return", Keyword), ("0", Number)]);
        assert_eq!(toks(&CSHARP, "var s = @\"C:\\x\"; string t;"), vec![("var", Keyword), ("@\"C:\\x\"", Str), ("string", Type)]);
        assert_eq!(toks(&JAVA, "public class A { int x = 2; }"), vec![("public", Keyword), ("class", Keyword), ("A", Type), ("int", Type), ("2", Number)]);
        assert_eq!(toks(&GO, "func f() error { return nil }"), vec![("func", Keyword), ("error", Type), ("return", Keyword), ("nil", Keyword)]);
        assert_eq!(toks(&SHELL, "if [ -n \"$X\" ]; then echo $HOME; fi # c"), vec![("if", Keyword), ("\"$X\"", Str), ("then", Keyword), ("echo", Keyword), ("$HOME", Type), ("fi", Keyword), ("# c", Comment)]);
        assert_eq!(toks(&BATCH, "REM c\n:: c2\nIF EXIST %F% echo \"x\""), vec![("REM c", Comment), (":: c2", Comment), ("IF", Keyword), ("EXIST", Keyword), ("%F%", Type), ("echo", Keyword), ("\"x\"", Str)]);
        assert_eq!(
            toks(&SLINT, "in-out property <length> w: 10px; // c"),
            vec![("in-out", Keyword), ("property", Keyword), ("length", Type), ("10px", Number), ("// c", Comment)]
        );
        assert_eq!(toks(&SQL, "SELECT a FROM t WHERE b = 'x' -- c"), vec![("SELECT", Keyword), ("FROM", Keyword), ("WHERE", Keyword), ("'x'", Str), ("-- c", Comment)]);
        assert_eq!(
            toks(&LOG, "2026-10-23 08:15:01.123 ERROR [enum] \"C:\\x\" took 42 ms\nwarning: 3 left, info\n"),
            vec![("2026-10-23 08:15:01.123", Comment), ("ERROR", Keyword), ("[enum]", Type), ("\"C:\\x\"", Str), ("42", Number), ("warning", Number), ("3", Number), ("info", Comment)]
        );
        assert_eq!(toks(&DIFF, "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same\n"), vec![("--- a", Keyword), ("+++ b", Keyword), ("@@ -1 +1 @@", Type), ("-old", Number), ("+new", Str)]);
        assert!(lang_of("RS").is_some() && lang_of("txt").is_none() && lang_of("csv").is_none());
    }

    #[test]
    fn a_chunk_boundary_inside_a_comment_or_a_string() {
        let mut st = State::default();
        let a = "fn a() {} /* starts\nhere";
        let t = tokens(&RUST, a, &mut st);
        assert_eq!((&a[t.last().unwrap().0.clone()], st), ("/* starts\nhere", State { comment: true, string: None }));
        let b = "still */ let x";
        let t = tokens(&RUST, b, &mut st);
        assert_eq!(t.iter().map(|(r, k)| (&b[r.clone()], *k)).collect::<Vec<_>>(), vec![("still */", Tok::Comment), ("let", Tok::Keyword)]);
        assert_eq!(st, State::default());
        // A Python triple-quoted string across two chunks, and a JS template string.
        let mut st = State::default();
        tokens(&PYTHON, "x = \"\"\"doc\nmore", &mut st);
        assert_eq!(st.string, Some(0));
        let c = "end\"\"\" + 1";
        let t = tokens(&PYTHON, c, &mut st);
        assert_eq!(t.iter().map(|(r, k)| (&c[r.clone()], *k)).collect::<Vec<_>>(), vec![("end\"\"\"", Tok::Str), ("1", Tok::Number)]);
        let mut st = State::default();
        tokens(&JS, "a = `x", &mut st);
        assert_eq!(st.string, Some(0));
        // A one-line string never carries.
        let mut st = State::default();
        tokens(&C, "char *s = \"open", &mut st);
        assert_eq!(st, State::default());
        // A PowerShell block comment and here-string.
        let mut st = State::default();
        tokens(&POWERSHELL, "<# help", &mut st);
        assert!(st.comment);
        let mut st = State::default();
        tokens(&POWERSHELL, "$s = @\"\nbody", &mut st);
        assert_eq!(st.string, Some(0));
        // TOML's multi-line string.
        let mut st = State::default();
        let t = "s = \"\"\"\nline\n\"\"\"\nk = 1\n";
        assert_eq!(toks(&TOML, t).last(), Some(&("1", Tok::Number)));
        tokens(&TOML, "s = \"\"\"\nline", &mut st);
        assert!(st.string.is_some());
    }

    #[test]
    fn markup_reads_back_as_the_text() {
        use slint::StyledText;
        let text = "fn main() {\n    let x = \"*not* bold\"; // 1. # [x](y) <b>\n\n}";
        let mut st = State::default();
        let runs = tokens(&RUST, text, &mut st);
        let md = markup(text, &runs, &LIGHT);
        StyledText::from_markdown(&md).unwrap_or_else(|e| panic!("{md}: {e}"));
        // The text comes back whole (the indentation as no-break spaces), and Slint reads it as
        // the same runs a hand-written markup of the expected text gives.
        let unmarked = md.replace("\\\n", "\n").replace('\u{A0}', " ");
        let unmarked = regex_free_strip(&unmarked);
        assert_eq!(unmarked, text.replace("\n\n", "\n \n"));
        let ours = StyledText::from_markdown(&md).unwrap();
        let theirs = StyledText::from_markdown(&format!(
            "<font color=\"#7c3aed\">fn</font> main\\(\\) \\{{\\\n\u{A0}\u{A0}\u{A0}\u{A0}<font color=\"#7c3aed\">let</font> x \\= \
             <font color=\"#15803d\">\\\"\\*not\\* bold\\\"</font>\\; <font color=\"#737373\">\\/\\/ 1\\. \\# \\[x\\]\\(y\\) \\<b\\></font>\\\n\u{A0}\\\n\\}}"
        ))
        .unwrap();
        assert_eq!(ours, theirs);
        assert!(md.contains("<font color=\"#7c3aed\">fn</font>"), "{md}");
        assert!(md.contains("<font color=\"#737373\">\\/\\/ 1\\. \\# \\[x\\]\\(y\\) \\<b\\></font>"), "{md}");
        // A comment over a line's end is closed and opened again around the break.
        let text = "/* a\nb */";
        let md = markup(text, &tokens(&C, text, &mut State::default()), &DARK);
        assert_eq!(md, "<font color=\"#a3a3a3\">\\/\\* a</font>\\\n<font color=\"#a3a3a3\">b \\*\\/</font>");
        StyledText::from_markdown(&md).unwrap();
    }
}

