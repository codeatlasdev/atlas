//! Capture settings (`settings.json` → `capture`, camelCase): where files go, the hotkeys, what happens after a
//! capture. Pure data + validation; applying it (registering hotkeys, creating the folder) is `shots::mod`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::hotkeys;

/// The capture actions that have a hotkey. Order is the order of the Settings rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hotkey {
    /// One HUD over the frozen screen to pick region, window, screen, record, GIF or text (CleanShot ⇧⌘5 style).
    AllInOne,
    /// Crosshair overlay: drag a region (Space switches to window mode).
    Region,
    /// The same region as the last region capture, now.
    Previous,
    /// The window under the pointer, picked by clicking it.
    Window,
    /// The display under the pointer, now.
    Fullscreen,
    /// Drag a region and read its text (on-device OCR).
    Ocr,
    /// Record a region (or the display) to mp4.
    Record,
    /// Record a region to GIF.
    Gif,
}

impl Hotkey {
    /// The action named `name` (`region`, `window`, `fullscreen`, `record`, `gif`, `allInOne`, `previous`, `ocr`):
    /// the names of `Keys`' fields.
    pub fn from_name(name: &str) -> Option<Hotkey> {
        Hotkey::ALL.into_iter().find(|h| {
            serde_json::to_value(h)
                .ok()
                .and_then(|v| v.as_str().map(|s| s == name))
                == Some(true)
        })
    }

    pub const ALL: [Hotkey; 8] = [
        Hotkey::AllInOne,
        Hotkey::Region,
        Hotkey::Previous,
        Hotkey::Window,
        Hotkey::Fullscreen,
        Hotkey::Ocr,
        Hotkey::Record,
        Hotkey::Gif,
    ];

    /// Name used in refusal sentences.
    pub fn label(self) -> &'static str {
        match self {
            Hotkey::AllInOne => "All-in-one capture",
            Hotkey::Region => "Capture region",
            Hotkey::Previous => "Capture previous area",
            Hotkey::Window => "Capture window",
            Hotkey::Fullscreen => "Capture screen",
            Hotkey::Ocr => "Capture text",
            Hotkey::Record => "Record screen",
            Hotkey::Gif => "Record GIF",
        }
    }
}

/// The hotkey of each capture action, `""` = none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Keys {
    pub all_in_one: String,
    pub region: String,
    pub previous: String,
    pub window: String,
    pub fullscreen: String,
    pub ocr: String,
    pub record: String,
    pub gif: String,
}

impl Default for Keys {
    /// ⌃⌥ + digit: the family of the dock's own keys. None of these is a macOS system key (⇧⌘3/4/5/6 are, and
    /// ⌃⇧⌘3/4 copy to the clipboard), none is a standard app command, and ⌃⌥ digits are free in the apps that
    /// matter (Chrome, Safari, VS Code and the terminal emulators use ⌘ or ⌃ alone).
    fn default() -> Self {
        Self {
            all_in_one: "ctrl-alt-1".into(),
            region: "ctrl-alt-4".into(),
            previous: "ctrl-alt-2".into(),
            window: "ctrl-alt-5".into(),
            fullscreen: "ctrl-alt-3".into(),
            ocr: "ctrl-alt-8".into(),
            record: "ctrl-alt-6".into(),
            gif: "ctrl-alt-7".into(),
        }
    }
}

impl Keys {
    pub fn get(&self, h: Hotkey) -> &str {
        match h {
            Hotkey::AllInOne => &self.all_in_one,
            Hotkey::Region => &self.region,
            Hotkey::Previous => &self.previous,
            Hotkey::Window => &self.window,
            Hotkey::Fullscreen => &self.fullscreen,
            Hotkey::Ocr => &self.ocr,
            Hotkey::Record => &self.record,
            Hotkey::Gif => &self.gif,
        }
    }

    pub fn entries(&self) -> Vec<(Hotkey, &str)> {
        Hotkey::ALL.iter().map(|h| (*h, self.get(*h))).collect()
    }
}

/// A set of keys a person can pick in one click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub keys: Keys,
    /// A sentence shown under the preset when picking it costs something.
    pub note: Option<&'static str>,
    /// The preset uses macOS's own screenshot keys: Settings asks the person to free them first.
    pub needs_system_keys: bool,
}

/// The presets Settings offers. `shift-cmd-s` is here because the maintainer asked for it: it is also "Save As" in
/// most apps, and a global hotkey wins over the app in front, so the note says so before it is picked.
pub fn presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "default",
            label: "⌃⌥1–8 (default)",
            keys: Keys::default(),
            note: None,
            needs_system_keys: false,
        },
        Preset {
            id: "cleanshot",
            label: "⇧⌘3 / 4 / 5 / 6 (CleanShot)",
            keys: Keys {
                all_in_one: "cmd-shift-5".into(),
                region: "cmd-shift-4".into(),
                previous: "cmd-shift-2".into(),
                window: "cmd-shift-7".into(),
                fullscreen: "cmd-shift-3".into(),
                ocr: "cmd-shift-8".into(),
                record: "cmd-shift-6".into(),
                gif: "cmd-shift-9".into(),
            },
            note: Some(
                "These are macOS's own screenshot keys (⇧⌘3, ⇧⌘4, ⇧⌘5, ⇧⌘6), and macOS answers them before any app. Atlas never changes a system setting for you: open System Settings > Keyboard > Keyboard Shortcuts > Screenshots, untick them, then tick \"I turned them off\" here. Until then this preset stays unavailable.",
            ),
            needs_system_keys: true,
        },
        Preset {
            id: "ctrl-shift",
            label: "⌃⇧3–9 (without ⌘)",
            keys: Keys {
                all_in_one: "ctrl-shift-5".into(),
                region: "ctrl-shift-4".into(),
                previous: "ctrl-shift-2".into(),
                window: "ctrl-shift-7".into(),
                fullscreen: "ctrl-shift-3".into(),
                ocr: "ctrl-shift-8".into(),
                record: "ctrl-shift-6".into(),
                gif: "ctrl-shift-9".into(),
            },
            note: Some(
                "⌃⇧4 is not a macOS key, but a few terminals and editors use ⌃⇧ digits; if one stops responding, pick another preset.",
            ),
            needs_system_keys: false,
        },
        Preset {
            id: "shift-cmd-s",
            label: "⇧⌘S for a region",
            keys: Keys {
                region: "shift-cmd-s".into(),
                ..Keys::default()
            },
            note: Some(
                "⇧⌘S is \"Save As\" in most apps. As a global hotkey it wins over the app in front, so Save As stops working everywhere while Atlas runs. macOS keeps ⇧⌘3, ⇧⌘4 and ⇧⌘5 for itself, which is why they are not offered here.",
            ),
            needs_system_keys: false,
        },
    ]
}

/// What happens to a capture the moment it is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct After {
    /// Put the capture on the clipboard: a screenshot as an image, a recording or GIF as the file.
    pub copy: bool,
    /// Show the quick-access thumbnail.
    pub quick_access: bool,
    /// Seconds the thumbnail stays before it dismisses itself; 0 = until dismissed.
    pub dismiss_secs: u32,
    /// Play the shutter sound.
    pub sound: bool,
}

impl Default for After {
    fn default() -> Self {
        Self {
            copy: true,
            quick_access: true,
            dismiss_secs: 6,
            sound: true,
        }
    }
}

/// The mode the All-in-One HUD and `Previous` remember: the last thing the person picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LastMode {
    Region,
    Window,
    Screen,
    Record,
    Gif,
    Text,
}

/// Retina choice for stills: the display's own pixels, or one point = one pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    Native,
    #[serde(rename = "1x")]
    One,
}

/// Screenshot options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Still {
    /// What the All-in-One HUD opens on.
    pub last_mode: LastMode,
    /// Self-timer before a still is taken: 0 (off), 3, 5 or 10 seconds.
    pub timer_secs: u32,
    /// A window capture keeps its drop shadow (on a transparent background).
    pub window_shadow: bool,
    /// A window capture sits on a padded background instead (wins over the shadow when both are on).
    pub window_padding: bool,
    /// `native` = retina pixels, `1x` = one pixel per point.
    pub scale: Scale,
    /// Hide the desktop icons while a still or recording is taken.
    pub hide_icons: bool,
    /// A dragged region is captured the moment the mouse is released; off: it stays held until Return.
    pub capture_on_release: bool,
}

impl Default for Still {
    fn default() -> Self {
        Self {
            last_mode: LastMode::Region,
            timer_secs: 0,
            window_shadow: true,
            window_padding: false,
            scale: Scale::Native,
            hide_icons: false,
            capture_on_release: true,
        }
    }
}

/// The self-timer values the HUD offers.
pub const TIMER_VALUES: [u32; 4] = [0, 3, 5, 10];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

/// The quick-access stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Quick {
    pub position: Corner,
    /// How many cards stack in the corner before the oldest goes.
    pub stack: u32,
}

impl Default for Quick {
    fn default() -> Self {
        Self {
            position: Corner::BottomRight,
            stack: 5,
        }
    }
}

pub const QUICK_STACK_MIN: u32 = 1;
pub const QUICK_STACK_MAX: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoQuality {
    Low,
    Medium,
    High,
}

/// Recording options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Recording {
    /// Show the keys you press (a pill at the bottom of the recorded region).
    pub keystrokes: bool,
    /// Show a ring on each click.
    pub clicks: bool,
    pub microphone: bool,
    pub system_audio: bool,
    /// A round camera bubble in a corner of the recording.
    pub webcam: bool,
    /// Silence notifications while recording.
    pub do_not_disturb: bool,
    pub quality: VideoQuality,
}

impl Default for Recording {
    fn default() -> Self {
        Self {
            keystrokes: false,
            clicks: true,
            microphone: false,
            system_audio: false,
            webcam: false,
            do_not_disturb: true,
            quality: VideoQuality::High,
        }
    }
}

/// The capture settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CapturePrefs {
    /// Folder the files are saved to; empty = `~/Pictures/Atlas Captures`.
    pub folder: String,
    pub keys: Keys,
    pub after: After,
    pub still: Still,
    pub quick: Quick,
    pub recording: Recording,
    /// Highlight the cursor in recordings.
    pub highlight_cursor: bool,
    /// Keep the capture hotkeys registered. Off: none is registered (the dock's own keys are unaffected).
    pub enabled: bool,
    /// How many captures the history keeps (files stay on disk; only the list is trimmed).
    pub history: u32,
    /// The person says they turned macOS's own ⇧⌘3–6 screenshot shortcuts off, so Atlas may register them. Atlas
    /// never changes that system setting itself.
    pub system_keys_free: bool,
}

impl Default for CapturePrefs {
    fn default() -> Self {
        Self {
            folder: String::new(),
            keys: Keys::default(),
            after: After::default(),
            still: Still::default(),
            quick: Quick::default(),
            recording: Recording::default(),
            highlight_cursor: true,
            enabled: true,
            history: 50,
            system_keys_free: false,
        }
    }
}

/// History size limits: below 1 it would be useless, above this the JSON would be slow to load for no benefit.
pub const HISTORY_MIN: u32 = 1;
pub const HISTORY_MAX: u32 = 500;

impl CapturePrefs {
    /// The prefs in `settings.json`; unreadable = defaults.
    pub fn load() -> Self {
        crate::shell_env::load_settings()
            .extra
            .get("capture")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// The folder files are saved to.
    pub fn dir(&self) -> PathBuf {
        if self.folder.trim().is_empty() {
            dirs::picture_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(std::env::temp_dir)
                .join("Atlas Captures")
        } else {
            PathBuf::from(self.folder.trim())
        }
    }

    /// Refuse what cannot work, in a sentence for the person. `others` are the strokes already owned by the dock
    /// (`("Hide the dock", "ctrl-alt-d")` …): a clash with them is as bad as one between two capture keys.
    pub fn validate(&self, others: &[(&str, &str)]) -> Result<(), String> {
        if !(HISTORY_MIN..=HISTORY_MAX).contains(&self.history) {
            return Err(format!(
                "History keeps between {HISTORY_MIN} and {HISTORY_MAX} captures"
            ));
        }
        if !self.folder.trim().is_empty() && !std::path::Path::new(self.folder.trim()).is_absolute()
        {
            return Err("The folder must be an absolute path".into());
        }
        if !TIMER_VALUES.contains(&self.still.timer_secs) {
            return Err("The timer is off, 3, 5 or 10 seconds".into());
        }
        if !(QUICK_STACK_MIN..=QUICK_STACK_MAX).contains(&self.quick.stack) {
            return Err(format!(
                "The quick-access stack holds between {QUICK_STACK_MIN} and {QUICK_STACK_MAX} captures"
            ));
        }
        let mut all: Vec<(&str, &str)> = Hotkey::ALL
            .iter()
            .map(|h| (h.label(), self.keys.get(*h)))
            .collect();
        for (h, stroke) in self.keys.entries() {
            if let hotkeys::Verdict::Refused(r) = hotkeys::check_with(stroke, self.system_keys_free)
            {
                return Err(format!("{}: {r}", h.label()));
            }
        }
        all.extend_from_slice(others);
        if let Some((a, b)) = hotkeys::clash(&all) {
            return Err(format!("\"{a}\" and \"{b}\" use the same key"));
        }
        Ok(())
    }
}

/// Prefix of the menu ids of the capture submenu (`capture:allInOne`, …): the tray and the app menu share it.
pub const MENU_PREFIX: &str = "capture:";

/// One capture entry of a native menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: String,
    pub label: &'static str,
    /// The stroke in Mac symbols (`⌃⌥4`), `""` when the action has no key.
    pub stroke: String,
}

/// Every capture action as a menu entry, in the Settings order.
pub fn menu_entries(keys: &Keys) -> Vec<MenuEntry> {
    Hotkey::ALL
        .iter()
        .map(|h| {
            let name = serde_json::to_value(h)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            MenuEntry {
                id: format!("{MENU_PREFIX}{name}"),
                label: h.label(),
                stroke: stroke_symbols(keys.get(*h)),
            }
        })
        .collect()
}

/// The action a capture menu id stands for; `None` for any other id.
pub fn hotkey_of_menu_id(id: &str) -> Option<Hotkey> {
    Hotkey::from_name(id.strip_prefix(MENU_PREFIX)?)
}

/// A stroke (`cmd-shift-4`) as the symbols a Mac shows (`⇧⌘4`): modifiers in the system order ⌃⌥⇧⌘, then the key
/// (letters upper-case, named keys capitalised). Empty in, empty out.
pub fn stroke_symbols(stroke: &str) -> String {
    let mut mods = [false; 4];
    let mut key = String::new();
    let parts: Vec<&str> = stroke.split('-').collect();
    for (i, p) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" if !last => mods[0] = true,
            "alt" | "option" if !last => mods[1] = true,
            "shift" if !last => mods[2] = true,
            "cmd" | "command" | "super" if !last => mods[3] = true,
            "" => {}
            _ => key = p.to_string(),
        }
    }
    if key.is_empty() {
        return String::new();
    }
    let key = if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        let mut c = key.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default()
    };
    let mut out = String::new();
    for (on, sym) in mods.iter().zip(["⌃", "⌥", "⇧", "⌘"]) {
        if *on {
            out.push_str(sym);
        }
    }
    out + &key
}

/// The value after `current` in Off → 3 → 5 → 10 → Off; anything else (a hand-edited file) starts over at off.
pub fn next_timer(current: u32) -> u32 {
    TIMER_VALUES
        .iter()
        .position(|v| *v == current)
        .and_then(|i| TIMER_VALUES.get(i + 1))
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The debug bridge starts a capture by the same names the settings keys use; an unknown name is no action.
    #[test]
    fn every_hotkey_is_found_by_the_name_of_its_key_field() {
        for h in Hotkey::ALL {
            let name = serde_json::to_value(h).unwrap();
            assert_eq!(Hotkey::from_name(name.as_str().unwrap()), Some(h));
            // and it is a field of `Keys`
            let keys = serde_json::to_value(Keys::default()).unwrap();
            assert!(keys.get(name.as_str().unwrap()).is_some(), "{name}");
        }
        assert_eq!(Hotkey::from_name("Region"), None);
        assert_eq!(Hotkey::from_name(""), None);
    }

    #[test]
    fn the_defaults_validate_and_serialize_with_the_keys_the_page_reads() {
        let p = CapturePrefs::default();
        assert!(p.validate(&[]).is_ok());
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["keys"]["region"], "ctrl-alt-4");
        assert_eq!(v["after"]["quickAccess"], true);
        assert_eq!(v["after"]["dismissSecs"], 6);
        assert_eq!(v["highlightCursor"], true);
        assert!(v.get("highlight_cursor").is_none(), "camelCase only");
    }

    /// A settings.json from before a field existed, or hand-edited, still loads; a wrong type is the default.
    #[test]
    fn partial_files_keep_defaults_for_what_they_leave_out() {
        let p: CapturePrefs =
            serde_json::from_value(serde_json::json!({"keys": {"region": "ctrl-alt-9"}})).unwrap();
        assert_eq!(p.keys.region, "ctrl-alt-9");
        assert_eq!(
            p.keys.window, "ctrl-alt-5",
            "an omitted key keeps its default"
        );
        assert!(p.after.copy);
        assert!(
            serde_json::from_value::<CapturePrefs>(serde_json::json!({"history": "x"})).is_err()
        );
    }

    /// The default keys must not collide with the dock's defaults: that would make a fresh install fail to
    /// register one of them.
    #[test]
    fn the_default_capture_keys_do_not_clash_with_the_dock_keys() {
        let dock = [
            ("Message an agent", "ctrl-alt-space"),
            ("Hide the dock", "ctrl-alt-d"),
        ];
        assert!(CapturePrefs::default().validate(&dock).is_ok());
    }

    #[test]
    fn two_actions_on_one_key_are_refused_by_name_in_any_modifier_order() {
        let mut p = CapturePrefs::default();
        p.keys.gif = "alt-ctrl-4".into();
        let e = p.validate(&[]).unwrap_err();
        assert!(
            e.contains("Capture region") && e.contains("Record GIF"),
            "{e}"
        );
    }

    #[test]
    fn a_capture_key_that_clashes_with_the_dock_names_the_dock_action() {
        let mut p = CapturePrefs::default();
        p.keys.region = "ctrl-alt-d".into();
        let e = p.validate(&[("Hide the dock", "ctrl-alt-d")]).unwrap_err();
        assert!(e.contains("Hide the dock"), "{e}");
    }

    /// The incident behind the presets: ⇧⌘3/4/5 are macOS's own screenshot keys; registering one would silently
    /// never fire. Settings refuses them with the reason.
    #[test]
    fn macos_screenshot_keys_are_refused_with_the_action_that_asked() {
        let mut p = CapturePrefs::default();
        p.keys.fullscreen = "cmd-shift-3".into();
        let e = p.validate(&[]).unwrap_err();
        assert!(e.starts_with("Capture screen:") && e.contains("⇧⌘3"), "{e}");
    }

    /// ⇧⌘S is allowed (the maintainer asked for it): it is a warning in the UI, not a refusal here.
    #[test]
    fn shift_cmd_s_is_allowed_but_its_preset_carries_the_save_as_note() {
        let mut p = CapturePrefs::default();
        p.keys.region = "shift-cmd-s".into();
        assert!(p.validate(&[]).is_ok());
        let preset = presets()
            .into_iter()
            .find(|p| p.keys.region == "shift-cmd-s")
            .expect("a ⇧⌘S preset");
        let note = preset.note.expect("a note");
        assert!(note.contains("Save As") && note.contains("⇧⌘4"), "{note}");
    }

    #[test]
    fn every_preset_is_valid_and_distinct() {
        let dock = [
            ("Message an agent", "ctrl-alt-space"),
            ("Hide the dock", "ctrl-alt-d"),
        ];
        let ps = presets();
        for p in &ps {
            // A preset on the macOS screenshot keys is valid once they are freed (its own test covers the refusal).
            let prefs = CapturePrefs {
                keys: p.keys.clone(),
                system_keys_free: p.id == "cleanshot",
                ..CapturePrefs::default()
            };
            assert!(
                prefs.validate(&dock).is_ok(),
                "{}: {:?}",
                p.id,
                prefs.validate(&dock)
            );
        }
        let ids: std::collections::HashSet<_> = ps.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), ps.len());
        assert_eq!(
            ps[0].keys,
            Keys::default(),
            "the first preset is the default"
        );
    }

    #[test]
    fn empty_keys_are_unset_hotkeys_not_clashes() {
        let mut p = CapturePrefs::default();
        p.keys.region.clear();
        p.keys.window.clear();
        assert!(p.validate(&[]).is_ok());
    }

    #[test]
    fn history_and_folder_are_bounded() {
        let mut p = CapturePrefs {
            history: 0,
            ..CapturePrefs::default()
        };
        assert!(p.validate(&[]).is_err());
        p.history = HISTORY_MAX + 1;
        assert!(p.validate(&[]).is_err());
        p.history = 10;
        p.folder = "relative/dir".into();
        assert!(p.validate(&[]).unwrap_err().contains("absolute"));
        p.folder = "/Users/x/Shots".into();
        assert!(p.validate(&[]).is_ok());
        assert_eq!(p.dir(), PathBuf::from("/Users/x/Shots"));
    }

    #[test]
    fn the_default_folder_is_atlas_captures_in_pictures() {
        let d = CapturePrefs::default().dir();
        assert!(d.ends_with("Atlas Captures"), "{d:?}");
    }

    // ───────── CleanShot parity contract (all-in-one, options, system-key attestation) ─────────

    #[test]
    fn the_new_actions_have_keys_labels_and_names() {
        let keys = serde_json::to_value(Keys::default()).unwrap();
        for name in ["allInOne", "previous", "ocr"] {
            let h = Hotkey::from_name(name).unwrap_or_else(|| panic!("{name}"));
            assert!(keys.get(name).is_some(), "{name} is a field of Keys");
            assert!(!h.label().is_empty());
        }
        assert_eq!(Hotkey::ALL.len(), 8);
    }

    #[test]
    fn the_defaults_serialize_the_option_groups_the_pages_read() {
        let v = serde_json::to_value(CapturePrefs::default()).unwrap();
        assert_eq!(v["still"]["lastMode"], "region");
        assert_eq!(v["still"]["timerSecs"], 0);
        assert_eq!(v["still"]["windowShadow"], true);
        assert_eq!(v["still"]["windowPadding"], false);
        assert_eq!(v["still"]["scale"], "native");
        assert_eq!(v["still"]["hideIcons"], false);
        assert_eq!(v["still"]["captureOnRelease"], true);
        assert_eq!(v["quick"]["position"], "bottomRight");
        assert_eq!(v["quick"]["stack"], 5);
        assert_eq!(v["recording"]["keystrokes"], false);
        assert_eq!(v["recording"]["clicks"], true);
        assert_eq!(v["recording"]["microphone"], false);
        assert_eq!(v["recording"]["systemAudio"], false);
        assert_eq!(v["recording"]["webcam"], false);
        assert_eq!(v["recording"]["doNotDisturb"], true);
        assert_eq!(v["recording"]["quality"], "high");
        assert_eq!(v["systemKeysFree"], false);
        assert!(v["still"].get("last_mode").is_none(), "camelCase only");
    }

    /// Capture-on-release is on by default, and a prefs file written before the key existed loads as on.
    #[test]
    fn capture_on_release_defaults_on_and_an_older_file_without_the_key_loads_as_on() {
        assert!(Still::default().capture_on_release);
        let old: CapturePrefs = serde_json::from_value(serde_json::json!({
            "still": {"lastMode": "window", "timerSecs": 3, "hideIcons": true}
        }))
        .unwrap();
        assert!(old.still.capture_on_release);
        assert_eq!(old.still.timer_secs, 3);
        let off: CapturePrefs = serde_json::from_value(serde_json::json!({
            "still": {"captureOnRelease": false}
        }))
        .unwrap();
        assert!(!off.still.capture_on_release);
        // a file with no `still` at all
        let none: CapturePrefs = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(none.still.capture_on_release);
    }

    #[test]
    fn the_timer_only_takes_the_cleanshot_values() {
        let mut p = CapturePrefs::default();
        for ok in [0, 3, 5, 10] {
            p.still.timer_secs = ok;
            assert!(p.validate(&[]).is_ok(), "{ok}");
        }
        p.still.timer_secs = 7;
        assert!(p.validate(&[]).unwrap_err().contains("timer"));
    }

    #[test]
    fn the_quick_stack_is_bounded() {
        let mut p = CapturePrefs::default();
        p.quick.stack = 0;
        assert!(p.validate(&[]).is_err());
        p.quick.stack = QUICK_STACK_MAX + 1;
        assert!(p.validate(&[]).is_err());
        p.quick.stack = QUICK_STACK_MAX;
        assert!(p.validate(&[]).is_ok());
    }

    /// The CleanShot preset is ⇧⌘3/4/5/6 and is refused until the person says they freed those keys in macOS
    /// (we never change a system setting ourselves).
    #[test]
    fn the_cleanshot_preset_needs_the_system_keys_freed_first() {
        let ps = presets();
        let cs = ps.iter().find(|p| p.id == "cleanshot").expect("cleanshot");
        assert_eq!(cs.keys.fullscreen, "cmd-shift-3");
        assert_eq!(cs.keys.region, "cmd-shift-4");
        assert_eq!(cs.keys.all_in_one, "cmd-shift-5");
        assert_eq!(cs.keys.record, "cmd-shift-6");
        let note = cs.note.expect("a note explaining the system keys");
        assert!(
            note.contains("System Settings") && note.contains("never"),
            "{note}"
        );

        let mut p = CapturePrefs {
            keys: cs.keys.clone(),
            ..CapturePrefs::default()
        };
        let e = p.validate(&[]).unwrap_err();
        assert!(
            e.starts_with("All-in-one capture:") && e.contains("⇧⌘5"),
            "{e}"
        );
        p.system_keys_free = true;
        assert!(p.validate(&[]).is_ok(), "{:?}", p.validate(&[]));
    }

    /// Freed system keys let ⇧⌘3–6 through and nothing else: ⌘Space stays refused.
    #[test]
    fn freeing_the_system_keys_does_not_free_other_system_keys() {
        let mut p = CapturePrefs {
            system_keys_free: true,
            ..CapturePrefs::default()
        };
        p.keys.region = "cmd-space".into();
        assert!(p.validate(&[]).is_err());
    }

    #[test]
    fn every_preset_validates_with_the_system_keys_freed_and_conflicts_are_still_found() {
        for pr in presets() {
            let p = CapturePrefs {
                keys: pr.keys.clone(),
                system_keys_free: true,
                ..CapturePrefs::default()
            };
            assert!(p.validate(&[]).is_ok(), "{}: {:?}", pr.id, p.validate(&[]));
        }
        let mut p = CapturePrefs {
            system_keys_free: true,
            ..CapturePrefs::default()
        };
        p.keys.region = "cmd-shift-3".into();
        p.keys.fullscreen = "cmd-shift-3".into();
        assert!(p.validate(&[]).unwrap_err().contains("same key"));
    }

    /// The tray/menu-bar submenu lists every capture action, in the Settings order, each with the stroke it has now
    /// (so the menu teaches the key) and an id the click handler maps back to the action.
    #[test]
    fn the_menu_lists_every_action_with_its_key_and_a_round_trippable_id() {
        let entries = menu_entries(&Keys::default());
        assert_eq!(entries.len(), Hotkey::ALL.len());
        for (e, h) in entries.iter().zip(Hotkey::ALL) {
            assert_eq!(e.label, h.label());
            assert_eq!(hotkey_of_menu_id(&e.id), Some(h), "{}", e.id);
        }
        assert_eq!(entries[1].stroke, "⌃⌥4");
        assert_eq!(hotkey_of_menu_id("capture:nope"), None);
        assert_eq!(hotkey_of_menu_id("dock:open"), None);
        assert_eq!(hotkey_of_menu_id("region"), None);
    }

    /// An action without a key still has a menu entry (it is how you reach it), just no stroke text.
    #[test]
    fn an_unset_key_still_has_a_menu_entry_without_a_stroke() {
        let mut k = Keys::default();
        k.region.clear();
        let e = menu_entries(&k);
        assert_eq!(e[1].stroke, "");
        assert_eq!(e[1].label, "Capture region");
    }

    #[test]
    fn strokes_display_with_mac_symbols_in_the_system_order() {
        assert_eq!(stroke_symbols("cmd-shift-4"), "⇧⌘4");
        assert_eq!(stroke_symbols("shift-cmd-4"), "⇧⌘4");
        assert_eq!(stroke_symbols("ctrl-alt-shift-cmd-space"), "⌃⌥⇧⌘Space");
        assert_eq!(stroke_symbols("ctrl-alt-3"), "⌃⌥3");
        assert_eq!(stroke_symbols("ctrl-shift-s"), "⌃⇧S");
        assert_eq!(stroke_symbols(""), "");
    }

    /// The HUD's timer button cycles Off → 3 → 5 → 10 → Off and the result is always a value Rust accepts.
    #[test]
    fn the_timer_cycles_through_the_accepted_values_and_never_leaves_them() {
        assert_eq!(next_timer(0), 3);
        assert_eq!(next_timer(3), 5);
        assert_eq!(next_timer(5), 10);
        assert_eq!(next_timer(10), 0);
        // a hand-edited value that is not one of them starts the cycle over
        assert_eq!(next_timer(7), 0);
        let mut v = 0;
        for _ in 0..8 {
            v = next_timer(v);
            assert!(TIMER_VALUES.contains(&v));
        }
    }

    fn fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/capture-prefs.json")
    }

    /// Contract with the Settings page: `__fixtures__/capture-prefs.json` is the REAL serialization of the
    /// defaults and the presets (what `shots_prefs` and `shots_presets` return), and `settings.test.ts` reads it.
    /// A field renamed or added in Rust turns this red until the fixture (and so the TS types) follow:
    /// `UPDATE_FIXTURES=1 cargo nextest run -p atlas-desktop capture_prefs_fixture` rewrites it.
    #[test]
    fn the_capture_prefs_fixture_is_what_rust_serializes() {
        let real = serde_json::json!({
            "prefs": CapturePrefs::default(),
            "presets": presets(),
        });
        let want = serde_json::to_string_pretty(&real).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(fixture_path(), &want).unwrap();
        }
        let have = std::fs::read_to_string(fixture_path()).expect(
            "apps/desktop/src/lib/__fixtures__/capture-prefs.json (UPDATE_FIXTURES=1 writes it)",
        );
        // Compared as JSON, not text: the page's formatter (prettier) owns the file's whitespace.
        let have: serde_json::Value = serde_json::from_str(&have).expect("the fixture is JSON");
        assert_eq!(
            have, real,
            "the fixture is stale: run with UPDATE_FIXTURES=1"
        );
    }
}
