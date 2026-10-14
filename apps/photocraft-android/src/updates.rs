//! Help › Check for Updates… and Help › Check for Updates at Startup.
//!
//! The Activity does the checking, downloading and installing (`Updater.java`, from the fork's
//! GitHub releases); Rust only asks it to check now (the bridge's `check-update` marker) and keeps
//! the startup setting, a `update-check-off` marker in the files directory that the Activity reads
//! at launch.

use crate::bridge::Bridge;
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::platform_menu::PlatformMenu;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CHECK: &str = "android.update.check";
const AT_STARTUP: &str = "android.update.atStartup";

fn off_marker(files_dir: &Path) -> PathBuf {
    files_dir.join("update-check-off")
}

/// The Help rows.
pub fn menu(files_dir: &Path, bridge: Arc<Bridge>) -> PlatformMenu {
    let (read, write) = (off_marker(files_dir), off_marker(files_dir));
    PlatformMenu {
        items: Box::new(move || {
            let row = |id: &str, label: &str, checked: Option<bool>| MenuItem {
                id: id.into(),
                label: label.into(),
                path: vec!["Help".into()],
                shortcut: None,
                enabled: true,
                checked,
                color: None,
            };
            vec![row(CHECK, "Check for Updates…", None), row(AT_STARTUP, "Check for Updates at Startup", Some(!read.exists()))]
        }),
        run: Box::new(move |id| {
            match id {
                CHECK => bridge.request_update_check()?,
                AT_STARTUP => {
                    let result = if write.exists() { std::fs::remove_file(&write) } else { std::fs::write(&write, b"") };
                    result.map_err(|e| format!("couldn't change the update setting: {e}"))?;
                }
                _ => return Err(format!("unknown update command {id}")),
            }
            Ok(Value::Null)
        }),
    }
}

/// Several shells' rows as one menu: each row runs in the menu that made it.
pub fn combine(menus: Vec<PlatformMenu>) -> PlatformMenu {
    let menus = std::rc::Rc::new(std::cell::RefCell::new(menus));
    let read = menus.clone();
    PlatformMenu {
        items: Box::new(move || read.borrow().iter().flat_map(|m| (m.items)()).collect()),
        run: Box::new(move |id| {
            let mut menus = menus.borrow_mut();
            let owner = menus.iter_mut().find(|m| (m.items)().iter().any(|i| i.id == id));
            match owner {
                Some(m) => (m.run)(id),
                None => Err(format!("unknown command {id}")),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("pc-updates-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rows_ask_for_a_check_and_toggle_the_startup_check() {
        let d = dir();
        let bridge = Arc::new(Bridge::new(&d).unwrap());
        let mut m = menu(&d, bridge.clone());
        let checked = |m: &PlatformMenu| (m.items)().iter().find(|i| i.id == AT_STARTUP).and_then(|i| i.checked);
        assert_eq!(checked(&m), Some(true), "on by default");
        (m.run)(AT_STARTUP).unwrap();
        assert_eq!(checked(&m), Some(false));
        assert!(d.join("update-check-off").exists(), "the Activity reads this");
        (m.run)(AT_STARTUP).unwrap();
        assert_eq!(checked(&m), Some(true));
        (m.run)(CHECK).unwrap();
        assert!(bridge.root().join("check-update").exists());
        assert!((m.run)("android.nope").is_err());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn combined_menus_dispatch_to_their_owner() {
        let d = dir();
        let bridge = Arc::new(Bridge::new(&d).unwrap());
        let pad = crate::pad::Pad::new(&d);
        let mut m = combine(vec![pad.menu(), menu(&d, bridge.clone())]);
        let ids: Vec<String> = (m.items)().into_iter().map(|i| i.id).collect();
        assert!(ids.iter().any(|i| i == CHECK) && ids.iter().any(|i| i.starts_with("android.pad.")));
        (m.run)(CHECK).unwrap();
        assert!(bridge.root().join("check-update").exists());
        assert!((m.run)("android.nope").is_err());
        std::fs::remove_dir_all(d).ok();
    }
}
