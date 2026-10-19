//! Phone or tablet layout: **Window › Layout** picks Automatic (a phone when the screen's
//! smallest side is under [`PHONE_MAX_DP`], Android's own phone/tablet line, so a phone stays
//! a phone on its side), Phone or Tablet. Saved in the app's files directory (`layout.txt`).
//!
//! The phone layout ([`photocraft_ui_egui::PhotocraftApp::compact_layout`]) makes Auto interface
//! scale smaller, dialogs as wide as the window (scrolling when taller), and the Shortcut Pad
//! slimmer.

use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::platform_menu::PlatformMenu;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Screens whose smallest side is under this many dp are phones.
pub const PHONE_MAX_DP: f32 = 600.0;

const MENU: &[&str] = &["Window", "Layout"];
const PREFIX: &str = "android.layout.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    #[default]
    Auto,
    Phone,
    Tablet,
}

impl LayoutMode {
    pub const ALL: [LayoutMode; 3] = [LayoutMode::Auto, LayoutMode::Phone, LayoutMode::Tablet];

    fn key(self) -> &'static str {
        match self {
            LayoutMode::Auto => "auto",
            LayoutMode::Phone => "phone",
            LayoutMode::Tablet => "tablet",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LayoutMode::Auto => "Automatic (Phone or Tablet)",
            LayoutMode::Phone => "Phone",
            LayoutMode::Tablet => "Tablet",
        }
    }

    fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == k.trim())
    }

    /// Whether a screen whose smallest side is `smallest_dp` gets the phone layout.
    pub fn is_phone(self, smallest_dp: f32) -> bool {
        match self {
            LayoutMode::Auto => smallest_dp > 0.0 && smallest_dp < PHONE_MAX_DP,
            LayoutMode::Phone => true,
            LayoutMode::Tablet => false,
        }
    }
}

struct Shared {
    mode: LayoutMode,
    path: PathBuf,
}

pub struct Layout {
    shared: Rc<RefCell<Shared>>,
}

impl Layout {
    pub fn new(files_dir: &std::path::Path) -> Self {
        let path = files_dir.join("layout.txt");
        let mode = std::fs::read_to_string(&path).ok().and_then(|t| LayoutMode::from_key(&t)).unwrap_or_default();
        Self { shared: Rc::new(RefCell::new(Shared { mode, path })) }
    }

    /// Whether this frame gets the phone layout. egui points times the zoom factor are dp (the
    /// interface scale doesn't change the answer).
    pub fn is_phone(&self, ctx: &egui::Context) -> bool {
        let r = ctx.content_rect();
        self.shared.borrow().mode.is_phone(r.width().min(r.height()) * ctx.zoom_factor())
    }

    /// The Window › Layout rows.
    pub fn menu(&self) -> PlatformMenu {
        let (read, write) = (self.shared.clone(), self.shared.clone());
        PlatformMenu {
            items: Box::new(move || {
                let mode = read.borrow().mode;
                LayoutMode::ALL
                    .iter()
                    .map(|m| MenuItem {
                        id: format!("{PREFIX}{}", m.key()),
                        label: m.label().into(),
                        path: MENU.iter().map(|s| s.to_string()).collect(),
                        shortcut: None,
                        enabled: true,
                        checked: Some(mode == *m),
                        color: None,
                    })
                    .collect()
            }),
            run: Box::new(move |id| {
                let mode = id.strip_prefix(PREFIX).and_then(LayoutMode::from_key).ok_or_else(|| format!("unknown Layout command {id}"))?;
                let mut s = write.borrow_mut();
                s.mode = mode;
                if let Err(e) = photocraft_format::atomic_write(&s.path, mode.key().as_bytes()) {
                    log::warn!("couldn't save the layout choice: {e}");
                }
                Ok(Value::Null)
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_are_told_from_tablets_by_their_smallest_side() {
        assert!(LayoutMode::Auto.is_phone(411.0), "a phone, either way up");
        assert!(!LayoutMode::Auto.is_phone(800.0), "a tablet");
        assert!(!LayoutMode::Auto.is_phone(0.0), "no screen yet");
        assert!(LayoutMode::Phone.is_phone(1200.0) && !LayoutMode::Tablet.is_phone(300.0), "the choice wins");
    }

    #[test]
    fn the_choice_is_saved() {
        let dir = std::env::temp_dir().join(format!("pc-layout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let l = Layout::new(&dir);
        assert_eq!(l.shared.borrow().mode, LayoutMode::Auto);
        (l.menu().run)("android.layout.phone").unwrap();
        assert_eq!(Layout::new(&dir).shared.borrow().mode, LayoutMode::Phone);
        assert!((l.menu().run)("android.layout.huge").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
