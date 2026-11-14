//! Names and port offsets for new worktrees.

use super::registry::Entry;

const SEA_NAMES: [&str; 48] = [
    "anchovy",
    "barracuda",
    "beluga",
    "blenny",
    "clownfish",
    "cod",
    "coral",
    "cuttlefish",
    "dolphin",
    "dugong",
    "eel",
    "flounder",
    "goby",
    "grouper",
    "haddock",
    "halibut",
    "herring",
    "jellyfish",
    "krill",
    "lamprey",
    "limpet",
    "lobster",
    "mackerel",
    "manatee",
    "marlin",
    "minnow",
    "narwhal",
    "nautilus",
    "octopus",
    "orca",
    "oyster",
    "pufferfish",
    "remora",
    "salmon",
    "sardine",
    "scallop",
    "seahorse",
    "shrimp",
    "snapper",
    "squid",
    "starfish",
    "stingray",
    "sturgeon",
    "swordfish",
    "tuna",
    "turbot",
    "urchin",
    "walrus",
];

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    out.chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

pub fn random_sea_name() -> String {
    let n = crate::paths::random_hex(2)
        .ok()
        .and_then(|h| usize::from_str_radix(&h, 16).ok())
        .unwrap_or(0);
    SEA_NAMES[n % SEA_NAMES.len()].to_string()
}

/// Smallest free multiple of 100 among the repo's worktrees.
pub fn free_offset(entries: &[Entry], repo: &str) -> u16 {
    (1..100)
        .map(|k| k * 100)
        .find(|o| {
            !entries
                .iter()
                .any(|e| e.parent.as_deref() == Some(repo) && e.port_offset == *o)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Fix: Login bug!!"), "fix-login-bug");
        assert_eq!(slug("  --  "), "");
        assert_eq!(slug(&"a".repeat(60)).len(), 40);
    }

    #[test]
    fn offsets_skip_used_slots_per_repo() {
        let e = |parent: &str, o| Entry {
            id: String::new(),
            root: String::new(),
            pinned: false,
            added: String::new(),
            parent: Some(parent.into()),
            port_offset: o,
            branch: None,
            host: None,
            identity: None,
        };
        assert_eq!(free_offset(&[], "r"), 100);
        assert_eq!(
            free_offset(&[e("r", 100), e("r", 300), e("x", 200)], "r"),
            200
        );
    }
}
