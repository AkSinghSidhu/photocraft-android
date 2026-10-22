//! Simple Mode: PhotoCraft's touch-first shell (View › UI Mode ▸ Simple Mode).
//!
//! Modelled on what Clip Studio Paint calls Simple Mode for behaviour only — a compact floating
//! shell for pen and finger on a tablet — but drawn entirely in PhotoCraft's own theme and with
//! its own icons; no upstream assets. Studio Mode (the desktop shell) is untouched, and
//! everything this shell does not draw keeps working: the canvas with all of its gestures and
//! tools, the Shortcut Pad on Android, dialogs and the View › UI Mode check items in the menu
//! bar (the entry point back from Studio).
//!
//! Phase 1: the mode-switch button (bottom-left), the undo / redo / save cluster (top-right), the
//! left tool rail (tap selects, double-tap opens that tool's options), and the brush's size and
//! opacity as vertical sliders on the right edge. The layers strip and menu overlay come in
//! later phases.

use egui::{Align2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use crate::PhotocraftApp;
use crate::state::Tool;
use crate::theme::Tokens;

/// Diameter of the floating round buttons, in points: a comfortable touch target.
const BUTTON: f32 = 44.0;
/// Corner margin of the floating buttons on their two outer sides.
const MARGIN: f32 = 16.0;
/// Width of one bottom pill slider: the per-screen track shrinks under narrow viewports.
const PILL_MAX: f32 = 264.0;
const PILL_MIN: f32 = 150.0;
/// The layers palette's width, its thumbnails and a row's height.
const PALETTE_W: f32 = 84.0;
const THUMB: f32 = 64.0;
const THUMB_ROW: f32 = 72.0;
/// Height of the overlay cards' command rows (the sheet cards' every entry).
const SHEET_ROW: f32 = 40.0;
/// Width of the overlay cards: the quick actions, the tips and the layer actions.
const SHEET_W: f32 = 240.0;

/// The floating shell drawn over the canvas in Simple Mode, under every dialog.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    swap_button(app, ctx);
    top_bar(app, ctx);
    if app.session.active().is_some() {
        tool_rail(app, ctx);
        bottom_sliders(app, ctx);
        layers_palette(app, ctx);
    }
    tool_options(app, ctx);
    quick_actions(app, ctx);
    tips(app, ctx);
    layer_actions(app, ctx);
    menu_overlay(app, ctx);
}

/// Bottom-left: the round mode switch, back to Studio Mode.
fn swap_button(app: &mut PhotocraftApp, ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("pc-simple-mode-switch")).order(egui::Order::Middle).anchor(Align2::LEFT_BOTTOM, vec2(MARGIN, -MARGIN)).show(ctx, |ui| {
        let resp = round_button(ui, "arrow-left-right", "Switch to Studio Mode", false);
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
    let (button, gap) = top_metrics(ctx);
    let undo = app.session.is_enabled("edit.undo");
    let redo = app.session.is_enabled("edit.redo");
    let doc = app.session.active().is_some();
    let quick_id = egui::Id::new("pc-simple-quick-open");
    egui::Area::new(egui::Id::new("pc-simple-edit-cluster")).order(egui::Order::Middle).anchor(Align2::RIGHT_TOP, vec2(-MARGIN, MARGIN)).constrain(false).show(
        ctx,
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.horizontal(|ui| {
                for cmd in [
                    ClusterCommand { icon: "undo-2", label: "Undo", id: "edit.undo", enabled: undo },
                    ClusterCommand { icon: "redo-2", label: "Redo", id: "edit.redo", enabled: redo },
                    ClusterCommand { icon: "save", label: "Save", id: "file.save", enabled: doc },
                ] {
                    command_button(ui, app, ctx, cmd, button);
                }
                let on = ctx.data(|d| d.get_temp::<bool>(quick_id)).unwrap_or(false);
                let quick = round_button_sized(ui, "ellipsis", "Quick actions", on, button);
                if quick.clicked() {
                    ctx.data_mut(|d| d.insert_temp(quick_id, !on));
                }
            });
        },
    );
}

/// A round command button that runs a command on tap; dimmed rather than hidden when disabled,
/// so the cluster doesn't shift as edits happen.
/// One corner-cluster command: icon, translated label, menu id, and whether it's enabled
/// (dimmed rather than hidden so the cluster never shifts as edits happen).
struct ClusterCommand<'a> {
    icon: &'a str,
    label: &'a str,
    id: &'a str,
    enabled: bool,
}

fn command_button(ui: &mut egui::Ui, app: &mut PhotocraftApp, ctx: &egui::Context, cmd: ClusterCommand, button: f32) {
    if !cmd.enabled {
        let t = Tokens::get(ui.ctx());
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(button), Sense::hover());
        ui.painter().circle_filled(rect.center(), button * 0.5 - 1.0, t.dock);
        ui.painter().circle_stroke(rect.center(), button * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
        crate::icons::paint(ui, rect, cmd.icon, (button * 0.5).round(), t.text_dim);
        return;
    }
    let resp = round_button_sized(ui, cmd.icon, cmd.label, false, button);
    if resp.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, cmd.id, serde_json::json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// A tap that closes a sheet unless it landed on the sheet's own anchor (the button that opened
/// it) or the sheet card. Shared by the sheets' outside-closing, like the tool options'.
fn sheet_outside_tap(ctx: &egui::Context, anchors: &[egui::Id], cards: &[egui::Id]) -> bool {
    if !ctx.input(|i| i.pointer.any_click()) {
        return false;
    }
    let Some(p) = ctx.input(|i| i.pointer.interact_pos()) else { return false };
    let on = |id: egui::Id| ctx.memory(|m| m.area_rect(id)).is_some_and(|r| r.contains(p));
    !(anchors.iter().any(|&a| on(a)) || cards.iter().any(|&c| on(c)))
}

/// The top-right ellipsis's card: the File commands everyone taps most (the catalogue's own
/// labels, enablement and translations), still the shell's own tap-outside-close card.
fn quick_actions(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let id = egui::Id::new("pc-simple-quick-open");
    if !(ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)) {
        return;
    }
    if sheet_outside_tap(ctx, &[egui::Id::new("pc-simple-edit-cluster")], &[egui::Id::new("pc-simple-quick-card")]) {
        ctx.data_mut(|d| d.remove::<bool>(id));
        return;
    }
    // The exact File entries, in the reference sheet's spirit: new, open, save and both exports.
    let ids = ["file.new", "file.open", "file.export.quickExportAsPng", "file.export.exportAs", "file.save"];
    let items = crate::menus::menu_items(app);
    let rows: Vec<&crate::menus::MenuItem> = ids.iter().filter_map(|i| items.iter().find(|it| it.id == *i)).collect();
    let height = 14.0 + SHEET_ROW * rows.len() as f32 + 10.0;
    egui::Area::new(egui::Id::new("pc-simple-quick-card")).order(egui::Order::Middle).anchor(Align2::RIGHT_TOP, vec2(-MARGIN, MARGIN + BUTTON + 12.0)).show(
        ctx,
        |ui| {
            let t = Tokens::get(ui.ctx());
            let (card, _) = ui.allocate_exact_size(vec2(SHEET_W, height), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            for (i, it) in rows.iter().enumerate() {
                let r = Rect::from_min_size(pos2(card.left() + 8.0, card.top() + 14.0 + i as f32 * SHEET_ROW), vec2(SHEET_W - 16.0, SHEET_ROW));
                let resp = ui.interact(r, ui.id().with(("quick", i)), Sense::click());
                if resp.hovered() && it.enabled {
                    ui.painter().rect_filled(r, t.radius_sm as u8, t.hover);
                }
                ui.painter().text(
                    pos2(r.left() + 12.0, r.center().y),
                    Align2::LEFT_CENTER,
                    tl!(&it.label),
                    egui::FontId::proportional(13.0),
                    if it.enabled { t.text } else { t.text_faint },
                );
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, it.enabled, tl!(&it.label)));
                if resp.clicked()
                    && it.enabled
                    && let Err(e) = crate::menus::invoke(app, ctx, &it.id, serde_json::json!({}))
                {
                    app.ui.status = e;
                    app.ui.status_error = true;
                }
                if resp.clicked() {
                    ctx.data_mut(|d| d.remove::<bool>(id));
                }
            }
        },
    );
}

/// The lightbulb's card: the touch gestures the shell leans on, for a first session on the
/// tablet. Tapping anywhere outside the card (and its button) closes it.
fn tips(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let id = egui::Id::new("pc-simple-tips-open");
    if !(ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)) {
        return;
    }
    if sheet_outside_tap(ctx, &[egui::Id::new("pc-simple-menu-btn")], &[egui::Id::new("pc-simple-tips-card")]) {
        ctx.data_mut(|d| d.remove::<bool>(id));
        return;
    }
    let lines = ["Draw on the canvas with your pen or finger.", "Two fingers pan and zoom the view.", "Double-tap a tool for its options."];
    egui::Area::new(egui::Id::new("pc-simple-tips-card")).order(egui::Order::Middle).anchor(Align2::LEFT_TOP, vec2(MARGIN, MARGIN + BUTTON + 12.0)).show(
        ctx,
        |ui| {
            let t = Tokens::get(ui.ctx());
            let (card, _) = ui.allocate_exact_size(vec2(270.0, 150.0), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            ui.painter().text(pos2(card.left() + 14.0, card.top() + 22.0), Align2::LEFT_CENTER, tl!("Quick tips"), egui::FontId::proportional(13.0), t.text);
            for (i, line) in lines.iter().enumerate() {
                ui.painter().text(
                    pos2(card.left() + 14.0, card.top() + 52.0 + i as f32 * 28.0),
                    Align2::LEFT_CENTER,
                    tl!(line),
                    egui::FontId::proportional(12.0),
                    t.text_dim,
                );
            }
            let _ = app;
        },
    );
}

/// A round floating button in PhotoCraft's theme: a filled circle with a centred icon that reads
/// against any document, hover feedback, and a labelled widget so the shell and tests see it.
/// Selected buttons (the rail's active tool) get the accent treatment.
fn round_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, selected: bool) -> egui::Response {
    round_button_sized(ui, icon, tooltip, selected, BUTTON)
}

/// [`round_button`] at an explicit size (the corner clusters tighten on phone widths).
fn round_button_sized(ui: &mut egui::Ui, icon: &str, tooltip: &str, selected: bool, button: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(button), Sense::click());
    ui.painter().circle_filled(rect.center(), button * 0.5 - 1.0, if selected { t.accent_soft } else { t.dock });
    if resp.hovered() && !selected {
        ui.painter().circle_filled(rect.center(), button * 0.5 - 1.0, t.hover);
    }
    ui.painter().circle_stroke(
        rect.center(),
        button * 0.5 - 1.0,
        Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { t.accent_border } else { t.card_border }),
    );
    let tint = if selected || resp.hovered() { t.text } else { t.icon };
    crate::icons::paint(ui, rect, icon, (button * 0.5).round(), tint);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, tl!(tooltip)));
    resp.on_hover_text(tl!(tooltip))
}

/// CSP-simple's everyday tools in the left rail: tap selects, double-tap opens tool options.
const RAIL_TOOLS: [Tool; 7] = [Tool::Brush, Tool::Eraser, Tool::Eyedropper, Tool::PaintBucket, Tool::Lasso, Tool::MagicWand, Tool::Type];

/// Tools whose options the Simple shell can adjust so far; the rest get theirs with Phase 5's
/// tool-settings work (their double-tap stays inert).
fn tool_options_available(t: Tool) -> bool {
    matches!(t, Tool::Brush | Tool::Eraser | Tool::PaintBucket)
}

/// The left tool rail, open at the top under the top bar (the reference shell), with the
/// colour picks under the tools: drawing colour, paper white, and transparency.
fn tool_rail(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let active = app.ui.tool;
    egui::Area::new(egui::Id::new("pc-simple-tool-rail")).order(egui::Order::Middle).anchor(Align2::LEFT_TOP, vec2(MARGIN, MARGIN + BUTTON + 12.0)).show(
        ctx,
        |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.vertical(|ui| {
                for t in RAIL_TOOLS {
                    let resp = round_button(ui, crate::icons::tool_icon(t), t.label(), active == t);
                    if resp.clicked() && active != t {
                        let id = crate::tool_shortcuts::id(t);
                        if let Err(e) = crate::menus::invoke(app, ctx, &id, serde_json::json!({})) {
                            app.ui.status = e;
                            app.ui.status_error = true;
                        }
                    }
                    // Double-tap opens this tool's options (the reference tablet UI's gesture).
                    if resp.double_clicked() && tool_options_available(t) {
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("pc-simple-tool-options"), t));
                    }
                }
                // The divider, then the three colour picks: the drawing colour (the studio picker),
                // the paper white, and drawing with transparency (the eraser).
                let (line, _) = ui.allocate_exact_size(vec2(BUTTON, 1.0), Sense::hover());
                let t = Tokens::get(ctx);
                ui.painter().line_segment(
                    [pos2(line.left() + BUTTON / 4.0, line.center().y), pos2(line.right() - BUTTON / 4.0, line.center().y)],
                    Stroke::new(1.0, t.separator),
                );
                let fg = app.session.tools.foreground;
                let dot = color_dot(
                    ui,
                    egui::Color32::from_rgba_unmultiplied((fg[0] * 255.0) as u8, (fg[1] * 255.0) as u8, (fg[2] * 255.0) as u8, (fg[3] * 255.0) as u8),
                    false,
                    "Set foreground color",
                );
                if dot.clicked() {
                    crate::color_picker_ui::open(app, "foreground");
                }
                let white = color_dot(ui, egui::Color32::WHITE, false, "Paper color");
                if white.clicked() {
                    app.session.tools.foreground = [1.0, 1.0, 1.0, 1.0];
                    crate::type_tool::foreground_changed(app);
                }
                let dot = color_dot(ui, t.field, true, "Transparent color");
                if dot.clicked() {
                    let eraser = crate::tool_shortcuts::id(Tool::Eraser);
                    if let Err(e) = crate::menus::invoke(app, ctx, &eraser, serde_json::json!({})) {
                        app.ui.status = e;
                        app.ui.status_error = true;
                    }
                }
            });
        },
    );
}

/// One of the rail's colour picks: the visible dot rides smaller than the 44 pt hit ring. With
/// `checker`, a two-square hint of the transparency checkerboard fills the circle instead.
fn color_dot(ui: &mut egui::Ui, fill: egui::Color32, checker: bool, tooltip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::click());
    let c = rect.center();
    let d = BUTTON * 0.5 - 7.0;
    if checker {
        ui.painter().circle_filled(c, d, t.field);
        let q = d * 0.55;
        ui.painter().rect_filled(Rect::from_min_size(pos2(c.x - q + 1.5, c.y - q + 1.5), Vec2::splat(q)), t.radius_sm as u8, t.card_border);
        ui.painter().rect_filled(Rect::from_min_size(pos2(c.x + 0.5, c.y + 0.5), Vec2::splat(q)), t.radius_sm as u8, t.card_border);
    } else {
        ui.painter().circle_filled(c, d, fill);
    }
    ui.painter().circle_stroke(c, d, Stroke::new(1.0, t.text.gamma_multiply(0.9)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!(tooltip)));
    resp.on_hover_text(tl!(tooltip))
}

/// Double-tapping a rail tool opens its options card, floating just right of the rail. A tap
/// outside the rail and the card closes it again.
fn tool_options(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let id = egui::Id::new("pc-simple-tool-options");
    let Some(tool) = ctx.data(|d| d.get_temp::<Tool>(id)) else { return };
    // A tap outside the rail and the card closes it again (a canvas click that would start a
    // stroke, a menu tap, another rail button, …).
    let rail = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-tool-rail")));
    let card_rect = ctx.memory(|m| m.area_rect(id));
    if ctx.input(|i| i.pointer.any_click())
        && let Some(p) = ctx.input(|i| i.pointer.interact_pos())
        && !(rail.is_some_and(|r| r.contains(p)) || card_rect.is_some_and(|r| r.contains(p)))
    {
        ctx.data_mut(|d| d.remove::<Tool>(id));
        return;
    }
    // The card's height follows its rows: header 18, rows every 36 from 42, 16 above the bottom.
    let rows: usize = match tool {
        Tool::Brush => 4,       // Size, Opacity, Hardness, Flow
        Tool::Eraser => 2,      // Size, Opacity
        Tool::PaintBucket => 1, // Fill Opacity
        _ => 1,                 // the "coming" hint row
    };
    let height = 42.0 + 36.0 * (rows as f32 - 1.0) + 34.0;
    egui::Area::new(id).order(egui::Order::Middle).anchor(Align2::LEFT_CENTER, vec2(MARGIN + BUTTON + 12.0, 0.0)).show(ctx, |ui| {
        let t = Tokens::get(ui.ctx());
        // The painted card with the interactive rows positioned inside it.
        let (card, _) = ui.allocate_exact_size(vec2(260.0, height), Sense::hover());
        ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
        ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
        ui.painter().text(pos2(card.left() + 12.0, card.top() + 18.0), Align2::LEFT_CENTER, tl!(tool.label()), egui::FontId::proportional(13.0), t.text);
        // Sliders edit the active tool's brush with one journal entry per drag, exactly as
        // the options bar's fields do.
        if matches!(tool, Tool::Brush | Tool::Eraser) {
            crate::paint_mouse::sync_tool_brush(app);
            let before = app.session.tools.brush.clone();
            let mut b = before.clone();
            let block = crate::eraser_ui::block_mode(app, tool);
            // A Pencil-mode Eraser is always hard and at full flow (Photoshop); Block has a
            // fixed size, opacity and flow too.
            let flow_fixed = block || crate::eraser_ui::pencil_mode(app, tool);
            let percent = SliderSpec { lo: 0.0, hi: 1.0, square: false };
            options_slider(
                ui,
                card,
                0,
                &mut b.size,
                &SliderRow {
                    label: "Size",
                    caption: format!("{:.0} px", before.size),
                    spec: SliderSpec { lo: 1.0, hi: photocraft_engine::paint::MAX_BRUSH_SIZE, square: true },
                    enabled: !block,
                },
            );
            options_slider(
                ui,
                card,
                1,
                &mut b.opacity,
                &SliderRow { label: "Opacity", caption: format!("{:.0} %", before.opacity * 100.0), spec: percent, enabled: !block },
            );
            if tool == Tool::Brush {
                options_slider(
                    ui,
                    card,
                    2,
                    &mut b.hardness,
                    &SliderRow { label: "Hardness", caption: format!("{:.0} %", before.hardness * 100.0), spec: percent, enabled: !flow_fixed },
                );
                options_slider(
                    ui,
                    card,
                    3,
                    &mut b.flow,
                    &SliderRow { label: "Flow", caption: format!("{:.0} %", before.flow * 100.0), spec: percent, enabled: !flow_fixed },
                );
            }
            if b.size != before.size || b.opacity != before.opacity || b.hardness != before.hardness || b.flow != before.flow {
                crate::brush_panel::commit_gesture(app, ctx, &before, &b);
            }
        } else if tool == Tool::PaintBucket {
            // Fill Opacity is a tool option (the fill command takes it at fill time).
            let mut percent = app.ui.tool_options.fill_opacity;
            let row =
                SliderRow { label: "Opacity", caption: format!("{:.0} %", percent), spec: SliderSpec { lo: 1.0, hi: 100.0, square: false }, enabled: true };
            options_slider(ui, card, 0, &mut percent, &row);
            app.ui.tool_options.fill_opacity = percent;
        } else {
            ui.painter().text(
                pos2(card.left() + 12.0, card.top() + 42.0),
                Align2::LEFT_CENTER,
                "This tool's options are coming",
                egui::FontId::proportional(12.0),
                t.text_faint,
            );
        }
    });
}

/// A slider's value range and its travel curve: `square: true` gives small values the
/// precision (Photoshop's brush-size curve).
#[derive(Clone, Copy)]
struct SliderSpec {
    lo: f32,
    hi: f32,
    square: bool,
}

/// A row of the options card: its label, readout and slider state.
struct SliderRow {
    label: &'static str,
    /// The readout, updated by the caller as the value moves.
    caption: String,
    spec: SliderSpec,
    enabled: bool,
}

/// One row of the options card: label on the left, slider in the middle, readout on the right.
fn options_slider(ui: &mut egui::Ui, card: Rect, row: usize, value: &mut f32, r: &SliderRow) {
    let t = Tokens::get(ui.ctx());
    let y = card.top() + 42.0 + 36.0 * row as f32;
    ui.painter().text(
        pos2(card.left() + 12.0, y),
        Align2::LEFT_CENTER,
        tl!(r.label),
        egui::FontId::proportional(12.0),
        if r.enabled { t.text_dim } else { t.text_faint },
    );
    ui.painter().text(
        pos2(card.right() - 12.0, y),
        Align2::RIGHT_CENTER,
        r.caption.clone(),
        egui::FontId::proportional(12.0),
        if r.enabled { t.text } else { t.text_faint },
    );
    let track = Rect::from_center_size(pos2(card.left() + 140.0, y), vec2(104.0, 12.0));
    let resp = ui.interact(track, egui::Id::new(("pc-simple-option-slider", row)), if r.enabled { Sense::drag() } else { Sense::hover() });
    slider_draw(ui, track, false, resp, r.enabled, value, r.spec);
}

/// The size and opacity pill sliders at the bottom centre (the reference shell's layout):
/// the active tool's size and opacity where it has a concept (the Fill tool's fill
/// opacity), dim — still visible, like the reference — otherwise.
fn bottom_sliders(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let tool = app.ui.tool;
    let brushlike = tool.is_brushlike();
    let fill = tool == Tool::PaintBucket;
    if brushlike {
        crate::paint_mouse::sync_tool_brush(app);
    }
    let before = app.session.tools.brush.clone();
    let mut b = before.clone();
    // A Block-mode Eraser has a fixed size, opacity and flow (Photoshop).
    let block = brushlike && crate::eraser_ui::block_mode(app, tool);
    // The size edits the brush's; the Fill tool's pill edits its fill opacity (a tool option,
    // applied at fill time). Other tools keep both pills on the global brush, dimmed.
    let size_enabled = brushlike && !block;
    let size_spec = SliderSpec { lo: 1.0, hi: photocraft_engine::paint::MAX_BRUSH_SIZE, square: true };
    let opacity_enabled = if fill { true } else { brushlike && !block };
    let opacity_spec = if fill { SliderSpec { lo: 1.0, hi: 100.0, square: false } } else { SliderSpec { lo: 0.0, hi: 1.0, square: false } };
    let mut size = before.size;
    let mut percent = if fill { app.ui.tool_options.fill_opacity } else { before.opacity };
    let (size0, fill0, percent0) = (before.size, app.ui.tool_options.fill_opacity, percent);

    // The ride-to-the-right offset clears the rail and the mode switch on narrow screens; on
    // roomy ones the row centres on the canvas.
    let vw = ctx.input(|i| i.viewport_rect().width());
    let pw = ((vw - 2.0 * (MARGIN + BUTTON + 12.0) - 12.0) / 2.0).clamp(PILL_MIN, PILL_MAX);
    let offx = ((vw - (pw * 2.0 + 12.0)) / 2.0).max(MARGIN + BUTTON + 12.0);
    egui::Area::new(egui::Id::new("pc-simple-sliders"))
        .order(egui::Order::Middle)
        .anchor(Align2::LEFT_BOTTOM, vec2(offx, -(MARGIN + 2.0)))
        .constrain(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                let size_caption = format!("{:.1} px", size);
                let opacity_caption = format!("{:.0} %", if fill { percent } else { percent * 100.0 });
                pill(ui, pw, size_enabled, &mut size, size_spec, size_caption, "Size");
                pill(ui, pw, opacity_enabled, &mut percent, opacity_spec, opacity_caption, "Opacity");
            });
        });
    // One brush gesture per drag, exactly as the tool option sliders; the Fill tool writes its
    // tool option back directly.
    if (size != size0 || percent != percent0) && brushlike {
        b.size = size;
        b.opacity = percent;
        crate::brush_panel::commit_gesture(app, ctx, &before, &b);
    }
    if fill && percent != fill0 {
        app.ui.tool_options.fill_opacity = percent;
    }
}

/// One of the bottom pill sliders: the reference shell's rounded container with a slider track
/// and a numeric readout. Small values keep precision with `square`.
fn pill(ui: &mut egui::Ui, width: f32, enabled: bool, value: &mut f32, spec: SliderSpec, caption: String, label: &str) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, BUTTON), if enabled { Sense::drag() } else { Sense::hover() });
    ui.painter().rect_filled(rect, t.radius_lg as u8, t.card);
    ui.painter().rect_stroke(rect, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, enabled, tl!(label)));
    let readout = Rect::from_center_size(pos2(rect.right() - 36.0, rect.center().y), vec2(56.0, 28.0));
    ui.painter().rect_filled(readout, t.radius_sm as u8, t.field);
    ui.painter().text(readout.center(), Align2::CENTER_CENTER, &caption, egui::FontId::proportional(11.0), if enabled { t.text_dim } else { t.text_faint });
    let track = Rect::from_min_size(pos2(rect.left() + 16.0, rect.center().y - 5.0), vec2((rect.width() - 100.0).max(24.0), 10.0));
    let resp = resp.on_hover_text(tl!(label));
    slider_draw(ui, track, false, resp, enabled, value, spec);
}

/// Paints a slider over `track` (vertical: the low end at the bottom) and drags it. Small values
/// get precision when `square`. Returns whether the drag moved the value this frame.
fn slider_draw(ui: &mut egui::Ui, track: Rect, vertical: bool, resp: egui::Response, enabled: bool, value: &mut f32, spec: SliderSpec) -> bool {
    let t = Tokens::get(ui.ctx());
    let span = (spec.hi - spec.lo).max(f32::EPSILON);
    let clamped = (value.clamp(spec.lo, spec.hi) - spec.lo) / span;
    let frac = if spec.square { clamped.clamp(0.0, 1.0).sqrt() } else { clamped.clamp(0.0, 1.0) };
    // Where the knob sits: the low end at the bottom on a vertical track.
    let knob = if vertical { pos2(track.center().x, track.max.y - frac * track.height()) } else { pos2(track.min.x + frac * track.width(), track.center().y) };
    // Fill from the low end to the knob.
    let fill = if vertical {
        Rect::from_min_max(pos2(track.left(), knob.y), pos2(track.right(), track.max.y))
    } else {
        Rect::from_min_max(pos2(track.min.x, track.min.y), pos2(knob.x, track.max.y))
    };
    ui.painter().rect_filled(track, 5, t.field);
    if enabled && frac > 0.0 {
        ui.painter().rect_filled(fill, 5, t.icon);
    }
    let mut changed = false;
    if enabled
        && resp.dragged()
        && let Some(p) = ui.input(|i| i.pointer.interact_pos())
    {
        let read = if vertical { (track.max.y - p.y) / track.height() } else { (p.x - track.min.x) / track.width() };
        let read = read.clamp(0.0, 1.0);
        let v = if spec.square { read * read } else { read };
        let v = (spec.lo + v * span).clamp(spec.lo, spec.hi);
        if v != *value {
            *value = v;
            changed = true;
        }
    }
    ui.painter().circle_filled(knob, 9.0, if enabled { t.chrome } else { t.dock });
    ui.painter().circle_stroke(knob, 9.0, Stroke::new(if enabled { 2.0 } else { 1.0 }, if enabled { t.accent_border } else { t.card_border }));
    changed
}

/// The menu overlay's navigation: open, and which front-page entry is shown (`None` shows the
/// front page, 0 the documents list, 1.. the menu groups).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct MenuNav {
    open: bool,
    sub: Option<usize>,
}

/// Width of the menu overlay, and one of its rows' height.
const MENU_W: f32 = 320.0;
const MENU_ROW: f32 = 44.0;

/// The menu overlay's navigation id (constant-like, but `Id::new` isn't const).
fn menu_nav_id() -> egui::Id {
    egui::Id::new("pc-simple-menu-nav")
}

/// What one overlay row is and does: a heading, the back row, a page switch (the documents, a
/// menu group), a document to activate, or a command to invoke.
#[derive(Debug)]
enum MenuTap {
    Head(String),
    Back,
    Documents,
    Group(usize, String),
    Document(usize, String, bool),
    Command(crate::menus::MenuItem),
}

/// The row's accessibility label: the translated text the row shows.
fn menu_row_label(tap: &MenuTap) -> String {
    match tap {
        MenuTap::Head(text) => tl!(text.as_str()).to_string(),
        MenuTap::Back => tl!("Back").to_string(),
        MenuTap::Documents => tl!("Documents").to_string(),
        MenuTap::Group(_, g) => tl!(g.as_str()).to_string(),
        MenuTap::Document(_, name, _) => name.clone(),
        MenuTap::Command(item) => crate::menus::translated_menu_label(crate::i18n::current(), item),
    }
}

/// The corner clusters' button size and gap at the window's width: on a phone the three
/// left-side and four right-side buttons must fit side by side (16 pt margins + four 44 pt
/// buttons + tight gaps need 360 pt; below that the buttons step down to 36 pt — still a
/// fillable touch target).
fn top_metrics(ctx: &egui::Context) -> (f32, f32) {
    let w = ctx.input(|i| i.viewport_rect().width());
    let fit = |size: f32, gap: f32| MARGIN * 2.0 + 7.0 * size + 5.0 * gap;
    if fit(BUTTON, 12.0) <= w {
        (BUTTON, 12.0)
    } else if fit(BUTTON, 4.0) <= w {
        (BUTTON, 4.0)
    } else {
        (36.0, 4.0)
    }
}

/// The top corners (the reference shell's layout): Menu / Open / tips at the left, the
/// undo / redo / save cluster with the quick actions at the right, floating over the canvas.
fn top_bar(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let (button, gap) = top_metrics(ctx);
    let mut nav = ctx.data_mut(|d| d.get_temp::<MenuNav>(menu_nav_id()).unwrap_or_default());
    egui::Area::new(egui::Id::new("pc-simple-menu-btn")).order(egui::Order::Middle).anchor(Align2::LEFT_TOP, vec2(MARGIN, MARGIN)).constrain(false).show(
        ctx,
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.horizontal(|ui| {
                let menu = round_button_sized(ui, "menu", "Menu", nav.open, button);
                if menu.clicked() {
                    nav = MenuNav { open: !nav.open, sub: None };
                }
                let open = round_button(ui, "folder-open", "Open…", false);
                if open.clicked()
                    && let Err(e) = crate::menus::invoke(app, ctx, "file.open", serde_json::json!({}))
                {
                    app.ui.status = e;
                    app.ui.status_error = true;
                }
                let tips_id = egui::Id::new("pc-simple-tips-open");
                let on = ctx.data(|d| d.get_temp::<bool>(tips_id)).unwrap_or(false);
                let tips = round_button(ui, "lightbulb", "Quick tips", on);
                if tips.clicked() {
                    ctx.data_mut(|d| d.insert_temp(tips_id, !on));
                }
            });
        },
    );
    ctx.data_mut(|d| d.insert_temp(menu_nav_id(), nav));
    edit_cluster(app, ctx);
}

/// The overlay's rows for a level: `None` is the front page (the documents entry, then each
/// top-level menu group), 0 the documents, 1.. one group's commands (subgroups as headings,
/// the menus' separators skipped).
fn menu_taps(app: &PhotocraftApp, sub: Option<usize>, items: &[crate::menus::MenuItem]) -> Vec<MenuTap> {
    let mut groups: Vec<String> = Vec::new();
    for item in items {
        if let Some(grp) = item.path.first()
            && groups.last().is_none_or(|last| last != grp)
        {
            groups.push(grp.clone());
        }
    }
    match sub {
        None => {
            let mut taps = vec![MenuTap::Documents];
            taps.extend(groups.iter().enumerate().map(|(i, g)| MenuTap::Group(i + 1, g.clone())));
            taps
        }
        Some(0) => {
            let active = app.session.active_index();
            let mut taps = vec![MenuTap::Back];
            taps.extend(app.session.documents().iter().enumerate().map(|(i, st)| MenuTap::Document(i, st.doc.name.clone(), Some(i) == active)));
            taps
        }
        Some(k) => {
            let mut taps = vec![MenuTap::Back];
            if let Some(group) = groups.get(k - 1) {
                let mut last_head: Option<&str> = None;
                for item in items.iter().filter(|it| it.path.first() == Some(group)) {
                    if item.label == "---" {
                        continue; // The menus' separators don't belong in a flat list.
                    }
                    if item.path.len() > 1 && last_head != Some(item.path[1].as_str()) {
                        taps.push(MenuTap::Head(item.path[1].clone()));
                        last_head = Some(item.path[1].as_str());
                    }
                    taps.push(MenuTap::Command(item.clone()));
                }
            }
            taps
        }
    }
}

/// The command overlay: its front page lists the documents and every top-level menu group,
/// from `menus::menu_items`, so the commands are exactly the Studio bar's. A group opens its
/// rows (subgroups as headings), a row invokes its command and closes the overlay like the
/// menu bar would, and a tap outside the button and the card closes it.
fn menu_overlay(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut nav = ctx.data_mut(|d| d.get_temp::<MenuNav>(menu_nav_id()).unwrap_or_default());
    if !nav.open {
        ctx.data_mut(|d| d.insert_temp(menu_nav_id(), nav));
        return;
    }
    // A tap outside the button and the card closes the overlay (the tool options' rule); the
    // rects are from the frame before, which is fine: the shell is statically anchored.
    let btn = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-menu-btn")));
    let card = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-menu")));
    let (clicked, at) = ctx.input(|i| (i.pointer.any_click(), i.pointer.interact_pos()));
    if clicked
        && let Some(at) = at
        && !btn.is_some_and(|b| b.contains(at))
        && !card.is_some_and(|c| c.contains(at))
    {
        nav = MenuNav::default();
        ctx.data_mut(|d| d.insert_temp(menu_nav_id(), nav));
        return;
    }
    let items = crate::menus::menu_items(app);
    let taps = menu_taps(app, nav.sub, &items);
    egui::Area::new(egui::Id::new("pc-simple-menu")).order(egui::Order::Middle).anchor(Align2::LEFT_TOP, vec2(MARGIN, MARGIN + BUTTON + 10.0)).show(
        ctx,
        |ui| {
            let t = Tokens::get(ui.ctx());
            // The card, capped between its top offset and the bottom margin.
            let avail = ctx.viewport_rect().height() - MARGIN * 3.0 - BUTTON - 10.0;
            let h = (taps.len() as f32 * MENU_ROW).min(avail.max(MENU_ROW * 4.0));
            let (card, _) = ui.allocate_exact_size(vec2(MENU_W, h), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            // The rows, scrolled; each row is one whole-row tap target.
            let list = Rect::from_min_max(pos2(card.left() + 2.0, card.top() + 2.0), pos2(card.right() - 2.0, card.bottom() - 2.0));
            let mut list = ui.new_child(egui::UiBuilder::new().max_rect(list).id(egui::Id::new("pc-simple-menu-upper")));
            list.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().id_salt("pc-simple-menu-list").auto_shrink([false, true]).show(&mut list, |rows_ui| {
                for tap in taps.iter() {
                    // Fixed width: available_width changes once between egui's first passes,
                    // and a row that moves between press and release never sees the click. One
                    // click-sensed allocation is the row (a second `interact` on top would
                    // split press and release between the two widgets).
                    let (rect, resp) = rows_ui.allocate_exact_size(vec2(MENU_W - 8.0, MENU_ROW), Sense::click());
                    match tap {
                        MenuTap::Head(text) => {
                            rows_ui.painter().text(
                                pos2(rect.left() + 16.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                tl!(text.as_str()),
                                egui::FontId::proportional(12.5),
                                t.text_faint,
                            );
                        }
                        _ => {
                            let text = menu_row_label(tap);
                            // Disabled commands dim; the rest are at body size.
                            let dim = matches!(tap, MenuTap::Command(item) if !item.enabled);
                            let tint = if dim { t.text_faint } else { t.text };
                            let x = if matches!(tap, MenuTap::Back) { rect.left() + 36.0 } else { rect.left() + 16.0 };
                            rows_ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, egui::FontId::proportional(13.5), tint);
                            if matches!(tap, MenuTap::Back) {
                                crate::icons::paint(
                                    rows_ui,
                                    Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), Vec2::splat(20.0)),
                                    "chevron-left",
                                    16.0,
                                    t.icon,
                                );
                            }
                            // The trailing icon: a chevron for pages, a check for checked
                            // commands and the active document, else nothing.
                            let mut tick: Option<(&'static str, bool)> = None;
                            match tap {
                                MenuTap::Documents | MenuTap::Group(..) => tick = Some(("chevron-right", false)),
                                MenuTap::Document(.., is_active) => {
                                    if *is_active {
                                        tick = Some(("check", true));
                                    }
                                }
                                MenuTap::Command(item) if item.checked == Some(true) => tick = Some(("check", true)),
                                _ => {}
                            }
                            if let Some((icon, check)) = tick {
                                crate::icons::paint(
                                    rows_ui,
                                    Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), Vec2::splat(20.0)),
                                    icon,
                                    16.0,
                                    if check { t.accent_border } else { t.icon },
                                );
                            }
                            if let MenuTap::Command(item) = tap
                                && let Some(shortcut) = &item.shortcut
                            {
                                let x = rect.right() - if tick.is_some() { 44.0 } else { 16.0 };
                                rows_ui.painter().text(
                                    pos2(x, rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    shortcut.clone(),
                                    egui::FontId::proportional(11.5),
                                    t.text_faint,
                                );
                            }
                        }
                    }
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, menu_row_label(tap)));
                    if resp.clicked() {
                        match tap {
                            MenuTap::Documents => nav.sub = Some(0),
                            MenuTap::Group(k, _) => nav.sub = Some(*k),
                            MenuTap::Back => nav.sub = None,
                            MenuTap::Document(idx, _, _) => {
                                if let Err(e) = crate::menus::invoke(app, ctx, "document.activate", serde_json::json!({"document": *idx})) {
                                    app.ui.status = e;
                                    app.ui.status_error = true;
                                }
                                nav = MenuNav::default();
                            }
                            MenuTap::Command(item) if item.enabled => {
                                if let Err(e) = crate::menus::invoke(app, ctx, &item.id, serde_json::json!({})) {
                                    app.ui.status = e;
                                    app.ui.status_error = true;
                                }
                                nav = MenuNav::default();
                            }
                            _ => {}
                        }
                    }
                }
            });
        },
    );
    ctx.data_mut(|d| d.insert_temp(menu_nav_id(), nav));
}

/// The layers palette at the right edge, under the top cluster (the reference shell's
/// palette): new layer, its actions sheet, a thumbnail stack and the collapse caret.
fn layers_palette(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let active = st.active_layer;
    let rows = crate::layer_tree_ui::panel_rows(app, &doc);
    let open_id = egui::Id::new("pc-simple-layer-open");
    let open = ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    // The stack shows at most a screen's worth of thumbnails; the rest scrolls.
    let list_h = if !open || rows.is_empty() { 0.0 } else { (rows.len() as f32 * THUMB_ROW).min(ctx.input(|i| i.viewport_rect().height() * 0.45)) };
    let height = 16.0 + BUTTON + 8.0 + BUTTON + 8.0 + list_h + 8.0 + BUTTON + 16.0;
    egui::Area::new(egui::Id::new("pc-simple-layers")).order(egui::Order::Middle).anchor(Align2::RIGHT_TOP, vec2(-MARGIN, MARGIN + BUTTON + 12.0)).show(
        ctx,
        |ui| {
            let t = Tokens::get(ui.ctx());
            let (card, _) = ui.allocate_exact_size(vec2(PALETTE_W, height), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            // The panel's +, at touch size: a click adds the layer (the menu path opens the New
            // Layer dialog instead).
            let plus = Rect::from_center_size(pos2(card.center().x, card.top() + 16.0 + BUTTON * 0.5), Vec2::splat(BUTTON));
            let resp = ui.interact(plus, ui.id().with("add"), Sense::click());
            ui.painter().circle_filled(plus.center(), BUTTON * 0.5 - 1.0, if resp.hovered() { t.hover } else { t.dock });
            ui.painter().circle_stroke(plus.center(), BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
            crate::icons::paint(ui, plus, "plus", (BUTTON * 0.5).round(), if resp.hovered() { t.text } else { t.icon });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("New Layer")));
            if resp.clicked()
                && let Err(e) = app.run("layer.new.layer", serde_json::json!({}))
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            // The palette's ellipsis: the active layer's action sheet.
            let more = Rect::from_center_size(pos2(card.center().x, card.top() + 16.0 + BUTTON + 8.0 + BUTTON * 0.5), Vec2::splat(BUTTON));
            let actions_id = egui::Id::new("pc-simple-layer-actions-open");
            let on = ctx.data(|d| d.get_temp::<bool>(actions_id)).unwrap_or(false);
            let resp = ui.interact(more, ui.id().with("more"), Sense::click());
            ui.painter().circle_filled(more.center(), BUTTON * 0.5 - 1.0, if resp.hovered() { t.hover } else { t.dock });
            ui.painter().circle_stroke(more.center(), BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
            crate::icons::paint(ui, more, "ellipsis", (BUTTON * 0.5).round(), if resp.hovered() { t.text } else { t.icon });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("More")));
            if resp.clicked() {
                ctx.data_mut(|d| d.insert_temp(actions_id, !on));
            }
            // The thumbnail stack, scrolled when it overflows the palette's cap.
            if open && list_h > 0.0 {
                let list_top = card.top() + 16.0 + BUTTON + 8.0 + BUTTON + 8.0;
                let list_rect = Rect::from_min_max(pos2(card.left() + 4.0, list_top), pos2(card.right() - 4.0, list_top + list_h));
                let mut list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
                list.spacing_mut().item_spacing.y = 4.0;
                egui::ScrollArea::vertical().id_salt("pc-simple-layers-list").auto_shrink([false, true]).show(&mut list, |list_ui| {
                    for &(depth, l) in &rows {
                        palette_row(app, list_ui, ctx, &doc, depth, l, active);
                    }
                });
            }
            // The caret: up hides the stack, down shows it again.
            let caret = Rect::from_center_size(pos2(card.center().x, card.bottom() - 16.0 - BUTTON * 0.5), Vec2::splat(BUTTON));
            let resp = ui.interact(caret, ui.id().with("caret"), Sense::click());
            ui.painter().circle_filled(caret.center(), BUTTON * 0.5 - 1.0, if resp.hovered() { t.hover } else { t.card });
            ui.painter().circle_stroke(caret.center(), BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
            crate::icons::paint(
                ui,
                caret,
                if open { "chevron-up" } else { "chevron-down" },
                (BUTTON * 0.5).round(),
                if resp.hovered() { t.text } else { t.icon },
            );
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!(if open { "Collapse" } else { "Expand" })));
            if resp.clicked() {
                ctx.data_mut(|d| d.insert_temp(open_id, !open));
            }
        },
    );
}

/// One palette row: a square thumbnail (grouped ones indented, a group keeps its disclosure
/// triangle at the row's left), the eye as an overlay on the thumb, and tapping the thumb
/// selects the layer. The name travels in the row's accessibility label.
fn palette_row(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    doc: &photocraft_doc::Document,
    depth: usize,
    l: &photocraft_doc::Layer,
    active: Option<photocraft_doc::LayerId>,
) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), THUMB_ROW), Sense::hover());
    let mut actions: Vec<(String, serde_json::Value)> = Vec::new();
    let mut x = rect.left() + 4.0 + depth as f32 * 8.0;
    if crate::layer_tree_ui::disclosure(ui, rect, &mut x, l, &mut actions) {
        for (id, p) in actions {
            if let Err(e) = crate::menus::invoke(app, ctx, &id, p) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
    }
    let thumb = Rect::from_min_size(pos2(x + 2.0, rect.top() + 2.0), Vec2::splat(THUMB.min(rect.right() - x - 2.0)));
    // The active layer's ring, as in the panel.
    if active == Some(l.id) {
        ui.painter().rect_filled(thumb, t.radius_sm as u8, t.accent_soft);
    }
    // The thumbnail (the layers panel's cached image), fitted into the square.
    let (tex, frame) = app.layer_thumb(ui.ctx(), doc, l);
    let (fw, fh) = (frame.width() as f32, frame.height() as f32);
    if fw > 0.0 && fh > 0.0 && thumb.is_positive() {
        let scale = (thumb.width() / fw).min(thumb.height() / fh);
        let dst = Rect::from_center_size(thumb.center(), vec2(fw * scale, fh * scale));
        ui.painter().image(tex, dst, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    ui.painter().rect_stroke(
        thumb,
        t.radius_sm as u8,
        Stroke::new(if active == Some(l.id) { 2.0 } else { 1.0 }, if active == Some(l.id) { t.accent_border } else { t.card_border }),
        egui::StrokeKind::Middle,
    );
    // The eye as an overlay on the thumbnail's corner.
    let eye = Rect::from_min_size(pos2(thumb.right() - 20.0, thumb.top() + 4.0), Vec2::splat(18.0));
    let eye_resp = ui.interact(eye, ui.id().with(("eye", l.id.0)), Sense::click());
    ui.painter().circle_filled(eye.center(), 9.0, t.dock);
    ui.painter().circle_stroke(eye.center(), 9.0, Stroke::new(1.0, t.card_border));
    crate::icons::paint(ui, eye, if l.visible { "eye" } else { "eye-off" }, 11.0, if l.visible { t.text_dim } else { t.text_faint });
    eye_resp
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {}", if l.visible { tl!("Hide") } else { tl!("Show") }, l.name)));
    if eye_resp.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, "layer.setProps", serde_json::json!({"layer": l.id.0, "visible": !l.visible}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    // Tapping the thumbnail (the eye overlay sits above it) selects the layer.
    let body = Rect::from_min_max(pos2(thumb.left(), thumb.top()), pos2(thumb.right() - 24.0, thumb.bottom()));
    let sel = ui.interact(body, ui.id().with(("select", l.id.0)), Sense::click());
    sel.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {}", tl!("Select"), l.name)));
    if sel.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, "layer.select", serde_json::json!({"layer": l.id.0, "mode": "replace"}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// The palette's action sheet for the active layer: new, duplicate, delete, plus an opacity
/// slider. The sheet opens to the palette's left and closes when a tap lands outside.
fn layer_actions(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let id = egui::Id::new("pc-simple-layer-actions-open");
    if !(ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)) {
        return;
    }
    let palette = egui::Id::new("pc-simple-layers");
    if sheet_outside_tap(ctx, &[palette], &[egui::Id::new("pc-simple-layer-actions")]) {
        ctx.data_mut(|d| d.remove::<bool>(id));
        return;
    }
    let st = app.session.active().cloned();
    let active = st.as_ref().and_then(|s| s.active_layer);
    let opacity = active.and_then(|a| st.as_ref().and_then(|st| st.doc.layer(a))).map(|l| l.opacity).unwrap_or(1.0);
    let height = 8.0 + SHEET_ROW * 3.0 + 48.0 + 8.0;
    egui::Area::new(egui::Id::new("pc-simple-layer-actions"))
        .order(egui::Order::Middle)
        .anchor(Align2::RIGHT_TOP, vec2(-(MARGIN + PALETTE_W + 8.0), MARGIN + BUTTON + 12.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            let (card, _) = ui.allocate_exact_size(vec2(SHEET_W, height), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            // The three commands, in Photoshop's dropdown's order.
            let rows = [("New Layer", "layer.new.layer", true), ("Duplicate Layer…", "layer.duplicate", false), ("Delete layer", "layer.delete", false)];
            for (i, (label, cmd, direct)) in rows.iter().enumerate() {
                let enabled = st.is_some() && (*direct || app.session.is_enabled(cmd));
                let r = Rect::from_min_size(pos2(card.left() + 8.0, card.top() + 8.0 + i as f32 * SHEET_ROW), vec2(SHEET_W - 16.0, SHEET_ROW));
                let resp = ui.interact(r, ui.id().with(("action", i)), Sense::click());
                if resp.hovered() && enabled {
                    ui.painter().rect_filled(r, t.radius_sm as u8, t.hover);
                }
                ui.painter().text(
                    pos2(r.left() + 12.0, r.center().y),
                    Align2::LEFT_CENTER,
                    tl!(label),
                    egui::FontId::proportional(13.0),
                    if enabled { t.text } else { t.text_faint },
                );
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tl!(label)));
                if resp.clicked() && enabled {
                    let params = if *cmd == "layer.delete" { serde_json::json!({"__trash": true}) } else { serde_json::json!({}) };
                    // The engine commands run directly; the dialogs' commands take the menus path.
                    let result: Result<(), String> = if *direct {
                        match app.run(cmd, params) {
                            Ok(_) => Ok(()),
                            Err(e) => Err(e.to_string()),
                        }
                    } else {
                        crate::menus::invoke(app, ctx, cmd, params).map(|_| ())
                    };
                    if let Err(e) = result {
                        app.ui.status = e;
                        app.ui.status_error = true;
                    }
                    ctx.data_mut(|d| d.remove::<bool>(id));
                }
            }
            // The opacity slider, bound to the active layer with one journal entry per change.
            let band = egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(card.left() + 8.0, card.top() + 8.0 + SHEET_ROW * 3.0), pos2(card.right() - 8.0, card.bottom() - 8.0)));
            let mut row = ui.new_child(band);
            row.spacing_mut().item_spacing.x = 8.0;
            row.horizontal(|row| {
                let (label_rect, _) = row.allocate_exact_size(vec2(58.0, 32.0), Sense::hover());
                ui.painter().text(
                    pos2(label_rect.left() + 4.0, label_rect.center().y),
                    Align2::LEFT_CENTER,
                    tl!("Opacity"),
                    egui::FontId::proportional(12.0),
                    t.text,
                );
                let tw = (row.available_width() - 40.0 - 16.0).max(24.0);
                let (track_rect, resp) = row.allocate_exact_size(vec2(tw, 32.0), if active.is_some() { Sense::drag() } else { Sense::hover() });
                let track = Rect::from_center_size(track_rect.center(), vec2(tw - 8.0, 10.0));
                let mut op = opacity;
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, active.is_some(), tl!("Opacity")));
                let changed = slider_draw(row, track, false, resp, active.is_some(), &mut op, SliderSpec { lo: 0.0, hi: 1.0, square: false });
                let (read, _) = row.allocate_exact_size(vec2(40.0, 32.0), Sense::hover());
                ui.painter().text(read.right_center(), Align2::RIGHT_CENTER, format!("{:.0} %", op * 100.0), egui::FontId::proportional(11.0), t.text_dim);
                if changed
                    && let Some(a) = active
                    && let Err(e) = crate::menus::invoke(app, ctx, "layer.setProps", serde_json::json!({"layer": a.0, "opacity": op}))
                {
                    app.ui.status = e;
                    app.ui.status_error = true;
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2 as at2;
    use serde_json::json;

    /// Draw only the floating shell (as ui() does over the canvas in Simple Mode), so the
    /// widget rects under test are ours alone.
    fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
        frame_size(app, ctx, time, events, vec2(1024.0, 768.0));
    }

    /// [`frame`] at another `size` (the overlay lists the whole menu on a tall screen).
    fn frame_size(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, events: Vec<egui::Event>, size: egui::Vec2) {
        let mut out = ctx
            .run_ui(egui::RawInput { time: Some(time), events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() }, |ui| {
                show(app, ui.ctx())
            });
        out.textures_delta.clear();
    }

    fn buttons(ctx: &egui::Context) -> Vec<Rect> {
        ctx.viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == Vec2::splat(BUTTON)).map(|w| w.rect).collect())
    }

    /// Rects of the bottom pill sliders (wide and button-sized).
    fn pills(ctx: &egui::Context) -> Vec<Rect> {
        ctx.viewport(|v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, w)| w.iter())
                .filter(|w| (w.rect.height() - BUTTON).abs() < 0.01 && (PILL_MAX - 24.0..PILL_MAX + 1.0).contains(&w.rect.width()))
                .map(|w| w.rect)
                .collect()
        })
    }

    fn app_with_doc() -> (PhotocraftApp, egui::Context) {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 100, "height": 80})).unwrap();
        app.ui.view.ui_mode = "simple".into();
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
        (app, ctx)
    }

    /// The centre of the rail's `index`th button (top to bottom).
    fn rail_button(ctx: &egui::Context, index: usize) -> egui::Pos2 {
        let rail = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-tool-rail"))).unwrap();
        at2(rail.left() + BUTTON * 0.5, rail.top() + index as f32 * (BUTTON + 10.0) + BUTTON * 0.5)
    }

    /// The layers palette's `index`th thumbnail's centre (from the top) and, with `eye`, its
    /// eye overlay's centre.
    fn palette_row_center(ctx: &egui::Context, index: usize, eye: bool) -> egui::Pos2 {
        let area = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        let thumb = Rect::from_min_size(pos2(area.left() + 10.0, area.top() + 120.0 + index as f32 * (THUMB_ROW + 4.0) + 2.0), Vec2::splat(THUMB));
        if eye { at2(thumb.right() - 11.0, thumb.top() + 13.0) } else { thumb.center() }
    }

    /// The palette's heading buttons' centres: 0 the plus, 1 the ellipsis, 2 the caret.
    fn palette_head(ctx: &egui::Context, index: usize) -> egui::Pos2 {
        let a = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        let top = 16.0 + BUTTON + 8.0;
        let y = match index {
            0 => 16.0 + BUTTON * 0.5,
            1 => top + BUTTON * 0.5,
            _ => a.height() - 16.0 - BUTTON * 0.5,
        };
        at2(a.left() + PALETTE_W * 0.5, a.top() + y)
    }

    /// The top row's left cluster's `index`th button (0 the menu, 1 open, 2 the tips).
    fn top_left(ctx: &egui::Context, index: usize) -> egui::Pos2 {
        let a = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-menu-btn"))).unwrap();
        at2(a.left() + index as f32 * (BUTTON + top_metrics(ctx).1) + BUTTON * 0.5, a.top() + BUTTON * 0.5)
    }

    /// The top row's right cluster's `index`th button (0 undo, 1 redo, 2 save, 3 quick actions).
    fn top_right(ctx: &egui::Context, index: usize) -> egui::Pos2 {
        let a = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-edit-cluster"))).unwrap();
        at2(a.right() - (BUTTON * 0.5) - index as f32 * (BUTTON + top_metrics(ctx).1), a.top() + BUTTON * 0.5)
    }

    fn tap(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, pos: egui::Pos2) {
        tap_size(app, ctx, time, pos, vec2(1024.0, 768.0));
    }

    /// [`tap`] at another `size`, so a tap keeps its layout between press and release.
    fn tap_size(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, pos: egui::Pos2, size: egui::Vec2) {
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        // The move in its own pass first (kittest's order): a pointer that moves and presses in
        // one batch presses before the row has ever been hovered, and egui hands the press to
        // the widget it knew about last; the row then never sees the click.
        frame_size(app, ctx, time, vec![egui::Event::PointerMoved(pos)], size);
        frame_size(app, ctx, time + 0.01, vec![pointer(pos, true)], size);
        frame_size(app, ctx, time + 0.05, vec![pointer(pos, false)], size);
        frame_size(app, ctx, time + 0.1, vec![], size);
    }

    /// The corner clusters fit at a phone's width: every round button stays on screen and none
    /// overlaps another (the reference shell keeps all commands reachable on a narrow window).
    #[test]
    fn phone_width_keeps_corner_buttons_apart() {
        let (mut app, ctx) = app_with_doc();
        frame_size(&mut app, &ctx, 0.0, vec![], vec2(392.0, 844.0));
        frame_size(&mut app, &ctx, 0.1, vec![], vec2(392.0, 844.0));
        let rects = buttons(&ctx);
        assert!(rects.len() >= 8, "the top clusters and rail draw buttons: got {rects:?}");
        for r in &rects {
            assert!(r.left() >= 0.0 && r.right() <= 392.0, "button {r:?} runs off the phone width");
        }
        for a in &rects {
            for b in &rects {
                if a != b {
                    assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
                }
            }
        }
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
        // The top row's six buttons (menu, open, tips / undo, redo, save, quick actions), the
        // rail's seven tools and three colour picks, the palette's plus/ellipsis/caret, the mode
        // switch, and the mode-switch Areas' own containers (which share the buttons' size).
        let rects = buttons(&ctx);
        assert_eq!(rects.len(), 23, "21 round buttons plus the mode switch Areas' containers");
        for r in &rects {
            assert!(r.width() >= 44.0 && r.height() >= 44.0, "touch-sized: {r:?}");
            assert!(r.left() >= 0.0 && r.right() <= 1024.0 && r.top() >= 0.0 && r.bottom() <= 768.0, "on screen: {r:?}");
        }
        let lowest = rects.iter().copied().max_by(|a, b| a.bottom().total_cmp(&b.bottom())).unwrap();
        assert!(lowest.center().x < 512.0, "the mode switch is the lowest button, bottom-left: {lowest:?}");
        let topmost = rects.iter().copied().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        // The tab strip is hidden in Simple Mode: the top row's two clusters start at the margin.
        assert_eq!(topmost.top(), MARGIN, "the top row starts at the margin: {topmost:?}");
    }

    #[test]
    fn the_layers_palette_selects_and_adds_a_layer() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let id0 = app.session.active().unwrap().doc.layers.last().unwrap().id.0;
        app.run("layer.new.layer", json!({})).unwrap();
        frame(&mut app, &ctx, 0.2, vec![]);
        frame(&mut app, &ctx, 0.3, vec![]);
        // The new layer is on top and active: the palette's second thumbnail selects it back to
        // the Background below.
        assert_ne!(app.session.active().unwrap().active_layer.map(|l| l.0), Some(id0));
        tap(&mut app, &ctx, 1.0, palette_row_center(&ctx, 1, false));
        frame(&mut app, &ctx, 1.15, vec![]);
        assert_eq!(app.session.active().unwrap().active_layer.map(|l| l.0), Some(id0), "tapping a palette thumbnail selects the layer");
        // The round + at the palette's head adds another layer.
        tap(&mut app, &ctx, 1.2, palette_head(&ctx, 0));
        frame(&mut app, &ctx, 1.35, vec![]);
        assert_eq!(app.session.active().unwrap().doc.layers.len(), 3, "+ adds a layer");
    }

    #[test]
    fn the_layers_palette_toggles_visibility() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let visible = app.session.active().unwrap().doc.layers.last().unwrap().visible;
        tap(&mut app, &ctx, 1.0, palette_row_center(&ctx, 0, true));
        frame(&mut app, &ctx, 1.15, vec![]);
        let l = app.session.active().unwrap().doc.layers.last().unwrap();
        assert_eq!(l.visible, !visible, "the thumbnail's eye toggles visibility");
    }

    #[test]
    fn the_layers_strip_collapses_a_group() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // Group the document's one layer: the strip lists the group and its (expanded) content.
        app.run("layer.groupLayers", json!({})).unwrap();
        frame(&mut app, &ctx, 0.2, vec![]);
        frame(&mut app, &ctx, 0.3, vec![]);
        assert_eq!(crate::layer_tree_ui::panel_rows(&app, &app.session.active().unwrap().doc).len(), 2, "the group row lists expanded, so its content row too");
        // The group row's triangle (at the row's left, the palette lists indent by depth)
        // collapses it, leaving one row.
        let palette = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        tap(&mut app, &ctx, 1.0, at2(palette.left() + 9.0, palette.top() + 152.0));
        frame(&mut app, &ctx, 1.15, vec![]);
        assert_eq!(crate::layer_tree_ui::panel_rows(&app, &app.session.active().unwrap().doc).len(), 1, "a collapsed group lists its row alone");
    }

    #[test]
    fn the_rail_selects_a_tool() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        assert_eq!(app.ui.tool, Tool::Brush);
        // The second rail row is the Eraser.
        tap(&mut app, &ctx, 1.0, rail_button(&ctx, 1));
        frame(&mut app, &ctx, 1.2, vec![]);
        assert_eq!(app.ui.tool, Tool::Eraser, "tapping the rail selects the tool");
        // The Fill tool keeps its own sliders' shape: nothing for the marquee-like tools.
        tap(&mut app, &ctx, 1.3, rail_button(&ctx, 5));
        frame(&mut app, &ctx, 1.45, vec![]);
        assert_eq!(app.ui.tool, Tool::MagicWand);
    }

    #[test]
    fn double_tap_opens_the_tool_options_and_a_tap_elsewhere_closes_them() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let brush = rail_button(&ctx, 0);
        tap(&mut app, &ctx, 1.0, brush);
        tap(&mut app, &ctx, 1.25, brush);
        frame(&mut app, &ctx, 1.4, vec![]);
        let id = egui::Id::new("pc-simple-tool-options");
        assert!(ctx.data(|d| d.get_temp::<Tool>(id)).is_some(), "a double-tap on the rail's Brush opens its options card");
        // A tap on the canvas (a Brush stroke press) closes the card again.
        tap(&mut app, &ctx, 1.5, pos2(600.0, 400.0));
        frame(&mut app, &ctx, 1.65, vec![]);
        assert!(ctx.data(|d| d.get_temp::<Tool>(id)).is_none(), "tapping outside the rail and the card closes the options");
    }

    #[test]
    fn the_size_pill_resizes_the_brush() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let rects = pills(&ctx);
        assert_eq!(rects.len(), 2, "size and opacity pills at the bottom centre");
        let size = rects.iter().copied().min_by(|a, b| a.left().total_cmp(&b.left())).unwrap();
        let before = app.session.tools.brush.size;
        // Drag the knob left to right: the size opens up (square curve, the full travel the max).
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let from = pos2(size.left() + 22.0, size.center().y);
        frame(&mut app, &ctx, 2.0, vec![egui::Event::PointerMoved(from), pointer(from, true)]);
        for (dt, dx) in [(0.05, 40.0), (0.1, 90.0), (0.15, 160.0)] {
            let at = pos2(size.left() + 22.0 + dx, size.center().y);
            frame(&mut app, &ctx, 2.0 + dt, vec![egui::Event::PointerMoved(at)]);
        }
        frame(&mut app, &ctx, 2.2, vec![pointer(size.center(), false)]);
        frame(&mut app, &ctx, 2.25, vec![]);
        let after = app.session.tools.brush.size;
        assert!(after > before * 10.0, "dragging the size pill to the right opens the brush up: {before} -> {after}");
        // The opacity pill drags right too and stays inside its 0..1 range.
        let opacity = rects.iter().copied().max_by(|a, b| a.left().total_cmp(&b.left())).unwrap();
        let opacity_before = app.session.tools.brush.opacity;
        let at = pos2(opacity.left() + 24.0, opacity.center().y);
        frame(&mut app, &ctx, 3.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
        let mid = pos2(opacity.left() + 150.0, opacity.center().y);
        frame(&mut app, &ctx, 3.05, vec![egui::Event::PointerMoved(mid)]);
        frame(&mut app, &ctx, 3.1, vec![pointer(mid, false)]);
        frame(&mut app, &ctx, 3.15, vec![]);
        assert_ne!(app.session.tools.brush.opacity, opacity_before, "dragging the opacity pill changes the opacity");
        assert!((0.0..=1.0).contains(&app.session.tools.brush.opacity));
    }

    #[test]
    fn the_rails_colour_picks_work() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // The rail's column holds the seven tools and the three picks; the picks are the last
        // three BUTTON-sized rects in the rail column.
        let rail = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-tool-rail"))).unwrap();
        let mut dots = buttons(&ctx).into_iter().filter(|r| (r.left() - rail.left()).abs() < 2.0 && rail.contains_rect(*r)).collect::<Vec<_>>();
        assert_eq!(dots.len(), 10, "the rail's seven tools then its three colour picks");
        dots.sort_by(|a, b| a.top().total_cmp(&b.top()));
        dots.drain(0..7);
        assert_eq!(dots.len(), 3, "the drawing colour, the paper and the transparency pick");
        // Tapping the drawing colour opens the studio colour picker as a dialog.
        let dialogs = app.ui.dialogs.len();
        tap(&mut app, &ctx, 1.0, dots[0].center());
        frame(&mut app, &ctx, 1.15, vec![]);
        assert!(app.ui.dialogs.len() > dialogs, "the drawing colour dot opens the colour picker");
        // The paper pick sets the foreground white, the transparency pick the Eraser.
        app.ui.dialogs.clear();
        app.session.tools.foreground = [0.0, 0.0, 0.0, 1.0];
        tap(&mut app, &ctx, 2.0, dots[1].center());
        frame(&mut app, &ctx, 2.15, vec![]);
        assert_eq!(app.session.tools.foreground, [1.0, 1.0, 1.0, 1.0], "the paper pick sets white");
        assert_eq!(app.ui.dialogs.len(), 0, "the paper pick needs no dialog");
        tap(&mut app, &ctx, 3.0, dots[2].center());
        frame(&mut app, &ctx, 3.15, vec![]);
        assert_eq!(app.ui.tool, Tool::Eraser, "the transparency pick draws with the eraser");
    }

    #[test]
    fn the_quick_actions_sheet_opens_and_runs_a_file_command() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // The top-right ellipsis opens the sheet; a tap outside closes it.
        tap(&mut app, &ctx, 1.0, top_right(&ctx, 0));
        frame(&mut app, &ctx, 1.15, vec![]);
        let card = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-quick-card")));
        assert!(card.is_some(), "the quick actions card draws");
        tap(&mut app, &ctx, 2.0, pos2(600.0, 300.0));
        frame(&mut app, &ctx, 2.15, vec![]);
        assert!(!ctx.data(|d| d.get_temp::<bool>(egui::Id::new("pc-simple-quick-open")).unwrap_or(false)), "a tap outside closes the sheet");
        // Reopen and tap the Export As… row (rows run top to bottom, 0..): its dialog opens.
        tap(&mut app, &ctx, 3.0, top_right(&ctx, 0));
        frame(&mut app, &ctx, 3.15, vec![]);
        let card = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-quick-card"))).unwrap();
        let dialogs = app.ui.dialogs.len();
        // Row 0 is New… (the export rows need services the tests don't wire): its dialog opens.
        tap(&mut app, &ctx, 4.0, at2(card.center().x, card.top() + 14.0 + SHEET_ROW * 0.5));
        frame(&mut app, &ctx, 4.15, vec![]);
        assert!(app.ui.dialogs.len() > dialogs, "New… opens the new-document dialog");
        assert!(!ctx.data(|d| d.get_temp::<bool>(egui::Id::new("pc-simple-quick-open")).unwrap_or(false)), "a sheet row closes the sheet too");
    }

    #[test]
    fn the_tips_card_opens_from_the_lightbulb() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        tap(&mut app, &ctx, 1.0, top_left(&ctx, 2));
        frame(&mut app, &ctx, 1.15, vec![]);
        assert!(ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-tips-card"))).is_some(), "the tips card opens from the lightbulb");
        tap(&mut app, &ctx, 2.0, pos2(600.0, 300.0));
        frame(&mut app, &ctx, 2.15, vec![]);
        assert!(!ctx.data(|d| d.get_temp::<bool>(egui::Id::new("pc-simple-tips-open")).unwrap_or(false)), "a tap outside closes it");
    }

    #[test]
    fn the_layers_palette_collapse_and_actions() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        // The caret collapses the stack, leaving the palette's heading; the caret then expands.
        tap(&mut app, &ctx, 1.0, palette_head(&ctx, 2));
        frame(&mut app, &ctx, 1.15, vec![]);
        let palette = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        assert!(palette.height() < 200.0, "collapsed, the palette holds its heading alone: {palette:?}");
        assert_eq!(crate::layer_tree_ui::panel_rows(&app, &app.session.active().unwrap().doc).len(), 1);
        tap(&mut app, &ctx, 2.0, palette_head(&ctx, 2));
        frame(&mut app, &ctx, 2.15, vec![]);
        let palette = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        assert!(palette.height() > 200.0, "expanded again, the stack is back: {palette:?}");
        // The ellipsis's sheet: New Layer adds, and a sheet row closes the sheet.
        tap(&mut app, &ctx, 3.0, palette_head(&ctx, 1));
        frame(&mut app, &ctx, 3.15, vec![]);
        let sheet = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layer-actions")));
        assert!(sheet.is_some(), "the action sheet draws");
        let sheet = sheet.unwrap();
        tap(&mut app, &ctx, 4.0, at2(sheet.center().x, sheet.top() + 8.0 + SHEET_ROW * 0.5));
        frame(&mut app, &ctx, 4.15, vec![]);
        assert_eq!(app.session.active().unwrap().doc.layers.len(), 2, "the sheet's New Layer adds one");
        assert!(!ctx.data(|d| d.get_temp::<bool>(egui::Id::new("pc-simple-layer-actions-open")).unwrap_or(false)), "a sheet row closes it");
    }
}
