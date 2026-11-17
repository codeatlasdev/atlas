//! Embeds every `locales/<lang>/<area>.ftl` into the binary: a new language is a new directory,
//! a new area is a new file, and no Rust source lists either.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut langs: Vec<_> = std::fs::read_dir(&root)
        .expect("locales/ directory")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();
    langs.sort_by_key(|e| e.file_name());
    let mut out = String::from("pub(crate) const CATALOGS: &[(&str, &[(&str, &str)])] = &[\n");
    for lang in langs {
        let mut files: Vec<_> = std::fs::read_dir(lang.path())
            .expect("a locale directory")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ftl"))
            .collect();
        files.sort();
        let _ = writeln!(out, "    ({:?}, &[", lang.file_name().to_string_lossy());
        for f in files {
            let area = f.file_stem().unwrap().to_string_lossy().into_owned();
            let path = f.to_string_lossy().replace('\\', "/");
            let _ = writeln!(out, "        ({area:?}, include_str!({path:?})),");
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalogs.rs");
    std::fs::write(dest, out).expect("write catalogs.rs");
}
