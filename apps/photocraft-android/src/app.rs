//! The Android entry point: GameActivity starts `android_main` on its own thread once the
//! library is loaded.

use crate::bridge::Bridge;
use photocraft_engine::Session;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::stroke_trace::Summary;
use photocraft_ui_egui::stylus::PenSample;
use photocraft_ui_egui::theme::ThemeKind;
use std::sync::Arc;
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::activity::input::{ImeOptions, InputType, TextInputAction, TextInputState, TextSpan};

/// android-activity looks the entry point up by name, hence `no_mangle` (the crate's only
/// `unsafe`; see `Cargo.toml`).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("PhotoCraft"));
    // Panics end up in logcat rather than vanishing with the process.
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    if let Err(e) = run(app) {
        log::error!("PhotoCraft stopped: {e}");
    }
}

fn run(android: AndroidApp) -> Result<(), String> {
    let files_dir = android.internal_data_path().unwrap_or_else(crate::services::fallback_files_dir);
    log::info!("PhotoCraft {} starting; data in {}", photocraft_engine::build_info::long_version(), files_dir.display());
    let bridge = Arc::new(Bridge::new(&files_dir).map_err(|e| format!("couldn't create the file bridge: {e}"))?);

    let mut options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        // eframe hands it to winit's event loop (`with_android_app`).
        android_app: Some(android.clone()),
        ..Default::default()
    };
    // The adapter's real texture limits, so big documents stay on the GPU; Vulkan first, GLES
    // on devices without a usable Vulkan driver.
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut options.wgpu_options.wgpu_setup);
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        create.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL;
    }
    let present = present_settings(&mut options, &android);

    eframe::run_native(
        "PhotoCraft",
        options,
        Box::new(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::Pro);
            let inbox: photocraft_ui_egui::Inbox = Arc::default();
            // Fonts imported with Type › Import Font… load with the system's on every start.
            photocraft_text::fonts::add_font_dir(crate::fonts::fonts_dir(&files_dir));
            let mut services = crate::services::services(bridge.clone(), &files_dir);
            services.inbox = Some(inbox.clone());
            // The Shortcut Pad (Window › Shortcut Pad) and updates (Help › Check for Updates…):
            // their menu rows go to the app's menus.
            let pad = crate::pad::Pad::new(&files_dir);
            let touch = crate::touch::TouchInput::new(&files_dir);
            let layout = crate::layout::Layout::new(&files_dir);
            let font_outcome = crate::fonts::Outcome::default();
            let wake = cc.egui_ctx.clone();
            let fonts_menu = crate::fonts::menu(&files_dir, bridge.clone(), font_outcome.clone(), move || wake.request_repaint());
            services.platform_menu =
                Some(crate::updates::combine(vec![layout.menu(), pad.menu(), touch.menu(), fonts_menu, crate::updates::menu(&files_dir, bridge.clone())]));
            crate::services::watch_incoming(bridge.clone(), inbox, cc.egui_ctx.clone());
            let mut app = PhotocraftApp::new(Session::new(), services);
            app.background_jobs = true;
            app.set_theme(&cc.egui_ctx, ThemeKind::Pro);
            // The pen's pressure comes from the Activity; a finger's touch "force" is its
            // contact size, which would paint near-invisible strokes.
            app.stylus.touch_force = false;
            enable_stroke_trace(&mut app, &android);
            app.perf.gpu_info.present = present;
            let feed = app.stylus.feed.clone();
            let ctx = cc.egui_ctx.clone();
            let first = std::sync::atomic::AtomicBool::new(true);
            let installed = photocraft_tablet::android::install(move |s| {
                // Once per run, so logcat shows the pen path is live and what the digitizer reports.
                if let Some(s) = s
                    && first.swap(false, std::sync::atomic::Ordering::Relaxed)
                {
                    log::info!("first pen sample: {s:?}");
                }
                feed.set(s.map(|s| PenSample { pressure: s.pressure, tilt_x: s.tilt_x, tilt_y: s.tilt_y, rotation: s.rotation, eraser: s.eraser }));
                ctx.request_repaint();
            });
            if let Err(e) = installed {
                log::warn!("{e}");
            }
            if let Some(rs) = cc.wgpu_render_state.clone() {
                let info = rs.adapter.get_info();
                log::info!("wgpu adapter: {} ({:?})", info.name, info.backend);
                app.perf.gpu_info.set_adapter(&info);
                if info.device_type != eframe::wgpu::DeviceType::Cpu {
                    app.set_wgpu(rs);
                }
            }
            Ok(Box::new(Shell {
                app,
                android,
                bridge: bridge.clone(),
                ime: String::new(),
                editing: false,
                soft_keyboard: false,
                keys: Default::default(),
                pad,
                font_outcome,
                touch,
                layout,
                pen_path: Vec::new(),
                drawn: Default::default(),
                trace_logged: None,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

/// The stroke latency trace (`docs/development.md`), on when a file named `stroke-trace` is in
/// the app's external data directory (`/sdcard/Android/data/<package>/files`, which `adb` can
/// reach): every frame of each stroke is appended to `stroke-trace.csv` beside it and each
/// stroke's summary goes to logcat (tag `PhotoCraft`). There are no environment variables to set
/// on Android.
fn enable_stroke_trace(app: &mut PhotocraftApp, android: &AndroidApp) {
    let Some(dir) = android.external_data_path() else { return };
    if dir.join("stroke-trace").exists() {
        let csv = dir.join("stroke-trace.csv");
        log::info!("stroke trace on: frames go to {}", csv.display());
        app.stroke_trace.set(&csv.to_string_lossy());
    }
}

/// Latency experiments: the `present` file beside the `stroke-trace` flag sets the swap chain's
/// frame latency and vsync (see [`crate::present`]). Returns how frames are presented, for the
/// trace.
fn present_settings(options: &mut eframe::NativeOptions, android: &AndroidApp) -> String {
    let surface = &mut options.wgpu_options.surface;
    let text = android.external_data_path().and_then(|dir| std::fs::read_to_string(dir.join("present")).ok());
    let (present, problems) = crate::present::parse(text.as_deref().unwrap_or_default());
    for p in problems {
        log::warn!("{p}");
    }
    if let Some(vsync) = present.vsync {
        surface.present_mode = if vsync { eframe::wgpu::PresentMode::Fifo } else { eframe::wgpu::PresentMode::AutoNoVsync };
    }
    surface.desired_maximum_frame_latency = present.latency.or(surface.desired_maximum_frame_latency);
    let latency = surface.desired_maximum_frame_latency.map_or("default".to_string(), |n| n.to_string());
    let description = format!("{:?}, latency {latency}", surface.present_mode);
    log::info!("presentation: {description}");
    description
}

/// A named section on Perfetto's timeline, when a trace is running (`adb shell perfetto`).
fn section(name: &str) -> Option<ndk::trace::Section> {
    ndk::trace::is_trace_enabled().then(|| ndk::trace::Section::new(name).ok()).flatten()
}

/// The app plus what the Android shell adds each frame: on-screen keyboard text (see
/// [`crate::ime`]) and hardware keyboard modifiers ([`crate::keys`]).
struct Shell {
    app: PhotocraftApp,
    android: AndroidApp,
    bridge: Arc<Bridge>,
    /// The keyboard's text as last seen.
    ime: String,
    /// A text field had focus last frame.
    editing: bool,
    /// The Shortcut Pad's keyboard key opened the on-screen keyboard (it stays open, typing
    /// shortcuts, until the key is tapped again).
    soft_keyboard: bool,
    /// Hardware keyboard modifiers (see [`crate::keys`]).
    keys: crate::keys::Held,
    /// The Shortcut Pad (see [`crate::pad`]).
    pad: crate::pad::Pad,
    /// The last Type › Import Font… result, for the status bar.
    font_outcome: crate::fonts::Outcome,
    /// Touch gestures and their settings (see [`crate::touch`]).
    touch: crate::touch::TouchInput,
    /// Pen sample positions not yet put into a frame (see [`crate::pen_path`]).
    pen_path: Vec<(f32, f32)>,
    /// Phone or tablet layout (see [`crate::layout`]).
    layout: crate::layout::Layout,
    /// Where the last frame drew everything (see [`crate::touch::Snapshot`]).
    drawn: crate::touch::Snapshot,
    /// The last stroke trace summary written to logcat.
    trace_logged: Option<Summary>,
}

impl Shell {
    /// Set the keyboard up for a text field or for shortcuts (`flags` on top of plain text) and
    /// empty its buffer.
    fn reset_keyboard(&mut self, flags: InputType) {
        self.android.set_ime_editor_info(InputType::TYPE_CLASS_TEXT | flags, TextInputAction::None, ImeOptions::IME_FLAG_NO_FULLSCREEN);
        self.android.set_text_input_state(TextInputState { text: String::new(), selection: TextSpan { start: 0, end: 0 }, compose_region: None });
        self.ime.clear();
    }
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let _section = section("photocraft:logic");
        // Phone or tablet, decided again every frame (rotation, a foldable opening, split screen).
        let phone = self.layout.is_phone(ctx);
        self.app.compact_layout = phone;
        self.pad.compact = phone;
        self.pad.run_pending(&mut self.app, ctx);
        if let Some((message, error)) = self.font_outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            self.app.ui.status = message;
            self.app.ui.status_error = error;
        }
        self.app.logic(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // The on-screen keyboard takes the bottom of the window: the app lays out in what's left
        // above it (panels, menus and dialogs alike), as apps that resize for the keyboard do.
        let ime = photocraft_tablet::android::ime_height_px() / ctx.pixels_per_point();
        if ime > 1.0
            && let Some(r) = &mut raw_input.screen_rect
        {
            r.max.y = (r.max.y - ime).max(r.min.y + r.height() * 0.3);
        }
        if self.pad.take_keyboard_tap() {
            self.soft_keyboard = !self.soft_keyboard;
            if self.soft_keyboard {
                self.bridge.take_keyboard_hidden();
                if !self.editing {
                    self.reset_keyboard(InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS | InputType::TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
                }
                // Explicitly (not "implicit"), so Android shows it with no text view focused.
                self.android.show_soft_input(false);
            } else {
                self.android.hide_soft_input(false);
            }
            self.pad.keyboard_on = self.soft_keyboard;
        }
        if self.soft_keyboard && self.bridge.take_keyboard_hidden() {
            // Put away with its own hide key or Back (it covers the pad's key while open).
            self.soft_keyboard = false;
            self.pad.keyboard_on = false;
        }
        // A text field, or the Type tool editing text on the canvas (no egui field has focus
        // then, but its typing comes the same way: text and Backspace/Enter events).
        let canvas_text = self.app.ui.text_edit.is_some();
        let editing = ctx.egui_wants_keyboard_input() || canvas_text;
        if editing && !self.editing {
            // A field just gained focus. Multi-line, so the keyboard's Enter key types a newline
            // into the text state (an editor action would be dropped by winit), which
            // `ime::diff` turns into Enter.
            self.reset_keyboard(InputType::TYPE_TEXT_FLAG_MULTI_LINE);
            if canvas_text {
                // winit opens the keyboard only for egui's own fields.
                self.android.show_soft_input(false);
            }
        }
        if !editing && self.editing {
            if self.soft_keyboard {
                // Back to typing shortcuts.
                self.reset_keyboard(InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS | InputType::TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
            } else {
                // Focus left the field (Enter, a click elsewhere): put the keyboard away, which
                // winit doesn't, so it stops covering the dialog's buttons.
                self.android.hide_soft_input(false);
            }
        }
        self.editing = editing;
        if editing || self.soft_keyboard {
            let now = self.android.text_input_state().text;
            if now != self.ime {
                let edit = crate::ime::diff(&self.ime, &now);
                if editing {
                    raw_input.events.extend(crate::ime::events(&edit));
                    self.ime = now;
                } else {
                    // No field to type into: the letters are shortcuts. Each is handled once,
                    // from an empty buffer again (no suggestions, so nothing is composing).
                    raw_input.events.extend(crate::ime::key_events(&edit));
                    self.reset_keyboard(InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS | InputType::TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
                }
            }
            // GameActivity's text events don't wake the UI (winit drops them): poll while typing.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        // The pad's touches become key presses (before the modifiers are worked out from them)
        // and wheel commands; the rest of the input goes on to the app.
        let config = self.touch.config();
        // The pen's side buttons and eraser end (Touch Input settings): erasing is the pen
        // hook's, right-clicks while touching are the router's, presses are handled here.
        use crate::touch::config::{PenAction, PenControl};
        use photocraft_tablet::{android, motion};
        android::set_erasing_controls(config.erasing_controls());
        let presses = android::take_presses();
        for p in &presses {
            let setting = config.pen(p.control);
            if setting.action != PenAction::Shortcut {
                continue;
            }
            match crate::pad::config::parse(&setting.shortcut) {
                Ok(chord) if !chord.is_empty() => {
                    raw_input.events.extend(crate::pad::press(chord));
                    raw_input.events.extend(crate::pad::release(chord));
                }
                Ok(_) => {}
                Err(e) => log::warn!("pen {:?}: {e}", p.control),
            }
        }
        let secondary = |id: egui::TouchId| {
            let buttons = android::pointer_buttons(id.0);
            match android::pointer_tool(id.0) {
                Some(motion::tool::MOUSE) => buttons & motion::button::SECONDARY != 0,
                Some(t) => {
                    let used = motion::controls(t, buttons);
                    PenControl::ALL.iter().any(|c| used[c.index()] && config.pen(*c).action == PenAction::RightClick)
                }
                None => false,
            }
        };
        let env = crate::pad::router::Env {
            // Gestures move the canvas only while a document is open.
            canvas: self.app.session.active_index().map(|_| self.app.last_canvas_rect),
            is_finger: &|id: egui::TouchId| photocraft_tablet::android::is_finger(id.0),
            pen_near: photocraft_tablet::android::pen_seen_within(std::time::Duration::from_millis(300)),
            config: &config,
            list_item: &|p: egui::Pos2| self.drawn.list_item(p),
            secondary: &secondary,
            scroll_strip: photocraft_ui_egui::panels::options_bar_overflow(ctx),
            now: raw_input.time.unwrap_or_else(|| ctx.input(|i| i.time)),
        };
        // Every pen sample's position, not just each event's latest (see `pen_path`). Samples
        // wait for a frame with the pen's touch in it; a new touch-down drops older ones.
        let is_pen = |id: egui::TouchId| matches!(android::pointer_tool(id.0), Some(motion::tool::STYLUS | motion::tool::ERASER));
        if raw_input.events.iter().any(|e| matches!(e, egui::Event::Touch { id, phase: egui::TouchPhase::Start, .. } if is_pen(*id))) {
            self.pen_path.clear();
        }
        self.pen_path.extend(android::take_pen_path());
        if self.pen_path.len() > 480 {
            let extra = self.pen_path.len() - 480;
            self.pen_path.drain(..extra);
        }
        if crate::pen_path::splice(&mut raw_input.events, &self.pen_path, ctx.pixels_per_point(), is_pen) > 0 {
            self.pen_path.clear();
        }
        let drawn = &self.drawn;
        self.pad.input(ctx, raw_input, &env, |p| drawn.covered(p));
        // A side button pressed while the pen hovers: a right-click under the pen (after the
        // router, which replaces pointer events with its own).
        let ppp = ctx.pixels_per_point();
        for p in presses {
            if let (Some((x, y)), PenAction::RightClick, PenControl::Lower | PenControl::Upper | PenControl::Top) =
                (p.hover, config.pen(p.control).action, p.control)
            {
                let pos = egui::pos2(x / ppp, y / ppp);
                raw_input.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    raw_input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
        }
        self.keys.apply(raw_input);
        self.app.raw_input_hook(ctx, raw_input);
        // After the app's hook, which starts the trace's frame.
        if let Some(age) = android::take_input_age() {
            self.app.stroke_trace.input_age(age.newest_ms);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let _section = section("photocraft:ui");
        let tokens = photocraft_ui_egui::theme::Tokens::for_kind(self.app.ui.theme);
        // The pad first, so the app lays out beside it.
        self.pad.show(ui, &tokens, &self.app);
        let section = section("photocraft:app");
        self.app.ui(ui, frame);
        drop(section);
        if self.app.stroke_trace.enabled() && self.app.stroke_trace.last != self.trace_logged {
            self.trace_logged = self.app.stroke_trace.last.clone();
            if let Some(summary) = &self.trace_logged {
                log::info!("{}", summary.line());
            }
        }
        self.pad.settings(ui.ctx(), &tokens);
        self.touch.settings(ui.ctx(), &tokens, photocraft_tablet::android::controls_held(), photocraft_tablet::android::last_buttons());
        // What was drawn where, for the next frame's touches (see `touch::Snapshot`).
        self.drawn = crate::touch::Snapshot::take(ui.ctx());
    }
}
