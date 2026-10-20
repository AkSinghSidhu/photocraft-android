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
/// The tab strip rides above the canvas; the top-right cluster clears it so it never covers a
/// tab. While Simple Mode still shows the tab strip (it hides once the menu overlay provides
/// document switching), the floating cluster starts below it.
const BELOW_TABS: f32 = 48.0;
/// Travel length of the right-edge sliders, in points.
const SLIDER: f32 = 200.0;
/// Width and height of the floating layers strip, and its row heights.
const STRIP_W: f32 = 280.0;
const STRIP_H: f32 = 260.0;
const ROW: f32 = 40.0;
/// The rows' eye cell and (groups') disclosure cell, so rows line up either way.
const EYE_CELL: f32 = 44.0;
const DISC_CELL: f32 = 28.0;

/// The floating shell drawn over the canvas in Simple Mode, under every dialog.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    swap_button(app, ctx);
    edit_cluster(app, ctx);
    if app.session.active().is_some() {
        tool_rail(app, ctx);
        brush_sliders(app, ctx);
        layers_strip(app, ctx);
    }
    tool_options(app, ctx);
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
    let resp = round_button(ui, icon, label, false);
    if resp.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, id, serde_json::json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// A round floating button in PhotoCraft's theme: a filled circle with a centred icon that reads
/// against any document, hover feedback, and a labelled widget so the shell and tests see it.
/// Selected buttons (the rail's active tool) get the accent treatment.
fn round_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::click());
    ui.painter().circle_filled(rect.center(), BUTTON * 0.5 - 1.0, if selected { t.accent_soft } else { t.dock });
    if resp.hovered() && !selected {
        ui.painter().circle_filled(rect.center(), BUTTON * 0.5 - 1.0, t.hover);
    }
    ui.painter().circle_stroke(
        rect.center(),
        BUTTON * 0.5 - 1.0,
        Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { t.accent_border } else { t.card_border }),
    );
    let tint = if selected || resp.hovered() { t.text } else { t.icon };
    crate::icons::paint(ui, rect, icon, (BUTTON * 0.5).round(), tint);
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

/// The left tool rail, vertically centred (the corners keep the mode switch and the cluster).
fn tool_rail(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let active = app.ui.tool;
    egui::Area::new(egui::Id::new("pc-simple-tool-rail")).order(egui::Order::Middle).anchor(Align2::LEFT_CENTER, vec2(MARGIN, 0.0)).show(ctx, |ui| {
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
        });
    });
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
                crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &b);
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

/// The brush's size and opacity as vertical sliders on the right edge for the brushlike tools,
/// and the Fill tool's opacity; nothing for tools without a size or opacity concept.
fn brush_sliders(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let tool = app.ui.tool;
    if !(tool.is_brushlike() || tool == Tool::PaintBucket) {
        return;
    }
    egui::Area::new(egui::Id::new("pc-simple-sliders")).order(egui::Order::Middle).anchor(Align2::RIGHT_CENTER, vec2(-MARGIN, 0.0)).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 16.0;
        if tool.is_brushlike() {
            crate::paint_mouse::sync_tool_brush(app);
            let before = app.session.tools.brush.clone();
            let mut b = before.clone();
            // A Block-mode Eraser has a fixed size, opacity and flow (Photoshop).
            let block = crate::eraser_ui::block_mode(app, tool);
            let size_caption = format!("{:.0} px", before.size);
            let opacity_caption = format!("{:.0} %", before.opacity * 100.0);
            let size_changed =
                vslider(ui, !block, &mut b.size, SliderSpec { lo: 1.0, hi: photocraft_engine::paint::MAX_BRUSH_SIZE, square: true }, size_caption);
            let changed = vslider(ui, !block, &mut b.opacity, SliderSpec { lo: 0.0, hi: 1.0, square: false }, opacity_caption) || size_changed;
            if changed {
                crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &b);
            }
        } else if tool == Tool::PaintBucket {
            let mut percent = app.ui.tool_options.fill_opacity;
            let caption = format!("{:.0} %", app.ui.tool_options.fill_opacity);
            vslider(ui, true, &mut percent, SliderSpec { lo: 1.0, hi: 100.0, square: false }, caption);
            app.ui.tool_options.fill_opacity = percent;
        }
    });
}

/// A vertical slider in flow: `SLIDER` tall with its caption under it, `BUTTON` wide as hit
/// area. Small values get precision with `square` (Photoshop's brush-size curve).
fn vslider(ui: &mut egui::Ui, enabled: bool, value: &mut f32, spec: SliderSpec, caption: String) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON, SLIDER), if enabled { Sense::drag() } else { Sense::hover() });
    let track = Rect::from_center_size(pos2(rect.center().x, rect.center().y - 7.0), vec2(12.0, SLIDER - 44.0));
    let changed = slider_draw(ui, track, true, resp, enabled, value, spec);
    let (cap, _) = ui.allocate_exact_size(vec2(BUTTON, 14.0), Sense::hover());
    ui.painter().text(
        cap.center(),
        Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional(10.0),
        if enabled { Tokens::get(ui.ctx()).text_dim } else { Tokens::get(ui.ctx()).text_faint },
    );
    changed
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

/// The floating layers strip, bottom-right (left of the slider column, clearing the corners):
/// thumbnails, names and eyes. A tap selects, the eye hides, and + adds a layer.
fn layers_strip(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let active = st.active_layer;
    let rows = crate::layer_tree_ui::panel_rows(app, &doc);
    egui::Area::new(egui::Id::new("pc-simple-layers"))
        .order(egui::Order::Middle)
        // To the left of the size/opacity column so neither covers the other.
        .anchor(Align2::RIGHT_BOTTOM, vec2(-(MARGIN + BUTTON + 12.0), -MARGIN))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            // The card: filled with the card colour and a border, like the app's popovers.
            let (card, _) = ui.allocate_exact_size(vec2(STRIP_W, STRIP_H), Sense::hover());
            ui.painter().rect_filled(card, t.radius_lg as u8, t.card);
            ui.painter().rect_stroke(card, t.radius_lg as u8, Stroke::new(1.0, t.card_border), egui::StrokeKind::Middle);
            ui.painter().text(pos2(card.left() + 12.0, card.top() + 22.0), Align2::LEFT_CENTER, tl!("Layers"), egui::FontId::proportional(13.0), t.text);
            // Add a layer (the panel's +, at touch size).
            let add_c = pos2(card.right() - 12.0 - BUTTON * 0.5, card.top() + 22.0);
            let add = Rect::from_center_size(add_c, Vec2::splat(BUTTON));
            let resp = ui.interact(add, ui.id().with("add"), Sense::click());
            ui.painter().circle_filled(add_c, BUTTON * 0.5 - 1.0, if resp.hovered() { t.hover } else { t.dock });
            ui.painter().circle_stroke(add_c, BUTTON * 0.5 - 1.0, Stroke::new(1.0, t.card_border));
            crate::icons::paint(ui, add, "plus", (BUTTON * 0.5).round(), if resp.hovered() { t.text } else { t.icon });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("New Layer")));
            // The panel's +: a click adds the layer directly (the menu path opens the New Layer
            // dialog instead).
            if resp.clicked()
                && let Err(e) = app.run("layer.new.layer", serde_json::json!({}))
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            // The rows, scrolled inside the card's list frame (group rows keep the tree order and
            // depth of the Layers panel, through the same row source).
            let list_rect = Rect::from_min_max(pos2(card.left() + 2.0, card.top() + 44.0), pos2(card.right() - 2.0, card.bottom() - 4.0));
            let mut list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
            list.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().id_salt("pc-simple-layers-list").auto_shrink([false, true]).show(&mut list, |list_ui| {
                for &(depth, l) in &rows {
                    strip_row(app, list_ui, ctx, &doc, depth, l, active);
                }
            });
        });
}

/// One strip row: the eye at the left, a group's disclosure triangle, the thumbnail, and the
/// name. Tapping the row (outside the eye and the triangle) selects the layer.
fn strip_row(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    doc: &photocraft_doc::Document,
    depth: usize,
    l: &photocraft_doc::Layer,
    active: Option<photocraft_doc::LayerId>,
) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    if active == Some(l.id) {
        ui.painter().rect_filled(rect, 0, t.accent_soft);
    }
    // The eye toggles visibility directly (Layer Properties' visible, one step).
    let eye = Rect::from_min_size(rect.min, vec2(EYE_CELL, ROW));
    let eye_resp = ui.interact(eye, ui.id().with(("eye", l.id.0)), Sense::click());
    crate::icons::paint(ui, eye, if l.visible { "eye" } else { "eye-off" }, 16.0, if l.visible { t.text_dim } else { t.text_faint });
    eye_resp
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {}", if l.visible { tl!("Hide") } else { tl!("Show") }, l.name)));
    let visible = !l.visible;
    if eye_resp.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, "layer.setProps", serde_json::json!({"layer": l.id.0, "visible": visible}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    // A group's disclosure triangle (the Layers panel's, so the behaviour is the same), then
    // the thumbnail and the name; the rest of the row selects the layer.
    let mut x = rect.left() + EYE_CELL + depth as f32 * 8.0;
    let mut actions: Vec<(String, serde_json::Value)> = Vec::new();
    if crate::layer_tree_ui::disclosure(ui, rect, &mut x, l, &mut actions) {
        for (id, p) in actions {
            let id = if id == "layer.setExpanded" {
                id
            } else {
                continue;
            };
            if let Err(e) = crate::menus::invoke(app, ctx, &id, p) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
    } else {
        x += DISC_CELL;
    }
    // The thumbnail (the layers panel's cached image), fitted in its 32 pt cell.
    let cell = Rect::from_min_size(pos2(x + 4.0, rect.top() + 4.0), Vec2::splat(32.0));
    let (tex, frame) = app.layer_thumb(ui.ctx(), doc, l);
    let (fw, fh) = (frame.width() as f32, frame.height() as f32);
    if fw > 0.0 && fh > 0.0 && cell.is_positive() {
        let scale = (cell.width() / fw).min(cell.height() / fh);
        let dst = Rect::from_center_size(cell.center(), vec2(fw * scale, fh * scale));
        ui.painter().image(tex, dst, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    // The name: ellipsised in what is left (thumbnails ~7 pt a glyph at this size).
    let name_x = cell.right() + 8.0;
    let width = (rect.right() - 10.0 - name_x).max(0.0);
    let mut name = l.name.clone();
    let max = (width / 7.0) as usize;
    if name.chars().count() > max {
        name = name.chars().take(max.saturating_sub(1)).collect::<String>() + "…";
    }
    let shown = name.clone();
    ui.painter().text(pos2(name_x, rect.center().y), Align2::LEFT_CENTER, name, egui::FontId::proportional(12.0), t.text);
    // Select: tap the row body (the eye and triangle interactions sit above it).
    let body = Rect::from_min_max(pos2(rect.left() + EYE_CELL + DISC_CELL, rect.top()), pos2(rect.right(), rect.bottom()));
    let sel = ui.interact(body, ui.id().with(("select", l.id.0)), Sense::click());
    sel.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {shown}", tl!("Select"))));
    if sel.clicked()
        && let Err(e) = crate::menus::invoke(app, ctx, "layer.select", serde_json::json!({"layer": l.id.0, "mode": "replace"}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2 as at2;
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

    /// Rects of the sliders (taller than wide by the travel plus the caption).
    fn sliders(ctx: &egui::Context) -> Vec<Rect> {
        ctx.viewport(|v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, w)| w.iter())
                .filter(|w| (w.rect.height() - SLIDER).abs() < 0.01 && (w.rect.width() - BUTTON).abs() < 0.01)
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

    /// The centre of the layers strip's `index`th row (from the top) and, with `eye`, the row's
    /// eye cell.
    fn strip_row_center(ctx: &egui::Context, index: usize, eye: bool) -> egui::Pos2 {
        let strip = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        let x = if eye { strip.left() + EYE_CELL * 0.5 } else { strip.left() + EYE_CELL + DISC_CELL + 40.0 };
        at2(x, strip.top() + 44.0 + index as f32 * ROW + ROW * 0.5)
    }

    fn tap(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, pos: egui::Pos2) {
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(app, ctx, time, vec![egui::Event::PointerMoved(pos), pointer(pos, true)]);
        frame(app, ctx, time + 0.05, vec![pointer(pos, false)]);
        frame(app, ctx, time + 0.1, vec![]);
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
        // The four Phase-0 buttons (mode switch, undo, redo, save), the rail's seven tools, the
        // layers strip's round add button, and the mode-switch Area's two own container widgets,
        // which share the button's size. Pinning the census keeps the shell's floating additions
        // visible egui-side changes land.
        let rects = buttons(&ctx);
        assert_eq!(rects.len(), 14, "11 round buttons plus the mode-switch Area's containers");
        for r in &rects {
            assert!(r.width() >= 44.0 && r.height() >= 44.0, "touch-sized: {r:?}");
            assert!(r.left() >= 0.0 && r.right() <= 1024.0 && r.top() >= 0.0 && r.bottom() <= 768.0, "on screen: {r:?}");
        }
        let lowest = rects.iter().copied().max_by(|a, b| a.bottom().total_cmp(&b.bottom())).unwrap();
        assert!(lowest.center().x < 512.0, "the mode switch is the lowest button, bottom-left: {lowest:?}");
        let topmost = rects.iter().copied().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        assert!(topmost.center().x > 512.0, "the undo/redo/save cluster sits right: {topmost:?}");
    }

    #[test]
    fn the_layers_strip_selects_and_adds_a_layer() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let id0 = app.session.active().unwrap().doc.layers.last().unwrap().id.0;
        app.run("layer.new.layer", json!({})).unwrap();
        frame(&mut app, &ctx, 0.2, vec![]);
        frame(&mut app, &ctx, 0.3, vec![]);
        // The new layer is on top and active: the strip's first row selects it back to the
        // Background below.
        assert_ne!(app.session.active().unwrap().active_layer.map(|l| l.0), Some(id0));
        tap(&mut app, &ctx, 1.0, strip_row_center(&ctx, 1, false));
        frame(&mut app, &ctx, 1.15, vec![]);
        assert_eq!(app.session.active().unwrap().active_layer.map(|l| l.0), Some(id0), "tapping a strip row selects the layer");
        // The round + above the rows adds another layer.
        let strip = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        tap(&mut app, &ctx, 1.2, at2(strip.right() - 12.0 - BUTTON * 0.5, strip.top() + 22.0));
        frame(&mut app, &ctx, 1.35, vec![]);
        assert_eq!(app.session.active().unwrap().doc.layers.len(), 3, "+ adds a layer");
    }

    #[test]
    fn the_layers_strip_toggles_visibility() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let id0 = app.session.active().unwrap().doc.layers.last().unwrap().id.0;
        let visible = app.session.active().unwrap().doc.layers.last().unwrap().visible;
        tap(&mut app, &ctx, 1.0, strip_row_center(&ctx, 0, true));
        frame(&mut app, &ctx, 1.15, vec![]);
        let l = app.session.active().unwrap().doc.layers.last().unwrap();
        assert_eq!(l.id.0, id0, "the strip lists the document's stack");
        assert_eq!(l.visible, !visible, "the row's eye toggles visibility");
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
        // The triangle cell right of the eye collapses it, leaving one row.
        let strip = ctx.memory(|m| m.area_rect(egui::Id::new("pc-simple-layers"))).unwrap();
        tap(&mut app, &ctx, 1.0, at2(strip.left() + EYE_CELL + 14.0, strip.top() + 44.0 + ROW * 0.5));
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
    fn the_size_slider_resizes_the_brush() {
        let (mut app, ctx) = app_with_doc();
        frame(&mut app, &ctx, 0.0, vec![]);
        frame(&mut app, &ctx, 0.1, vec![]);
        let rects = sliders(&ctx);
        assert_eq!(rects.len(), 2, "size and opacity for the Brush");
        let size = rects.iter().copied().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        let before = app.session.tools.brush.size;
        // Drag the knob from the low end to the top: the size opens up (square curve, so the
        // full travel reaches the maximum).
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let from = pos2(size.center().x, size.bottom() - 12.0);
        frame(&mut app, &ctx, 2.0, vec![egui::Event::PointerMoved(from), pointer(from, true)]);
        for (dt, dy) in [(0.05, -30.0), (0.1, -70.0), (0.15, -120.0)] {
            let at = pos2(from.x, size.bottom() - 12.0 + dy);
            frame(&mut app, &ctx, 2.0 + dt, vec![egui::Event::PointerMoved(at)]);
        }
        frame(&mut app, &ctx, 2.2, vec![pointer(size.center(), false)]);
        frame(&mut app, &ctx, 2.25, vec![]);
        let after = app.session.tools.brush.size;
        assert!(after > before * 10.0, "dragging the size slider opens the brush up: {before} -> {after}");
        // The opacity slider keeps the opacity within range and touching it moves it too.
        let opacity_before = app.session.tools.brush.opacity;
        let at = pos2(size.center().x, rects.iter().copied().max_by(|a, b| a.top().total_cmp(&b.top())).unwrap().bottom() - 8.0);
        frame(&mut app, &ctx, 3.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
        let mid = pos2(at.x, at.y - 70.0);
        frame(&mut app, &ctx, 3.05, vec![egui::Event::PointerMoved(mid)]);
        frame(&mut app, &ctx, 3.1, vec![pointer(mid, false)]);
        frame(&mut app, &ctx, 3.15, vec![]);
        assert_ne!(app.session.tools.brush.opacity, opacity_before, "dragging the opacity slider changes the opacity");
        assert!((0.0..=1.0).contains(&app.session.tools.brush.opacity));
    }
}
