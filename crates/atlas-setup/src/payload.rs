//! The embedded Tauri NSIS installer. `build.rs` fills `PAYLOAD` from `ATLAS_SETUP_PAYLOAD`; a build
//! without it still compiles and reports, when run, that it carries nothing.

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

/// The message of a setup built without a payload.
pub const MISSING: &str = "This Atlas-Setup.exe was built without an installer inside \
(ATLAS_SETUP_PAYLOAD was not set at build time). Download Atlas-Setup.exe from the release page.";

/// `MZ`: a Windows executable. An embedded file that is not one means a wrong path at build time.
pub fn is_pe(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MZ")
}

/// The payload to unpack, or why there is none.
pub fn bytes() -> Result<&'static [u8], String> {
    check(PAYLOAD)
}

fn check(p: Option<&'static [u8]>) -> Result<&'static [u8], String> {
    match p {
        None => Err(MISSING.into()),
        Some(b) if !is_pe(b) => {
            Err("The installer inside this setup is not a Windows executable.".into())
        }
        Some(b) => Ok(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setup_without_payload_says_so_in_words() {
        let e = check(None).unwrap_err();
        assert!(
            e.contains("without an installer") && e.contains("release page"),
            "{e}"
        );
    }

    #[test]
    fn the_payload_of_this_build_is_either_a_real_executable_or_reported_missing() {
        // `cargo test` has no ATLAS_SETUP_PAYLOAD, a release build has: both must be coherent.
        match bytes() {
            Ok(b) => assert!(is_pe(b)),
            Err(e) => assert_eq!(e, MISSING),
        }
    }

    #[test]
    fn an_embedded_non_executable_is_refused() {
        // Incident guard: an HTML 404 saved by a failed download must never be run.
        assert!(check(Some(b"<html>404</html>")).is_err());
        assert!(check(Some(b"")).is_err());
        assert!(check(Some(b"MZ\x90\x00")).is_ok());
    }
}
