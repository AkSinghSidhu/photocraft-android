//! Simple Mode: PhotoCraft's touch-first shell (View › UI Mode ▸ Simple Mode).
//!
//! Modelled on what Clip Studio Paint calls Simple Mode for behaviour only — a compact floating
//! shell for pen and finger on a tablet — but drawn entirely in PhotoCraft's own theme and with
//! its own icons; no upstream assets. Studio Mode (the desktop shell) is untouched, and
//! everything this shell does not draw keeps working: the canvas with all of its gestures and
//! tools, the Shortcut Pad on Android, dialogs and the View › UI Mode check items in the menu
//! bar (the entry point back from Studio).
//!
//! Phase 1: the mode-switch button (bottom-left, like the reference tablet UI's round switch)
//! and the undo / redo / save cluster (top-right). The tool rail, brush-size and opacity
//! sliders, layers strip and menu overlay come in later phases.

use egui::{Align2, Sense, Stroke, Vec2, vec2};

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// Diameter of the floating round buttons, in points: a comfortable touch target.
const BUTTON: f32 = 44.0;
/// Corner margin of the floating buttons on their two outer sides.
const MARGIN: f32 = 16.0;
/// The tab strip rides above the canvas; the top-right cluster clears it so it never covers a
/// tab. While Simple Mode still shows the tab strip (it hides once the menu overlay provides
/// document switching), the floating cluster starts below it.
const BELOW_TABS: f32 = 48.0;

/// The floating shell drawn over the canvas in Simple Mode, under every dialog.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    swap_button(app, ctx);
    edit_cluster(app, ctx);
}

/// Bottom-left: the round mode switch, back to Studio Mode.
fn swap_button(app: &mut PhotocraftApp, ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("pc-simple-mode-switch")).order(egui::Order::Middle).anchor(Align2::LEFT_BOTTOM, vec2(MARGIN, -MARGIN)).show(ctx, |ui| {
        let resp = round_button(ui, "arrow-left-right", "Switch to Studio Mode");
        if resp.clicked()
            && let Err(e) = crate::menus::invoke(app, ctx, "view.uiMode.studio", serde_json::json!({}))
        {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    });
}

/// Top-right: undo, redo and save at touch size, floating over the canvas.
fn edit_cluster(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let undo = app.session.is_enabled("edit.undo");
    let redo = app.session.is_enabled("edit.redo");
    let doc = app.session.active().is_some();
    egui::Area::new(egui::Id::new("pc-simple-edit-cluster"))
        .order(egui::Order::Middle)
        .anchor(Align2::RIGHT_TOP, vec2(-MARGIN, MARGIN + BELOW_TABS))
        .constrain(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                command_button(ui, app, ctx, undo, "undo-2", "Undo", "edit.undo");
                command_button(ui, app, ctx, redo, "redo-2", "Redo", "edit.redo");
                command_button(ui, app, ctx, doc, "save", "Save", "file.save");
            });
        });
}

/// A round command button that runs a command on tap; dimmed rather than hidden when disabled,
/// so the cluster doesn't shift as edits happen.
fn command_button(ui: &mut egui::Ui, app: &mut PhotocraftApp, ctx: &egui::Context, enabled: bool, icon: &str, label: &str, id: &str) {
    if !enabled {
        let t = Tokens::get(ui.ctx());
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::hover());
        ui.painter().circle_filled(rect.center(), BUTTON * 0.5 - 1.0, t.dock);
        ui.painter().circle_stroke(rect.center(), BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
        crate::icons::paint(ui, rect, icon, (BUTTON * 0.5).round(), t.text_dim);
        return;
    }
    let resp = round_button(ui, icon, label);
    if resp.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, id, serde_json::json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// A round floating button in PhotoCraft's theme: a filled circle with a centred icon that reads
/// against any document, hover feedback, and a labelled widget so the shell and tests see it.
fn round_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::click());
    ui.painter().circle_filled(rect.center(), BUTTON * 0.5 - 1.0, t.dock);
    if resp.hovered() {
        ui.painter().circle_filled(rect.center(), BUTTON * 0.5 - 1.0, t.hover);
    }
    ui.painter().circle_stroke(rect.center(), BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
    let tint = if resp.hovered() { t.text } else { t.icon };
    crate::icons::paint(ui, rect, icon, (BUTTON * 0.5).round(), tint);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), false, tl!(tooltip)));
    resp.on_hover_text(tl!(tooltip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2};
    use serde_json::json;

    /// Draw only the floating shell (as ui() does over the canvas in Simple Mode), so the
    /// widget rects under test are ours alone.
    fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
        let mut out = ctx.run_ui(
            egui::RawInput { time: Some(time), events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1024.0, 768.0))), ..Default::default() },
            |ui| show(app, ui.ctx()),
        );
        out.textures_delta.clear();
    }

    fn buttons(ctx: &egui::Context) -> Vec<Rect> {
        ctx.viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == Vec2::splat(BUTTON)).map(|w| w.rect).collect())
    }

    fn app_with_doc() -> (PhotocraftApp, egui::Context) {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 100, "height": 80})).unwrap();
        app.ui.view.ui_mode = "simple".into();
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
        (app, ctx)
    }

    #[test]
    fn the_mode_switch_button_returns_to_studio_mode() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // The round switch sits bottom-left at the margins (the placement test pins the rect);
        // tapping its centre only ever reaches our shell, so the state change proves the hit.
        let at = pos2(MARGIN + BUTTON * 0.5, 768.0 - MARGIN - BUTTON * 0.5);
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&mut app, &ctx, 1.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
        frame(&mut app, &ctx, 1.05, vec![pointer(at, false)]);
        frame(&mut app, &ctx, 1.1, vec![]);
        assert!(!app.ui.view.simple(), "tapping the switch goes back to Studio Mode");
        assert_eq!(crate::view_cmds::checked(&app, "view.uiMode.studio"), Some(true));
    }

    #[test]
    fn the_shell_buttons_are_touch_sized_on_screen() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // Four buttons (mode switch, undo, redo, save) plus each button Area's own container
        // widget, which happens to share the button's size. Pinning the census keeps the
        // shell's floating additions visible if egui or the shell changes.
        let rects = buttons(&ctx);
        assert_eq!(rects.len(), 6, "mode switch, undo, redo, save plus the two Area containers");
        for r in &rects {
            assert!(r.width() >= 44.0 && r.height() >= 44.0, "touch-sized: {r:?}");
            assert!(r.left() >= 0.0 && r.right() <= 1024.0 && r.top() >= 0.0 && r.bottom() <= 768.0, "on screen: {r:?}");
        }
        let lowest = rects.iter().copied().max_by(|a, b| a.bottom().total_cmp(&b.bottom())).unwrap();
        assert!(lowest.center().x < 512.0, "the mode switch is the lowest button, bottom-left: {lowest:?}");
        let topmost = rects.iter().copied().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        assert!(topmost.center().x > 512.0, "the undo/redo/save cluster sits right: {topmost:?}");
    }
}
