//! Windows version resource of `atlas.exe` (ProductName, versions, description, company): shown in the
//! file's Properties and checked by the code-signing service (packaging/windows/signpath/). No-op for
//! other targets.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // `cfg(windows)` is the build host; the target (cargo-xwin cross builds on Linux) is in the env.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut res = tauri_winres::WindowsResource::new();
    res.set("ProductName", "Atlas");
    res.set("FileDescription", "Atlas CLI");
    res.set("CompanyName", "CodeAtlas");
    res.set("ProductVersion", &version);
    res.set("FileVersion", &version);
    res.compile().expect("compile the Windows resources");
}
