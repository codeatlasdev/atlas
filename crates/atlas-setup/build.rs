//! Build inputs of Atlas-Setup.exe:
//!
//! - the payload: the Tauri NSIS installer, embedded with `include_bytes!` from the file named by
//!   `ATLAS_SETUP_PAYLOAD`. Without it the crate still builds (so `cargo build --workspace` works) and
//!   the setup reports that it carries no installer;
//! - the logo: the tiles of `assets/logo-animated.svg` (path data + stagger delays), turned into Rust
//!   constants so the window draws the same mark the app animates, with no SVG parser at run time;
//! - on Windows targets, the icon, version info and the per-monitor DPI manifest.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));

    println!("cargo:rerun-if-env-changed=ATLAS_SETUP_PAYLOAD");
    println!("cargo:rerun-if-changed=build.rs");
    write_payload(&out);
    write_logo(&root, &out);
    // `cfg(windows)` is the build host; the target (cargo-xwin cross builds on Linux) is in the env.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resources(&root);
    }
}

fn write_payload(out: &Path) {
    let code = match std::env::var("ATLAS_SETUP_PAYLOAD") {
        Ok(p) if !p.trim().is_empty() => {
            let path = std::fs::canonicalize(&p)
                .unwrap_or_else(|e| panic!("ATLAS_SETUP_PAYLOAD={p} is not readable: {e}"));
            println!("cargo:rerun-if-changed={}", path.display());
            // `\\?\` prefixes confuse include_bytes! on some toolchains; forward slashes are fine.
            let s = path.display().to_string();
            let s = s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/");
            format!("pub const PAYLOAD: Option<&[u8]> = Some(include_bytes!({s:?}));\n")
        }
        _ => "pub const PAYLOAD: Option<&[u8]> = None;\n".to_string(),
    };
    std::fs::write(out.join("payload.rs"), code).expect("write payload.rs");
}

/// `LOGO_TILES: &[(&str, f32)]`: each tile's path data and its pop-in delay in seconds.
fn write_logo(root: &Path, out: &Path) {
    let svg_path = root.join("../../assets/logo-animated.svg");
    println!("cargo:rerun-if-changed={}", svg_path.display());
    let svg = std::fs::read_to_string(&svg_path).expect("read assets/logo-animated.svg");
    let mut code = String::from("pub const LOGO_TILES: &[(&str, f32)] = &[\n");
    for line in svg.lines().filter(|l| l.contains("class=\"t\"")) {
        let delay = attr_after(line, "--d:", 's').expect("tile delay");
        let d = attr_after(line, " d=\"", '"').expect("tile path");
        let _ = writeln!(code, "    ({d:?}, {delay}),");
    }
    code.push_str("];\n");
    std::fs::write(out.join("logo.rs"), code).expect("write logo.rs");
}

fn attr_after<'a>(line: &'a str, key: &str, end: char) -> Option<&'a str> {
    let rest = &line[line.find(key)? + key.len()..];
    Some(&rest[..rest.find(end)?])
}

fn embed_resources(root: &Path) {
    let icon = root.join("../atlas-desktop/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon(&icon.display().to_string());
    res.set("ProductName", "Atlas");
    res.set("FileDescription", "Atlas Setup");
    res.set("CompanyName", "CodeAtlas");
    res.set("ProductVersion", &version);
    res.set("FileVersion", &version);
    // Per-monitor v2 DPI awareness from the first instruction (crisp at 100-200 %), UTF-8 code
    // page, and no compatibility shims; `asInvoker`: a per-user install needs no elevation.
    res.set_manifest(
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
</assembly>"#,
    );
    res.compile().expect("compile the Windows resources");
}
