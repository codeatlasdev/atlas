//! Embeds the repository `skills/` tree into the binary.
//!
//! Generates `$OUT_DIR/embedded_skills.rs` containing a const table of
//! `(skill_dir, relative_file, contents)` built with `include_str!`, so skills
//! are version-locked to the binary that ships them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let skills_root = manifest_dir.join("../../skills");
    println!("cargo:rerun-if-changed={}", skills_root.display());

    let mut files: Vec<(String, String, PathBuf)> = Vec::new();
    if skills_root.is_dir() {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&skills_root)
            .expect("read skills/")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            collect(&dir, &dir, &name, &mut files);
        }
    }

    let mut out = String::from("pub(crate) const EMBEDDED_FILES: &[(&str, &str, &str)] = &[\n");
    for (skill, rel, abs) in &files {
        let abs = abs.canonicalize().expect("canonicalize skill file");
        println!("cargo:rerun-if-changed={}", abs.display());
        writeln!(
            out,
            "    ({skill:?}, {rel:?}, include_str!({:?})),",
            abs.display().to_string()
        )
        .unwrap();
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded_skills.rs");
    std::fs::write(dest, out).expect("write embedded_skills.rs");
}

/// Recursively collect `.md` files under a skill directory (relative paths use `/`).
fn collect(root: &Path, dir: &Path, skill: &str, out: &mut Vec<(String, String, PathBuf)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read skill dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        if fname.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect(root, &p, skill, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            out.push((skill.to_string(), rel, p));
        }
    }
}
