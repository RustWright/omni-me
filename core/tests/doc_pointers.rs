//! Source comments that point into the book must point at something real.
//!
//! Rationale in `docs/src/retrieval.md`.

use std::fs;
use std::path::{Path, PathBuf};

/// Every tree whose comments may reference the book — test trees included, and
/// workspace members or not.
const SOURCE_TREES: [&str; 7] = [
    "core/src",
    "core/tests",
    "agent/src",
    "server/src",
    "server/tests",
    "tauri-app/frontend/src",
    "tauri-app/src-tauri/src",
];

/// The opening of a reference, backtick included so prose never matches.
///
/// ⚠️ There is no escape for an *example* path: a backticked book path anywhere in
/// a comment is read as a real reference, including in this file. Illustrate the
/// syntax without the leading backtick.
const MARKER: &str = "`docs/src/";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core/ always has a parent")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Contiguous runs of comment lines, markers stripped and joined with a space.
///
/// ⚠️ Joined per *run*, not per file. An anchor may wrap across two lines — as
/// `agent/src/main.rs`'s does — so line-by-line scanning would miss it; joining a
/// whole file would instead let two unrelated blocks form a reference that is not
/// written anywhere.
fn comment_blocks(source: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();

    for line in source.lines() {
        let Some(rest) = line.trim_start().strip_prefix("//") else {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            continue;
        };
        let text = rest.trim_start_matches('!').trim_start_matches('/').trim();
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(text);
    }

    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

struct Reference {
    page: String,
    anchor: Option<String>,
}

/// The common form: a page reference, then `§` and an anchor, quoted or a bare word.
fn trailing_anchor(tail: &str) -> Option<String> {
    let rest = tail.trim_start().strip_prefix('§')?.trim_start();
    match rest.strip_prefix('"') {
        Some(quoted) => Some(quoted[..quoted.find('"')?].to_string()),
        None => Some(
            rest.split_whitespace()
                .next()?
                .trim_end_matches(['.', ',', ';', ':'])
                .to_string(),
        ),
    }
}

/// The other phrasing in use: a quoted anchor, then `section of`, then the page.
fn leading_anchor(before: &str) -> Option<String> {
    let head = before.trim_end().strip_suffix("section of")?.trim_end();
    let close = head.rfind('"')?;
    let open = head[..close].rfind('"')?;
    Some(head[open + 1..close].to_string())
}

fn references(block: &str) -> Vec<Reference> {
    let mut refs = Vec::new();
    let mut rest = block;

    while let Some(start) = rest.find(MARKER) {
        let after = &rest[start + MARKER.len()..];
        let Some(end) = after.find('`') else { break };
        let tail = &after[end + 1..];
        refs.push(Reference {
            page: after[..end].to_string(),
            anchor: trailing_anchor(tail).or_else(|| leading_anchor(&rest[..start])),
        });
        rest = tail;
    }
    refs
}

/// Headings match on containment so a heading may carry a `**(planned)**` suffix
/// without every pointer having to repeat it.
fn has_heading(markdown: &str, anchor: &str) -> bool {
    let needle = anchor.to_lowercase();
    markdown
        .lines()
        .filter(|line| line.starts_with('#'))
        .any(|line| line.to_lowercase().contains(&needle))
}

#[test]
fn every_docs_pointer_resolves() {
    let root = workspace_root();
    let book = root.join("docs/src");

    let mut files = Vec::new();
    for tree in SOURCE_TREES {
        rust_files(&root.join(tree), &mut files);
    }
    files.sort();

    let mut problems = Vec::new();
    let mut checked = 0usize;

    for file in &files {
        let Ok(source) = fs::read_to_string(file) else {
            continue;
        };
        let shown = file.strip_prefix(&root).unwrap_or(file).display();

        for block in comment_blocks(&source) {
            for reference in references(&block) {
                checked += 1;
                let Ok(markdown) = fs::read_to_string(book.join(&reference.page)) else {
                    problems.push(format!(
                        "{shown} -> docs/src/{}: no such page",
                        reference.page
                    ));
                    continue;
                };
                if let Some(anchor) = reference.anchor
                    && !has_heading(&markdown, &anchor)
                {
                    problems.push(format!(
                        "{shown} -> docs/src/{} § {anchor}: no heading matches",
                        reference.page
                    ));
                }
            }
        }
    }

    // Guards the parser itself: a scan that silently matched nothing would report
    // a clean run, which is the same failure it exists to catch.
    assert!(
        checked >= 15,
        "only {checked} book references found across {} files — the parser is broken, \
         not the codebase",
        files.len()
    );

    assert!(
        problems.is_empty(),
        "comments promise rationale the book does not hold. Nothing else reports \
         this: a pointer is prose, so it rots silently while reading as authority.\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn every_book_page_is_in_the_summary() {
    let book = workspace_root().join("docs/src");
    let summary = fs::read_to_string(book.join("SUMMARY.md")).expect("the book has a SUMMARY.md");

    let mut missing = Vec::new();
    for entry in fs::read_dir(&book).expect("docs/src is readable").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            let name = path
                .file_name()
                .expect("a file has a name")
                .to_string_lossy();
            if name != "SUMMARY.md" && !summary.contains(name.as_ref()) {
                missing.push(name.into_owned());
            }
        }
    }
    missing.sort();

    assert!(
        missing.is_empty(),
        "page(s) not listed in SUMMARY.md, so mdBook renders them nowhere and no \
         reader can reach them: {}",
        missing.join(", ")
    );
}
