//! The phone layout's dock (`panels::phone_dock`): only the icon rail takes the window's width,
//! and a group opens as a sheet over the canvas.

use egui::{Rect, vec2};
use egui_kittest::Harness;

use crate::PhotocraftApp;
use crate::dock::Group;

/// The dock and the canvas on a phone held upright; the frame's central panel rect is kept.
fn harness() -> Harness<'static, (PhotocraftApp, Rect)> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.compact_layout = true;
    let mut h = Harness::builder().with_size(vec2(400.0, 760.0)).build_ui_state(
        |ui, (app, central): &mut (PhotocraftApp, Rect)| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::panels::right_dock(app, ui);
            *central = egui::CentralPanel::default().show(ui, |ui| ui.max_rect()).inner;
        },
        (app, Rect::NOTHING),
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(3);
    h
}

#[test]
fn the_canvas_keeps_the_width_and_a_group_opens_over_it() {
    let mut h = harness();
    let central = h.state().1;
    // The window less the rail (44) and the central panel's own margins (16 a side).
    assert!(central.width() >= 400.0 - 44.0 - 32.0 - 1.0, "only the rail beside the canvas: {central:?}");
    assert!(crate::dock::last_rects(&h.ctx).is_empty(), "no group drawn until one is opened");
    h.state_mut().0.phone_panel = Some(Group::Layers);
    h.run_steps(3);
    assert_eq!(h.state().1, central, "an open group doesn't squeeze the canvas");
    let rects = crate::dock::last_rects(&h.ctx);
    let screen = h.ctx.content_rect();
    assert!(matches!(rects.as_slice(), [(Group::Layers, r)] if screen.contains_rect(*r) && r.intersects(central)), "Layers over the canvas: {rects:?}");
    // Closing it from its tab strip puts the sheet away.
    *Group::Layers.shown_mut(&mut h.state_mut().0.ui.panels) = false;
    h.run_steps(2);
    assert_eq!(h.state().0.phone_panel, None);
}
