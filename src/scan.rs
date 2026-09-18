use crate::read::{SMALL_FILE, with_file};
use crate::syntax::{self, Quote, Syntax};
use crate::walk::SourceFile;
use crate::{Kind, Removal, Report, Span, strip_into};

const MAX_MARKERS: usize = 8;

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn contains_str(haystack: &[&str], needle: &str) -> bool {
    let mut i = 0;
    while i < haystack.len() {
        if str_eq(haystack[i], needle) {
            return true;
        }
        i += 1;
    }
    false
}

const fn marker_count(syntax: &Syntax) -> usize {
    syntax.block.len() + syntax.doc_block.len() + syntax.doc_line.len() + syntax.line.len()
}

const _: () = {
    let mut i = 0;
    while i < syntax::ALL.len() {
        assert!(marker_count(&syntax::ALL[i]) <= MAX_MARKERS);
        i += 1;
    }
};

#[derive(Clone, Copy)]
pub(crate) struct Markers {
    items: [Marker; MAX_MARKERS],
    len: usize,
}

impl Markers {
    const fn push(&mut self, marker: Marker) {
        self.items[self.len] = marker;
        self.len += 1;
    }

    pub(crate) fn as_slice(&self) -> &[Marker] { &self.items[..self.len] }
}

#[derive(Clone, Copy)]
pub(crate) struct Marker {
    token: &'static str,
    kind: Kind,
    close: Option<&'static str>,
}

const fn markers(syntax: &Syntax) -> Markers {
    let empty = Marker { token: "", kind: Kind::Line, close: None };
    let mut all = Markers { items: [empty; MAX_MARKERS], len: 0 };

    let mut i = 0;
    while i < syntax.block.len() {
        let (open, close) = syntax.block[i];
        let kind = if contains_str(syntax.doc_block, open) { Kind::DocBlock } else { Kind::Block };
        all.push(Marker { token: open, kind, close: Some(close) });
        i += 1;
    }
    let close = if syntax.block.is_empty() { "*/" } else { syntax.block[0].1 };
    let mut i = 0;
    while i < syntax.doc_block.len() {
        all.push(Marker { token: syntax.doc_block[i], kind: Kind::DocBlock, close: Some(close) });
        i += 1;
    }
    let mut i = 0;
    while i < syntax.doc_line.len() {
        all.push(Marker { token: syntax.doc_line[i], kind: Kind::DocLine, close: None });
        i += 1;
    }
    let mut i = 0;
    while i < syntax.line.len() {
        all.push(Marker { token: syntax.line[i], kind: Kind::Line, close: None });
        i += 1;
    }

    let mut i = 1;
    while i < all.len {
        let mut j = i;
        while j > 0 && all.items[j - 1].token.len() < all.items[j].token.len() {
            all.items.swap(j - 1, j);
            j -= 1;
        }
        i += 1;
    }
    all
}

struct Skipped {
    at: usize,
    newlines: usize,
    last_newline: Option<usize>,
}

#[derive(Clone, Copy)]
struct Skipper {
    table: [bool; 256],
}

impl Skipper {
    const fn new(syntax: &Syntax, markers: &Markers) -> Self {
        let mut table = [false; 256];
        let mut i = 0;
        while i < markers.len {
            let first = markers.items[i].token.as_bytes()[0];
            table[first as usize] = true;
            table[first.to_ascii_uppercase() as usize] = true;
            i += 1;
        }
        let mut i = 0;
        while i < syntax.quotes.len() {
            match syntax.quotes[i] {
                Quote::Escaped(q) | Quote::Literal(q) | Quote::Triple(q) =>
                    table[q as usize] = true,
                Quote::RustRaw => table[b'r' as usize] = true,
                Quote::LuaLong => table[b'[' as usize] = true,
                Quote::CharOrLifetime => table[b'\'' as usize] = true,
                Quote::CData => table[b'<' as usize] = true,
                Quote::Heredoc => table[b'<' as usize] = true,
                Quote::JsRegex => table[b'/' as usize] = true,
                Quote::BlockScalar => {
                    table[b'|' as usize] = true;
                    table[b'>' as usize] = true;
                }
            }
            i += 1;
        }
        Self { table }
    }

    fn skip(&self, bytes: &[u8], from: usize) -> Skipped {
        let mut at = from;
        let mut newlines = 0;
        let mut last_newline = None;
        while at < bytes.len() && !self.table[bytes[at] as usize] {
            if breaks_line(bytes, at) {
                newlines += 1;
                last_newline = Some(at);
            }
            at += 1;
        }
        Skipped { at, newlines, last_newline }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Scanner {
    syntax: Syntax,
    markers: Markers,
    skipper: Skipper,
}

impl Scanner {
    pub(crate) const fn new(syntax: Syntax) -> Self {
        let markers = markers(&syntax);
        let skipper = Skipper::new(&syntax, &markers);
        Self { syntax, markers, skipper }
    }

    pub(crate) fn scan(&self, path: (u32, u32), bytes: &[u8], report: &mut Report) -> bool {
        let syntax = self.syntax;
        let markers = self.markers.as_slice();
        let skipper = &self.skipper;
        let mut at = 0;
        let mut line = 1;
        let mut line_start = 0;

        if bytes.starts_with(b"#!") && matching_marker(bytes, 0, 0, &syntax, markers).is_some() {
            at = end_of_line(bytes, 0);
        }

        while at < bytes.len() {
            if !skipper.table[bytes[at] as usize] {
                let skipped = skipper.skip(bytes, at);
                line += skipped.newlines;
                if let Some(offset) = skipped.last_newline {
                    line_start = offset + 1;
                }
                at = skipped.at;
                if at >= bytes.len() {
                    break;
                }
            }

            if let Some(marker) = matching_marker(bytes, at, line_start, &syntax, markers) {
                let end = match marker.close {
                    None => end_of_line(bytes, at),
                    Some(close) => {
                        let open = syntax.block.first().map_or(marker.token, |(open, _)| *open);
                        match end_of_block(bytes, at, open, close, syntax.nested_blocks) {
                            Some(end) => end,
                            None => return false,
                        }
                    }
                };
                let mut text_end = end;
                while text_end > at && bytes[text_end - 1].is_ascii_whitespace() {
                    text_end -= 1;
                }
                let text = report.intern(&bytes[at..text_end]);
                report.spans.push(Span {
                    path_at: path.0,
                    path_len: path.1,
                    text_at: text.0,
                    text_len: text.1,
                    line: line as u32,
                    column: (at - line_start + 1) as u32,
                    source_at: at as u32,
                    kind: marker.kind,
                });
                for offset in at..end.min(bytes.len()) {
                    if breaks_line(bytes, offset) {
                        line += 1;
                        line_start = offset + 1;
                    }
                }
                at = end;
                continue;
            }

            if let Some(end) = matching_quote(bytes, at, line_start, &syntax) {
                for offset in at..end.min(bytes.len()) {
                    if breaks_line(bytes, offset) {
                        line += 1;
                        line_start = offset + 1;
                    }
                }
                at = end;
                continue;
            }

            at += 1;
        }
        true
    }
}

fn matching_marker<'m>(
    bytes: &[u8],
    at: usize,
    line_start: usize,
    syntax: &Syntax,
    markers: &'m [Marker],
) -> Option<&'m Marker> {
    markers.iter().find(|marker| {
        let token = marker.token.as_bytes();
        let end = at + token.len();
        if end > bytes.len() {
            return false;
        }
        let matches = if syntax.case_insensitive_line {
            bytes[at..end].eq_ignore_ascii_case(token)
        } else {
            &bytes[at..end] == token
        };
        matches && boundary_ok(bytes, at, line_start, syntax, marker)
    })
}

fn boundary_ok(
    bytes: &[u8],
    at: usize,
    line_start: usize,
    syntax: &Syntax,
    marker: &Marker,
) -> bool {
    if marker.close.is_some() {
        return true;
    }
    if syntax.line_needs_statement_start
        && !bytes[line_start..at].iter().all(u8::is_ascii_whitespace)
    {
        return false;
    }
    if syntax.line_needs_word_start && at != line_start && !bytes[at - 1].is_ascii_whitespace() {
        return false;
    }
    if marker.token.as_bytes()[0].is_ascii_alphabetic() {
        if let Some(after) = bytes.get(at + marker.token.len()) {
            if after.is_ascii_alphanumeric() || *after == b'_' {
                return false;
            }
        }
    }
    true
}

fn matching_quote(bytes: &[u8], at: usize, line_start: usize, syntax: &Syntax) -> Option<usize> {
    for quote in syntax.quotes {
        match *quote {
            Quote::Triple(q) => {
                if bytes[at] == q && bytes.get(at + 1) == Some(&q) && bytes.get(at + 2) == Some(&q)
                {
                    return Some(end_of_triple(bytes, at, q));
                }
            }
            Quote::BlockScalar =>
                if let Some(end) = end_of_block_scalar(bytes, at, line_start) {
                    return Some(end);
                },
            Quote::Heredoc =>
                if let Some(end) = end_of_heredoc(bytes, at) {
                    return Some(end);
                },
            Quote::CData =>
                if bytes[at..].starts_with(b"<![CDATA[") {
                    return Some(end_of_cdata(bytes, at));
                },
            Quote::JsRegex =>
                if let Some(end) = end_of_js_regex(bytes, at, line_start) {
                    return Some(end);
                },
            Quote::CharOrLifetime =>
                if bytes[at] == b'\'' {
                    return Some(end_of_char_or_lifetime(bytes, at));
                },
            Quote::Escaped(q) =>
                if bytes[at] == q {
                    return Some(end_of_escaped(bytes, at, q));
                },
            Quote::Literal(q) =>
                if bytes[at] == q {
                    return Some(end_of_literal(bytes, at, q));
                },
            Quote::RustRaw =>
                if bytes[at] == b'r' {
                    if let Some(end) = end_of_rust_raw(bytes, at) {
                        return Some(end);
                    }
                },
            Quote::LuaLong =>
                if bytes[at] == b'[' {
                    if let Some(end) = end_of_lua_long(bytes, at) {
                        return Some(end);
                    }
                },
        }
    }
    None
}

fn breaks_line(bytes: &[u8], at: usize) -> bool {
    match bytes[at] {
        b'\n' => true,
        b'\r' => bytes.get(at + 1) != Some(&b'\n'),
        _ => false,
    }
}

fn end_of_line(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .map_or(bytes.len(), |n| start + n)
}

fn end_of_block(
    bytes: &[u8],
    start: usize,
    open: &str,
    close: &str,
    nested: bool,
) -> Option<usize> {
    let open = open.as_bytes();
    let close = close.as_bytes();
    let mut depth = 0;
    let mut i = start;

    while i < bytes.len() {
        if bytes[i..].starts_with(close) {
            depth -= 1;
            i += close.len();
            if depth <= 0 {
                return Some(i);
            }
            continue;
        }
        if nested && bytes[i..].starts_with(open) {
            depth += 1;
            i += open.len();
            continue;
        }
        if i == start {
            depth += 1;
            i += open.len();
            continue;
        }
        i += 1;
    }
    None
}

fn end_of_block_scalar(bytes: &[u8], at: usize, line_start: usize) -> Option<usize> {
    if !matches!(bytes[at], b'|' | b'>') {
        return None;
    }
    let mut i = at + 1;
    while matches!(bytes.get(i), Some(b'-') | Some(b'+') | Some(b'0'..=b'9')) {
        i += 1;
    }
    while matches!(bytes.get(i), Some(b' ') | Some(b'\t')) {
        i += 1;
    }
    if !matches!(bytes.get(i), Some(b'\n') | Some(b'\r') | None) {
        return None;
    }
    let indent = bytes[line_start..at].iter().take_while(|byte| **byte == b' ').count();
    let mut line = bytes[i..].iter().position(|byte| *byte == b'\n').map_or(bytes.len(), |n| i + n);
    while line < bytes.len() {
        let start = line + 1;
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |n| start + n);
        let text = &bytes[start..end];
        let spaces = text.iter().take_while(|byte| **byte == b' ').count();
        if spaces < text.len() && spaces <= indent {
            return Some(line);
        }
        line = end;
    }
    Some(bytes.len())
}

fn end_of_heredoc(bytes: &[u8], at: usize) -> Option<usize> {
    if !bytes[at..].starts_with(b"<<") {
        return None;
    }
    let mut i = at + 2;
    if matches!(bytes.get(i), Some(b'-') | Some(b'~')) {
        i += 1;
    }
    while matches!(bytes.get(i), Some(b' ') | Some(b'\t')) {
        i += 1;
    }
    let quote = match bytes.get(i) {
        Some(b'\'') | Some(b'"') => {
            let found = bytes[i];
            i += 1;
            Some(found)
        }
        _ => None,
    };
    if !matches!(bytes.get(i), Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_') {
        return None;
    }
    let word_start = i;
    while matches!(bytes.get(i), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_') {
        i += 1;
    }
    let word = &bytes[word_start..i];
    if let Some(found) = quote {
        if bytes.get(i) != Some(&found) {
            return None;
        }
        i += 1;
    }
    let mut line = bytes[i..].iter().position(|byte| *byte == b'\n').map_or(bytes.len(), |n| i + n);
    while line < bytes.len() {
        let start = line + 1;
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |n| start + n);
        let mut text = &bytes[start..end];
        while matches!(text.first(), Some(b' ') | Some(b'\t')) {
            text = &text[1..];
        }
        while matches!(text.last(), Some(b' ') | Some(b'\t') | Some(b'\r')) {
            text = &text[..text.len() - 1];
        }
        if text == word {
            return Some(end);
        }
        line = end;
    }
    Some(bytes.len())
}

fn end_of_cdata(bytes: &[u8], start: usize) -> usize {
    let mut i = start + b"<![CDATA[".len();
    while i < bytes.len() {
        if bytes[i..].starts_with(b"]]>") {
            return i + 3;
        }
        i += 1;
    }
    bytes.len()
}

fn regex_can_start(bytes: &[u8], at: usize) -> bool {
    let mut i = at;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i >= 2 && matches!(&bytes[i - 2..i], b"++" | b"--") {
        return false;
    }
    match bytes.get(i.wrapping_sub(1)) {
        None => true,
        Some(byte) => matches!(
            byte,
            b'=' | b'('
                | b','
                | b':'
                | b'['
                | b'!'
                | b'&'
                | b'|'
                | b'?'
                | b'{'
                | b'}'
                | b';'
                | b'+'
                | b'-'
                | b'*'
                | b'%'
                | b'^'
                | b'~'
                | b'<'
                | b'>'
                | b'\n'
                | b'\r'
        ),
    }
}

fn end_of_js_regex(bytes: &[u8], start: usize, _line_start: usize) -> Option<usize> {
    if bytes[start] != b'/' || matches!(bytes.get(start + 1), Some(b'/') | Some(b'*') | None) {
        return None;
    }
    if !regex_can_start(bytes, start) {
        return None;
    }
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'\n' | b'\r' => return None,
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

fn end_of_escaped(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == quote {
            return i + 1;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn end_of_literal(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn end_of_triple(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 3;
    while i + 2 < bytes.len() {
        if bytes[i] == quote && bytes[i + 1] == quote && bytes[i + 2] == quote {
            return i + 3;
        }
        i += 1;
    }
    bytes.len()
}

fn end_of_char_or_lifetime(bytes: &[u8], start: usize) -> usize {
    match bytes.get(start + 1) {
        Some(b'\\') => {
            let mut i = start + 2;
            while i < bytes.len() && bytes[i] != b'\'' {
                i += 1;
            }
            i + 1
        }
        Some(_) if bytes.get(start + 2) == Some(&b'\'') => start + 3,
        _ => start + 1,
    }
}

fn end_of_rust_raw(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut hashes = 0;
    while bytes.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    if bytes.get(i) != Some(&b'"') {
        return None;
    }

    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let close = i + 1 + hashes;
            if close <= bytes.len() && bytes[i + 1..close].iter().all(|b| *b == b'#') {
                return Some(close);
            }
        }
        i += 1;
    }
    Some(bytes.len())
}

fn end_of_lua_long(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut equals = 0;
    while bytes.get(i) == Some(&b'=') {
        equals += 1;
        i += 1;
    }
    if bytes.get(i) != Some(&b'[') {
        return None;
    }

    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let close = i + 1 + equals;
            if close < bytes.len()
                && bytes[i + 1..close].iter().all(|b| *b == b'=')
                && bytes[close] == b']'
            {
                return Some(close + 1);
            }
        }
        i += 1;
    }
    Some(bytes.len())
}

fn budget(files: &[SourceFile]) -> (usize, usize, usize) {
    let mut bytes = 0;
    let mut paths = 0;
    let mut largest = 0;
    for file in files {
        bytes += file.size;
        paths += file.path.as_os_str().len() + 1;
        largest = largest.max(file.size);
    }
    (bytes + paths + 64, (bytes / 24).max(64), largest + 64)
}

pub(crate) const PREPARED: [Scanner; syntax::ALL.len()] = {
    let mut out = [Scanner::new(syntax::ALL[0]); syntax::ALL.len()];
    let mut i = 1;
    while i < syntax::ALL.len() {
        out[i] = Scanner::new(syntax::ALL[i]);
        i += 1;
    }
    out
};

const _: () = {
    let mut i = 0;
    while i < syntax::ALL.len() {
        assert!(syntax::ALL[i].id as usize == i);
        i += 1;
    }
};

fn unsafe_to_change(bytes: &[u8]) -> Option<&'static str> {
    let mut at = 0;
    let mut line_start = true;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == 0 && at < 8192 {
            return Some("it looks binary");
        }
        if line_start
            && matches!(byte, b'<' | b'>')
            && (bytes[at..].starts_with(b"<<<<<<<") || bytes[at..].starts_with(b">>>>>>>"))
        {
            return Some("it has git conflict markers");
        }
        line_start = byte == b'\n';
        at += 1;
    }
    None
}

pub(crate) fn scan_batch(files: &[SourceFile], removal: Option<Removal>) -> Report {
    let (arena_bytes, span_count, largest) = budget(files);
    let mut report = Report::with_capacity(arena_bytes, span_count);
    let mut buffer = Vec::with_capacity(largest.min(SMALL_FILE));
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(256);
    let mut stripped: Vec<u8> = Vec::with_capacity(SMALL_FILE);

    for file in files {
        let Some(scanner) = PREPARED.get(file.syntax as usize) else {
            continue;
        };
        let path = report.intern_terminated(file.path.as_os_str().as_encoded_bytes());
        let opened = report.arena[path.0 as usize..].as_ptr();
        let mark = report.spans.len();
        with_file(opened, file.size, &mut buffer, |bytes| {
            if !scanner.scan(path, bytes, &mut report) {
                report.spans.truncate(mark);
                eprintln!("{}: left alone, a block comment is never closed", file.path.display());
                return;
            }
            if report.spans.len() == mark {
                return;
            }
            if let Some(reason) = unsafe_to_change(bytes) {
                report.spans.truncate(mark);
                eprintln!("{}: left alone, {reason}", file.path.display());
                return;
            }
            let Some(removal) = removal else {
                return;
            };
            ranges.clear();
            ranges.extend(report.spans[mark..].iter().filter(|span| removal.wants(span.kind)).map(
                |span| (span.source_at as usize, span.source_at as usize + span.text_len as usize),
            ));
            if ranges.is_empty() {
                return;
            }
            strip_into(bytes, &ranges, &mut stripped);
            if stripped != bytes && std::fs::write(&file.path, &stripped).is_err() {
                eprintln!("{}: could not write, left alone", file.path.display());
                report.spans.truncate(mark);
            }
        });
    }
    report
}
