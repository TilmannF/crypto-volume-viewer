#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "regression guard test asserts against source text, not runtime behavior"
)]

//! Regression guard: `cryptovol-app` is the framework-neutral application
//! core shared by the CLI and the GUI. It must never prompt for input or
//! print to stdout/stderr itself, and it must never depend on either the
//! CLI's argument-parsing framework or the GUI's desktop framework; callers
//! own all terminal I/O and all framework wiring.
//!
//! This test walks `crates/cryptovol-app/src` recursively and asserts none
//! of its source files reference `println!`, `eprintln!`, `print!(`,
//! `eprint!(`, `rpassword`, `clap`, or `tauri`.

use std::path::{Path, PathBuf};

const FORBIDDEN_SUBSTRINGS: &[&str] = &[
    "println!",
    "eprintln!",
    "print!(",
    "eprint!(",
    "rpassword",
    "clap",
    "tauri",
];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_terminal_io_or_gui_framework_deps_in_cryptovol_app_src() {
    let mut files = Vec::new();
    collect_rs_files(&src_dir(), &mut files);
    assert!(
        !files.is_empty(),
        "expected to find at least one .rs file under crates/cryptovol-app/src"
    );

    let mut violations = Vec::new();
    for path in &files {
        let contents = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", path.display()));
        for needle in FORBIDDEN_SUBSTRINGS {
            if contents.contains(needle) {
                violations.push(format!("{}: contains {needle:?}", path.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "cryptovol-app must never perform terminal I/O, prompt for passwords, or depend on \
         clap/tauri; found:\n{}",
        violations.join("\n")
    );
}
