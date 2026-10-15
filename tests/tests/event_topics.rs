//! Workspace-wide lint on event topic naming.
//!
//! `sororail_common::events` makes the topic scheme normative: every
//! `#[contractevent]` declares its fixed topics as exactly `[contract, action]`,
//! where `contract` is the crate's short name. Indexers filter on that first
//! topic, so an event that drifts to a different name silently disappears from
//! every dashboard built on the convention.
//!
//! The convention cannot be checked from inside the type system -- the topics
//! are string literals passed to an attribute macro -- so this test reads the
//! contract sources directly. It walks every `contracts/<crate>/src` tree, so a
//! new contract crate or a new file holding events is covered without editing
//! this test.

#![allow(clippy::arithmetic_side_effects)]

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Crates under `contracts/` that are libraries rather than contracts and so
/// must not declare events of their own. `common` mentions `#[contractevent]`
/// only inside doc comments, which the scan below already ignores.
const NON_CONTRACT_CRATES: &[&str] = &["common"];

/// One `#[contractevent(topics = [...])]` occurrence.
struct Occurrence {
    file: PathBuf,
    line: usize,
    topics: Vec<String>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the tests crate lives one level below the workspace root")
        .to_path_buf()
}

/// Every contract crate directory, as `(short_name, path)`.
///
/// The short name is the directory name, which is also what the convention
/// uses as the first topic (`contracts/batch_payout` -> `"batch_payout"`).
fn contract_crates() -> Vec<(String, PathBuf)> {
    let contracts = workspace_root().join("contracts");
    let mut crates: Vec<(String, PathBuf)> = fs::read_dir(&contracts)
        .expect("contracts/ is readable")
        .map(|entry| entry.expect("directory entry is readable").path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("crate directory name is UTF-8")
                .to_string();
            (name, path)
        })
        .collect();
    crates.sort();
    crates
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Parses the string literals out of `topics = [ ... ]`, if present.
///
/// Returns `None` when the attribute has no `topics` list (a bare
/// `#[contractevent]`, which would fall back to the struct name and so breaks
/// the convention too).
fn parse_topics(attr: &str) -> Option<Vec<String>> {
    let after = &attr[attr.find("topics")?..];
    let open = after.find('[')?;
    let close = after.find(']')?;
    let list = &after[open + 1..close];
    Some(
        list.split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.trim_matches('"').to_string())
            .collect(),
    )
}

/// Every `#[contractevent ...]` attribute in the crate's `src/`, excluding
/// comments and doc comments.
fn occurrences(crate_dir: &Path) -> Vec<Occurrence> {
    let mut files = Vec::new();
    rust_files(&crate_dir.join("src"), &mut files);
    files.sort();

    let mut found = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file).expect("source file is readable UTF-8");
        let lines: Vec<&str> = source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim_start();
            if trimmed.starts_with("#[contractevent") {
                // An attribute may be wrapped over several lines by rustfmt;
                // join until the closing `)]` (or `]` for a bare attribute).
                let start = i;
                let mut attr = trimmed.to_string();
                let closed = |a: &str| a.contains(")]") || a == "#[contractevent]";
                while !closed(&attr) && i + 1 < lines.len() {
                    i += 1;
                    attr.push_str(lines[i].trim());
                }
                found.push(Occurrence {
                    file: file.clone(),
                    line: start + 1,
                    topics: parse_topics(&attr).unwrap_or_default(),
                });
            }
            i += 1;
        }
    }
    found
}

fn is_snake_case(s: &str) -> bool {
    let valid_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
    !s.is_empty() && s.chars().all(valid_char) && !s.starts_with('_') && !s.ends_with('_')
}

#[test]
fn every_event_uses_contract_then_action_topics() {
    let root = workspace_root();
    let mut violations = Vec::new();

    for (name, dir) in contract_crates() {
        for occ in occurrences(&dir) {
            let at = format!(
                "{}:{}",
                occ.file.strip_prefix(&root).unwrap_or(&occ.file).display(),
                occ.line
            );
            if NON_CONTRACT_CRATES.contains(&name.as_str()) {
                violations.push(format!("{at}: library crate `{name}` declares an event"));
                continue;
            }
            match occ.topics.as_slice() {
                [contract, action] => {
                    if *contract != name {
                        violations.push(format!(
                            "{at}: first topic is \"{contract}\", expected crate name \"{name}\""
                        ));
                    }
                    if !is_snake_case(action) {
                        violations.push(format!(
                            "{at}: action topic \"{action}\" is not lower snake_case"
                        ));
                    }
                }
                other => violations.push(format!(
                    "{at}: expected exactly two fixed topics [\"{name}\", action], found {other:?}"
                )),
            }
        }
    }

    assert!(
        violations.is_empty(),
        "event topics break the convention in sororail_common::events:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn every_contract_crate_declares_at_least_one_event() {
    // Guards the scan itself: if a refactor moved events somewhere the walk
    // above cannot see (or changed the attribute spelling), the topic test
    // would pass vacuously. Every contract emits at least one event today.
    let silent: Vec<String> = contract_crates()
        .into_iter()
        .filter(|(name, _)| !NON_CONTRACT_CRATES.contains(&name.as_str()))
        .filter(|(_, dir)| occurrences(dir).is_empty())
        .map(|(name, _)| name)
        .collect();

    assert!(
        silent.is_empty(),
        "no #[contractevent] found in: {silent:?}"
    );
}

#[test]
fn batch_payout_executed_follows_the_convention() {
    // batch_payout is the structurally odd one out -- stateless, no
    // `storage.rs` -- so pin its one event explicitly (#64).
    let (_, dir) = contract_crates()
        .into_iter()
        .find(|(name, _)| name == "batch_payout")
        .expect("contracts/batch_payout exists");
    let topics: Vec<Vec<String>> = occurrences(&dir).into_iter().map(|o| o.topics).collect();
    assert!(
        topics.contains(&vec!["batch_payout".to_string(), "executed".to_string()]),
        "batch_payout events: {topics:?}"
    );
}
