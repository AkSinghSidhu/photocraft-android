//! Touch Input: multi-touch gestures on the canvas and fingers in the interface.
//!
//! - **On the canvas:** two fingers pan, pinch to zoom and twist to rotate the view, all at once
//!   (the image stays under the fingers); a quick two-finger tap undoes and a three-finger tap
//!   redoes (any shortcut can be chosen). One finger draws like the pen, moves the canvas, or
//!   does nothing.
//! - **Palm rejection:** fingers are ignored while the pen is down or near the screen.
//! - **Menus, panels and lists:** a flick keeps scrolling, and two fingers scroll.
//! - **Window › Touch Input** switches what one finger does and palm rejection, and opens Touch
//!   Input Settings… for the rest. Settings are saved in the app's files directory.
//!
//! The gestures themselves are recognised by the touch router (`pad::router`), which already
//! owns every touch to keep the Shortcut Pad's fingers off the canvas; this module holds the
//! settings and applies view changes to the active document.

pub mod config;

use crate::pad::config::{self as pad_config, PRESETS};
use crate::pad::router::ViewMove;
use config::{FingerMode, TouchConfig};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::canvas::ViewXform;
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::platform_menu::PlatformMenu;
use photocraft_ui_egui::theme::Tokens;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const MENU: &[&str] = &["Window", "Touch Input"];
const PALM: &str = "android.touch.palmRejection";
const SETTINGS: &str = "android.touch.settings";
const FINGER: &str = "android.touch.finger.";

/// The zoom range of the canvas (as the Shortcut Pad's wheel allows).
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.01..=32.0;

pub struct Shared {
    pub config: TouchConfig,
    pub settings_open: bool,
    path: PathBuf,
}

impl Shared {
    fn save(&self) {
        if let Err(e) = self.config.save(&self.path) {
            log::warn!("couldn't save the touch input settings: {e}");
        }
    }
}

pub struct TouchInput {
    shared: Rc<RefCell<Shared>>,
}

impl TouchInput {
    pub fn new(files_dir: &std::path::Path) -> Self {
        let path = files_dir.join("touch-input.json");
        let config = TouchConfig::load(&path);
        Self { shared: Rc::new(RefCell::new(Shared { config, settings_open: false, path })) }
    }

    pub fn config(&self) -> TouchConfig {
        self.shared.borrow().config.clone()
    }

    /// The Window › Touch Input rows.
    pub fn menu(&self) -> PlatformMenu {
        let (read, write) = (self.shared.clone(), self.shared.clone());
        PlatformMenu {
            items: Box::new(move || {
                let s = read.borrow();
                let row = |id: String, label: &str, checked: Option<bool>| MenuItem {
                    id,
                    label: label.into(),
                    path: MENU.iter().map(|s| s.to_string()).collect(),
                    shortcut: None,
                    enabled: true,
                    checked,
                    color: None,
                };
                let mut rows: Vec<MenuItem> =
                    FingerMode::ALL.iter().map(|m| row(format!("{FINGER}{m:?}"), m.menu_label(), Some(s.config.finger == *m))).collect();
                rows.push(row(PALM.into(), "Palm Rejection", Some(s.config.palm_rejection)));
                rows.push(row(SETTINGS.into(), "Touch Input Settings…", None));
                rows
            }),
            run: Box::new(move |id| {
                let mut s = write.borrow_mut();
                if let Some(mode) = id.strip_prefix(FINGER) {
                    s.config.finger = FingerMode::ALL.into_iter().find(|m| format!("{m:?}") == mode).ok_or_else(|| format!("unknown finger mode {mode}"))?;
                } else if id == PALM {
                    s.config.palm_rejection = !s.config.palm_rejection;
                } else if id == SETTINGS {
                    s.settings_open = true;
                    return Ok(Value::Null);
                } else {
                    return Err(format!("unknown Touch Input command {id}"));
                }
                s.save();
                Ok(Value::Null)
            }),
        }
    }

    /// The settings window, when open (after the app, so it sits on top).
    pub fn settings(&mut self, ctx: &egui::Context, tk: &Tokens) {
        let mut open = self.shared.borrow().settings_open;
        if !open {
            return;
        }
        let before = self.shared.borrow().config.clone();
        let mut c = before.clone();
        egui::Window::new("Touch Input")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.strong("One finger on the canvas");
                for m in FingerMode::ALL {
                    ui.radio_value(&mut c.finger, m, m.label());
                }
                ui.separator();
                ui.strong("Two fingers on the canvas");
                ui.checkbox(&mut c.pan, "Drag to move the canvas");
                ui.checkbox(&mut c.zoom, "Pinch to zoom");
                ui.checkbox(&mut c.rotate, "Twist to rotate the view");
                ui.separator();
                ui.strong("Taps on the canvas");
                egui::Grid::new("touch_taps").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                    tap_row(ui, tk, "Two-finger tap", "touch_two_tap", &mut c.two_finger_tap);
                    tap_row(ui, tk, "Three-finger tap", "touch_three_tap", &mut c.three_finger_tap);
                });
                ui.separator();
                ui.checkbox(&mut c.palm_rejection, "Palm rejection: ignore fingers while the pen is on or near the screen");
                ui.checkbox(&mut c.two_finger_scroll, "Two fingers scroll menus, panels and lists");
                ui.separator();
                if ui.button("Restore Defaults").clicked() {
                    c = TouchConfig::default();
                }
            });
        let mut s = self.shared.borrow_mut();
        s.settings_open = open;
        if c != before {
            s.config = c.sanitized();
            s.save();
        }
    }
}

/// A tap's row: its name, a preset menu and the shortcut (typed or chosen).
fn tap_row(ui: &mut egui::Ui, tk: &Tokens, name: &str, salt: &str, shortcut: &mut String) {
    ui.label(name);
    let current =
        PRESETS.iter().find(|(_, _, s)| s.eq_ignore_ascii_case(shortcut)).map_or(if shortcut.is_empty() { "Nothing" } else { "Custom" }, |(n, _, _)| *n);
    egui::ComboBox::from_id_salt(salt).selected_text(current).width(150.0).show_ui(ui, |ui| {
        if ui.selectable_label(shortcut.is_empty(), "Nothing").clicked() {
            shortcut.clear();
        }
        for (menu_name, _, s) in PRESETS {
            if ui.selectable_label(shortcut.eq_ignore_ascii_case(s), format!("{menu_name}  ({s})")).clicked() {
                *shortcut = (*s).into();
            }
        }
    });
    ui.vertical(|ui| {
        ui.add_sized([130.0, 24.0], egui::TextEdit::singleline(shortcut).hint_text("e.g. Ctrl+Z"));
        if let Err(e) = pad_config::parse(shortcut) {
            ui.colored_label(tk.danger, e);
        }
    });
    ui.end_row();
}

/// The view after a gesture: `m` applied to `xf` (the active canvas mapping). Returns the new
/// zoom factor relative to now, rotation and centre, keeping the document point under `m.from`
/// under `m.to`.
pub fn moved_view(xf: &ViewXform, zoom: f32, m: &ViewMove) -> (f32, f32, [f32; 2]) {
    let doc = xf.to_doc(m.from);
    let scale = if m.scale.is_finite() && m.scale > 0.0 { m.scale } else { 1.0 };
    let new_zoom = (zoom * scale).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
    // A flipped view mirrors the screen: a clockwise twist turns the camera the other way.
    let turn = if m.rotate.is_finite() { m.rotate } else { 0.0 };
    let turn = if xf.flip { -turn } else { turn };
    let rotation = (xf.rotation + turn + 540.0).rem_euclid(360.0) - 180.0;
    let moved = ViewXform { zoom: xf.zoom * (new_zoom / zoom), rotation, center: [0.0, 0.0], ..*xf };
    let off = moved.to_doc(m.to);
    let center = [(doc[0] - off[0]) as f32, (doc[1] - off[1]) as f32];
    (new_zoom, rotation, center)
}

/// Apply gesture moves to the active document's view.
pub fn apply(app: &mut PhotocraftApp, moves: &[ViewMove]) {
    if moves.is_empty() {
        return;
    }
    app.sync_views();
    for m in moves {
        let Some(i) = app.session.active_index() else { return };
        let Some(xf) = ViewXform::active(app) else { return };
        let Some(v) = app.ui.views.get_mut(i) else { return };
        let (zoom, rotation, center) = moved_view(&xf, v.zoom, m);
        if !(zoom.is_finite() && rotation.is_finite() && center.iter().all(|c| c.is_finite())) {
            return;
        }
        v.fit_pending = false;
        v.fill_pending = false;
        v.zoom = zoom;
        v.rotation = rotation;
        v.center = center;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};

    fn xf(rotation: f32, flip: bool) -> ViewXform {
        ViewXform { rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)), zoom: 1.0, center: [400.0, 300.0], flip, rotation, aspect: 1.0 }
    }

    /// The document point under the fingers stays under them, whatever the gesture.
    #[test]
    fn the_image_stays_under_the_fingers() {
        for (rot, flip) in [(0.0, false), (30.0, false), (-120.0, true)] {
            let x = xf(rot, flip);
            let m = ViewMove { from: pos2(200.0, 150.0), to: pos2(260.0, 190.0), scale: 1.7, rotate: 25.0 };
            let doc = x.to_doc(m.from);
            let (zoom, rotation, center) = moved_view(&x, 1.0, &m);
            let after = ViewXform { zoom: x.zoom * zoom, rotation, center, ..x };
            let p = after.to_screen(doc[0] as f32, doc[1] as f32);
            assert!(p.distance(m.to) < 0.01, "{rot} {flip}: {p:?}");
            assert!((zoom - 1.7).abs() < 1e-4);
        }
    }

    #[test]
    fn twist_turns_the_view_the_way_the_fingers_turn() {
        let m = ViewMove { from: pos2(400.0, 300.0), to: pos2(400.0, 300.0), scale: 1.0, rotate: 30.0 };
        assert_eq!(moved_view(&xf(0.0, false), 1.0, &m).1, 30.0);
        assert_eq!(moved_view(&xf(0.0, true), 1.0, &m).1, -30.0, "mirrored by View › Flip Horizontal");
        assert!((moved_view(&xf(170.0, false), 1.0, &m).1 + 160.0).abs() < 1e-3, "wrapped to (−180, 180]");
    }

    #[test]
    fn hostile_values_stay_in_range() {
        let m = ViewMove { from: pos2(1.0, 1.0), to: pos2(1.0, 1.0), scale: f32::NAN, rotate: f32::INFINITY };
        let (zoom, rotation, _) = moved_view(&xf(0.0, false), 1.0, &m);
        assert_eq!((zoom, rotation), (1.0, 0.0));
        let m = ViewMove { scale: 1e9, ..m };
        assert_eq!(moved_view(&xf(0.0, false), 1.0, &m).0, 32.0);
    }

    #[test]
    fn the_menu_switches_modes_and_palm_rejection() {
        let dir = std::env::temp_dir().join(format!("pc-touch-menu-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = TouchInput::new(&dir);
        let mut m = t.menu();
        let rows = (m.items)();
        assert!(rows.iter().all(|r| r.path == ["Window", "Touch Input"]));
        assert_eq!(rows.iter().filter(|r| r.checked == Some(true)).count(), 2, "Finger Draws and Palm Rejection");
        (m.run)("android.touch.finger.Navigate").unwrap();
        (m.run)(PALM).unwrap();
        assert_eq!(t.config().finger, FingerMode::Navigate);
        assert!(!t.config().palm_rejection);
        assert_eq!(TouchConfig::load(&dir.join("touch-input.json")).finger, FingerMode::Navigate, "saved");
        (m.run)(SETTINGS).unwrap();
        assert!(t.shared.borrow().settings_open);
        assert!((m.run)("android.touch.finger.Nope").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
