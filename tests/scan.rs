use std::path::PathBuf;

use shhhh::{Kind, find_in_bytes, syntax_for};

fn scan(name: &str, source: &str) -> Vec<(u32, u32, Kind)> {
    let path = PathBuf::from(name);
    let syntax = syntax_for(&path).unwrap_or_else(|| panic!("no syntax for {name}"));
    let report = find_in_bytes(&path, syntax, source.as_bytes());
    report.iter().map(|c| (c.line, c.column, c.kind)).collect()
}

#[test]
fn rust_basics() {
    assert_eq!(scan("a.rs", "fn a() {} // hi\n"), vec![(1, 11, Kind::Line)]);
    assert_eq!(scan("a.rs", "/// outer\n"), vec![(1, 1, Kind::DocLine)]);
    assert_eq!(scan("a.rs", "//! inner\n"), vec![(1, 1, Kind::DocLine)]);
    assert_eq!(scan("a.rs", "/** doc */\n"), vec![(1, 1, Kind::DocBlock)]);
    assert_eq!(scan("a.rs", "/* a /* b */ c */\n"), vec![(1, 1, Kind::Block)]);
}

#[test]
fn rust_strings_are_not_comments() {
    assert_eq!(scan("a.rs", "let u = \"https://a//b\";\n"), vec![]);
    assert_eq!(scan("a.rs", "let r = r#\"x // y\"#;\n"), vec![]);
    assert_eq!(scan("a.rs", "let q = r##\"deep \"# // still\"##;\n"), vec![]);
    assert_eq!(scan("a.rs", "let c = '/';\n"), vec![]);
    assert_eq!(scan("a.rs", "struct S<'a>(&'a str); // tail\n"), vec![(1, 24, Kind::Line)]);
}

#[test]
fn shader_languages() {
    assert_eq!(scan("a.glsl", "void main() {} // hi\n"), vec![(1, 16, Kind::Line)]);
    assert_eq!(scan("a.frag", "/* block */\n"), vec![(1, 1, Kind::Block)]);
    assert_eq!(scan("a.hlsl", "float4 c; // tint\n"), vec![(1, 11, Kind::Line)]);
    assert_eq!(scan("a.vert", "// top\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.metal", "// top\n"), vec![(1, 1, Kind::Line)]);
}

#[test]
fn wgsl_has_nested_block_comments() {
    assert_eq!(scan("a.wgsl", "/* a /* b */ c */ @vertex\n"), vec![(1, 1, Kind::Block)]);
    assert_eq!(scan("a.wgsl", "let x = 1.0; // hi\n"), vec![(1, 14, Kind::Line)]);
}

#[test]
fn c_block_comments_do_not_nest() {
    assert_eq!(scan("a.c", "/* a /* b */\nint x;\n"), vec![(1, 1, Kind::Block)]);
}

#[test]
fn shell_hash_needs_a_word_boundary() {
    assert_eq!(scan("a.sh", "# comment\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.sh", "echo hi # tail\n"), vec![(1, 9, Kind::Line)]);
    assert_eq!(scan("a.sh", "echo a#b\n"), vec![]);
    assert_eq!(scan("a.sh", "echo '# quoted'\n"), vec![]);
    assert_eq!(scan("a.sh", "echo \"# quoted\"\n"), vec![]);
}

#[test]
fn yaml_follows_the_same_rule() {
    assert_eq!(scan("a.yml", "key: value # note\n"), vec![(1, 12, Kind::Line)]);
    assert_eq!(scan("a.yaml", "url: http://a/#frag\n"), vec![]);
}

#[test]
fn python_triple_quotes() {
    assert_eq!(scan("a.py", "x = 1  # note\n"), vec![(1, 8, Kind::Line)]);
    assert_eq!(scan("a.py", "s = \"\"\"\n# not a comment\n\"\"\"\n"), vec![]);
    assert_eq!(scan("a.py", "s = '''# no'''\n"), vec![]);
}

#[test]
fn toml_strings_and_comments() {
    assert_eq!(scan("a.toml", "name = \"x\" # banned\n"), vec![(1, 12, Kind::Line)]);
    assert_eq!(scan("a.toml", "url = \"http://a/#frag\"\n"), vec![]);
    assert_eq!(scan("a.toml", "s = '''\nmulti # not\n'''\n"), vec![]);
}

#[test]
fn lua_line_and_long_comments() {
    assert_eq!(scan("a.lua", "local x = 1 -- note\n"), vec![(1, 13, Kind::Line)]);
    assert_eq!(scan("a.lua", "--[[ block\nstill ]]\n"), vec![(1, 1, Kind::Block)]);
    assert_eq!(scan("a.lua", "local s = [[ -- not ]]\n"), vec![]);
}

#[test]
fn batch_rem_and_double_colon() {
    assert_eq!(scan("a.bat", ":: note\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.bat", "REM note\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.bat", "rem note\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.cmd", "  :: indented\n"), vec![(1, 3, Kind::Line)]);
    assert_eq!(scan("a.bat", "echo remember\n"), vec![]);
}

#[test]
fn xml_and_html() {
    assert_eq!(scan("a.xml", "<!-- note -->\n"), vec![(1, 1, Kind::Block)]);
    assert_eq!(scan("a.svg", "<rect/> <!-- tail -->\n"), vec![(1, 9, Kind::Block)]);
    assert_eq!(scan("a.html", "<p>a -- b</p>\n"), vec![]);
}

#[test]
fn ini_and_powershell() {
    assert_eq!(scan("a.ini", "; note\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.ini", "# also\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.ps1", "Get-Item # note\n"), vec![(1, 10, Kind::Line)]);
    assert_eq!(scan("a.ps1", "<# block #>\n"), vec![(1, 1, Kind::Block)]);
}

#[test]
fn zig_has_no_block_comments() {
    assert_eq!(scan("a.zig", "const x = 1; // note\n"), vec![(1, 14, Kind::Line)]);
    assert_eq!(scan("a.zig", "/// doc\n"), vec![(1, 1, Kind::DocLine)]);
}

#[test]
fn makefile_and_cmakelists_by_name() {
    assert_eq!(scan("Makefile", "# note\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("CMakeLists.txt", "# note\n"), vec![(1, 1, Kind::Line)]);
}

#[test]
fn line_numbers_survive_multiline_constructs() {
    assert_eq!(scan("a.rs", "let s = \"one\ntwo\";\n// here\n"), vec![(3, 1, Kind::Line)]);
    assert_eq!(scan("a.rs", "/* one\ntwo */\n// here\n"), vec![
        (1, 1, Kind::Block),
        (3, 1, Kind::Line)
    ]);
}

#[test]
fn windows_line_endings() {
    assert_eq!(scan("a.rs", "fn a() {} // hi\r\nfn b() {}\r\n"), vec![(1, 11, Kind::Line)]);
    assert_eq!(scan("a.bat", ":: note\r\necho hi\r\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.sh", "echo hi # tail\r\n"), vec![(1, 9, Kind::Line)]);
}

#[test]
fn carriage_returns_are_trimmed_from_text() {
    let path = PathBuf::from("a.rs");
    let syntax = syntax_for(&path).expect("rust");
    let report = find_in_bytes(&path, syntax, b"let x = 1; // keep\r\n");
    let first = report.iter().next().expect("one comment");
    assert_eq!(first.text, "// keep");
}

#[test]
fn line_numbers_are_right_with_crlf() {
    let source = "fn a() {}\r\nfn b() {}\r\n// third line\r\n";
    assert_eq!(scan("a.rs", source), vec![(3, 1, Kind::Line)]);
}

#[test]
fn clean_sources_report_nothing() {
    assert_eq!(scan("a.rs", "fn main() {}\n"), vec![]);
    assert_eq!(scan("a.glsl", "void main() {}\n"), vec![]);
    assert_eq!(scan("a.sh", "echo hi\n"), vec![]);
}

fn texts(name: &str, source: &str) -> Vec<String> {
    let path = PathBuf::from(name);
    let syntax = syntax_for(&path).unwrap_or_else(|| panic!("no syntax for {name}"));
    let report = find_in_bytes(&path, syntax, source.as_bytes());
    report.iter().map(|c| c.text.to_string()).collect()
}

#[test]
fn shebang_is_not_a_comment() {
    assert_eq!(scan("t.py", "#!/usr/bin/env python3\n# real\nx = 1\n"), vec![(2, 1, Kind::Line)]);
    assert_eq!(texts("t.py", "#!/usr/bin/env python3\n# real\n"), vec!["# real"]);
    assert_eq!(scan("t.sh", "#!/usr/bin/env sh\nset -e\n# note\n"), vec![(3, 1, Kind::Line)]);
    assert_eq!(scan("t.rb", "#!/usr/bin/env ruby\n# note\n"), vec![(2, 1, Kind::Line)]);
}

#[test]
fn shebang_alone_reports_nothing() {
    assert_eq!(scan("t.sh", "#!/bin/sh\n"), vec![]);
    assert_eq!(scan("t.py", "#!/usr/bin/env python3"), vec![]);
}

#[test]
fn hash_on_line_one_that_is_not_a_shebang_is_still_a_comment() {
    assert_eq!(scan("t.py", "# not a shebang\nx = 1\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("t.toml", "# header\nk = 1\n"), vec![(1, 1, Kind::Line)]);
}

#[test]
fn rust_cargo_script_shebang_is_unaffected() {
    let src = "#!/usr/bin/env -S cargo +nightly -Zscript\n// a comment\nfn main() {}\n";
    assert_eq!(scan("t.rs", src), vec![(2, 1, Kind::Line)]);
}

fn stripped(name: &str, source: &str) -> String {
    let path = PathBuf::from(name);
    let syntax = syntax_for(&path).unwrap_or_else(|| panic!("no syntax for {name}"));
    let report = find_in_bytes(&path, syntax, source.as_bytes());
    let ranges: Vec<(usize, usize)> =
        report.iter().map(|c| (c.offset as usize, c.offset as usize + c.text.len())).collect();
    String::from_utf8(shhhh::strip(source.as_bytes(), &ranges)).expect("utf8 in, utf8 out")
}

#[test]
fn strip_whole_line_comment_drops_the_line() {
    assert_eq!(stripped("a.rs", "fn a() {}\n// gone\nfn b() {}\n"), "fn a() {}\nfn b() {}\n");
    assert_eq!(stripped("a.rs", "    // indented\nfn a() {}\n"), "fn a() {}\n");
    assert_eq!(stripped("a.rs", "/// doc\nfn a() {}\n"), "fn a() {}\n");
}

#[test]
fn strip_trailing_comment_keeps_the_code_and_no_trailing_space() {
    assert_eq!(stripped("a.rs", "fn a() {} // hi\n"), "fn a() {}\n");
    assert_eq!(stripped("a.rs", "let x = 1;\t// hi\n"), "let x = 1;\n");
    assert_eq!(stripped("a.py", "x = 1  # hi\n"), "x = 1\n");
}

#[test]
fn strip_block_comment_spanning_lines() {
    assert_eq!(
        stripped("a.rs", "fn a() {}\n/* one\n   two */\nfn b() {}\n"),
        "fn a() {}\nfn b() {}\n"
    );
    assert_eq!(stripped("a.rs", "let x = /* mid */ 1;\n"), "let x = 1;\n");
}

#[test]
fn strip_leaves_shebang_alone() {
    assert_eq!(
        stripped("a.py", "#!/usr/bin/env python3\n# gone\nx = 1\n"),
        "#!/usr/bin/env python3\nx = 1\n"
    );
    assert_eq!(stripped("a.sh", "#!/bin/sh\nset -e\n"), "#!/bin/sh\nset -e\n");
}

#[test]
fn strip_does_not_touch_comment_markers_inside_strings() {
    assert_eq!(
        stripped("a.rs", "let s = \"// not a comment\";\n"),
        "let s = \"// not a comment\";\n"
    );
    assert_eq!(stripped("a.py", "s = '# not a comment'\n"), "s = '# not a comment'\n");
    assert_eq!(stripped("a.rs", "let u = \"http://x\"; // gone\n"), "let u = \"http://x\";\n");
}

#[test]
fn strip_is_idempotent() {
    let once = stripped("a.rs", "// a\nfn a() {} // b\n/* c */\nfn b() {}\n");
    assert_eq!(once, "fn a() {}\nfn b() {}\n");
    assert_eq!(stripped("a.rs", &once), once);
}

#[test]
fn strip_preserves_a_file_with_no_comments() {
    let src = "fn a() {\n    let s = \"x\";\n}\n";
    assert_eq!(stripped("a.rs", src), src);
}

#[test]
fn strip_handles_no_trailing_newline() {
    assert_eq!(stripped("a.rs", "fn a() {} // hi"), "fn a() {}");
    assert_eq!(stripped("a.rs", "// only"), "");
}

#[test]
fn single_quoted_strings_are_not_comment_starts() {
    assert_eq!(scan("a.py", "s = '# not a comment'\n"), vec![]);
    assert_eq!(scan("a.py", "s = '# no'  # yes\n"), vec![(1, 13, Kind::Line)]);
    assert_eq!(scan("a.lua", "s = '-- not a comment'\n"), vec![]);
    assert_eq!(scan("a.xml", "<a b='<!-- no -->'/>\n"), vec![]);
    assert_eq!(stripped("a.py", "s = '# not a comment'\n"), "s = '# not a comment'\n");
    assert_eq!(stripped("a.lua", "s = '-- keep'\n"), "s = '-- keep'\n");
}

#[test]
fn char_literals_and_lifetimes_still_work() {
    assert_eq!(scan("a.rs", "let c = '/'; // gone\n"), vec![(1, 14, Kind::Line)]);
    assert_eq!(scan("a.rs", "fn f<'a>(x: &'a str) {} // gone\n"), vec![(1, 25, Kind::Line)]);
    assert_eq!(scan("a.rs", "let c = '\\''; // gone\n"), vec![(1, 15, Kind::Line)]);
    assert_eq!(scan("a.c", "char c = '/'; // gone\n"), vec![(1, 15, Kind::Line)]);
    assert_eq!(stripped("a.rs", "let c = '/'; // gone\n"), "let c = '/';\n");
    assert_eq!(stripped("a.rs", "fn f<'a>(x: &'a str) {}\n"), "fn f<'a>(x: &'a str) {}\n");
}

#[test]
fn comments_that_are_not_utf8_are_still_removed() {
    let path = PathBuf::from("a.c");
    let syntax = syntax_for(&path).unwrap();
    let source: &[u8] = b"/* Joacim H\xe4ggmark */\nint x;\n";
    let report = find_in_bytes(&path, syntax, source);
    let ranges: Vec<(usize, usize)> =
        report.iter().map(|c| (c.offset as usize, c.offset as usize + c.len as usize)).collect();
    assert_eq!(ranges.len(), 1, "the comment is found");
    assert_eq!(ranges[0], (0, 21), "the range covers the raw bytes, not the utf8 prefix");
    assert_eq!(shhhh::strip(source, &ranges), b"int x;\n", "and it is removed");
}

#[test]
fn doc_block_nests_on_the_language_open_not_the_doc_marker() {
    let src = "/*!\nsee `src/**/foo.rs` here\n*/\nfn a() {}\n";
    assert_eq!(scan("a.rs", src), vec![(1, 1, Kind::DocBlock)], "one comment, not truncated");
    assert_eq!(stripped("a.rs", src), "fn a() {}\n");
    let plain = "/*\nsee `src/**/foo.rs` here\n*/\nfn a() {}\n";
    assert_eq!(stripped("a.rs", plain), "fn a() {}\n");
    let starred = "/**\nsee `a/**/b` here\n*/\nfn a() {}\n";
    assert_eq!(stripped("a.rs", starred), "fn a() {}\n");
}

#[test]
fn empty_block_comment() {
    assert_eq!(stripped("a.rs", "let x = 1;/**/\n"), "let x = 1;\n");
    assert_eq!(stripped("a.rs", "/**/\nfn a() {}\n"), "fn a() {}\n");
    assert_eq!(stripped("a.rs", "/***/\nfn a() {}\n"), "fn a() {}\n");
}

fn in_temp(name: &str, source: &str) -> (PathBuf, usize) {
    let dir = std::env::temp_dir().join(format!("shhhh_t_{}_{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write");
    let n = shhhh::find_in_file(&path).len();
    (path, n)
}

#[test]
fn files_with_git_conflict_markers_are_left_alone() {
    let conflicted = "fn f() {\n<<<<<<< HEAD\n    // a\n=======\n    // b\n>>>>>>> x\n}\n";
    let (path, n) = in_temp("conflict.rs", conflicted);
    assert_eq!(n, 0, "a conflicted file reports nothing, so nothing is removed");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), conflicted);
}

#[test]
fn python_doctests_are_not_conflict_markers() {
    let doctest =
        "def f():\n    \"\"\"\n    >>> f()\n    1\n    \"\"\"\n    return 1  # keep me out\n";
    let (_, n) = in_temp("doctest.py", doctest);
    assert_eq!(n, 1, ">>> is three chars, not a conflict marker; the docstring is a string");
}

#[test]
fn shift_operators_are_not_conflict_markers() {
    let (_, n) = in_temp("shift.rs", "let a = x >> 7; // gone\nlet b = y << 7;\n");
    assert_eq!(n, 1);
}

#[test]
fn every_extension_maps_to_a_prepared_scanner() {
    const EXTENSIONS: &[&str] = &[
        "rs", "c", "h", "cpp", "hpp", "m", "mm", "metal", "cs", "java", "js", "mjs", "cjs", "jsx",
        "ts", "tsx", "go", "glsl", "vert", "frag", "hlsl", "wgsl", "swift", "kt", "scala", "odin",
        "zig", "sh", "bash", "zsh", "fish", "yml", "yaml", "rb", "pl", "r", "py", "pyi", "toml",
        "gd", "cmake", "mk", "just", "nix", "ex", "jl", "tf", "conf", "ini", "cfg", "lua", "xml",
        "html", "svg", "vue", "bat", "cmd", "ps1", "psm1", "nim", "json5", "jsonc", "proto",
    ];
    for ext in EXTENSIONS {
        let path = PathBuf::from(format!("a.{ext}"));
        let syntax = syntax_for(&path).unwrap_or_else(|| panic!(".{ext} has no syntax"));
        // a scan must not panic indexing the prepared table
        let found = find_in_bytes(&path, syntax, b"x\n");
        assert_eq!(found.len(), 0, ".{ext} scans cleanly");
    }
}

#[test]
fn batch_rem_needs_a_word_boundary() {
    assert_eq!(scan("a.bat", "rem a comment\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.bat", "REM upper\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.bat", "rem\n"), vec![(1, 1, Kind::Line)]);
    assert_eq!(scan("a.bat", "remove this file\n"), vec![], "rem is not a prefix match");
    assert_eq!(scan("a.bat", "REMOTE_HOST=x\n"), vec![]);
}

#[test]
fn carriage_return_ends_a_line_comment() {
    assert_eq!(stripped("a.rs", "fn a() {}\r// old mac\rfn b() {}\r"), "fn a() {}\rfn b() {}\r");
    assert_eq!(
        stripped("a.rs", "a();\r\nb(); // x\r\n// whole\r\nc();\r\n"),
        "a();\r\nb();\r\nc();\r\n"
    );
}

#[test]
fn js_regex_literals_are_not_comments() {
    assert_eq!(scan("a.js", "const re = /https?:\\/\\//;\n"), vec![]);
    assert_eq!(scan("a.js", "const e = /ab[/]c/g;\n"), vec![]);
    assert_eq!(scan("a.js", "const d = a / b; // gone\n"), vec![(1, 18, Kind::Line)]);
    assert_eq!(stripped("a.js", "const re = /a\\/\\//; // gone\n"), "const re = /a\\/\\//;\n");
}

#[test]
fn xml_cdata_is_not_scanned() {
    assert_eq!(scan("a.xml", "<r><![CDATA[ <!-- keep --> ]]></r>\n"), vec![]);
    assert_eq!(scan("a.xml", "<!-- gone -->\n"), vec![(1, 1, Kind::Block)]);
}

#[test]
fn shell_heredocs_are_not_scanned() {
    assert_eq!(scan("a.sh", "cat <<EOF\n# keep\nEOF\n"), vec![]);
    assert_eq!(scan("a.sh", "cat <<-\"END\"\n# keep\nEND\necho x  # gone\n"), vec![(
        4,
        9,
        Kind::Line
    )]);
}

#[test]
fn yaml_block_scalars_are_not_scanned() {
    assert_eq!(scan("a.yaml", "key: |\n  # keep\n  two\n"), vec![]);
    assert_eq!(scan("a.yaml", "key: |\n  # keep\nother: 1  # gone\n"), vec![(3, 11, Kind::Line)]);
    assert_eq!(scan("a.yaml", "child:\n  <<: *b  # gone\n"), vec![(2, 11, Kind::Line)]);
}

#[test]
fn an_unterminated_block_comment_reports_nothing() {
    let (path, n) = in_temp("unterm.rs", "fn a() {}\n/* never closed\nfn b() {}\n");
    assert_eq!(n, 0, "the file is left alone rather than truncated");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn a() {}\n/* never closed\nfn b() {}\n");
}

#[test]
fn binary_files_are_left_alone() {
    let mut bytes: Vec<u8> = (0u8..=255).collect();
    bytes.extend_from_slice(b"\n// trailing\n");
    let dir = std::env::temp_dir().join(format!("shhhh_bin_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("b.rs");
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(shhhh::find_in_file(&path).len(), 0);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn division_after_a_postfix_operator_is_not_a_regex() {
    assert_eq!(scan("a.js", "let x = a++ / b;  // gone\n"), vec![(1, 19, Kind::Line)]);
    assert_eq!(scan("a.js", "let y = a-- / b;  // gone\n"), vec![(1, 19, Kind::Line)]);
    assert_eq!(scan("a.js", "let z = a + /re/.source;  // gone\n"), vec![(1, 27, Kind::Line)]);
}

#[test]
fn unterminated_skip_regions_do_not_panic() {
    assert_eq!(scan("a.xml", "<r><![CDATA[ unterminated\nmore\n"), vec![]);
    assert_eq!(scan("a.sh", "cat <<EOF\nnever terminated\n"), vec![]);
    assert_eq!(scan("a.yaml", "key: |\n  only\n"), vec![]);
}

#[test]
fn bash_here_strings_are_left_alone() {
    assert_eq!(scan("a.sh", "cmd <<<\"# here string\"\necho a  # gone\n"), vec![(
        2,
        9,
        Kind::Line
    )]);
}

#[test]
fn yaml_explicit_indent_indicator() {
    assert_eq!(scan("a.yaml", "key: |2\n    # keep\nother: 1  # gone\n"), vec![(
        3,
        11,
        Kind::Line
    )]);
}

#[test]
fn heredoc_delimiter_spacing_and_whitespace() {
    assert_eq!(scan("a.sh", "cat << EOF\n# keep\nEOF\necho a  # gone\n"), vec![(4, 9, Kind::Line)]);
    assert_eq!(scan("a.sh", "cat <<EOF\n# keep\nEOF   \necho b  # gone\n"), vec![(
        4,
        9,
        Kind::Line
    )]);
    assert_eq!(scan("a.sh", "cat <<-\tEOF\n# keep\n\tEOF\necho c  # gone\n"), vec![(
        4,
        9,
        Kind::Line
    )]);
}

#[test]
fn arithmetic_shift_is_not_a_heredoc() {
    assert_eq!(scan("a.sh", "x=$(( 1 << 2 ))\necho c  # gone\n"), vec![(2, 9, Kind::Line)]);
    assert_eq!(scan("a.sh", "x=$((a << 2))\necho d  # gone\n"), vec![(2, 9, Kind::Line)]);
}

#[test]
fn regex_character_classes_and_division_chains() {
    assert_eq!(scan("a.js", "const a = /[\\]]/;  // gone\n"), vec![(1, 20, Kind::Line)]);
    assert_eq!(scan("a.js", "const c = x/y/z;  // gone\n"), vec![(1, 19, Kind::Line)]);
}

#[test]
fn overlapping_and_symlinked_roots_do_not_double_strip() {
    let dir = std::env::temp_dir().join(format!("shhhh_roots_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let file = dir.join("src/a.rs");
    std::fs::write(&file, "fn one() {}   // aaaa\nfn two() {}   // bbbb\n").unwrap();
    let found = shhhh::source_files_under(&dir);
    assert_eq!(found.len(), 1, "one file under the tree");
    assert_eq!(shhhh::find_in_file(&file).len(), 2);
}

#[test]
fn markdown_is_never_processed() {
    assert!(syntax_for(&PathBuf::from("a.md")).is_none());
    assert!(syntax_for(&PathBuf::from("a.markdown")).is_none());
    let (path, n) = in_temp("doc.md", "<!-- a comment -->\ntext\n");
    assert_eq!(n, 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "<!-- a comment -->\ntext\n");
}

#[test]
fn a_git_merge_in_progress_is_detected() {
    let dir = std::env::temp_dir().join(format!("shhhh_git_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    assert!(shhhh::git_operation_in_progress(&dir).is_none());
    std::fs::write(dir.join(".git/MERGE_HEAD"), "x").unwrap();
    assert_eq!(shhhh::git_operation_in_progress(&dir).as_deref(), Some("merge"));
    std::fs::remove_file(dir.join(".git/MERGE_HEAD")).unwrap();
    std::fs::create_dir_all(dir.join(".git/rebase-merge")).unwrap();
    assert_eq!(shhhh::git_operation_in_progress(&dir).as_deref(), Some("rebase"));
}

#[test]
fn gitignore_applies_from_any_scan_root() {
    let dir = std::env::temp_dir().join(format!("shhhh_gi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("src/build")).unwrap();
    std::fs::write(dir.join(".gitignore"), "build/\n*.gen.rs\n").unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn a() {} // c\n").unwrap();
    std::fs::write(dir.join("src/build/out.rs"), "fn b() {} // c\n").unwrap();
    std::fs::write(dir.join("src/thing.gen.rs"), "fn c() {} // c\n").unwrap();

    let names = |root: &std::path::Path| -> Vec<String> {
        let mut found: Vec<String> = shhhh::source_files_under(root)
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    };
    assert_eq!(names(&dir), vec![".gitignore", "main.rs"], "build/ and *.gen.rs are ignored");
    assert_eq!(
        names(&dir.join("src")),
        vec!["main.rs"],
        "the repo-root .gitignore applies from a subdirectory"
    );
    let named = shhhh::source_files_under(&dir.join("src/thing.gen.rs"));
    assert_eq!(named.len(), 0, "an ignored file named directly is still ignored");
}

#[test]
fn an_unknown_syntax_id_is_skipped_rather_than_panicking() {
    use std::sync::atomic::Ordering;
    let dir = std::env::temp_dir().join(format!("shhhh_panic_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let files: Vec<shhhh::SourceFile> = (0..64)
        .map(|n| {
            let path = dir.join(format!("f{n}.rs"));
            let _ = std::fs::write(&path, "fn f() {}\n");
            // an id no prepared scanner exists for: the worker will panic indexing it
            shhhh::SourceFile { path, size: 10, syntax: 200 }
        })
        .collect();
    let reports = shhhh::find_in_files(&files);
    let found: usize = reports.iter().map(|report| report.len()).sum();
    assert_eq!(found, 0, "an id with no prepared scanner yields nothing");
    assert!(!shhhh::INCOMPLETE.load(Ordering::Relaxed), "and no worker died doing it");
}

#[test]
fn gitignore_rules_only_apply_inside_a_repository() {
    let dir = std::env::temp_dir().join(format!("shhhh_norepo_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("work/sub")).unwrap();
    std::fs::write(dir.join(".gitignore"), "sub\n").unwrap();
    std::fs::write(dir.join("work/sub/a.rs"), "fn f() {} // c\n").unwrap();

    let outside = shhhh::source_files_under(&dir.join("work"));
    assert_eq!(outside.len(), 1, "a .gitignore outside any repository means nothing");

    std::fs::create_dir_all(dir.join("work/.git")).unwrap();
    std::fs::write(dir.join("work/.gitignore"), "sub\n").unwrap();
    let inside = shhhh::source_files_under(&dir.join("work"));
    assert_eq!(inside.len(), 1, "inside one, the rule applies and only .gitignore is left");
    assert_eq!(inside[0].path.file_name().unwrap(), ".gitignore");
}

#[test]
fn a_file_that_grew_since_the_walk_is_not_truncated() {
    let dir = std::env::temp_dir().join(format!("shhhh_grew_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let small = dir.join("small.rs");
    let grown = dir.join("grown.rs");
    std::fs::write(&small, "fn a() {} // c\n").unwrap();
    std::fs::write(&grown, "fn b() {} // c\n".repeat(400)).unwrap();

    // both claim the small size, as if grown.rs expanded after the walk stat'd it
    let files = vec![shhhh::SourceFile { path: small, size: 15, syntax: 0 }, shhhh::SourceFile {
        path: grown.clone(),
        size: 15,
        syntax: 0,
    }];
    shhhh::remove_in_files(&files, shhhh::Removal { docs_only: false, code_only: false });
    let after = std::fs::read_to_string(&grown).unwrap();
    assert_eq!(after, "fn b() {}\n".repeat(400), "every line survives, only comments go");
}

#[test]
fn a_line_of_only_comments_goes_even_when_there_are_several() {
    assert_eq!(stripped("a.rs", "/*a*/ /*b*/\nfn f() {}\n"), "fn f() {}\n");
    assert_eq!(stripped("a.rs", "  /*a*/\t/*b*/  \nfn f() {}\n"), "fn f() {}\n");
    assert_eq!(stripped("a.rs", "let a = /*one*/ x /*two*/ ;\n"), "let a = x ;\n");
    assert_eq!(stripped("a.rs", "let b = /*a*//*b*/ y;\n"), "let b = y;\n");
    assert_eq!(stripped("a.rs", "fn f() { /*a*/ }\n"), "fn f() { }\n");
    assert_eq!(stripped("a.rs", "let e = /*multi\nline*/ w;\n"), "let e = w;\n");
}

#[test]
fn unterminated_strings_swallow_rather_than_corrupt() {
    assert_eq!(stripped("a.rs", "let a = \"abc\\"), "let a = \"abc\\");
    assert_eq!(
        stripped("a.rs", "let c = r#\"unterminated\n// after\n"),
        "let c = r#\"unterminated\n// after\n"
    );
    assert_eq!(stripped("a.lua", "local e = [==[ x\n-- after\n"), "local e = [==[ x\n-- after\n");
    assert_eq!(stripped("a.rs", "let f = r\"a\" ; // gone\n"), "let f = r\"a\" ;\n");
    assert_eq!(stripped("a.lua", "local j = [=[y]=] -- gone\n"), "local j = [=[y]=]\n");
}

#[test]
fn strip_never_produces_more_than_it_was_given() {
    let text = b"fn a() {} // one\nfn b() {} // two\n";
    let cases: [&[(usize, usize)]; 8] = [
        &[(16, 10)],
        &[(999, 1000)],
        &[(10, 999)],
        &[(10, 16), (12, 20)],
        &[(26, 32), (10, 16)],
        &[(10, 10)],
        &[(0, 34)],
        &[(34, 34)],
    ];
    for ranges in cases {
        let out = shhhh::strip(text, ranges);
        assert!(out.len() <= text.len(), "strip only ever deletes: {ranges:?}");
    }
    assert_eq!(shhhh::strip(text, &[(16, 10)]), text, "a reversed range is ignored");
}

#[test]
fn line_numbers_agree_across_line_ending_styles() {
    let lf = "fn a() {}\n// two\nfn b() {}\n// four\n";
    let cr = "fn a() {}\r// two\rfn b() {}\r// four\r";
    let crlf = "fn a() {}\r\n// two\r\nfn b() {}\r\n// four\r\n";
    let want = vec![(2, 1, Kind::Line), (4, 1, Kind::Line)];
    assert_eq!(scan("a.rs", lf), want);
    assert_eq!(scan("a.rs", cr), want, "a carriage return alone still ends a line");
    assert_eq!(scan("a.rs", crlf), want, "and a pair counts once");
}

#[test]
fn line_numbers_survive_a_multi_line_string() {
    assert_eq!(scan("a.py", "s = \"\"\"\na\nb\n\"\"\"\n# five\n"), vec![(5, 1, Kind::Line)]);
    assert_eq!(scan("a.sh", "cat <<EOF\na\nb\nEOF\n# five\n"), vec![(5, 1, Kind::Line)]);
}
