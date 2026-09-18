use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use crate::syntax_for;

pub static UNREADABLE: AtomicBool = AtomicBool::new(false);

const MID_OPERATION: [(&str, &str); 6] = [
    ("MERGE_HEAD", "merge"),
    ("rebase-merge", "rebase"),
    ("rebase-apply", "rebase"),
    ("CHERRY_PICK_HEAD", "cherry-pick"),
    ("REVERT_HEAD", "revert"),
    ("BISECT_LOG", "bisect"),
];

pub fn git_operation_in_progress(from: &Path) -> Option<String> {
    for ancestor in from.ancestors() {
        let marker = ancestor.join(".git");
        let git = match std::fs::metadata(&marker) {
            Ok(found) if found.is_dir() => marker,
            Ok(_) => {
                let Ok(text) = std::fs::read_to_string(&marker) else {
                    continue;
                };
                let Some(target) = text.strip_prefix("gitdir:") else {
                    continue;
                };
                ancestor.join(target.trim())
            }
            Err(_) => continue,
        };
        for (marker, operation) in MID_OPERATION {
            if git.join(marker).exists() {
                return Some(operation.to_string());
            }
        }
        return None;
    }
    None
}

pub struct SourceFile {
    pub path: PathBuf,
    pub size: usize,
    pub syntax: u8,
}

pub struct Ignore {
    names: Vec<String>,
    suffixes: Vec<String>,
}

impl Ignore {
    fn read(root: &Path) -> Self {
        let mut names = Vec::new();
        let mut suffixes = Vec::new();
        let from = if root.is_dir() { root } else { root.parent().unwrap_or(root) };
        let mut inside_repository = false;
        for ancestor in from.ancestors() {
            Self::merge(ancestor, &mut names, &mut suffixes);
            if ancestor.join(".git").exists() {
                inside_repository = true;
                break;
            }
        }
        if !inside_repository {
            names.clear();
            suffixes.clear();
        }
        Self { names, suffixes }
    }

    fn merge(dir: &Path, names: &mut Vec<String>, suffixes: &mut Vec<String>) {
        if let Ok(text) = std::fs::read_to_string(dir.join(".gitignore")) {
            for line in text.lines() {
                let rule = line.trim();
                if rule.is_empty() || rule.starts_with('#') || rule.starts_with('!') {
                    continue;
                }
                let rule = rule.trim_end_matches('/').trim_start_matches('/');
                if rule.contains('/') {
                    continue;
                }
                match rule.strip_prefix('*') {
                    Some(suffix) if !suffix.is_empty() && !suffix.contains('*') =>
                        suffixes.push(suffix.to_string()),
                    _ =>
                        if !rule.contains('*') {
                            names.push(rule.to_string());
                        },
                }
            }
        }
    }

    fn skips(&self, name: &OsStr) -> bool {
        let Some(text) = name.to_str() else {
            return false;
        };
        self.names.iter().any(|rule| rule == text)
            || self.suffixes.iter().any(|rule| text.ends_with(rule.as_str()))
    }
}

struct Queue {
    ignore: Ignore,
    dirs: Mutex<Vec<PathBuf>>,
    pending: AtomicUsize,
    wake: Condvar,
}

impl Queue {
    fn take(&self) -> Option<PathBuf> {
        let mut dirs = self.dirs.lock().unwrap_or_else(|held| held.into_inner());
        loop {
            if let Some(dir) = dirs.pop() {
                return Some(dir);
            }
            if self.pending.load(Ordering::Acquire) == 0 {
                return None;
            }
            dirs = self.wake.wait(dirs).unwrap_or_else(|held| held.into_inner());
        }
    }

    fn give(&self, subdirs: Vec<PathBuf>) {
        if subdirs.is_empty() {
            return;
        }
        self.pending.fetch_add(subdirs.len(), Ordering::AcqRel);
        let mut dirs = self.dirs.lock().unwrap_or_else(|held| held.into_inner());
        dirs.extend(subdirs);
        self.wake.notify_all();
    }

    fn finish(&self) {
        if self.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
            let _held = self.dirs.lock().unwrap_or_else(|held| held.into_inner());
            self.wake.notify_all();
        }
    }
}

pub fn source_files_under(root: &Path) -> Vec<SourceFile> {
    if root.is_file() {
        let mut found = Vec::new();
        if root.file_name().is_some_and(|name| Ignore::read(root).skips(name)) {
            return found;
        }
        if let Some(syntax) = syntax_for(root) {
            let size = root.metadata().map_or(0, |at| at.len() as usize);
            found.push(SourceFile { path: root.to_path_buf(), size, syntax: syntax.id });
        }
        return found;
    }

    let queue = Queue {
        ignore: Ignore::read(root),
        dirs: Mutex::new(vec![root.to_path_buf()]),
        pending: AtomicUsize::new(1),
        wake: Condvar::new(),
    };
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());

    let mut found: Vec<SourceFile> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers).map(|_| scope.spawn(|| drain(&queue))).collect();
        handles.into_iter().filter_map(|worker| worker.join().ok()).flatten().collect()
    });
    found.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    found
}

fn skipped(name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes == b"target" || bytes == b"node_modules"
}

fn skipped_dir(name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes.first() == Some(&b'.') || skipped(name)
}

fn drain(queue: &Queue) -> Vec<SourceFile> {
    let mut found = Vec::with_capacity(256);
    while let Some(dir) = queue.take() {
        visit(&dir, queue, &mut found);
        queue.finish();
    }
    found
}

fn visit(dir: &Path, queue: &Queue, found: &mut Vec<SourceFile>) {
    let entries = match dir.read_dir() {
        Ok(entries) => entries,
        Err(problem) => {
            eprintln!("{}: {problem}", dir.display());
            UNREADABLE.store(true, Ordering::Relaxed);
            return;
        }
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let child = entry.path();
        if child.file_name().is_some_and(|name| skipped(name) || queue.ignore.skips(name)) {
            continue;
        }
        if kind.is_dir() {
            if child.file_name().is_some_and(skipped_dir) {
                continue;
            }
            subdirs.push(child);
        } else if kind.is_file() {
            if let Some(syntax) = syntax_for(&child) {
                let size = entry.metadata().map_or(0, |at| at.len() as usize);
                found.push(SourceFile { path: child, size, syntax: syntax.id });
            }
        }
    }
    queue.give(subdirs);
}
