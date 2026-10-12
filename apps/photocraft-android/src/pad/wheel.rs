//! The pad's wheel: a finger sliding around the ring turns, and the current function turns the
//! movement into app commands. Clockwise zooms in, rotates the view clockwise, grows the
//! brush, hardens it, or redoes; counter-clockwise the opposite. Rotation follows the finger;
//! the others step once per [`detent`] degrees, like the ring's click-stops on a tablet.

use super::config::WheelMode;
use serde_json::{Value, json};

/// Degrees between the ring's tick marks.
pub const TICK: f32 = 15.0;

/// Degrees of ring travel per step: zoom levels and history states are big steps (a third of a
/// turn passes 4), brush sizes small ones.
pub fn detent(mode: WheelMode) -> f32 {
    match mode {
        WheelMode::Zoom | WheelMode::History => 30.0,
        WheelMode::BrushSize => 10.0,
        WheelMode::BrushHardness | WheelMode::Rotate => 15.0,
    }
}

/// The angle of `p` around `center` in degrees, clockwise on screen (y points down) from the
/// positive x axis.
pub fn angle(center: egui::Pos2, p: egui::Pos2) -> f32 {
    (p.y - center.y).atan2(p.x - center.x).to_degrees()
}

/// One finger's turn of the ring.
#[derive(Clone, Copy, Debug, Default)]
pub struct Turn {
    last: Option<f32>,
    /// Travel not yet spent on a step.
    pending: f32,
}

impl Turn {
    /// The finger is at `angle`: the commands for the movement since the last call.
    pub fn moved(&mut self, mode: WheelMode, angle: f32) -> Vec<(&'static str, Value)> {
        if !angle.is_finite() {
            return Vec::new();
        }
        let Some(last) = self.last.replace(angle) else { return Vec::new() };
        // The short way round: crossing ±180° is a small step, not a full turn back.
        let delta = (angle - last + 540.0).rem_euclid(360.0) - 180.0;
        if mode == WheelMode::Rotate {
            return if delta.abs() > 0.01 { vec![("view.rotateView", json!({ "delta": delta }))] } else { Vec::new() };
        }
        let detent = detent(mode);
        self.pending += delta;
        let steps = (self.pending / detent).trunc();
        self.pending -= steps * detent;
        let (up, down) = match mode {
            WheelMode::Zoom => ("view.zoomIn", "view.zoomOut"),
            WheelMode::BrushSize => ("tools.increaseBrushSize", "tools.decreaseBrushSize"),
            WheelMode::BrushHardness => ("tools.increaseBrushHardness", "tools.decreaseBrushHardness"),
            WheelMode::History => ("edit.redo", "edit.undo"),
            WheelMode::Rotate => return Vec::new(),
        };
        let id = if steps > 0.0 { up } else { down };
        (0..steps.abs().min(24.0) as usize).map(|_| (id, json!({}))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn ids(v: Vec<(&'static str, Value)>) -> Vec<&'static str> {
        v.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn angles_run_clockwise_on_screen() {
        let c = pos2(10.0, 10.0);
        assert!((angle(c, pos2(20.0, 10.0)) - 0.0).abs() < 1e-4);
        assert!((angle(c, pos2(10.0, 20.0)) - 90.0).abs() < 1e-4, "below the centre is a quarter turn clockwise");
    }

    #[test]
    fn stepped_functions_step_once_per_detent_in_the_turn_direction() {
        // Zoom: 30° per step.
        let mut t = Turn::default();
        assert!(t.moved(WheelMode::Zoom, 0.0).is_empty(), "the first touch only sets the start");
        assert!(t.moved(WheelMode::Zoom, 20.0).is_empty(), "less than a detent");
        assert_eq!(ids(t.moved(WheelMode::Zoom, 31.0)), ["view.zoomIn"], "20 + 11 crosses one detent");
        assert_eq!(ids(t.moved(WheelMode::Zoom, 91.0)), ["view.zoomIn", "view.zoomIn"]);
        // 1° was left over; back 90° makes −89°: two steps out, −29° kept for the next.
        assert_eq!(ids(t.moved(WheelMode::Zoom, 1.0)), ["view.zoomOut", "view.zoomOut"], "back 90°");
        assert_eq!(ids(t.moved(WheelMode::Zoom, 0.0)), ["view.zoomOut"], "the kept −29° and 1° more");
        // Brush size: 10° per step.
        let mut t = Turn::default();
        t.moved(WheelMode::BrushSize, 0.0);
        assert_eq!(ids(t.moved(WheelMode::BrushSize, -21.0)), ["tools.decreaseBrushSize", "tools.decreaseBrushSize"]);
        // History: 30° per step, counter-clockwise undoes.
        let mut t = Turn::default();
        t.moved(WheelMode::History, 0.0);
        assert_eq!(ids(t.moved(WheelMode::History, -30.0)), ["edit.undo"]);
        assert_eq!(ids(t.moved(WheelMode::History, 0.0)), ["edit.redo"]);
    }

    #[test]
    fn crossing_the_seam_is_a_small_step() {
        let mut t = Turn::default();
        t.moved(WheelMode::Zoom, 165.0);
        assert_eq!(ids(t.moved(WheelMode::Zoom, -165.0)), ["view.zoomIn"], "165° → 195° is +30°, not −330°");
    }

    #[test]
    fn rotation_follows_the_finger() {
        let mut t = Turn::default();
        t.moved(WheelMode::Rotate, 0.0);
        let v = t.moved(WheelMode::Rotate, 7.5);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "view.rotateView");
        assert!((v[0].1["delta"].as_f64().unwrap() - 7.5).abs() < 1e-4);
        assert!(t.moved(WheelMode::Rotate, 7.5).is_empty(), "no movement, no command");
    }

    #[test]
    fn hostile_angles_do_nothing() {
        let mut t = Turn::default();
        t.moved(WheelMode::Zoom, 0.0);
        assert!(t.moved(WheelMode::Zoom, f32::NAN).is_empty());
        assert!(t.moved(WheelMode::Zoom, f32::INFINITY).is_empty());
        assert!(t.moved(WheelMode::Zoom, 1e30).len() <= 24, "steps are capped");
    }
}
