//! The Shortcut Pad: a slim strip on the left or right edge of the window with 2 to 10 keys
//! (split above and below) and a wheel, like the ExpressKeys and Touch Ring of a pen tablet.
//!
//! - A key presses its shortcut while it is touched: `Ctrl+Z` undoes, a modifier on its own
//!   (`Alt`) is held, so the pen Alt-clicks while a finger rests on the key.
//! - Sliding a finger around the wheel zooms, rotates the view, sizes or hardens the brush, or
//!   steps through history; tapping its centre switches to the next of the chosen functions.
//! - Window › Shortcut Pad shows or hides it, moves it to either side and opens its settings
//!   (how many keys, each key's shortcut, the wheel's functions), which are saved in the app's
//!   files directory.
//!
//! Touch routing ([`router`]) keeps the pad's fingers away from the canvas, so a key can be held
//! while the pen draws.

pub mod config;
pub mod router;
pub mod wheel;

use config::{Chord, MAX_KEYS, MIN_KEYS, PadConfig, Side, WheelMode};
use egui::{Align2, Event, FontId, Key, Modifiers, Stroke};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::platform_menu::PlatformMenu;
use photocraft_ui_egui::theme::Tokens;
use router::{Control, Layout, PadTouch, Routed, Router};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// The settings and whether their window is open: shared with the menu rows.
pub struct Shared {
    pub config: PadConfig,
    pub settings_open: bool,
    path: PathBuf,
}

impl Shared {
    fn save(&self) {
        if let Err(e) = self.config.save(&self.path) {
            log::error!("couldn't save the Shortcut Pad settings: {e}");
        }
    }
}

const MENU: [&str; 2] = ["Window", "Shortcut Pad"];
pub const SHOW: &str = "android.pad.show";
pub const LEFT: &str = "android.pad.left";
pub const RIGHT: &str = "android.pad.right";
pub const SETTINGS: &str = "android.pad.settings";

pub struct Pad {
    shared: Rc<RefCell<Shared>>,
    router: Router,
    /// Where the pad was drawn last frame (`None`: hidden).
    layout: Option<Layout>,
    /// The wheel function in use (an index into the configured list).
    mode: usize,
    turn: wheel::Turn,
    /// The ring is being turned.
    turning: bool,
    /// Keys being held, with the chord they pressed (released with the same one even if the
    /// settings change meanwhile).
    held: HashMap<usize, Chord>,
    /// Wheel commands waiting for the frame (they need the app and the egui context).
    pending: Vec<(&'static str, Value)>,
}

impl Pad {
    pub fn new(files_dir: &std::path::Path) -> Self {
        let path = files_dir.join("shortcut-pad.json");
        let config = PadConfig::load(&path);
        Self {
            shared: Rc::new(RefCell::new(Shared { config, settings_open: false, path })),
            router: Router::default(),
            layout: None,
            mode: 0,
            turn: wheel::Turn::default(),
            turning: false,
            held: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn shared(&self) -> Rc<RefCell<Shared>> {
        self.shared.clone()
    }

    /// The Window › Shortcut Pad rows.
    pub fn menu(&self) -> PlatformMenu {
        let (read, write) = (self.shared.clone(), self.shared.clone());
        PlatformMenu {
            items: Box::new(move || {
                let s = read.borrow();
                let row = |id: &str, label: &str, checked: Option<bool>| MenuItem {
                    id: id.into(),
                    label: label.into(),
                    path: MENU.iter().map(|s| s.to_string()).collect(),
                    shortcut: None,
                    enabled: true,
                    checked,
                    color: None,
                };
                vec![
                    row(SHOW, "Show Shortcut Pad", Some(s.config.enabled)),
                    row(LEFT, "On the Left", Some(s.config.side == Side::Left)),
                    row(RIGHT, "On the Right", Some(s.config.side == Side::Right)),
                    row(SETTINGS, "Shortcut Pad Settings…", None),
                ]
            }),
            run: Box::new(move |id| {
                let mut s = write.borrow_mut();
                match id {
                    SHOW => s.config.enabled = !s.config.enabled,
                    LEFT | RIGHT => {
                        s.config.side = if id == LEFT { Side::Left } else { Side::Right };
                        s.config.enabled = true;
                    }
                    SETTINGS => {
                        s.settings_open = true;
                        return Ok(Value::Null);
                    }
                    _ => return Err(format!("unknown Shortcut Pad command {id}")),
                }
                s.save();
                Ok(Value::Null)
            }),
        }
    }

    fn mode(&self) -> WheelMode {
        let s = self.shared.borrow();
        let modes = &s.config.wheel_modes;
        modes.get(self.mode % modes.len().max(1)).copied().unwrap_or(WheelMode::Zoom)
    }

    /// Route a frame's input (from `raw_input_hook`, before modifiers are applied): pad touches
    /// become key presses and wheel commands, the rest reaches the app.
    pub fn input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let shown = self.shared.borrow().config.enabled;
        let layout = if shown { self.layout.clone() } else { None };
        // A window or menu drawn over the pad last frame (the settings window, a menu dropping
        // down) keeps the touches there.
        let covered = |p: egui::Pos2| ctx.layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background);
        let routed = self.router.route(std::mem::take(&mut raw.events), layout.as_ref(), covered);
        let mut out = Vec::with_capacity(routed.len());
        if layout.is_none() {
            self.release_all(&mut out);
        }
        for r in routed {
            match r {
                Routed::App(e) => out.push(e),
                Routed::Pad(t) => self.touch(t, &mut out),
            }
        }
        raw.events = out;
    }

    fn touch(&mut self, t: PadTouch, out: &mut Vec<Event>) {
        match t {
            PadTouch::Down(Control::Key(i), _) => {
                let shortcut = self.shared.borrow().config.keys.get(i).map(|k| k.shortcut.clone()).unwrap_or_default();
                match config::parse(&shortcut) {
                    Ok(chord) if !chord.is_empty() => {
                        out.extend(press(chord));
                        if let Some(old) = self.held.insert(i, chord) {
                            out.extend(release(old));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("Shortcut Pad key {}: {e}", i + 1),
                }
            }
            PadTouch::Up(Control::Key(i)) => {
                if let Some(chord) = self.held.remove(&i) {
                    out.extend(release(chord));
                }
            }
            PadTouch::Down(Control::Ring, p) | PadTouch::Move(Control::Ring, p) => {
                if let Some(center) = self.layout.as_ref().map(|l| l.center) {
                    if matches!(t, PadTouch::Down(..)) {
                        self.turn = wheel::Turn::default();
                        self.turning = true;
                    }
                    let mode = self.mode();
                    self.pending.extend(self.turn.moved(mode, wheel::angle(center, p)));
                }
            }
            PadTouch::Up(Control::Ring) => self.turning = false,
            PadTouch::Up(Control::Center) => {
                let n = self.shared.borrow().config.wheel_modes.len().max(1);
                self.mode = (self.mode + 1) % n;
            }
            _ => {}
        }
    }

    fn release_all(&mut self, out: &mut Vec<Event>) {
        for (_, chord) in self.held.drain() {
            out.extend(release(chord));
        }
        self.turning = false;
    }

    /// Run the wheel's commands (from `logic`, before the app's).
    pub fn run_pending(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        for (id, params) in std::mem::take(&mut self.pending) {
            if let Err(e) = photocraft_ui_egui::menus::invoke(app, ctx, id, params) {
                // Nothing to zoom without a document, no redo at the newest state…: not errors
                // worth showing for a wheel turn.
                log::debug!("Shortcut Pad wheel: {id}: {e}");
            }
        }
    }

    /// Draw the pad (before the app, which then lays out in the rest of the window).
    pub fn show(&mut self, ui: &mut egui::Ui, tk: &Tokens) {
        let config = self.shared.borrow().config.clone();
        if !config.enabled {
            self.layout = None;
            return;
        }
        let panel = match config.side {
            Side::Left => egui::Panel::left("shortcut_pad"),
            Side::Right => egui::Panel::right("shortcut_pad"),
        };
        let frame = egui::Frame::new().fill(tk.chrome).stroke(Stroke::new(1.0, tk.separator));
        let mode = self.mode();
        let layout = panel
            .exact_size(router::WIDTH)
            .resizable(false)
            .frame(frame)
            .show(ui, |ui| {
                let l = Layout::new(ui.max_rect(), config.keys.len());
                self.paint(ui.painter(), &l, &config, mode, tk);
                l
            })
            .inner;
        self.layout = Some(layout);
    }

    fn paint(&self, p: &egui::Painter, l: &Layout, config: &PadConfig, mode: WheelMode, tk: &Tokens) {
        // Labels a size smaller on keys shrunk to fit a short screen.
        let font = FontId::proportional(if l.key_height() < 36.0 { 11.0 } else { 12.0 });
        for (i, rect) in l.keys.iter().enumerate() {
            let down = self.held.contains_key(&i);
            p.rect(*rect, tk.radius, if down { tk.accent } else { tk.field }, Stroke::new(1.0, tk.field_border), egui::StrokeKind::Inside);
            let label = config.keys.get(i).map(|k| k.label.as_str()).unwrap_or("");
            let label = label.to_string();
            let color = if down { tk.primary_text } else { tk.text };
            // The largest size that fits the key's width (typed labels can be wide), clipped
            // to the key at the smallest.
            let room = rect.width() - 6.0;
            let galley = [font.size, font.size - 1.0, font.size - 2.0, 9.0]
                .into_iter()
                .map(|size| p.layout_no_wrap(label.clone(), FontId::proportional(size), color))
                .find(|g| g.size().x <= room)
                .unwrap_or_else(|| p.layout_no_wrap(label.clone(), FontId::proportional(9.0), color));
            p.with_clip_rect(rect.shrink(2.0)).galley(rect.center() - galley.size() / 2.0, galley, color);
        }
        // The ring, with click-stop marks, and the centre button naming the function.
        let ring = if self.turning { tk.accent } else { tk.field_border };
        p.circle(l.center, l.ring_outer, tk.field, Stroke::new(1.5, ring));
        let mid = (l.ring_outer + l.ring_inner) / 2.0;
        for k in 0..24 {
            let a = (k as f32 * wheel::TICK).to_radians();
            p.circle_filled(l.center + egui::vec2(a.cos(), a.sin()) * mid, 1.2, tk.text_faint);
        }
        p.circle(l.center, l.ring_inner, tk.card, Stroke::new(1.0, tk.field_border));
        let n = config.wheel_modes.len();
        let current = self.mode % n.max(1);
        // One light per function, the current one lit (like a tablet's ring LEDs).
        for k in 0..n {
            let a = (-90.0 + (k as f32 - (n as f32 - 1.0) / 2.0) * 16.0f32).to_radians();
            let at = l.center + egui::vec2(a.cos(), a.sin()) * (l.ring_outer - 5.0);
            p.circle_filled(at, 2.2, if k == current { tk.accent } else { tk.text_faint });
        }
        p.text(l.center, Align2::CENTER_CENTER, mode.short(), FontId::proportional(9.0), tk.text_dim);
    }

    /// The settings window, when open (after the app, so it sits on top).
    pub fn settings(&mut self, ctx: &egui::Context, tk: &Tokens) {
        let mut open = self.shared.borrow().settings_open;
        if !open {
            return;
        }
        let before = self.shared.borrow().config.clone();
        let mut c = before.clone();
        egui::Window::new("Shortcut Pad")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.checkbox(&mut c.enabled, "Show the Shortcut Pad");
                ui.horizontal(|ui| {
                    ui.label("Side:");
                    ui.radio_value(&mut c.side, Side::Left, "Left");
                    ui.radio_value(&mut c.side, Side::Right, "Right");
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Number of keys:");
                    let mut n = c.keys.len();
                    if ui.add(egui::Slider::new(&mut n, MIN_KEYS..=MAX_KEYS)).changed() {
                        c.set_key_count(n);
                    }
                    ui.label(format!("{} above the wheel, {} below", n.div_ceil(2), n / 2));
                });
                ui.label("Keys (top to bottom). A modifier on its own, such as Alt, is held while the key is touched.");
                let above = c.keys.len().div_ceil(2);
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("pad_keys").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("");
                        ui.label("Label");
                        ui.label("Preset");
                        ui.label("Shortcut");
                        ui.end_row();
                        for (i, key) in c.keys.iter_mut().enumerate() {
                            ui.label(format!("Key {}", i + 1)).on_hover_text(if i < above { "Above the wheel" } else { "Below the wheel" });
                            ui.add_sized([84.0, 24.0], egui::TextEdit::singleline(&mut key.label).char_limit(8));
                            egui::ComboBox::from_id_salt(("pad_preset", i)).selected_text("Choose…").width(110.0).show_ui(ui, |ui| {
                                for (name, label, shortcut) in config::PRESETS {
                                    if ui.selectable_label(key.shortcut == *shortcut, format!("{name}  ({shortcut})")).clicked() {
                                        key.label = (*label).into();
                                        key.shortcut = (*shortcut).into();
                                    }
                                }
                                if ui.selectable_label(key.shortcut.is_empty(), "Nothing").clicked() {
                                    key.label.clear();
                                    key.shortcut.clear();
                                }
                            });
                            ui.vertical(|ui| {
                                ui.add_sized([130.0, 24.0], egui::TextEdit::singleline(&mut key.shortcut).hint_text("e.g. Ctrl+Z"));
                                if let Err(e) = config::parse(&key.shortcut) {
                                    ui.colored_label(tk.danger, e);
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.label("Wheel: tapping its centre switches between");
                for m in WheelMode::ALL {
                    let mut on = c.wheel_modes.contains(&m);
                    let last = on && c.wheel_modes.len() == 1;
                    if ui.add_enabled(!last, egui::Checkbox::new(&mut on, m.label())).changed() {
                        if on {
                            // Keep the configured order: the order of ALL.
                            c.wheel_modes.push(m);
                            c.wheel_modes.sort_by_key(|x| WheelMode::ALL.iter().position(|a| a == x));
                        } else {
                            c.wheel_modes.retain(|x| *x != m);
                        }
                    }
                }
                ui.separator();
                if ui.button("Restore Defaults").clicked() {
                    c = PadConfig::default();
                }
            });
        let mut s = self.shared.borrow_mut();
        s.settings_open = open;
        if c != before {
            s.config = c.sanitized();
            s.save();
        }
    }

    /// Whether the pad needs frames: a finger is on it.
    pub fn active(&self) -> bool {
        self.router.pad_active()
    }
}

fn key_event(key: Key, pressed: bool) -> Event {
    Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: Modifiers::NONE }
}

fn modifier_keys(c: Chord) -> Vec<Key> {
    [(c.ctrl, Key::ControlLeft), (c.alt, Key::AltLeft), (c.shift, Key::ShiftLeft)].into_iter().filter(|(on, _)| *on).map(|(_, k)| k).collect()
}

/// The key events of pressing a chord: its modifiers, then its key.
pub fn press(c: Chord) -> Vec<Event> {
    let mut v: Vec<Event> = modifier_keys(c).into_iter().map(|k| key_event(k, true)).collect();
    v.extend(c.key.map(|k| key_event(k, true)));
    v
}

/// The key events of releasing a chord: its key, then its modifiers.
pub fn release(c: Chord) -> Vec<Event> {
    let mut v: Vec<Event> = c.key.map(|k| key_event(k, false)).into_iter().collect();
    v.extend(modifier_keys(c).into_iter().rev().map(|k| key_event(k, false)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, TouchDeviceId, TouchId, TouchPhase, pos2, vec2};

    fn pad() -> (Pad, PathBuf) {
        let dir = std::env::temp_dir().join(format!("photocraft-padmod-{}-{}", std::process::id(), rand()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Pad::new(&dir);
        p.layout = Some(Layout::new(Rect::from_min_size(pos2(1000.0, 0.0), vec2(router::WIDTH, 800.0)), 4));
        (p, dir)
    }

    fn rand() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    fn touch(id: u64, phase: TouchPhase, pos: egui::Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos, force: None }
    }

    fn run(p: &mut Pad, events: Vec<Event>) -> Vec<Event> {
        let mut raw = egui::RawInput { events, ..Default::default() };
        p.input(&egui::Context::default(), &mut raw);
        raw.events
    }

    fn keys(events: &[Event]) -> Vec<(Key, bool)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Key { key, pressed, .. } => Some((*key, *pressed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_key_presses_its_shortcut_and_releases_it() {
        let (mut p, dir) = pad();
        let k0 = p.layout.clone().unwrap().keys[0].center();
        // Default key 1: Undo (Ctrl+Z).
        let down = run(&mut p, vec![touch(1, TouchPhase::Start, k0)]);
        assert_eq!(keys(&down), [(Key::ControlLeft, true), (Key::Z, true)]);
        assert!(!down.iter().any(|e| matches!(e, Event::Touch { .. } | Event::PointerButton { .. })), "the finger doesn't reach the canvas");
        let up = run(&mut p, vec![touch(1, TouchPhase::End, k0)]);
        assert_eq!(keys(&up), [(Key::Z, false), (Key::ControlLeft, false)]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_modifier_key_is_held_while_touched() {
        let (mut p, dir) = pad();
        let k2 = p.layout.clone().unwrap().keys[2].center(); // default: Alt
        assert_eq!(keys(&run(&mut p, vec![touch(1, TouchPhase::Start, k2)])), [(Key::AltLeft, true)]);
        assert!(keys(&run(&mut p, vec![touch(1, TouchPhase::Move, k2)])).is_empty(), "held, not repeated");
        // Hiding the pad releases it (no stuck Alt).
        p.shared.borrow_mut().config.enabled = false;
        assert_eq!(keys(&run(&mut p, vec![])), [(Key::AltLeft, false)]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_wheel_queues_commands_and_the_centre_switches_function() {
        let (mut p, dir) = pad();
        let l = p.layout.clone().unwrap();
        let at = |deg: f32| l.center + vec2(deg.to_radians().cos(), deg.to_radians().sin()) * (l.ring_outer - 4.0);
        run(&mut p, vec![touch(1, TouchPhase::Start, at(0.0)), touch(1, TouchPhase::Move, at(35.0)), touch(1, TouchPhase::Move, at(70.0))]);
        assert_eq!(p.pending.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["view.zoomIn", "view.zoomIn"], "default first function: zoom");
        run(&mut p, vec![touch(1, TouchPhase::End, at(70.0))]);
        p.pending.clear();
        // Tap the centre: Rotate View.
        run(&mut p, vec![touch(2, TouchPhase::Start, l.center), touch(2, TouchPhase::End, l.center)]);
        assert_eq!(p.mode(), WheelMode::Rotate);
        run(&mut p, vec![touch(3, TouchPhase::Start, at(0.0)), touch(3, TouchPhase::Move, at(10.0))]);
        assert_eq!(p.pending.first().map(|(id, _)| *id), Some("view.rotateView"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn menu_rows_toggle_move_and_persist() {
        let (p, dir) = pad();
        let mut menu = p.menu();
        let checked = |m: &PlatformMenu, id: &str| (m.items)().into_iter().find(|i| i.id == id).and_then(|i| i.checked);
        assert_eq!(checked(&menu, SHOW), Some(true));
        assert_eq!(checked(&menu, RIGHT), Some(true));
        (menu.run)(SHOW).unwrap();
        assert_eq!(checked(&menu, SHOW), Some(false));
        (menu.run)(LEFT).unwrap();
        assert_eq!((checked(&menu, LEFT), checked(&menu, SHOW)), (Some(true), Some(true)), "choosing a side shows it");
        (menu.run)(SETTINGS).unwrap();
        assert!(p.shared.borrow().settings_open);
        // Saved: a new pad (next launch) has them.
        let again = Pad::new(&dir);
        assert_eq!(again.shared.borrow().config.side, Side::Left);
        assert!((menu.run)("android.pad.bogus").is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    /// Ten keys: the sixth (the first below the wheel) presses its own shortcut.
    #[test]
    fn a_ten_key_pad_routes_every_key() {
        let (mut p, dir) = pad();
        p.shared.borrow_mut().config.set_key_count(10);
        let count = p.shared.borrow().config.keys.len();
        p.layout = Some(Layout::new(Rect::from_min_size(pos2(1000.0, 0.0), vec2(router::WIDTH, 800.0)), count));
        let l = p.layout.clone().unwrap();
        assert!(l.keys[5].top() > l.center.y, "key 6 is the first below the wheel");
        let shortcut = p.shared.borrow().config.keys[5].shortcut.clone();
        let chord = config::parse(&shortcut).unwrap();
        let down = run(&mut p, vec![touch(1, TouchPhase::Start, l.keys[5].center())]);
        assert_eq!(keys(&down), keys(&press(chord)), "{shortcut}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn press_and_release_mirror_each_other() {
        let c = config::parse("Ctrl+Shift+Alt+S").unwrap();
        let down: Vec<(Key, bool)> = keys(&press(c));
        let up: Vec<(Key, bool)> = keys(&release(c));
        assert_eq!(down.iter().map(|k| k.0).collect::<Vec<_>>(), up.iter().rev().map(|k| k.0).collect::<Vec<_>>());
    }
}
