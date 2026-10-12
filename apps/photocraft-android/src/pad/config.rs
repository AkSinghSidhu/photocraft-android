//! The Shortcut Pad's settings: which side it sits on, what its four keys press and which
//! functions the wheel's centre button cycles through. Saved as JSON in the app's files
//! directory; a missing or unreadable file gives the defaults.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// How many keys the pad has (two above the wheel, two below).
pub const KEYS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    #[default]
    Right,
}

/// One key: the text it shows and the shortcut it presses (`"Ctrl+Z"`, `"B"`, or a modifier on
/// its own, `"Alt"`, held for as long as the key is touched). An empty shortcut does nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySlot {
    pub label: String,
    pub shortcut: String,
}

impl KeySlot {
    fn new(label: &str, shortcut: &str) -> Self {
        Self { label: label.into(), shortcut: shortcut.into() }
    }
}

/// What turning the wheel does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WheelMode {
    Zoom,
    Rotate,
    BrushSize,
    BrushHardness,
    History,
}

impl WheelMode {
    pub const ALL: [WheelMode; 5] = [WheelMode::Zoom, WheelMode::Rotate, WheelMode::BrushSize, WheelMode::BrushHardness, WheelMode::History];

    /// The name in the settings.
    pub fn label(self) -> &'static str {
        match self {
            WheelMode::Zoom => "Zoom",
            WheelMode::Rotate => "Rotate View",
            WheelMode::BrushSize => "Brush Size",
            WheelMode::BrushHardness => "Brush Hardness",
            WheelMode::History => "Undo / Redo",
        }
    }

    /// The short name on the wheel's centre button.
    pub fn short(self) -> &'static str {
        match self {
            WheelMode::Zoom => "Zoom",
            WheelMode::Rotate => "Rotate",
            WheelMode::BrushSize => "Size",
            WheelMode::BrushHardness => "Hard",
            WheelMode::History => "Undo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PadConfig {
    pub enabled: bool,
    pub side: Side,
    pub keys: Vec<KeySlot>,
    /// The functions the centre button cycles through, in order (never empty once
    /// [`Self::sanitized`]).
    pub wheel_modes: Vec<WheelMode>,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            side: Side::Right,
            keys: vec![KeySlot::new("Undo", "Ctrl+Z"), KeySlot::new("Redo", "Ctrl+Shift+Z"), KeySlot::new("Alt", "Alt"), KeySlot::new("Pan", "Space")],
            wheel_modes: vec![WheelMode::Zoom, WheelMode::Rotate, WheelMode::BrushSize],
        }
    }
}

impl PadConfig {
    /// Exactly [`KEYS`] keys, at least one wheel function, no function twice.
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        self.keys.truncate(KEYS);
        while self.keys.len() < KEYS {
            let i = self.keys.len();
            self.keys.push(defaults.keys.get(i).cloned().unwrap_or_else(|| KeySlot::new("", "")));
        }
        let mut seen = Vec::new();
        self.wheel_modes.retain(|m| {
            let new = !seen.contains(m);
            seen.push(*m);
            new
        });
        if self.wheel_modes.is_empty() {
            self.wheel_modes = defaults.wheel_modes;
        }
        self
    }

    pub fn load(path: &Path) -> Self {
        let read = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<PadConfig>(&b).map_err(|e| log::warn!("shortcut pad settings: {e}")).ok());
        read.unwrap_or_default().sanitized()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        photocraft_format::atomic_write(path, &text).map_err(|e| e.to_string())
    }
}

/// Shortcuts the settings offer (label, shortcut); any other can be typed.
pub const PRESETS: &[(&str, &str)] = &[
    ("Undo", "Ctrl+Z"),
    ("Redo", "Ctrl+Shift+Z"),
    ("Alt", "Alt"),
    ("Shift", "Shift"),
    ("Ctrl", "Ctrl"),
    ("Pan", "Space"),
    ("Brush", "B"),
    ("Eraser", "E"),
    ("Eyedropper", "I"),
    ("Swap Colors", "X"),
    ("Deselect", "Ctrl+D"),
    ("Transform", "Ctrl+T"),
    ("Fit", "Ctrl+0"),
    ("100%", "Ctrl+1"),
    ("Save", "Ctrl+S"),
    ("Panels", "Tab"),
];

/// A parsed shortcut: modifiers and an optional key (none: the modifiers alone, held).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Option<egui::Key>,
}

impl Chord {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Parse `"Ctrl+Shift+Z"`, `"alt"`, `"Space"`, `"]"`. Modifier names: Ctrl/Control/Cmd,
/// Alt/Option, Shift (any case). `Ok(empty)` for an empty string; `Err` names what isn't a key.
pub fn parse(shortcut: &str) -> Result<Chord, String> {
    let mut c = Chord::default();
    let s = shortcut.trim();
    if s.is_empty() {
        return Ok(c);
    }
    // "+" alone or last ("Ctrl++") is the plus key, not a separator.
    let (mods, last) = match s.strip_suffix("++") {
        Some(rest) => (rest, "+"),
        None if s == "+" => ("", "+"),
        None => s.rsplit_once('+').unwrap_or(("", s)),
    };
    for part in mods.split('+').map(str::trim).filter(|p| !p.is_empty()).chain(std::iter::once(last.trim())) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => c.ctrl = true,
            "alt" | "option" | "opt" => c.alt = true,
            "shift" => c.shift = true,
            _ if c.key.is_none() && part == last.trim() => {
                let name = if part.chars().count() == 1 { part.to_ascii_uppercase() } else { part.to_string() };
                c.key = Some(egui::Key::from_name(&name).or_else(|| egui::Key::from_name(part)).ok_or_else(|| format!("\"{part}\" is not a key"))?);
            }
            _ => return Err(format!("\"{part}\" is not a modifier")),
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Key;

    #[test]
    fn shortcuts_parse() {
        assert_eq!(parse("Ctrl+Z"), Ok(Chord { ctrl: true, key: Some(Key::Z), ..Default::default() }));
        assert_eq!(parse("ctrl + shift + z"), Ok(Chord { ctrl: true, shift: true, key: Some(Key::Z), ..Default::default() }));
        assert_eq!(parse("Alt"), Ok(Chord { alt: true, ..Default::default() }), "a modifier on its own");
        assert_eq!(parse("Space"), Ok(Chord { key: Some(Key::Space), ..Default::default() }));
        assert_eq!(parse("b"), Ok(Chord { key: Some(Key::B), ..Default::default() }));
        assert_eq!(parse("]"), Ok(Chord { key: Some(Key::CloseBracket), ..Default::default() }));
        assert_eq!(parse("Ctrl+0"), Ok(Chord { ctrl: true, key: Some(Key::Num0), ..Default::default() }));
        assert_eq!(parse("Ctrl++"), Ok(Chord { ctrl: true, key: Some(Key::Plus), ..Default::default() }));
        assert_eq!(parse("Tab"), Ok(Chord { key: Some(Key::Tab), ..Default::default() }));
        assert_eq!(parse(""), Ok(Chord::default()));
        assert!(parse("Ctrl+Banana").is_err());
        assert!(parse("Hyper+Z").is_err(), "unknown modifier");
    }

    #[test]
    fn every_preset_parses() {
        for (label, s) in PRESETS {
            assert!(parse(s).is_ok_and(|c| !c.is_empty()), "{label}: {s}");
        }
        for k in PadConfig::default().keys {
            assert!(parse(&k.shortcut).is_ok(), "{k:?}");
        }
    }

    #[test]
    fn settings_round_trip_and_bad_files_give_defaults() {
        let dir = std::env::temp_dir().join(format!("photocraft-pad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pad.json");
        assert_eq!(PadConfig::load(&path), PadConfig::default(), "no file yet");
        let mut c = PadConfig { side: Side::Left, enabled: false, ..Default::default() };
        c.keys[0] = KeySlot::new("Brush", "B");
        c.wheel_modes = vec![WheelMode::History];
        c.save(&path).unwrap();
        assert_eq!(PadConfig::load(&path), c);
        std::fs::write(&path, b"{broken").unwrap();
        assert_eq!(PadConfig::load(&path), PadConfig::default());
        // Missing fields take their defaults; too few keys are filled, repeats and an empty
        // wheel list fixed.
        std::fs::write(&path, br#"{"side":"left","keys":[{"label":"A","shortcut":"A"}],"wheelModes":[]}"#).unwrap();
        let c = PadConfig::load(&path);
        assert_eq!((c.side, c.keys.len(), c.keys[0].label.as_str(), c.enabled), (Side::Left, KEYS, "A", true));
        assert_eq!(c.wheel_modes, PadConfig::default().wheel_modes);
        let c = PadConfig { wheel_modes: vec![WheelMode::Zoom, WheelMode::Zoom, WheelMode::Rotate], ..Default::default() }.sanitized();
        assert_eq!(c.wheel_modes, [WheelMode::Zoom, WheelMode::Rotate]);
        std::fs::remove_dir_all(dir).ok();
    }
}
