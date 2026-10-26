//! Failed navigations of a browser tab, as the `browser://<tab>` event the frontend's `browserLoad.ts` reads.
//!
//! wry reports `Started`/`Finished` only. A refused connection, an unknown host or a bad certificate ends in
//! WebKit's `didFailProvisionalNavigation` (and a failure after the first byte in `didFailNavigation`), which wry
//! does not implement: the tab would stay blank with no event at all. `browser_fail_mac.rs` hooks those two
//! callbacks and hands the error here; the shape is pure so it is unit-tested without a webview.

use serde_json::{Value, json};

/// `browser://<tab>` payload for a failed load of `url`. `domain`/`code` are the `NSError` ones, so the frontend
/// can tell a refused connection from a bad certificate; `message` is WebKit's own localized text. Mirrors
/// `LoadEvent` (`event: "fail"`) in `apps/desktop/src/lib/browserLoad.ts`.
pub fn payload(url: &str, code: i64, domain: &str, message: &str) -> Value {
    json!({"event": "fail", "url": url, "code": code, "domain": domain, "message": message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_what_the_frontend_reads() {
        let p = payload(
            "http://localhost:1/",
            -1004,
            "NSURLErrorDomain",
            "Could not connect to the server.",
        );
        assert_eq!(p["event"], "fail");
        assert_eq!(p["url"], "http://localhost:1/");
        assert_eq!(p["code"], -1004);
        assert_eq!(p["domain"], "NSURLErrorDomain");
        assert_eq!(p["message"], "Could not connect to the server.");
    }

    #[test]
    fn quotes_in_a_message_stay_valid_json() {
        let p = payload("http://a/", -1, "D", "bad \"thing\"\n");
        let back: Value = serde_json::from_str(&p.to_string()).unwrap();
        assert_eq!(back["message"], "bad \"thing\"\n");
    }
}
