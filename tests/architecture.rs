//! Architecture tests: enforce the layering documented in `src/lib.rs` and
//! `docs/ARCHITECTURE.md`, so a stray import cannot quietly couple layers.

use std::path::{Path, PathBuf};

/// Layers from the bottom up. A layer may use layers listed before it, never
/// layers listed after it. (`app` sits on top and may use everything;
/// `strings` is shared text usable from anywhere.)
const LAYERS: &[&str] = &[
    "platform",
    "memory",
    "engine",
    "integration",
    "cli",
    "gui",
    "app",
];

/// Layers that are independent of each other despite their order in
/// [`LAYERS`]: the CLI and the GUI are siblings.
const SIBLINGS: &[(&str, &str)] = &[
    ("cli", "gui"),
    ("gui", "cli"),
    ("engine", "integration"),
    ("integration", "engine"),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("readable source directory")
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

/// Source files belonging to `layer` (`src/<layer>.rs` or `src/<layer>/**`).
fn layer_files(layer: &str) -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let dir = src.join(layer);
    if dir.is_dir() {
        rust_files(&dir, &mut files);
    }
    let file = src.join(format!("{layer}.rs"));
    if file.is_file() {
        files.push(file);
    }
    assert!(!files.is_empty(), "no source files for layer {layer}");
    files
}

#[test]
fn layers_only_depend_downwards() {
    let mut violations = Vec::new();
    for (level, layer) in LAYERS.iter().enumerate() {
        let forbidden: Vec<&str> = LAYERS[level + 1..]
            .iter()
            .copied()
            .chain(
                SIBLINGS
                    .iter()
                    .filter(|(from, _)| from == layer)
                    .map(|(_, to)| *to),
            )
            .collect();
        for file in layer_files(layer) {
            let source = std::fs::read_to_string(&file).expect("readable source file");
            for (number, line) in source.lines().enumerate() {
                let code = line.split("//").next().unwrap_or_default();
                for target in &forbidden {
                    if code.contains(&format!("crate::{target}::"))
                        || code.contains(&format!("crate::{target};"))
                        || code.contains(&format!("crate::{target}}}"))
                        || code.contains(&format!("crate::{target},"))
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
    }
    assert!(
        violations.is_empty(),
        "layering violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn unsafe_code_lives_only_in_the_platform_layer() {
    let mut offenders = Vec::new();
    for layer in LAYERS.iter().filter(|l| **l != "platform") {
        for file in layer_files(layer) {
            let source = std::fs::read_to_string(&file).expect("readable source file");
            for (number, line) in source.lines().enumerate() {
                let code = line.split("//").next().unwrap_or_default();
                if code.contains("unsafe {")
                    || code.contains("unsafe fn")
                    || code.contains("unsafe impl")
                {
                    offenders.push(format!("{}:{}", file.display(), number + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "unsafe outside platform:\n{}",
        offenders.join("\n")
    );
}
