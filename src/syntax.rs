#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    BlockScalar,
    CData,
    Heredoc,
    JsRegex,
    CharOrLifetime,
    Escaped(u8),
    Literal(u8),
    Triple(u8),
    RustRaw,
    LuaLong,
}

#[derive(Clone, Copy)]
pub struct Syntax {
    pub id: u8,
    pub name: &'static str,
    pub line: &'static [&'static str],
    pub block: &'static [(&'static str, &'static str)],
    pub doc_line: &'static [&'static str],
    pub doc_block: &'static [&'static str],
    pub nested_blocks: bool,
    pub line_needs_word_start: bool,
    pub line_needs_statement_start: bool,
    pub case_insensitive_line: bool,
    pub quotes: &'static [Quote],
}

const NONE: Syntax = Syntax {
    id: u8::MAX,
    name: "",
    line: &[],
    block: &[],
    doc_line: &[],
    doc_block: &[],
    nested_blocks: false,
    line_needs_word_start: false,
    line_needs_statement_start: false,
    case_insensitive_line: false,
    quotes: &[],
};

const SLASH_STAR: &[(&str, &str)] = &[("/*", "*/")];
const C_QUOTES: &[Quote] = &[Quote::Escaped(b'"'), Quote::CharOrLifetime];

pub const RUST: Syntax = Syntax {
    id: 0,
    name: "rust",
    line: &["//"],
    block: SLASH_STAR,
    doc_line: &["///", "//!"],
    doc_block: &["/**", "/*!"],
    nested_blocks: true,
    quotes: &[Quote::RustRaw, Quote::Escaped(b'"'), Quote::CharOrLifetime],
    ..NONE
};

pub const C_LIKE: Syntax = Syntax {
    id: 1,
    name: "c-like",
    line: &["//"],
    block: SLASH_STAR,
    doc_line: &["///"],
    doc_block: &["/**"],
    quotes: C_QUOTES,
    ..NONE
};

pub const C_LIKE_NESTED: Syntax =
    Syntax { id: 2, name: "c-like-nested", nested_blocks: true, ..C_LIKE };

pub const HASH: Syntax = Syntax {
    id: 3,
    name: "hash",
    line: &["#"],
    quotes: &[Quote::Escaped(b'"'), Quote::Literal(b'\'')],
    ..NONE
};

pub const HASH_WORD: Syntax = Syntax {
    id: 4,
    name: "hash-word-start",
    line: &["#"],
    line_needs_word_start: true,
    quotes: &[Quote::Heredoc, Quote::Escaped(b'"'), Quote::Literal(b'\'')],
    ..NONE
};

pub const TOML: Syntax = Syntax {
    id: 5,
    name: "toml",
    line: &["#"],
    quotes: &[
        Quote::Triple(b'"'),
        Quote::Triple(b'\''),
        Quote::Escaped(b'"'),
        Quote::Literal(b'\''),
    ],
    ..NONE
};

pub const PYTHON: Syntax = Syntax {
    id: 6,
    name: "python",
    line: &["#"],
    quotes: &[
        Quote::Triple(b'"'),
        Quote::Triple(b'\''),
        Quote::Escaped(b'"'),
        Quote::Escaped(b'\''),
    ],
    ..NONE
};

pub const INI: Syntax = Syntax {
    id: 7,
    name: "ini",
    line: &[";", "#"],
    quotes: &[Quote::Escaped(b'"'), Quote::Literal(b'\'')],
    ..NONE
};

pub const LUA: Syntax = Syntax {
    id: 8,
    name: "lua",
    line: &["--"],
    block: &[("--[[", "]]")],
    doc_line: &["---"],
    quotes: &[Quote::LuaLong, Quote::Escaped(b'"'), Quote::Escaped(b'\'')],
    ..NONE
};

pub const XML: Syntax = Syntax {
    id: 9,
    name: "xml",
    block: &[("<!--", "-->")],
    quotes: &[Quote::CData, Quote::Escaped(b'"'), Quote::Escaped(b'\'')],
    ..NONE
};

pub const BATCH: Syntax = Syntax {
    id: 10,
    name: "batch",
    line: &["::", "rem"],
    line_needs_statement_start: true,
    case_insensitive_line: true,
    quotes: &[Quote::Escaped(b'"')],
    ..NONE
};

pub const POWERSHELL: Syntax = Syntax {
    id: 11,
    name: "powershell",
    line: &["#"],
    block: &[("<#", "#>")],
    quotes: &[Quote::Escaped(b'"'), Quote::Literal(b'\'')],
    ..NONE
};

pub const NIM: Syntax = Syntax {
    id: 12,
    name: "nim",
    line: &["#"],
    block: &[("#[", "]#")],
    nested_blocks: true,
    quotes: &[Quote::Triple(b'"'), Quote::Escaped(b'"'), Quote::CharOrLifetime],
    ..NONE
};

pub const ZIG: Syntax = Syntax {
    id: 13,
    name: "zig",
    line: &["//"],
    doc_line: &["///", "//!"],
    quotes: C_QUOTES,
    ..NONE
};

fn lowered<'a>(value: &str, buffer: &'a mut [u8; 32]) -> Option<&'a str> {
    if value.len() > buffer.len() {
        return None;
    }
    for (slot, byte) in buffer.iter_mut().zip(value.bytes()) {
        *slot = byte.to_ascii_lowercase();
    }
    std::str::from_utf8(&buffer[..value.len()]).ok()
}

pub const JS: Syntax = Syntax {
    id: 14,
    name: "js",
    line: &["//"],
    block: SLASH_STAR,
    doc_line: &["///"],
    doc_block: &["/**"],
    quotes: &[Quote::JsRegex, Quote::Escaped(b'"'), Quote::Escaped(b'\''), Quote::Escaped(b'`')],
    ..NONE
};

pub const YAML: Syntax = Syntax {
    id: 15,
    name: "yaml",
    line: &["#"],
    line_needs_word_start: true,
    quotes: &[Quote::BlockScalar, Quote::Escaped(b'"'), Quote::Literal(b'\'')],
    ..NONE
};

pub const ALL: &[Syntax] = &[
    RUST,
    C_LIKE,
    C_LIKE_NESTED,
    HASH,
    HASH_WORD,
    TOML,
    PYTHON,
    INI,
    LUA,
    XML,
    BATCH,
    POWERSHELL,
    NIM,
    ZIG,
    JS,
    YAML,
];

pub fn for_extension(extension: &str) -> Option<Syntax> {
    let mut buffer = [0u8; 32];
    let syntax = match lowered(extension, &mut buffer)? {
        "rs" => RUST,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" | "inl" | "m" | "mm" | "metal"
        | "cs" | "java" | "go" | "glsl" | "vert" | "frag" | "geom" | "tesc" | "tese" | "comp"
        | "vs" | "fs" | "hlsl" | "hlsli" | "fx" | "fxh" | "cg" | "cginc" | "shader" | "usf"
        | "ush" | "jsonc" | "json5" | "as" | "d" | "dart" | "gradle" | "groovy" | "php"
        | "proto" | "slang" => C_LIKE,
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" => JS,
        "wgsl" | "swift" | "kt" | "kts" | "scala" | "odin" | "jai" => C_LIKE_NESTED,
        "zig" => ZIG,
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "rb" | "pl" | "r" => HASH_WORD,
        "yml" | "yaml" => YAML,
        "py" | "pyi" | "pyw" => PYTHON,
        "toml" => TOML,
        "gd" | "cmake" | "mk" | "make" | "just" | "dockerfile" | "nix" | "ex" | "exs" | "jl"
        | "tf" | "gitignore" | "conf" => HASH,
        "ini" | "cfg" => INI,
        "lua" => LUA,
        "xml" | "html" | "htm" | "xhtml" | "svg" | "ui" | "vue" | "xaml" | "plist" | "resx" => XML,
        "bat" | "cmd" => BATCH,
        "ps1" | "psm1" | "psd1" => POWERSHELL,
        "nim" | "nims" => NIM,
        _ => return None,
    };
    Some(syntax)
}

pub fn for_file_name(name: &str) -> Option<Syntax> {
    let mut buffer = [0u8; 32];
    match lowered(name, &mut buffer)? {
        "makefile" | "gnumakefile" | "cmakelists.txt" | "dockerfile" | "justfile" | "rakefile"
        | "gemfile" | "brewfile" | ".gitignore" | ".dockerignore" | ".env" => Some(HASH),
        _ => None,
    }
}
