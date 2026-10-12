fn main() {
    // tauri's generate_context! embeds frontendDist at compile time and fails when
    // the dir is missing (fresh clone, CI, `cargo test`); the real build fills it.
    std::fs::create_dir_all(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/build"
    ))
    .expect("create frontendDist dir");
    tauri_build::build()
}
