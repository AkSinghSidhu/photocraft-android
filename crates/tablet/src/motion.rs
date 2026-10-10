//! Android `MotionEvent` values → pen samples (pure; tested on every platform).
//!
//! The Activity passes each event's raw fields through [`crate::android`]: the masked action,
//! the pointer's tool type, the button state, and the `AXIS_PRESSURE`, `AXIS_TILT` and
//! `AXIS_ORIENTATION` values. A stylus (Wacom EMR pens, S Pen, USI pens) reports
//! `TOOL_TYPE_STYLUS` for its tip and `TOOL_TYPE_ERASER` for its eraser end.
//!
//! Android describes the pen's pose in polar form: `AXIS_TILT` is the angle from the screen's
//! normal (0 upright, π/2 lying flat) and `AXIS_ORIENTATION` the direction the pen points in
//! the screen plane (0 up, π/2 right, ±π down). Samples use the W3C Pointer Events tilt pair
//! instead (degrees, +x leaning right, +y leaning towards the user); the conversion projects the
//! pen onto the XZ and YZ planes, as browsers do for Android pens.
//!
//! Semantics match the web runner's Pointer Events listener: a sample while the pen touches,
//! kept through the lift (the stroke's last points keep their pressure), cleared when it hovers,
//! when the event is cancelled, and for fingers and mice.

use crate::{Sample, Update};

/// `MotionEvent.getActionMasked()` values.
pub mod action {
    pub const DOWN: i32 = 0;
    pub const UP: i32 = 1;
    pub const MOVE: i32 = 2;
    pub const CANCEL: i32 = 3;
    pub const POINTER_DOWN: i32 = 5;
    pub const POINTER_UP: i32 = 6;
    pub const HOVER_MOVE: i32 = 7;
    pub const HOVER_ENTER: i32 = 9;
    pub const HOVER_EXIT: i32 = 10;
}

/// `MotionEvent.getToolType()` values.
pub mod tool {
    pub const UNKNOWN: i32 = 0;
    pub const FINGER: i32 = 1;
    pub const STYLUS: i32 = 2;
    pub const MOUSE: i32 = 3;
    pub const ERASER: i32 = 4;
}

/// One pointer of one `MotionEvent`, as the Activity read it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawEvent {
    pub action: i32,
    pub tool: i32,
    pub buttons: i32,
    /// `AXIS_PRESSURE`, nominally 0..1 (some digitizers overshoot; samples clamp).
    pub pressure: f32,
    /// `AXIS_TILT` in radians, 0..π/2.
    pub tilt: f32,
    /// `AXIS_ORIENTATION` in radians, -π..π.
    pub orientation: f32,
}

/// What `e` means for the current pen sample.
pub fn handle(e: &RawEvent) -> Update {
    let pen = matches!(e.tool, tool::STYLUS | tool::ERASER);
    if !pen {
        return Update::Set(None);
    }
    match e.action {
        action::DOWN | action::MOVE | action::POINTER_DOWN => Update::Set(Some(sample(e))),
        // Keep the last sample through the lift: the stroke's final points still need it.
        action::UP | action::POINTER_UP => Update::Keep,
        action::CANCEL | action::HOVER_ENTER | action::HOVER_MOVE | action::HOVER_EXIT => Update::Set(None),
        _ => Update::Keep,
    }
}

/// The sample of a pen contact.
fn sample(e: &RawEvent) -> Sample {
    let (tilt_x, tilt_y) = tilt_xy(e.tilt, e.orientation);
    Sample { pressure: e.pressure, tilt_x, tilt_y, rotation: 0.0, eraser: e.tool == tool::ERASER }.sanitized()
}

/// Android's polar pose (tilt from the normal, orientation in the screen plane, radians) as the
/// W3C tilt pair in degrees. Non-finite input reads as upright.
pub fn tilt_xy(tilt: f32, orientation: f32) -> (f32, f32) {
    if !tilt.is_finite() || !orientation.is_finite() {
        return (0.0, 0.0);
    }
    let tilt = tilt.clamp(0.0, std::f32::consts::FRAC_PI_2);
    // f32 cos(π/2) is a hair below 0, which would flip atan2 to ±180°.
    let (r, z) = (tilt.sin(), tilt.cos().max(0.0));
    let x = (-orientation).sin() * r;
    let y = (-orientation).cos() * r;
    (x.atan2(z).to_degrees(), y.atan2(z).to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    fn pen(action: i32, pressure: f32) -> RawEvent {
        RawEvent { action, tool: tool::STYLUS, pressure, ..Default::default() }
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn a_pen_contact_reports_pressure_until_it_hovers() {
        assert_eq!(handle(&pen(action::HOVER_ENTER, 0.0)), Update::Set(None));
        let Update::Set(Some(s)) = handle(&pen(action::DOWN, 0.25)) else { panic!("down is a sample") };
        assert_eq!((s.pressure, s.eraser), (0.25, false));
        let Update::Set(Some(s)) = handle(&pen(action::MOVE, 0.75)) else { panic!("move is a sample") };
        assert_eq!(s.pressure, 0.75);
        assert_eq!(handle(&pen(action::UP, 0.0)), Update::Keep, "the lift keeps the last pressure");
        assert_eq!(handle(&pen(action::HOVER_MOVE, 0.0)), Update::Set(None));
        assert_eq!(handle(&pen(action::CANCEL, 0.5)), Update::Set(None));
        assert_eq!(handle(&pen(42, 0.5)), Update::Keep, "unknown actions change nothing");
    }

    #[test]
    fn fingers_and_mice_are_not_pens() {
        for t in [tool::FINGER, tool::MOUSE, tool::UNKNOWN, 99] {
            assert_eq!(handle(&RawEvent { action: action::MOVE, tool: t, pressure: 0.3, ..Default::default() }), Update::Set(None), "tool {t}");
        }
    }

    #[test]
    fn the_eraser_end_is_flagged() {
        let e = RawEvent { action: action::DOWN, tool: tool::ERASER, pressure: 0.5, ..Default::default() };
        assert!(matches!(handle(&e), Update::Set(Some(Sample { eraser: true, .. }))));
    }

    #[test]
    fn tilt_converts_to_the_w3c_pair() {
        assert!(close(tilt_xy(0.0, 1.0), (0.0, 0.0)), "upright has no tilt whatever the orientation");
        // 45° from upright, pointing up the screen: the pen leans towards the user (+y).
        assert!(close(tilt_xy(FRAC_PI_4, 0.0), (0.0, 45.0)));
        // Pointing right: the barrel leans left (-x); pointing left: right (+x); down: away (-y).
        assert!(close(tilt_xy(FRAC_PI_4, FRAC_PI_2), (-45.0, 0.0)));
        assert!(close(tilt_xy(FRAC_PI_4, -FRAC_PI_2), (45.0, 0.0)));
        assert!(close(tilt_xy(FRAC_PI_4, PI), (0.0, -45.0)));
        // Diagonal: both components, each below the full tilt.
        let (x, y) = tilt_xy(FRAC_PI_4, -FRAC_PI_4);
        assert!(x > 0.0 && y > 0.0 && x < 45.0 && y < 45.0, "{x} {y}");
        // Lying flat reaches ±90 on the axis it points along.
        assert!(close(tilt_xy(FRAC_PI_2, 0.0), (0.0, 90.0)));
    }

    #[test]
    fn hostile_values_never_escape_the_documented_ranges() {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -10.0, 1e9] {
            let e = RawEvent { action: action::MOVE, tool: tool::STYLUS, buttons: -1, pressure: v, tilt: v, orientation: v };
            let Update::Set(Some(s)) = handle(&e) else { panic!("a pen move is a sample") };
            assert!((0.0..=1.0).contains(&s.pressure), "{v}: {s:?}");
            assert!((-90.0..=90.0).contains(&s.tilt_x) && (-90.0..=90.0).contains(&s.tilt_y), "{v}: {s:?}");
            assert_eq!(s.rotation, 0.0);
        }
    }
}
