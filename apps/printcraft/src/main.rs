//! PrintCraft desktop app.
//!
//! Usage: `printcraft [options] [files…]`
//!
//! View options (applied after the files open; also the seed of the UI control channel):
//! `--page N  --zoom 150  --layout continuous|two-up|single  --panel comments|bookmarks|pages|fields|layers|attachments|none
//!  --theme light|dark  --mode all|read|edit|convert|sign  --tool <catalogue id>  --left open|closed
//!  --organize on  --fields on  --dialog properties|shortcuts|about  --palette <query>  --home on`
//!
//! `--control <file>` enables the UI control channel (off by default): the app listens on a random
//! loopback port and writes `{"port", "token", "pid"}` to `<file>` (owner-only permissions).
//! Agents then drive it with `printcraft-cli ui --control <file> <method> …`.

// Release builds on Windows are GUI-subsystem programs, so launching the app doesn't open a console
// window next to it (#57). `--version` and diagnostics then go nowhere when started from a terminal
// (std ignores the missing console handles, so nothing fails); debug builds keep the console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use printcraft_ui_egui::window_state::{Startup, WindowState};
use printcraft_ui_egui::{APP_NAME, PrintCraftApp};

#[cfg(target_os = "macos")]
mod apple_events;
mod updates;

/// Freedesktop app id: the `.desktop` file name and the hicolor icon name.
const APP_ID: &str = "ai.storyteller.printcraft";

/// The app icon (assets/app-icon/README.md). macOS gets the version on Apple's icon grid, with a
/// transparent margin; Windows and Linux get the full-bleed tile.
#[cfg(target_os = "macos")]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/printcraft-1024.png");
#[cfg(not(target_os = "macos"))]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.printcraft.png");

fn main() -> eframe::Result {
    // Last-resort guard (AGENTS.md §4): commands, edits, opens and saves catch panics and report
    // them; this hook logs every panic, caught or not, with a backtrace when RUST_BACKTRACE is set.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("printcraft: internal error: {info}");
        let trace = std::backtrace::Backtrace::capture();
        if trace.status() == std::backtrace::BacktraceStatus::Captured {
            eprintln!("{trace}");
        }
    }));
    let mut files = Vec::new();
    let mut options: Vec<(String, String)> = Vec::new();
    let mut control_file: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" => {
                println!("pedeefe {}", printcraft_ui_egui::updates::APP_VERSION);
                return Ok(());
            }
            "--control" => control_file = args.next(),
            flag if flag.starts_with("--") => {
                let value = args.next().unwrap_or_default();
                options.push((flag.trim_start_matches("--").to_string(), value));
            }
            _ => files.push(a),
        }
    }
    let integrated = cfg!(target_os = "macos");
    // eframe would otherwise derive the settings folder from the app id: keep it under the app's
    // name, apart from an installed PrintCraft's.
    let persistence_path = eframe::storage_dir(APP_NAME).map(|d| d.join("app.ron"));
    // How the window was left, read before it exists so that it opens once, in place
    // (window_state.rs explains the steps).
    let startup = persistence_path.as_deref().and_then(WindowState::read_saved).unwrap_or_default().startup();
    let mut viewport = startup
        .builder(egui::ViewportBuilder::default())
        .with_title(APP_NAME)
        .with_min_inner_size([820.0, 520.0])
        .with_drag_and_drop(true)
        // Wayland app id: matches packaging/linux/ai.storyteller.printcraft.desktop.
        .with_app_id(APP_ID);
    // Dock, taskbar, Alt-Tab and launcher icon when running unbundled.
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => eprintln!("printcraft: app icon: {e}"),
    }
    if integrated {
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    }
    let mut native = eframe::NativeOptions { viewport, persistence_path, ..Default::default() };
    configure_window(&mut native, startup);
    configure_gpu(&mut native);
    // Finder, Open With and the Dock deliver files as Apple events, not arguments; catch the one
    // that launched us as well as later ones. Lives until the event loop returns.
    #[cfg(target_os = "macos")]
    let apple_events = apple_events::AppleEvents::install();
    #[cfg(target_os = "macos")]
    let apple_events = &apple_events;
    eframe::run_native(
        APP_NAME,
        native,
        Box::new(move |cc| {
            let mut app = PrintCraftApp::new();
            if let Some(json) = cc.storage.and_then(|s| s.get_string("printcraft")) {
                app.restore(&json);
            }
            // Maximized last time (or the first start): maximized once its first frame is shown.
            app.restore_window();
            app.integrated_titlebar = integrated;
            app.update_source = Some(std::sync::Arc::new(updates::latest_release));
            app.keychain_ids = cfg!(target_os = "macos");
            #[cfg(target_os = "macos")]
            {
                app.os_events = Some(apple_events.connect(&cc.egui_ctx));
            }
            if let Some(file) = &control_file {
                let client = app.attach_control(&cc.egui_ctx);
                match printcraft_ui_egui::control::serve(client).and_then(|ep| write_control_file(file, ep.port, &ep.token).map(|()| ep.port)) {
                    Ok(port) => eprintln!("printcraft: UI control channel on 127.0.0.1:{port} (connection details in {file})"),
                    Err(e) => eprintln!("printcraft: --control {file}: {e}"),
                }
            }
            // Autosave unsaved changes; offer to recover documents a crashed session left behind.
            if let Some(dir) = printcraft_ui_egui::RecoveryStore::default_dir() {
                app.enable_recovery(printcraft_ui_egui::RecoveryStore::new(dir));
            }
            for f in files {
                app.open_path(&f);
            }
            for (k, v) in options {
                if let Err(e) = app.set_option(&k, &v) {
                    eprintln!("printcraft: --{k} {v}: {e}");
                }
            }
            // Fonts and theme now, so the first frame (the one the window appears with) is the
            // interface rather than an empty window.
            app.prepare(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

/// Write the control endpoint so that only the current user can read the token.
fn write_control_file(path: &str, port: u16, token: &str) -> std::io::Result<()> {
    let json = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() }).to_string();
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    let mut f = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(json.as_bytes())
}

/// Where and how the window opens (window_state.rs explains the steps). eframe keeps the window's
/// geometry in the settings and applies it, kept on a connected screen, while it creates the hidden
/// window; the hook then makes sure the window is never created maximized (that flashes on Windows)
/// and puts a maximized window's area back exactly. Only the first start is centred.
fn configure_window(native: &mut eframe::NativeOptions, startup: Startup) {
    native.persist_window = true;
    native.centered = startup.centered;
    native.window_builder = Some(Box::new(move |b| startup.adjust(b)));
}

/// How wgpu finds a GPU. Each choice yields to its wgpu environment variable.
///
/// - Draw on the integrated GPU unless `WGPU_POWER_PREF` says otherwise. A PDF viewer has no use
///   for a discrete GPU, and on hybrid-graphics laptops (NVIDIA Optimus) the discrete one can lose
///   or corrupt its memory across suspend and screen lock, leaving the window illegible (issue #8).
///   It also saves battery. Machines with one GPU are unaffected.
/// - On Windows, use Direct3D 12, falling back to OpenGL, and never load Vulkan drivers unless
///   `WGPU_BACKEND` asks for them. Creating a Vulkan instance loads every installed Vulkan driver
///   into the process, and a faulty one (an Intel driver in issue #37) crashed PrintCraft before
///   its window appeared. D3D12 is the native, best-supported backend there.
fn configure_gpu(native: &mut eframe::NativeOptions) {
    let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native.wgpu_options.wgpu_setup else { return };
    if std::env::var_os("WGPU_POWER_PREF").is_none() {
        setup.power_preference = eframe::wgpu::PowerPreference::LowPower;
    }
    if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
}

#[cfg(test)]
mod tests {
    use printcraft_ui_egui::window_state::WindowState;

    #[test]
    fn the_window_is_never_created_maximized_and_only_the_first_start_is_centred() {
        let saved = WindowState {
            size: Some([1100.0, 700.0]),
            pos: Some([100.0, 80.0]),
            maximized: true,
            max_pos: Some([-8.0, -8.0]),
            max_size: Some([1920.0, 1009.0]),
        };
        let mut native = eframe::NativeOptions::default();
        super::configure_window(&mut native, saved.startup());
        assert!(native.persist_window && !native.centered);
        let hook = native.window_builder.take().expect("hook installed");
        // What eframe hands over after applying the maximized window it saved, nudged on screen.
        let saved_by_eframe = egui::ViewportBuilder::default().with_position([0.0, 0.0]).with_inner_size([1920.0, 1009.0]).with_maximized(true);
        let b = hook(saved_by_eframe);
        assert_eq!((b.maximized, b.fullscreen, b.position), (Some(false), Some(false), Some(egui::pos2(-8.0, -8.0))));

        let mut first = eframe::NativeOptions::default();
        super::configure_window(&mut first, WindowState::default().startup());
        assert!(first.centered, "the first start opens centred, then maximized");
    }

    #[test]
    fn gpu_backends_avoid_vulkan_on_windows_and_prefer_low_power() {
        let mut native = eframe::NativeOptions::default();
        super::configure_gpu(&mut native);
        let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &native.wgpu_options.wgpu_setup else {
            panic!("default setup creates its own instance")
        };
        if std::env::var_os("WGPU_POWER_PREF").is_none() {
            assert_eq!(setup.power_preference, eframe::wgpu::PowerPreference::LowPower);
        }
        if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
            let backends = setup.instance_descriptor.backends;
            assert!(backends.contains(eframe::wgpu::Backends::DX12), "{backends:?}");
            assert!(!backends.contains(eframe::wgpu::Backends::VULKAN), "issue #37: {backends:?}");
        }
    }
}
