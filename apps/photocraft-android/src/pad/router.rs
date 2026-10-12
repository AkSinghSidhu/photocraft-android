//! The pad's geometry and touch routing.
//!
//! egui turns only the first finger down into the pointer (egui-winit's touch emulation), so a
//! finger resting on a pad key would leave the pen without a pointer: no Alt-click, no drawing
//! while holding Space. While the pad is shown, [`Router`] takes over that emulation: touches
//! that start on the pad belong to the pad (they reach the app neither as touches nor as the
//! pointer), and the first touch elsewhere drives the pointer, whatever is held on the pad.
//! On Android every pointer event is emulated from a touch (winit reports even a mouse as
//! one), so the router drops egui-winit's pointer events and makes its own.

use egui::{Event, Pos2, Rect, TouchId, TouchPhase, pos2, vec2};

/// The pad's width in points.
pub const WIDTH: f32 = 72.0;
const MARGIN: f32 = 6.0;
const KEY_H: f32 = 44.0;
const GAP: f32 = 6.0;
const WHEEL_GAP: f32 = 12.0;

/// A part of the pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Key(usize),
    Ring,
    Center,
    /// The pad's background: touches there do nothing (and don't reach the canvas).
    Panel,
}

/// Where the pad's parts are, for a pad of a given size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    pub keys: [Rect; 4],
    pub center: Pos2,
    pub ring_outer: f32,
    pub ring_inner: f32,
}

impl Layout {
    /// Two keys, the wheel, two keys, centred vertically in `panel`.
    pub fn new(panel: Rect) -> Self {
        let w = (panel.width() - 2.0 * MARGIN).max(1.0);
        let wheel = w;
        let total = 4.0 * KEY_H + 2.0 * GAP + 2.0 * WHEEL_GAP + wheel;
        let mut y = panel.center().y - total / 2.0;
        let x = panel.left() + MARGIN;
        let key = |y: &mut f32| {
            let r = Rect::from_min_size(pos2(x, *y), vec2(w, KEY_H));
            *y += KEY_H + GAP;
            r
        };
        let k0 = key(&mut y);
        let k1 = key(&mut y);
        y += WHEEL_GAP - GAP;
        let center = pos2(x + w / 2.0, y + wheel / 2.0);
        y += wheel + WHEEL_GAP;
        let k2 = key(&mut y);
        let k3 = key(&mut y);
        Self { panel, keys: [k0, k1, k2, k3], center, ring_outer: wheel / 2.0, ring_inner: wheel * 0.3 }
    }

    /// What a touch at `p` (inside the pad) lands on. The wheel takes a little slop around its
    /// edge; keys take half the gap around them.
    pub fn hit(&self, p: Pos2) -> Control {
        let d = p.distance(self.center);
        if d <= self.ring_inner {
            return Control::Center;
        }
        if d <= self.ring_outer + WHEEL_GAP / 2.0 {
            return Control::Ring;
        }
        self.keys.iter().position(|k| k.expand(GAP / 2.0).contains(p)).map_or(Control::Panel, Control::Key)
    }
}

/// A pad touch, for the pad to act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadTouch {
    Down(Control, Pos2),
    Move(Control, Pos2),
    Up(Control),
}

/// The routed input, in its original order.
#[derive(Clone, Debug, PartialEq)]
pub enum Routed {
    App(Event),
    Pad(PadTouch),
}

#[derive(Default)]
pub struct Router {
    /// Touches that started on the pad, and what they started on (a few fingers at most).
    pad: Vec<(TouchId, Control)>,
    /// The touch driving the pointer.
    primary: Option<TouchId>,
}

impl Router {
    /// Route one frame's events. `pad` is where the pad was drawn last frame (`None`: hidden,
    /// then every touch goes to the app as before). `covered` says whether a window or menu was
    /// drawn over a point of the pad: touches there belong to it, not to the pad.
    pub fn route(&mut self, events: Vec<Event>, pad: Option<&Layout>, covered: impl Fn(Pos2) -> bool) -> Vec<Routed> {
        let Some(layout) = pad else {
            self.pad.clear();
            self.primary = None;
            return events.into_iter().map(Routed::App).collect();
        };
        let mut out = Vec::with_capacity(events.len() + 4);
        for e in events {
            match e {
                // egui-winit's emulated pointer: replaced by the router's own below.
                Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone => {}
                Event::Touch { id, phase, pos, .. } => {
                    if let Some(control) = self.pad.iter().find(|(t, _)| *t == id).map(|(_, c)| *c) {
                        match phase {
                            TouchPhase::Move => out.push(Routed::Pad(PadTouch::Move(control, pos))),
                            TouchPhase::End | TouchPhase::Cancel => {
                                self.pad.retain(|(t, _)| *t != id);
                                out.push(Routed::Pad(PadTouch::Up(control)));
                            }
                            TouchPhase::Start => {}
                        }
                        continue;
                    }
                    if phase == TouchPhase::Start && layout.panel.contains(pos) && !covered(pos) {
                        let control = layout.hit(pos);
                        self.pad.retain(|(t, _)| *t != id);
                        self.pad.push((id, control));
                        out.push(Routed::Pad(PadTouch::Down(control, pos)));
                        continue;
                    }
                    out.push(Routed::App(e.clone()));
                    let primary = match phase {
                        TouchPhase::Start if self.primary.is_none() => {
                            self.primary = Some(id);
                            true
                        }
                        _ => self.primary == Some(id),
                    };
                    if !primary {
                        continue;
                    }
                    let button = |pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
                    match phase {
                        TouchPhase::Start => {
                            out.push(Routed::App(Event::PointerMoved(pos)));
                            out.push(Routed::App(button(true)));
                        }
                        TouchPhase::Move => out.push(Routed::App(Event::PointerMoved(pos))),
                        TouchPhase::End => {
                            self.primary = None;
                            out.push(Routed::App(button(false)));
                            out.push(Routed::App(Event::PointerGone));
                        }
                        TouchPhase::Cancel => {
                            self.primary = None;
                            out.push(Routed::App(Event::PointerGone));
                        }
                    }
                }
                other => out.push(Routed::App(other)),
            }
        }
        out
    }

    /// Whether a pad touch is down (the UI repaints while one is).
    pub fn pad_active(&self) -> bool {
        !self.pad.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::TouchDeviceId;

    fn layout() -> Layout {
        Layout::new(Rect::from_min_size(pos2(1000.0, 0.0), vec2(WIDTH, 800.0)))
    }

    fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos, force: Some(1.0) }
    }

    fn app_events(r: &[Routed]) -> Vec<&Event> {
        r.iter()
            .filter_map(|r| match r {
                Routed::App(e) => Some(e),
                Routed::Pad(_) => None,
            })
            .collect()
    }

    #[test]
    fn layout_fits_the_pad_and_parts_are_hit() {
        let l = layout();
        for k in l.keys {
            assert!(l.panel.contains_rect(k), "{k:?}");
        }
        assert!(l.keys[1].bottom() < l.center.y - l.ring_outer && l.keys[2].top() > l.center.y + l.ring_outer, "keys above and below the wheel");
        assert_eq!(l.hit(l.center), Control::Center);
        assert_eq!(l.hit(l.center + vec2(l.ring_outer - 2.0, 0.0)), Control::Ring);
        for (i, k) in l.keys.iter().enumerate() {
            assert_eq!(l.hit(k.center()), Control::Key(i));
        }
        assert_eq!(l.hit(pos2(l.panel.center().x, l.panel.top() + 2.0)), Control::Panel);
    }

    #[test]
    fn hidden_pad_changes_nothing() {
        let mut r = Router::default();
        let events = vec![touch(1, TouchPhase::Start, pos2(5.0, 5.0)), Event::PointerMoved(pos2(5.0, 5.0))];
        assert_eq!(r.route(events.clone(), None, |_| false), events.into_iter().map(Routed::App).collect::<Vec<_>>());
    }

    /// A finger holds a pad key, then the pen draws: the pen drives the pointer although it is
    /// the second touch, and the finger never reaches the app.
    #[test]
    fn the_pen_draws_while_a_finger_holds_a_key() {
        let l = layout();
        let mut r = Router::default();
        let key = l.keys[2].center();
        // egui-winit made the finger the pointer: those events are dropped.
        let out = r.route(
            vec![
                touch(1, TouchPhase::Start, key),
                Event::PointerMoved(key),
                Event::PointerButton { pos: key, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            ],
            Some(&l),
            |_| false,
        );
        assert_eq!(out, vec![Routed::Pad(PadTouch::Down(Control::Key(2), key))]);
        assert!(r.pad_active());

        let a = pos2(100.0, 100.0);
        let b = pos2(150.0, 120.0);
        let out = r.route(vec![touch(2, TouchPhase::Start, a)], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerMoved(p), Event::PointerButton { pressed: true, .. }] if *p == a));
        let out = r.route(vec![touch(2, TouchPhase::Move, b), touch(1, TouchPhase::Move, key)], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerMoved(p)] if *p == b));
        assert!(out.contains(&Routed::Pad(PadTouch::Move(Control::Key(2), key))));
        let out = r.route(vec![touch(2, TouchPhase::End, b), touch(1, TouchPhase::End, key)], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerButton { pressed: false, .. }, Event::PointerGone]));
        assert_eq!(out.last(), Some(&Routed::Pad(PadTouch::Up(Control::Key(2)))));
        assert!(!r.pad_active());
    }

    /// A window or menu over the pad (the settings window, a menu dropping down) gets the
    /// touches there; the rest of the pad still works.
    #[test]
    fn a_window_over_the_pad_takes_its_touches() {
        let l = layout();
        let mut r = Router::default();
        let covered_key = l.keys[0].center();
        let out = r.route(vec![touch(1, TouchPhase::Start, covered_key)], Some(&l), |p| p.y < l.center.y);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerMoved(_), Event::PointerButton { pressed: true, .. }]), "{out:?}");
        r.route(vec![touch(1, TouchPhase::End, covered_key)], Some(&l), |_| false);
        let free_key = l.keys[3].center();
        let out = r.route(vec![touch(2, TouchPhase::Start, free_key)], Some(&l), |p| p.y < l.center.y);
        assert_eq!(out, vec![Routed::Pad(PadTouch::Down(Control::Key(3), free_key))]);
    }

    #[test]
    fn a_second_canvas_finger_is_not_the_pointer_and_order_is_kept() {
        let l = layout();
        let mut r = Router::default();
        r.route(vec![touch(1, TouchPhase::Start, pos2(10.0, 10.0))], Some(&l), |_| false);
        let out = r.route(vec![touch(2, TouchPhase::Start, pos2(20.0, 20.0)), Event::Text("x".into())], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::Text(_)]), "only the touch, no pointer: {out:?}");
        // A touch that starts off the pad and moves onto it stays the app's.
        let out = r.route(vec![touch(1, TouchPhase::Move, l.keys[0].center())], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerMoved(_)]));
        // Cancel ends the pointer without a click.
        let out = r.route(vec![touch(1, TouchPhase::Cancel, pos2(10.0, 10.0))], Some(&l), |_| false);
        assert!(matches!(app_events(&out).as_slice(), [Event::Touch { .. }, Event::PointerGone]));
    }
}
