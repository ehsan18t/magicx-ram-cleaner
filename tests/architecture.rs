//! Architecture tests: enforce the layering documented in `src/lib.rs` and
//! `docs/ARCHITECTURE.md`, so a stray import cannot quietly couple layers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Each layer and the layers it may use (besides itself and the shared
/// `strings` and `ids` modules).
/// Mirrors the table in `docs/ARCHITECTURE.md`.
const ALLOWED: &[(&str, &[&str])] = &[
    ("platform", &[]),
    ("memory", &["platform"]),
    ("engine", &["memory", "platform"]),
    ("integration", &["platform"]),
    ("cli", &["engine", "integration", "memory", "platform"]),
    ("gui", &["engine", "integration", "memory", "platform"]),
    (
        "app",
        &["cli", "gui", "engine", "integration", "memory", "platform"],
    ),
];

/// Shared modules every layer may use.
const SHARED: &[&str] = &["strings", "ids"];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("readable directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Module path of a source file, e.g. `src/gui/panels/about.rs` ->
/// `["gui", "panels", "about"]`, `src/gui/mod.rs` -> `["gui"]`.
fn module_path(file: &Path) -> Vec<String> {
    let relative = file.strip_prefix(src_dir()).expect("file under src");
    let mut parts: Vec<String> = relative
        .iter()
        .map(|p| p.to_string_lossy().trim_end_matches(".rs").to_owned())
        .collect();
    if parts.last().is_some_and(|p| p == "mod") {
        parts.pop();
    }
    parts
}

/// Code of a line without its `//` comment.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or_default()
}

/// Top-level crate modules a line of `module`'s code refers to, via
/// `crate::x`, `crate::{x, y}` or `super::...::x` paths.
fn referenced_layers(line: &str, module: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let code = code(line);

    // crate::x / crate::{x, y::z, ...}
    for (start, _) in code.match_indices("crate::") {
        let rest = &code[start + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            let mut depth = 0usize;
            let mut item = String::new();
            for c in group.chars() {
                match c {
                    '{' => depth += 1,
                    '}' if depth == 0 => break,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        found.extend(first_segment(&item));
                        item.clear();
                    }
                    _ if depth == 0 => item.push(c),
                    _ => {}
                }
            }
            found.extend(first_segment(&item));
        } else {
            found.extend(first_segment(rest));
        }
    }

    // super::super::x: climbing past the layer root reaches the crate root.
    let mut rest = code;
    while let Some(start) = rest.find("super::") {
        let mut tail = &rest[start..];
        let mut levels = 0;
        while let Some(next) = tail.strip_prefix("super::") {
            levels += 1;
            tail = next;
        }
        if levels >= module.len() {
            found.extend(first_segment(tail));
        }
        rest = tail;
    }
    found
}

/// The first path segment of `path` (`gui::theme` -> `gui`), if any.
fn first_segment(path: &str) -> Option<String> {
    let name: String = path
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[test]
fn layers_only_use_the_layers_they_are_allowed_to() {
    let layers: Vec<&str> = ALLOWED.iter().map(|(layer, _)| *layer).collect();
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);

    let mut violations = Vec::new();
    for file in files {
        let module = module_path(&file);
        let Some((layer, allowed)) = ALLOWED
            .iter()
            .find(|(l, _)| Some(*l) == module.first().map(String::as_str))
        else {
            continue; // lib.rs, main.rs and shared modules
        };
        let source = std::fs::read_to_string(&file).expect("readable source file");
        for (number, line) in source.lines().enumerate() {
            for target in referenced_layers(line, &module) {
                let is_layer = layers.contains(&target.as_str());
                if is_layer
                    && target != *layer
                    && !allowed.contains(&target.as_str())
                    && !SHARED.contains(&target.as_str())
                {
                    violations.push(format!(
                        "{}:{}: `{layer}` must not depend on `{target}`",
                        file.display(),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "layering violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn only_the_platform_layer_may_allow_unsafe_code() {
    // The compiler denies unsafe code crate-wide; this guards the exception
    // list, so nobody quietly opts another module back in.
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    let mut offenders = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("readable source file");
        let lines: Vec<&str> = source.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let code = code(line);
            if !(code.contains("allow(unsafe_code)") || code.contains("expect(unsafe_code)")) {
                continue;
            }
            let next = lines.get(number + 1).copied().unwrap_or_default();
            let is_platform_decl = file.ends_with("lib.rs")
                && code.trim() == "#[allow(unsafe_code)]"
                && next.trim() == "pub mod platform;";
            if !is_platform_decl {
                offenders.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "unsafe code allowed outside platform:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn reference_parser_sees_all_import_styles() {
    let in_memory = ["memory".to_owned(), "format".to_owned()];
    let found = |line: &str| referenced_layers(line, &in_memory);
    assert!(found("use crate::gui::theme;").contains("gui"));
    assert!(found("use crate::{cli, engine::Cleaner};").contains("cli"));
    assert!(found("use crate::{cli, engine::Cleaner};").contains("engine"));
    assert!(found("use crate::{a::{b, c}, gui};").contains("gui"));
    assert!(found("use super::super::gui::theme;").contains("gui"));
    assert!(
        found("use super::Thing;").is_empty(),
        "stays inside its own layer"
    );
    assert!(
        found("// use crate::gui;").is_empty(),
        "comments are ignored"
    );
}
