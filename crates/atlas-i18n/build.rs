//! Embeds the messages this crate serves: `i18n/messages/<area>/<locale>.json` for every `common*`
//! area and every area `i18n/surfaces.json` lists under `surfaces.rust.areas`. Nothing is
//! generated or committed; a message that does not parse fails the BUILD, naming the file and key.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[path = "src/message.rs"]
#[allow(dead_code)]
mod message;

fn main() {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n");
    println!("cargo:rerun-if-changed={}", root.join("messages").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("surfaces.json").display()
    );
    let surfaces: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("surfaces.json")).expect("i18n/surfaces.json"),
    )
    .expect("i18n/surfaces.json is JSON");
    let mut areas: Vec<String> = surfaces["surfaces"]["rust"]["areas"]
        .as_array()
        .expect("surfaces.rust.areas")
        .iter()
        .map(|a| a.as_str().expect("an area name").to_owned())
        .collect();
    let messages = root.join("messages");
    for entry in std::fs::read_dir(&messages)
        .expect("i18n/messages")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && (name == "common" || name.starts_with("common_")) {
            areas.push(name);
        }
    }
    areas.sort();
    areas.dedup();

    // locale -> [(area, absolute path)]
    let mut by_locale: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for area in &areas {
        let dir = messages.join(area);
        let files = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("area `{area}`: {e}"));
        for f in files.flatten() {
            let path = f.path();
            let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if path.extension().is_none_or(|e| e != "json") || stem == "meta" || stem == "lock" {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a catalog file");
            let value: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            for (key, v) in value
                .as_object()
                .unwrap_or_else(|| panic!("{}: not an object", path.display()))
            {
                if let Err(e) = message::parse(v) {
                    panic!("{}: `{key}`: {e}", path.display());
                }
                if !key.starts_with(&format!("{area}_")) {
                    panic!("{}: `{key}` must start with `{area}_`", path.display());
                }
                let other = if stem == "en" {
                    owner.insert(key.clone(), area.clone())
                } else {
                    None
                };
                if let Some(other) = other.filter(|o| o != area) {
                    panic!("key `{key}` is defined in areas `{other}` and `{area}`");
                }
            }
            println!("cargo:rerun-if-changed={}", path.display());
            by_locale
                .entry(stem)
                .or_default()
                .push((area.clone(), path));
        }
    }

    let mut out = String::from("pub(crate) const CATALOGS: &[(&str, &[(&str, &str)])] = &[\n");
    for (locale, files) in &by_locale {
        let _ = writeln!(out, "    ({locale:?}, &[");
        for (area, path) in files {
            let p = path.to_string_lossy().replace('\\', "/");
            let _ = writeln!(out, "        ({area:?}, include_str!({p:?})),");
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalogs.rs");
    std::fs::write(dest, out).expect("write catalogs.rs");
}
