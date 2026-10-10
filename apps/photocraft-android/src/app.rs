//! The Android entry point: GameActivity starts `android_main` on its own thread once the
//! library is loaded.

use crate::bridge::Bridge;
use photocraft_engine::Session;
use photocraft_ui_egui::PhotocraftApp;
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

    eframe::run_native(
        "PhotoCraft",
        options,
        Box::new(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::Pro);
            let inbox: photocraft_ui_egui::Inbox = Arc::default();
            let mut services = crate::services::services(bridge.clone(), &files_dir);
            services.inbox = Some(inbox.clone());
            crate::services::watch_incoming(bridge.clone(), inbox, cc.egui_ctx.clone());
            let mut app = PhotocraftApp::new(Session::new(), services);
            app.background_jobs = true;
            app.set_theme(&cc.egui_ctx, ThemeKind::Pro);
            // The pen's pressure comes from the Activity; a finger's touch "force" is its
            // contact size, which would paint near-invisible strokes.
            app.stylus.touch_force = false;
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
            Ok(Box::new(Shell { app, android, ime: String::new(), editing: false }))
        }),
    )
    .map_err(|e| e.to_string())
}

/// The app plus what the Android shell adds each frame: on-screen keyboard text (see
/// [`crate::ime`]).
struct Shell {
    app: PhotocraftApp,
    android: AndroidApp,
    /// The keyboard's text as last seen.
    ime: String,
    /// A text field had focus last frame.
    editing: bool,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let editing = ctx.egui_wants_keyboard_input();
        if editing && !self.editing {
            // A field just gained focus. Multi-line, so the keyboard's Enter key types a newline
            // into the text state (an editor action would be dropped by winit), which
            // `ime::diff` turns into Enter.
            self.android.set_ime_editor_info(
                InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_FLAG_MULTI_LINE,
                TextInputAction::None,
                ImeOptions::IME_FLAG_NO_FULLSCREEN,
            );
            // Start the keyboard from an empty buffer.
            self.android.set_text_input_state(TextInputState { text: String::new(), selection: TextSpan { start: 0, end: 0 }, compose_region: None });
            self.ime.clear();
        }
        if !editing && self.editing {
            // Focus left the field (Enter, a click elsewhere): put the keyboard away, which
            // winit doesn't, so it stops covering the dialog's buttons.
            self.android.hide_soft_input(false);
        }
        self.editing = editing;
        if editing {
            let now = self.android.text_input_state().text;
            if now != self.ime {
                raw_input.events.extend(crate::ime::events(&crate::ime::diff(&self.ime, &now)));
                self.ime = now;
            }
            // GameActivity's text events don't wake the UI (winit drops them): poll while typing.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.app.raw_input_hook(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }
}
