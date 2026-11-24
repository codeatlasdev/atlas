//! `atlas crash-fingerprint` (#320) against the real binary: the fingerprint of a crash report file, with and without
//! the symbols' `resolve` output, and its refusals.

use atlas_i18n::t_in;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(["crash-fingerprint"])
        .args(args)
        .env("ATLAS_LANG", "en")
        .output()
        .expect("spawn atlas")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

const NATIVE: &str = "version = \"1.5.0\"\ncommit = \"abc\"\nbuild_id = \"b1\"\nload_bias = \"0x55d000000000\"\ntimestamp = 1\nkind = \"native\"\nsignal = \"SIGSEGV\"\nmsg = \"native crash: SIGSEGV\"\nframes = [\"0x55d000a11111\", \"0x55d000a22222\", \"0x7f0000011111\", \"0x55d000c33333\"]\n";

#[test]
fn a_panic_report_prints_the_fingerprint_it_carries_and_an_old_one_the_computed_value() {
    let dir = tempfile::tempdir().unwrap();
    let want = atlas_core::fingerprint::fingerprint("boom", "crates/a/src/b.rs:1:2");
    let new = dir.path().join("crash-1.toml");
    std::fs::write(
        &new,
        format!("msg = \"boom\"\nlocation = \"crates/a/src/b.rs:1:2\"\nfingerprint = \"{want}\"\n"),
    )
    .unwrap();
    let old = dir.path().join("crash-2.toml");
    std::fs::write(
        &old,
        "msg = \"boom\"\nlocation = \"/x/crates/a/src/b.rs:9:9\"\n",
    )
    .unwrap();
    for f in [&new, &old] {
        let o = run(&[f.to_str().unwrap()]);
        assert!(o.status.success(), "{o:?}");
        assert_eq!(out(&o), want);
    }
}

#[test]
fn a_native_report_is_keyed_by_the_resolved_function_when_given_and_by_the_offset_when_not() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("crash-native-1-2.toml");
    std::fs::write(&report, NATIVE).unwrap();
    let resolved = dir.path().join("resolved.txt");
    std::fs::write(
        &resolved,
        "0x55d000a11111 atlas_core::crash_native::imp::handler /b/crates/atlas-core/src/crash_native.rs:250\n0x55d000c33333 atlas_desktop::term::paint /b/crates/atlas-desktop/src/term/paint.rs:88\n",
    )
    .unwrap();
    let by_offset = run(&[report.to_str().unwrap()]);
    let by_symbol = run(&[
        report.to_str().unwrap(),
        "--resolved",
        resolved.to_str().unwrap(),
    ]);
    assert!(by_offset.status.success() && by_symbol.status.success());
    assert_eq!(
        out(&by_symbol),
        atlas_core::fingerprint::with_key(
            "native crash: SIGSEGV",
            "atlas_desktop::term::paint|crates/atlas-desktop/src/term/paint.rs"
        )
    );
    assert_eq!(
        Some(out(&by_offset)),
        atlas_core::crash_fingerprint::of_report(NATIVE, None)
    );
    assert_ne!(out(&by_offset), out(&by_symbol));
}

#[test]
fn a_report_that_cannot_be_read_or_keyed_says_so_and_fails() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("crash-3.toml");
    std::fs::write(&empty, "timestamp = 1\n").unwrap();
    let p = empty.to_str().unwrap();
    let o = run(&[p]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains(&t_in!(
        "en",
        "cmd_misc_crash_fingerprint_none",
        path = p
    )));
    let gone = dir.path().join("nope.toml");
    let o = run(&[gone.to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains(gone.to_str().unwrap()));
}
