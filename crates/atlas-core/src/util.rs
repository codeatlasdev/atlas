/// Expand a leading `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        let home = crate::paths::home_dir();
        format!("{}{}", home.display(), &path[1..])
    } else {
        path.to_string()
    }
}

/// Read and parse a JSON file; a missing file yields `T::default()`.
pub fn load_json_or_default<T: Default + serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> anyhow::Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        // An absolute home on this OS (`C:\…` on Windows, where `/home/dev` is not absolute).
        let home = std::env::temp_dir().join("home-dev");
        unsafe { std::env::set_var("HOME", &home) };
        assert_eq!(expand_tilde("~/foo"), format!("{}/foo", home.display()));
        assert_eq!(expand_tilde("/absolute"), "/absolute");
        assert_eq!(expand_tilde("relative"), "relative");
    }

    #[test]
    fn load_json_or_default_handles_missing_and_present() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("v.json");
        assert_eq!(
            load_json_or_default::<Vec<u8>>(&p).unwrap(),
            Vec::<u8>::new()
        );
        std::fs::write(&p, "[1,2]").unwrap();
        assert_eq!(load_json_or_default::<Vec<u8>>(&p).unwrap(), vec![1, 2]);
        std::fs::write(&p, "nope").unwrap();
        assert!(load_json_or_default::<Vec<u8>>(&p).is_err());
    }
}
