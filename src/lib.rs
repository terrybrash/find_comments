mod read;
mod scan;
mod syntax;
mod walk;

use std::path::Path;

use scan::{Scanner, scan_batch};
pub use syntax::Syntax;
pub use walk::{SourceFile, UNREADABLE, git_operation_in_progress, source_files_under};

pub static INCOMPLETE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Line,
    Block,
    DocLine,
    DocBlock,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Line => "line",
            Kind::Block => "block",
            Kind::DocLine => "doc-line",
            Kind::DocBlock => "doc-block",
        }
    }

    pub fn is_doc(self) -> bool { matches!(self, Kind::DocLine | Kind::DocBlock) }
}

#[derive(Clone, Copy)]
pub(crate) struct Span {
    pub(crate) path_at: u32,
    pub(crate) path_len: u32,
    pub(crate) text_at: u32,
    pub(crate) text_len: u32,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) source_at: u32,
    pub(crate) kind: Kind,
}

#[derive(Clone, Copy)]
pub struct Comment<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub line: u32,
    pub column: u32,
    pub offset: u32,
    pub len: u32,
    pub kind: Kind,
}

pub struct Report {
    pub(crate) arena: Vec<u8>,
    pub(crate) spans: Vec<Span>,
}

impl Report {
    pub(crate) fn with_capacity(bytes: usize, spans: usize) -> Self {
        Self { arena: Vec::with_capacity(bytes), spans: Vec::with_capacity(spans) }
    }

    pub(crate) fn intern(&mut self, bytes: &[u8]) -> (u32, u32) {
        let at = self.arena.len() as u32;
        self.arena.extend_from_slice(bytes);
        (at, bytes.len() as u32)
    }

    pub(crate) fn intern_terminated(&mut self, bytes: &[u8]) -> (u32, u32) {
        let span = self.intern(bytes);
        self.arena.push(0);
        span
    }

    pub fn len(&self) -> usize { self.spans.len() }

    pub fn is_empty(&self) -> bool { self.spans.is_empty() }

    pub fn iter(&self) -> impl Iterator<Item = Comment<'_>> {
        self.spans.iter().map(|span| Comment {
            path: self.slice(span.path_at, span.path_len),
            text: self.slice(span.text_at, span.text_len),
            line: span.line,
            column: span.column,
            offset: span.source_at,
            len: span.text_len,
            kind: span.kind,
        })
    }

    fn slice(&self, at: u32, len: u32) -> &str {
        let bytes = &self.arena[at as usize..(at + len) as usize];
        match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(bad) => std::str::from_utf8(&bytes[..bad.valid_up_to()]).unwrap_or(""),
        }
    }
}

pub fn syntax_for(path: &Path) -> Option<Syntax> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if let Some(found) = syntax::for_file_name(name) {
            return Some(found);
        }
    }
    syntax::for_extension(path.extension()?.to_str()?)
}

const OPEN_COST: usize = 8 * 1024;

fn weight(file: &SourceFile) -> usize { file.size + OPEN_COST }

fn balanced(files: &[SourceFile], workers: usize) -> Vec<&[SourceFile]> {
    let total: usize = files.iter().map(weight).sum();
    let mut chunks = Vec::with_capacity(workers);
    let mut start = 0;
    let mut carried = 0;
    let mut cut = 1;
    for (at, file) in files.iter().enumerate() {
        carried += weight(file);
        if cut < workers && carried * workers >= total * cut {
            chunks.push(&files[start..=at]);
            start = at + 1;
            cut += 1;
        }
    }
    if start < files.len() {
        chunks.push(&files[start..]);
    }
    chunks
}

#[derive(Clone, Copy)]
pub struct Removal {
    pub docs_only: bool,
    pub code_only: bool,
}

impl Removal {
    pub(crate) fn wants(&self, kind: Kind) -> bool {
        !(self.docs_only && !kind.is_doc() || self.code_only && kind.is_doc())
    }
}

pub fn remove_in_files(files: &[SourceFile], removal: Removal) -> Vec<Report> {
    batches(files, Some(removal))
}

pub fn find_in_files(files: &[SourceFile]) -> Vec<Report> { batches(files, None) }

fn batches(files: &[SourceFile], removal: Option<Removal>) -> Vec<Report> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(files.len());
    if workers < 2 {
        return vec![scan_batch(files, removal)];
    }

    std::thread::scope(|scope| {
        let handles: Vec<_> = balanced(files, workers)
            .into_iter()
            .map(|chunk| scope.spawn(move || scan_batch(chunk, removal)))
            .collect();
        handles
            .into_iter()
            .filter_map(|worker| match worker.join() {
                Ok(report) => Some(report),
                Err(_) => {
                    eprintln!("a worker stopped early; some files were not looked at");
                    INCOMPLETE.store(true, std::sync::atomic::Ordering::Relaxed);
                    None
                }
            })
            .collect()
    })
}

pub fn find_in_file(path: &Path) -> Report {
    let Some(syntax) = syntax_for(path) else {
        return Report::with_capacity(0, 0);
    };
    let size = path.metadata().map_or(0, |at| at.len() as usize);
    scan_batch(&[SourceFile { path: path.to_path_buf(), size, syntax: syntax.id }], None)
}

pub fn find_in_bytes(path: &Path, syntax: Syntax, bytes: &[u8]) -> Report {
    let mut report = Report::with_capacity(bytes.len() + path.as_os_str().len() + 64, 64);
    let interned = report.intern(path.as_os_str().as_encoded_bytes());
    Scanner::new(syntax).scan(interned, bytes, &mut report);
    report
}

fn is_word(byte: u8) -> bool { byte.is_ascii_alphanumeric() || byte == b'_' }

pub fn strip(bytes: &[u8], comments: &[(usize, usize)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    strip_into(bytes, comments, &mut out);
    out
}

pub(crate) fn strip_into(bytes: &[u8], comments: &[(usize, usize)], out: &mut Vec<u8>) {
    out.clear();
    let mut copied = 0;
    for (at, &(start, mut end)) in comments.iter().enumerate() {
        if start < copied || end < start || end > bytes.len() {
            continue;
        }
        // a run of comments separated only by blanks acts as one, so a line
        // holding nothing else disappears rather than turning blank
        for &(next, after) in &comments[at + 1..] {
            if next < end || after > bytes.len() {
                break;
            }
            if !bytes[end..next].iter().all(|byte| matches!(byte, b' ' | b'\t')) {
                break;
            }
            end = after;
        }
        let ends_line = |byte: &u8| *byte == b'\n' || *byte == b'\r';
        let line_start = bytes[..start].iter().rposition(ends_line).map_or(0, |at| at + 1);
        let line_end = bytes[end..].iter().position(ends_line).map_or(bytes.len(), |at| end + at);
        let alone = bytes[line_start..start].iter().all(u8::is_ascii_whitespace)
            && bytes[end..line_end].iter().all(u8::is_ascii_whitespace);
        if alone {
            out.extend_from_slice(&bytes[copied..line_start]);
            copied = copied.max(match bytes.get(line_end) {
                Some(b'\r') if bytes.get(line_end + 1) == Some(&b'\n') => line_end + 2,
                Some(_) => line_end + 1,
                None => line_end,
            });
            continue;
        }
        let mut cut = start;
        while cut > line_start && matches!(bytes[cut - 1], b' ' | b'\t') {
            cut -= 1;
        }
        out.extend_from_slice(&bytes[copied..cut]);
        copied = copied.max(end);
        let glued = cut > 0 && end < bytes.len() && is_word(bytes[cut - 1]) && is_word(bytes[end]);
        if glued {
            out.push(b' ');
        }
    }
    out.extend_from_slice(&bytes[copied..]);
}
