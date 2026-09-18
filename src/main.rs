use std::io::{BufWriter, Write};
use std::path::PathBuf;

use shhhh::{
    Comment, INCOMPLETE, Removal, UNREADABLE, git_operation_in_progress, remove_in_files,
    source_files_under,
};

const USAGE: &str = "\
shhhh - remove every comment in a source tree and report what it removed

USAGE:
    shhhh [OPTIONS] [PATH]...

ARGS:
    <PATH>...    Files or directories to scan (default: the current directory)

OPTIONS:
    --json       Emit JSON, one object per removed comment
    --docs-only  Remove only doc comments
    --code-only  Remove only non-doc comments
    -q, --quiet  Print nothing; use the exit status
    -h, --help   Print this message
    -V, --version

EXIT STATUS:
    0    nothing removed
    1    comments removed
    2    bad usage

SUPPORTED LANGUAGES:
    Rust, C/C++, GLSL, HLSL, WGSL, Metal, C#, Java, JS/TS, Go, Swift, Kotlin,
    Zig, Odin, Python, Shell, YAML, TOML, INI, Lua, GDScript, CMake, Make,
    XML/HTML/SVG, Batch, PowerShell, Nim and more";

struct Options {
    json: bool,
    docs_only: bool,
    code_only: bool,
    quiet: bool,
    paths: Vec<PathBuf>,
}

fn main() {
    let options = match parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => std::process::exit(0),
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let mut missing = false;
    let mut roots: Vec<PathBuf> = Vec::with_capacity(options.paths.len());
    for path in &options.paths {
        match path.canonicalize() {
            Ok(root) => roots.push(root),
            Err(problem) => {
                eprintln!("{}: {problem}", path.display());
                missing = true;
            }
        }
    }
    if missing {
        std::process::exit(2);
    }
    roots.sort();
    roots.dedup();

    for root in &roots {
        if let Some(operation) = git_operation_in_progress(root) {
            eprintln!("{}: a git {operation} is in progress, nothing was changed", root.display());
            std::process::exit(2);
        }
    }

    let mut files: Vec<_> = roots.iter().flat_map(|path| source_files_under(path)).collect();
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    let reports = remove_in_files(&files, Removal {
        docs_only: options.docs_only,
        code_only: options.code_only,
    });

    let stdout = std::io::stdout();
    let mut out = BufWriter::with_capacity(256 * 1024, stdout.lock());
    let mut reported = 0usize;
    let mut files_seen = 0usize;
    let mut last_path: Option<&str> = None;

    if options.json && !options.quiet {
        let _ = out.write_all(b"[");
    }

    for report in &reports {
        for comment in report.iter() {
            if options.docs_only && !comment.kind.is_doc() {
                continue;
            }
            if options.code_only && comment.kind.is_doc() {
                continue;
            }
            if last_path != Some(comment.path) {
                files_seen += 1;
                last_path = Some(comment.path);
            }
            if !options.quiet {
                if options.json {
                    write_json(&mut out, &comment, reported);
                } else {
                    write_human(&mut out, &comment);
                }
            }
            reported += 1;
        }
    }

    if options.json && !options.quiet {
        let _ = out.write_all(b"\n]\n");
    }
    let _ = out.flush();

    if !options.quiet && !options.json {
        eprintln!(
            "removed {reported} {} from {files_seen} {}",
            if reported == 1 { "comment" } else { "comments" },
            if files_seen == 1 { "file" } else { "files" }
        );
    }

    let order = std::sync::atomic::Ordering::Relaxed;
    if UNREADABLE.load(order) || INCOMPLETE.load(order) {
        std::process::exit(2);
    }
    std::process::exit(if reported == 0 { 0 } else { 1 });
}

fn write_human(out: &mut impl Write, comment: &Comment) {
    let (head, ellipsis) = first_line(comment.text);
    let _ = writeln!(
        out,
        "{}:{}:{}: {}: {head}{ellipsis}",
        comment.path,
        comment.line,
        comment.column,
        comment.kind.as_str()
    );
}

fn first_line(text: &str) -> (&str, &str) {
    match text.find('\n') {
        Some(at) => (&text[..at], " ..."),
        None => (text, ""),
    }
}

fn write_json(out: &mut impl Write, comment: &Comment, index: usize) {
    let _ = out.write_all(if index == 0 { b"\n  " } else { b",\n  " });
    let _ = write!(out, "{{\"path\":");
    write_quoted(out, comment.path);
    let _ = write!(
        out,
        ",\"line\":{},\"column\":{},\"kind\":\"{}\",\"text\":",
        comment.line,
        comment.column,
        comment.kind.as_str()
    );
    write_quoted(out, comment.text);
    let _ = out.write_all(b"}");
}

fn write_quoted(out: &mut impl Write, value: &str) {
    let _ = out.write_all(b"\"");
    for c in value.chars() {
        let _ = match c {
            '"' => out.write_all(b"\\\""),
            '\\' => out.write_all(b"\\\\"),
            '\n' => out.write_all(b"\\n"),
            '\r' => out.write_all(b"\\r"),
            '\t' => out.write_all(b"\\t"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32),
            c => {
                let mut buffer = [0u8; 4];
                out.write_all(c.encode_utf8(&mut buffer).as_bytes())
            }
        };
    }
    let _ = out.write_all(b"\"");
}

fn parse(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options {
        json: false,
        docs_only: false,
        code_only: false,
        quiet: false,
        paths: Vec::new(),
    };

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("shhhh {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--json" => options.json = true,
            "--docs-only" => options.docs_only = true,
            "--code-only" => options.code_only = true,
            "-q" | "--quiet" => options.quiet = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            path => options.paths.push(PathBuf::from(path)),
        }
    }

    if options.docs_only && options.code_only {
        return Err("--docs-only and --code-only are mutually exclusive".into());
    }
    if options.paths.is_empty() {
        options.paths.push(PathBuf::from("."));
    }
    Ok(Some(options))
}
