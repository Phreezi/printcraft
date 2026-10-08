//! PdfCraft desktop app.
//!
//! Usage: `pdfcraft [options] [files…]`
//! `--create-images [images…]` stages the images in one PDF and asks for the page DPI.
//!
//! View options (applied after the files open; also the seed of the UI control channel):
//! `--page N  --zoom 150  --layout continuous|two-up|single  --panel comments|bookmarks|pages|fields|layers|attachments|none
//!  --theme light|dark|system  --language auto|<code>  --mode all|read|edit|convert|sign  --tool <catalogue id>  --left open|closed
//!  --organize on  --fields on  --dialog properties|shortcuts|about  --palette <query>  --home on
//!  --cover on|off  --default-layout continuous|two-up|single  --default-zoom fit-width|fit-page|<percent>  --language-prompt show|hide`
//!
//! `--print FILE…` prints the files on the default printer and `--print-to PRINTER FILE…` on the
//! named one, with Quick Print's default settings and no window, then exits (the shell's print
//! and printto verbs, Outlook's Quick Print; `quick_print`). `--printer-driver` and
//! `--printer-port` (the printto verb's `%3` and `%4`) are accepted and ignored.
//!
//! `--new-instance` runs a separate app even when one is running. Otherwise a launch while the
//! app runs hands its files to it and exits (`single_instance`).
//!
//! `--control <file>` enables the UI control channel (off by default): the app listens on a random
//! loopback port and writes `{"port", "token", "pid"}` to `<file>` (owner-only permissions).
//! Agents then drive it with `pdfcraft-cli ui --control <file> <method> …`.

// Release builds on Windows are GUI-subsystem programs, so launching the app doesn't open a console
// window next to it (#57). `--version` and diagnostics then go nowhere when started from a terminal
// (std ignores the missing console handles, so nothing fails); debug builds keep the console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use pdfcraft_ui_egui::window_state::{LEGACY_STORAGE_KEY, STORAGE_KEY, Startup, WindowState};
use pdfcraft_ui_egui::{APP_NAME, PdfCraftApp};

#[cfg(target_os = "macos")]
mod apple_events;
mod logging;
mod quick_print;
mod single_instance;
mod updates;

/// Freedesktop app id: the `.desktop` file name and the hicolor icon name.
const APP_ID: &str = "ai.storyteller.pdfcraft";

/// The app icon (assets/app-icon/README.md). macOS gets the version on Apple's icon grid, with a
/// transparent margin; Windows and Linux get the full-bleed tile.
#[cfg(target_os = "macos")]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/pdfcraft-1024.png");
#[cfg(not(target_os = "macos"))]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.pdfcraft.png");

/// The settings folder: `app.ron` and the `logs` folder (docs/development.md). eframe would
/// otherwise derive it from the app id; keep it under the app's name, apart from an installed
/// PdfCraft's. PeDeeFe never moves or touches PrintCraft's or PdfCraft's folders, so it has no
/// legacy-folder migration.
fn settings_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir(APP_NAME)
}

fn main() -> eframe::Result {
    // First, so the panic hook and every start-up warning are recorded (`logging`).
    let logger = logging::install();
    // Last-resort guard (AGENTS.md §4): commands, edits, opens and saves catch panics and report
    // them; this hook logs every panic, caught or not, with a backtrace when RUST_BACKTRACE is set.
    std::panic::set_hook(Box::new(|info| {
        let trace = std::backtrace::Backtrace::capture();
        let report = if trace.status() == std::backtrace::BacktraceStatus::Captured {
            format!("internal error: {info}\n{trace}")
        } else {
            format!("internal error: {info}")
        };
        // Standard error and the log file; standard error alone when RUST_LOG turned errors off.
        if log::log_enabled!(log::Level::Error) {
            log::error!("{report}");
        } else {
            // `eprintln!` panics on a broken stderr pipe, and a panic inside the panic hook aborts.
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("pdfcraft: {report}\n"));
        }
    }));
    let cli = parse_args(std::env::args().skip(1));
    if cli.version {
        println!("pedeefe {}", pdfcraft_ui_egui::updates::APP_VERSION);
        return Ok(());
    }
    // The shell's print and printto verbs (Outlook's Quick Print): print, then exit, before any
    // window or single-instance hand-off (quick_print.rs).
    if let Some(request) = &cli.print {
        let code = quick_print::run(logger, settings_dir().as_deref(), request, &cli.files);
        std::process::exit(code);
    }
    let Cli { files, options, control_file, create_images, new_instance, .. } = cli;
    let integrated = cfg!(target_os = "macos");
    // A plain launch (files, or nothing) while the app runs: the running app takes the files and
    // comes to the front. Launches with options (`--control`, `--create-images`, view options)
    // and `--new-instance` run on their own.
    let plain = options.is_empty() && control_file.is_none() && !create_images && !new_instance;
    let instance = match settings_dir().filter(|_| plain) {
        None => None,
        Some(dir) => match single_instance::start(&dir, &single_instance::absolute_paths(&files)) {
            single_instance::Outcome::Forwarded => return Ok(()),
            single_instance::Outcome::Primary(server) => Some(server),
            single_instance::Outcome::Alone(why) => {
                log::warn!("running without single instance: {why}");
                None
            }
        },
    };
    let persistence_path = settings_dir().map(|d| d.join("app.ron"));
    // How the window was left, read before it exists so that it opens once, in place
    // (window_state.rs explains the steps).
    let startup = persistence_path.as_deref().and_then(WindowState::read_saved).unwrap_or_default().startup();
    // Every window: tabs torn off into new windows get the same (window_state.rs places only the first).
    let mut template = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_min_inner_size([820.0, 520.0])
        .with_drag_and_drop(true)
        // Wayland app id: matches packaging/linux/ai.storyteller.pdfcraft.desktop.
        .with_app_id(APP_ID);
    // Dock, taskbar, Alt-Tab and launcher icon when running unbundled.
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => template = template.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    if integrated {
        template = template.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    }
    let viewport = startup.builder(template.clone());
    // The log file lives in the settings folder; opened after the arguments, so `--version` leaves
    // no file behind. Records logged until now are written to it first.
    if let (Some(logger), Some(dir)) = (logger, settings_dir()) {
        match logger.attach_dir(&dir.join("logs")) {
            Ok(path) => log::info!("{APP_NAME} {}, log file {}", pdfcraft_ui_egui::updates::APP_VERSION, path.display()),
            // Standard error only by now (`attach_dir` gave up on the file); unlike `eprintln!`, never panics.
            Err(e) => log::warn!("no log file: {e}"),
        }
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
    // Requests from later launches; the server itself lives until the app ends.
    let later = instance.as_ref().map(|server| (server.waker(), server.events()));
    let result = eframe::run_native(
        APP_NAME,
        native,
        Box::new(move |cc| {
            let mut app = PdfCraftApp::new();
            if let Some(json) = cc.storage.and_then(|s| s.get_string(STORAGE_KEY).or_else(|| s.get_string(LEGACY_STORAGE_KEY))) {
                app.restore(&json);
            }
            // First start (or no language chosen yet): ask English or Portuguese. A
            // `--language` option below answers it instead.
            app.ask_language_if_unset();
            // Maximized last time (or the first start): maximized once its first frame is shown.
            app.restore_window();
            app.integrated_titlebar = integrated;
            app.update_source = Some(std::sync::Arc::new(updates::latest_release));
            app.keychain_ids = cfg!(target_os = "macos");
            app.window_template = template;
            // Files from Finder (macOS) and from later launches of the app.
            #[cfg(target_os = "macos")]
            let mut apple = apple_events.connect(&cc.egui_ctx);
            let mut later = later.map(|(wake, events)| {
                wake(&cc.egui_ctx);
                events
            });
            app.os_events = Some(Box::new(move || {
                #[cfg(target_os = "macos")]
                let mut events = apple();
                #[cfg(not(target_os = "macos"))]
                let mut events = Vec::new();
                if let Some(poll) = later.as_mut() {
                    events.extend(poll());
                }
                events
            }));
            if let Some(file) = &control_file {
                let client = app.attach_control(&cc.egui_ctx);
                match pdfcraft_ui_egui::control::serve(client).and_then(|ep| write_control_file(file, ep.port, &ep.token).map(|()| ep.port)) {
                    // Never the token (AGENTS.md §3): it stays in the owner-only file.
                    Ok(port) => log::info!("UI control channel on 127.0.0.1:{port} (connection details in {file})"),
                    Err(e) => log::error!("--control {file}: {e}"),
                }
            }
            // Autosave unsaved changes; offer to recover documents a crashed session left behind.
            if let Some(dir) = pdfcraft_ui_egui::RecoveryStore::default_dir() {
                app.enable_recovery(pdfcraft_ui_egui::RecoveryStore::new(dir));
            }
            if create_images {
                if let Err(e) = app.begin_image_import_paths(&files) {
                    app.notify(e);
                }
            } else {
                for f in files {
                    app.open_path(&f);
                }
            }
            for (k, v) in options {
                if let Err(e) = app.set_option(&k, &v) {
                    log::warn!("--{k} {v}: {e}");
                }
            }
            // Fonts and theme now, so the first frame (the one the window appears with) is the
            // interface rather than an empty window.
            app.prepare(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
    drop(instance);
    result
}

/// The command line.
#[derive(Debug, Default, PartialEq)]
struct Cli {
    /// `--version`: print the version and exit.
    version: bool,
    files: Vec<String>,
    /// View options (`--page 3`, …) for `PdfCraftApp::set_option`.
    options: Vec<(String, String)>,
    control_file: Option<String>,
    create_images: bool,
    new_instance: bool,
    /// `--print` / `--print-to PRINTER`: Quick Print the files (or the mistake to report).
    print: Option<quick_print::Request>,
}

/// Read the arguments (without the program name). Every value is accepted; Quick Print's
/// mistakes (no file, no printer name) are kept in [`Cli::print`] to be reported.
fn parse_args(args: impl IntoIterator<Item = String>) -> Cli {
    use pdfcraft_ui_egui::quick_print::Usage;
    let mut cli = Cli::default();
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" => {
                cli.version = true;
                return cli;
            }
            "--control" => cli.control_file = args.next(),
            "--create-images" => cli.create_images = true,
            "--new-instance" => cli.new_instance = true,
            "--print" => cli.print = Some(Ok(quick_print::Target::Default)),
            "--print-to" => {
                cli.print = Some(match args.next().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
                    Some(printer) => Ok(quick_print::Target::Printer(printer)),
                    None => Err(Usage::NoPrinter),
                })
            }
            // The printto verb's driver and port (`%3`, `%4`): Windows finds the printer by name.
            "--printer-driver" | "--printer-port" => {
                let _ = args.next();
            }
            flag if flag.starts_with("--") => {
                let value = args.next().unwrap_or_default();
                cli.options.push((flag.trim_start_matches("--").to_string(), value));
            }
            _ => cli.files.push(a),
        }
    }
    if matches!(cli.print, Some(Ok(_))) && cli.files.is_empty() {
        cli.print = Some(Err(Usage::NoFile));
    }
    cli
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
/// - On Linux, draw on a GPU that a monitor is plugged into. On a desktop whose monitors all hang
///   off the discrete GPU, drawing on the integrated one leaves the window black under Wayland
///   compositors on NVIDIA. Among the GPUs that drive a display, the integrated one still wins.
/// - On Windows, use Direct3D 12, falling back to OpenGL, and never load Vulkan drivers unless
///   `WGPU_BACKEND` asks for them. Creating a Vulkan instance loads every installed Vulkan driver
///   into the process, and a faulty one (an Intel driver in issue #37) crashed PdfCraft before
///   its window appeared. D3D12 is the native, best-supported backend there.
fn configure_gpu(native: &mut eframe::NativeOptions) {
    let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native.wgpu_options.wgpu_setup else { return };
    if std::env::var_os("WGPU_POWER_PREF").is_none() {
        setup.power_preference = eframe::wgpu::PowerPreference::LowPower;
        #[cfg(target_os = "linux")]
        {
            let displays = linux_display_gpus(std::path::Path::new("/sys/class/drm"));
            // Without sysfs (containers, remote sessions) the power preference alone decides.
            if !displays.is_empty() {
                setup.native_adapter_selector = Some(std::sync::Arc::new(move |adapters, surface| {
                    let usable: Vec<&eframe::wgpu::Adapter> = adapters.iter().filter(|a| surface.is_none_or(|s| a.is_surface_supported(s))).collect();
                    let infos: Vec<(u32, u32, eframe::wgpu::DeviceType)> = usable
                        .iter()
                        .map(|a| {
                            let info = a.get_info();
                            (info.vendor, info.device, info.device_type)
                        })
                        .collect();
                    pick_adapter(&infos, &displays)
                        .and_then(|i| usable.get(i))
                        .map(|a| (*a).clone())
                        .ok_or_else(|| "no GPU can draw to this window".to_string())
                }));
            }
        }
    }
    if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
}

/// PCI `(vendor, device)` ids of the GPUs with a connected monitor, read from the DRM connectors
/// under `drm` (`card1-DP-3/status` is `connected`, `card1/device/{vendor,device}` hold `0x10de`).
#[cfg(target_os = "linux")]
fn linux_display_gpus(drm: &std::path::Path) -> Vec<(u32, u32)> {
    let read_hex = |p: std::path::PathBuf| -> Option<u32> {
        let s = std::fs::read_to_string(p).ok()?;
        u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
    };
    let mut gpus = Vec::new();
    let Ok(entries) = std::fs::read_dir(drm) else { return gpus };
    // A machine has a handful of connectors; the cap only bounds a pathological sysfs.
    for entry in entries.flatten().take(256) {
        let name = entry.file_name();
        let Some((card, _connector)) = name.to_str().and_then(|n| n.split_once('-')) else { continue };
        let connected = std::fs::read_to_string(entry.path().join("status")).is_ok_and(|s| s.trim() == "connected");
        if !connected {
            continue;
        }
        let device = drm.join(card).join("device");
        if let (Some(v), Some(d)) = (read_hex(device.join("vendor")), read_hex(device.join("device")))
            && !gpus.contains(&(v, d))
        {
            gpus.push((v, d));
        }
    }
    gpus
}

/// Index of the adapter to draw with: one that drives a display (by PCI ids) first, then the most
/// frugal kind — integrated, discrete, other, virtual, software.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn pick_adapter(adapters: &[(u32, u32, eframe::wgpu::DeviceType)], displays: &[(u32, u32)]) -> Option<usize> {
    use eframe::wgpu::DeviceType;
    adapters
        .iter()
        .enumerate()
        .min_by_key(|(_, (vendor, device, kind))| {
            let drives_display = displays.contains(&(*vendor, *device));
            let frugality = match kind {
                DeviceType::IntegratedGpu => 0,
                DeviceType::DiscreteGpu => 1,
                DeviceType::Other => 2,
                DeviceType::VirtualGpu => 3,
                DeviceType::Cpu => 4,
            };
            (!drives_display, frugality)
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use pdfcraft_ui_egui::window_state::WindowState;

    fn args(list: &[&str]) -> super::Cli {
        super::parse_args(list.iter().map(|a| a.to_string()))
    }

    #[test]
    fn plain_launches_keep_their_files_and_view_options() {
        let cli = args(&["a.pdf", "--page", "3", "b.pdf", "--new-instance"]);
        assert_eq!(
            (cli.files, cli.options, cli.new_instance, cli.print),
            (vec!["a.pdf".into(), "b.pdf".into()], vec![("page".into(), "3".into())], true, None)
        );
        assert!(args(&["--version", "--print"]).version);
        let cli = args(&["--control", "ctl.json", "--create-images", "x.png"]);
        assert_eq!((cli.control_file.as_deref(), cli.create_images, cli.files), (Some("ctl.json"), true, vec!["x.png".to_string()]));
    }

    #[test]
    fn the_print_verbs_parse_to_quick_print_requests() {
        use super::quick_print::Target;
        use pdfcraft_ui_egui::quick_print::Usage;
        // The print verb: "pedeefe.exe" --print "%1".
        let cli = args(&["--print", r"C:\Users\me\AppData\Local\Temp\invoice.pdf"]);
        assert_eq!(cli.print, Some(Ok(Target::Default)));
        assert_eq!(cli.files, vec![r"C:\Users\me\AppData\Local\Temp\invoice.pdf".to_string()]);
        // The printto verb: "pedeefe.exe" --print-to "%2" "%1" --printer-driver "%3" --printer-port "%4".
        let cli = args(&["--print-to", "EPSON ET-16650 Series", "a b.pdf", "--printer-driver", "winspool", "--printer-port", "USB001"]);
        assert_eq!(cli.print, Some(Ok(Target::Printer("EPSON ET-16650 Series".into()))));
        assert_eq!(cli.files, vec!["a b.pdf".to_string()]);
        assert!(cli.options.is_empty(), "driver and port are not view options: {:?}", cli.options);
        // Callers that leave the driver and port empty.
        let cli = args(&["--print-to", "Office", "x.pdf", "--printer-driver", "", "--printer-port", ""]);
        assert_eq!((cli.print, cli.files.len()), (Some(Ok(Target::Printer("Office".into()))), 1));
        // Several files, and the file before the flag.
        let cli = args(&["one.pdf", "--print", "two.pdf"]);
        assert_eq!((cli.print, cli.files.len()), (Some(Ok(Target::Default)), 2));
        // Mistakes are kept, to be reported (never a crash, never the app opening instead).
        assert_eq!(args(&["--print"]).print, Some(Err(Usage::NoFile)));
        assert_eq!(args(&["--print-to", "Office"]).print, Some(Err(Usage::NoFile)));
        assert_eq!(args(&["--print-to"]).print, Some(Err(Usage::NoPrinter)));
        assert_eq!(args(&["--print-to", "  ", "a.pdf"]).print, Some(Err(Usage::NoPrinter)));
        // A printer name that looks like a flag is still the printer's.
        assert_eq!(args(&["--print-to", "--odd name", "a.pdf"]).print, Some(Ok(Target::Printer("--odd name".into()))));
    }

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

    const NVIDIA: (u32, u32) = (0x10de, 0x2684);
    const AMD_IGPU: (u32, u32) = (0x1002, 0x164e);

    fn adapters() -> Vec<(u32, u32, eframe::wgpu::DeviceType)> {
        use eframe::wgpu::DeviceType;
        vec![(NVIDIA.0, NVIDIA.1, DeviceType::DiscreteGpu), (AMD_IGPU.0, AMD_IGPU.1, DeviceType::IntegratedGpu), (0, 0, DeviceType::Cpu)]
    }

    #[test]
    fn pick_adapter_prefers_the_gpu_driving_the_monitors() {
        // A desktop whose monitors are all on the discrete GPU: the integrated one shows black.
        assert_eq!(super::pick_adapter(&adapters(), &[NVIDIA]), Some(0));
    }

    #[test]
    fn pick_adapter_keeps_the_integrated_gpu_on_hybrid_laptops() {
        // Issue #8: the panel is on the integrated GPU, an external monitor on the discrete one.
        assert_eq!(super::pick_adapter(&adapters(), &[NVIDIA, AMD_IGPU]), Some(1));
        assert_eq!(super::pick_adapter(&adapters(), &[AMD_IGPU]), Some(1));
    }

    #[test]
    fn pick_adapter_falls_back_to_low_power_without_a_match() {
        assert_eq!(super::pick_adapter(&adapters(), &[(0x8086, 0x1234)]), Some(1));
        assert_eq!(super::pick_adapter(&[], &[NVIDIA]), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_display_gpus_reads_connected_connectors() -> std::io::Result<()> {
        let dir = std::env::temp_dir().join(format!("pdfcraft-drm-{}", std::process::id()));
        let card = |name: &str, (vendor, device): (u32, u32)| -> std::io::Result<()> {
            std::fs::create_dir_all(dir.join(name).join("device"))?;
            std::fs::write(dir.join(name).join("device/vendor"), format!("{vendor:#06x}\n"))?;
            std::fs::write(dir.join(name).join("device/device"), format!("{device:#06x}\n"))
        };
        let connector = |name: &str, status: &str| -> std::io::Result<()> {
            std::fs::create_dir_all(dir.join(name))?;
            std::fs::write(dir.join(name).join("status"), format!("{status}\n"))
        };
        card("card1", NVIDIA)?;
        card("card2", AMD_IGPU)?;
        connector("card1-DP-3", "connected")?;
        connector("card1-DP-4", "connected")?;
        connector("card2-HDMI-A-1", "disconnected")?;
        connector("card2-Writeback-1", "unknown")?;
        let gpus = super::linux_display_gpus(&dir);
        std::fs::remove_dir_all(&dir)?;
        assert_eq!(gpus, vec![NVIDIA]);
        assert!(super::linux_display_gpus(std::path::Path::new("/nonexistent/drm")).is_empty());
        Ok(())
    }
}
