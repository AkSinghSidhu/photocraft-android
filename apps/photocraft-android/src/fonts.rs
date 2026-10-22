//! Type › Import Font…: pick font files with Android's document picker; each one is checked,
//! added to the font list at once, and kept in the app's `fonts` folder, which the font scan
//! reads on every start ([`photocraft_text::fonts::add_font_dir`]).

use crate::bridge::Bridge;
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::platform_menu::PlatformMenu;
use photocraft_ui_egui::{FileDialogAnswer, FileDialogRequest};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

const IMPORT: &str = "android.type.importFont";
/// Larger files aren't fonts anyone means to import (the biggest CJK collections are ~30 MB).
const MAX_FONT_BYTES: u64 = 128 << 20;
const EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// The outcome of an import, for the status bar: the message and whether it is an error.
pub type Outcome = Arc<Mutex<Option<(String, bool)>>>;

/// Where imported fonts are kept.
pub fn fonts_dir(files_dir: &Path) -> PathBuf {
    files_dir.join("fonts")
}

/// Check that `bytes` hold a font and add it to the shared font list. Returns the families added.
fn register(bytes: Vec<u8>) -> Result<Vec<String>, String> {
    let mut engine = photocraft_text::shared().lock().unwrap_or_else(PoisonError::into_inner);
    let families = engine.fonts.register_font_data(bytes);
    if families.is_empty() { Err("not a font file this app can read".into()) } else { Ok(families) }
}

/// The font extension `bytes` call for, from the file's signature: some document providers
/// hand a font over without its extension (or under a generic name).
pub fn sniffed_extension(bytes: &[u8]) -> Option<&'static str> {
    match bytes.get(..4)? {
        [0, 1, 0, 0] | b"true" => Some("ttf"),
        b"OTTO" => Some("otf"),
        b"ttcf" => Some("ttc"),
        _ => None,
    }
}

/// A safe file name for an imported font: the picked name's last part, letters, digits and a
/// few marks only, with its font extension (`sniffed`, from the contents, when the name has
/// none of the font extensions).
pub fn stored_name(picked: &Path, sniffed: Option<&str>) -> Option<String> {
    let name = picked.file_name()?.to_string_lossy();
    let named = Path::new(name.as_ref()).extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| EXTENSIONS.contains(&e.as_str()));
    let (ext, stem) = match named {
        Some(ext) => (ext, Path::new(name.as_ref()).file_stem()?.to_string_lossy().into_owned()),
        None => (sniffed?.to_string(), name.to_string()),
    };
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.') { c } else { '_' }).take(100).collect();
    let stem = stem.trim();
    (!stem.is_empty()).then(|| format!("{stem}.{ext}"))
}

/// Import one picked file into `dir`: returns the families it added.
pub fn import_file(picked: &Path, dir: &Path) -> Result<Vec<String>, String> {
    let shown = picked.file_name().map_or_else(|| picked.display().to_string(), |n| n.to_string_lossy().into_owned());
    let size = std::fs::metadata(picked).map_err(|e| format!("{shown}: {e}"))?.len();
    if size > MAX_FONT_BYTES {
        return Err(format!("{shown}: too large for a font"));
    }
    let bytes = std::fs::read(picked).map_err(|e| format!("{shown}: {e}"))?;
    let name = stored_name(picked, sniffed_extension(&bytes)).ok_or_else(|| format!("{shown}: not a .ttf, .otf, .ttc or .otc font"))?;
    let families = register(bytes.clone()).map_err(|e| format!("{shown}: {e}"))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{shown}: {e}"))?;
    photocraft_format::atomic_write(&dir.join(name), &bytes).map_err(|e| format!("{shown}: {e}"))?;
    Ok(families)
}

/// The status-bar message for a batch of imports.
pub fn summary(results: &[Result<Vec<String>, String>]) -> (String, bool) {
    let mut families: Vec<String> = results.iter().filter_map(|r| r.as_ref().ok()).flatten().cloned().collect();
    families.dedup();
    let errors: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
    match (families.is_empty(), errors.first()) {
        (true, Some(e)) => (format!("Couldn't import the font: {e}"), true),
        (true, None) => ("No font was imported.".into(), false),
        (false, None) => (format!("Font added: {}", families.join(", ")), false),
        (false, Some(e)) => (format!("Font added: {} (skipped: {e})", families.join(", ")), true),
    }
}

/// The Type › Import Font… row. `outcome` gets the result; `wake` repaints the UI.
pub fn menu(files_dir: &Path, bridge: Arc<Bridge>, outcome: Outcome, wake: impl Fn() + Send + Sync + Clone + 'static) -> PlatformMenu {
    let dir = fonts_dir(files_dir);
    PlatformMenu {
        items: Box::new(|| {
            vec![MenuItem {
                id: IMPORT.into(),
                label: "Import Font…".into(),
                path: vec!["Type".into()],
                shortcut: None,
                enabled: true,
                checked: None,
                color: None,
            }]
        }),
        run: Box::new(move |id| {
            if id != IMPORT {
                return Err(format!("unknown font command {id}"));
            }
            let (dir, outcome, wake) = (dir.clone(), outcome.clone(), wake.clone());
            let request = FileDialogRequest::Open { multiple: true, initial_dir: None, extensions: Some(EXTENSIONS.iter().map(|e| e.to_string()).collect()) };
            crate::services::ask(&bridge, request, move |answer| {
                let paths = match answer {
                    Some(FileDialogAnswer::Paths(paths)) => paths,
                    Some(FileDialogAnswer::Unavailable(e)) => {
                        *outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some((format!("Couldn't open the file picker: {e}"), true));
                        wake();
                        return;
                    }
                    // Cancelled: nothing to say.
                    _ => return,
                };
                log::info!("importing {} font file(s)", paths.len());
                let results: Vec<_> = paths.iter().map(|p| import_file(Path::new(p), &dir)).collect();
                *outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(summary(&results));
                wake();
            });
            Ok(Value::Null)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_names_are_safe_font_files() {
        assert_eq!(stored_name(Path::new("/x/inbox/3/Comic Neue Bold.TTF"), None).as_deref(), Some("Comic Neue Bold.ttf"));
        assert_eq!(stored_name(Path::new("/x/../../evil/a?b*.otf"), None).as_deref(), Some("a_b_.otf"));
        assert_eq!(stored_name(Path::new("/x/notes.txt"), None), None);
        assert_eq!(stored_name(Path::new("/x/.ttf"), None), None);
        // A provider that drops the extension: the contents name it.
        assert_eq!(stored_name(Path::new("/x/inbox/4/Comic Neue"), Some("ttf")).as_deref(), Some("Comic Neue.ttf"));
        assert_eq!(stored_name(Path::new("/x/inbox/4/document"), None), None);
    }

    #[test]
    fn importing_checks_the_file_is_a_font() {
        let dir = std::env::temp_dir().join(format!("pc-fonts-{}", std::process::id()));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        // A real font (the bundled Inter) under a new name, and a fake one.
        let good = src.join("My Inter.ttf");
        std::fs::write(&good, &*photocraft_text::fonts::INTER_REGULAR).unwrap();
        let fake = src.join("Fake.ttf");
        std::fs::write(&fake, b"not a font").unwrap();
        let out = dir.join("fonts");
        let families = import_file(&good, &out).unwrap();
        assert!(families.iter().any(|f| f == "Inter"), "{families:?}");
        assert!(out.join("My Inter.ttf").exists(), "kept for the next start");
        assert!(import_file(&fake, &out).unwrap_err().contains("not a font"));
        assert!(!out.join("Fake.ttf").exists());
        assert!(import_file(&src.join("notes.txt"), &out).is_err());
        // No extension at all: still a font by its contents.
        let bare = src.join("Inter Bare");
        std::fs::write(&bare, &*photocraft_text::fonts::INTER_REGULAR).unwrap();
        assert!(import_file(&bare, &out).is_ok());
        assert!(out.join("Inter Bare.ttf").exists());
        assert_eq!(sniffed_extension(b"OTTO...."), Some("otf"));
        assert_eq!(sniffed_extension(b"PK"), None);
        let (msg, err) = summary(&[Ok(families), Err("Fake.ttf: not a font".into())]);
        assert!(msg.starts_with("Font added: Inter") && msg.contains("skipped") && err, "{msg}");
        std::fs::remove_dir_all(dir).ok();
    }
}
