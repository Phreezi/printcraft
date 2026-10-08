//! The app window is remembered between sessions (normal size and position, and maximized) and
//! comes back without flicker: its geometry is settled before it is created, it is never created
//! maximized, and the app doesn't move or resize it at start.

use egui::{ViewportBuilder, ViewportCommand, pos2, vec2};
use printcraft_ui_egui::PrintCraftApp;
use printcraft_ui_egui::window_state::{DEFAULT_SIZE, Startup, WindowState};

fn info(maximized: bool, inner: [f32; 4], outer: [f32; 2]) -> egui::ViewportInfo {
    egui::ViewportInfo {
        maximized: Some(maximized),
        minimized: Some(false),
        fullscreen: Some(false),
        inner_rect: Some(egui::Rect::from_min_size(egui::pos2(inner[0], inner[1]), egui::vec2(inner[2], inner[3]))),
        outer_rect: Some(egui::Rect::from_min_size(egui::pos2(outer[0], outer[1]), egui::vec2(inner[2] + 16.0, inner[3] + 40.0))),
        ..Default::default()
    }
}

const SAVED: WindowState = WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 100.0]), maximized: true, max_pos: Some([-8.0, -8.0]) };

#[test]
fn the_normal_size_survives_maximizing() {
    let mut s = WindowState::default();
    s.observe(&info(false, [108.0, 140.0, 1100.0, 700.0], [100.0, 100.0]));
    assert_eq!(s, WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 100.0]), ..WindowState::default() });
    // Maximized: the normal size and position are kept for un-maximizing, and where the maximized
    // window sits is noted (the next start creates the window there).
    s.observe(&info(true, [0.0, 31.0, 1920.0, 1009.0], [-8.0, -8.0]));
    assert_eq!(s, SAVED);
    // Minimized and full screen aren't remembered.
    s.observe(&egui::ViewportInfo { minimized: Some(true), maximized: Some(false), ..Default::default() });
    assert!(s.maximized);
}

#[test]
fn the_start_is_decided_before_the_window_exists() {
    // The first start opens centred at the default size, then maximized.
    let first = WindowState::default().startup();
    assert_eq!(first, Startup { size: DEFAULT_SIZE, centered: true, maximized: true, max_pos: None });
    assert_eq!(first.builder(ViewportBuilder::default()).inner_size, Some(vec2(1440.0, 920.0)));
    // Afterwards: as it was left, never re-centred.
    assert_eq!(SAVED.startup(), Startup { size: [1100.0, 700.0], centered: false, maximized: true, max_pos: Some([-8.0, -8.0]) });
    let normal = WindowState { maximized: false, ..SAVED }.startup();
    assert!(!normal.maximized && !normal.centered);
}

/// The builder eframe hands to the hook after applying the geometry it saved.
fn eframe_restored(pos: [f32; 2], size: [f32; 2], maximized: bool) -> ViewportBuilder {
    ViewportBuilder::default().with_position(pos).with_inner_size(size).with_fullscreen(false).with_maximized(maximized)
}

#[test]
fn the_window_is_never_created_maximized_and_a_maximized_one_starts_exactly_in_place() {
    let start = SAVED.startup();
    // eframe brought back the maximized window, nudged inside the screen by its border: it is
    // created at exactly the place it had, not maximized (maximized once shown).
    let b = start.adjust(eframe_restored([0.0, 0.0], [1920.0, 1009.0], true));
    assert_eq!((b.position, b.inner_size, b.maximized, b.fullscreen), (Some(pos2(-8.0, -8.0)), Some(vec2(1920.0, 1009.0)), Some(false), Some(false)));
    // Moved further, the screen changed (a monitor unplugged): eframe's place is kept.
    let b = start.adjust(eframe_restored([1200.0, 0.0], [1920.0, 1009.0], true));
    assert_eq!(b.position, Some(pos2(1200.0, 0.0)));
    assert_eq!(b.maximized, Some(false));
    // A normal window keeps eframe's geometry as it is.
    let normal = WindowState { maximized: false, ..SAVED }.startup();
    let b = normal.adjust(eframe_restored([300.0, 200.0], [1000.0, 650.0], false));
    assert_eq!((b.position, b.inner_size, b.maximized), (Some(pos2(300.0, 200.0)), Some(vec2(1000.0, 650.0)), Some(false)));
    // eframe recorded a maximized window but the app's own record says normal: the normal size,
    // placed by the system, rather than a full-screen window that isn't maximized.
    let b = normal.adjust(eframe_restored([0.0, 0.0], [1920.0, 1009.0], true));
    assert_eq!((b.position, b.inner_size), (None, Some(vec2(1100.0, 700.0))));
}

#[test]
fn a_window_eframe_recorded_minimized_or_full_screen_opens_at_the_normal_size() {
    let start = WindowState { maximized: false, ..SAVED }.startup();
    // Minimized at exit: eframe saw 0 × 0 far off screen (and clamped it to 64 × 64).
    let b = start.adjust(eframe_restored([0.0, 0.0], [64.0, 64.0], false));
    assert_eq!((b.position, b.inner_size, b.maximized), (None, Some(vec2(1100.0, 700.0)), Some(false)));
    // Full screen at exit.
    let b = start.adjust(ViewportBuilder::default().with_position([0.0, 0.0]).with_inner_size([1920.0, 1080.0]).with_fullscreen(true));
    assert_eq!((b.position, b.inner_size, b.fullscreen), (None, Some(vec2(1100.0, 700.0)), Some(false)));
    // Nothing saved by eframe: the size the app chose, never maximized at creation.
    let b = WindowState::default().startup().adjust(WindowState::default().startup().builder(ViewportBuilder::default()));
    assert_eq!((b.position, b.inner_size, b.maximized), (None, Some(vec2(1440.0, 920.0)), Some(false)));
}

#[test]
fn un_maximizing_gives_back_the_normal_size_and_a_position_on_that_screen() {
    // Created (and so un-maximized to) the full-screen area at (-8, -8).
    let created = Some(pos2(-8.0, -8.0));
    let restored = info(false, [0.0, 31.0, 1920.0, 1009.0], [-8.0, -8.0]);
    assert_eq!(
        SAVED.unmaximize_commands(&restored, created),
        [ViewportCommand::InnerSize(vec2(1100.0, 700.0)), ViewportCommand::OuterPosition(pos2(100.0, 100.0))]
    );
    // A position on another monitor (perhaps unplugged): centred on the screen it was maximized on.
    let elsewhere = WindowState { pos: Some([2500.0, 100.0]), ..SAVED };
    let cmds = elsewhere.unmaximize_commands(&restored, created);
    let [ViewportCommand::InnerSize(_), ViewportCommand::OuterPosition(p)] = cmds.as_slice() else { panic!("{cmds:?}") };
    assert!((300.0..=500.0).contains(&p.x) && (100.0..=250.0).contains(&p.y), "centred: {p:?}");
    // Dragged off the top of the screen: it stays where the drag put it, at its normal size.
    let dragged = info(false, [400.0, 300.0, 1920.0, 1009.0], [392.0, 269.0]);
    assert_eq!(SAVED.unmaximize_commands(&dragged, created), [ViewportCommand::InnerSize(vec2(1100.0, 700.0))]);
    // Never bigger than the screen; nothing to do when it already has its normal size.
    let big = WindowState { size: Some([3000.0, 700.0]), ..SAVED };
    assert_eq!(big.unmaximize_commands(&restored, created).first(), Some(&ViewportCommand::InnerSize(vec2(1920.0, 700.0))));
    let normal = info(false, [108.0, 131.0, 1100.0, 700.0], [100.0, 100.0]);
    assert!(SAVED.unmaximize_commands(&normal, created).is_empty());
    assert!(WindowState::default().unmaximize_commands(&restored, created).is_empty(), "no normal size known");
}

#[test]
fn window_state_is_saved_with_the_settings_and_checked_on_the_way_back() {
    let mut app = PrintCraftApp::new();
    app.window_state = WindowState { size: Some([1200.0, 800.0]), pos: Some([40.0, 30.0]), maximized: true, max_pos: Some([-11.0, -11.0]) };
    let saved = app.persist();
    let mut again = PrintCraftApp::new();
    again.restore(&saved);
    assert_eq!(again.window_state, app.window_state);
    // Untrusted settings: impossible sizes and positions are dropped.
    let mut odd = PrintCraftApp::new();
    odd.restore(r#"{"window": {"size": [1e9, 5], "pos": [null, 3], "maximized": true, "max_pos": [1e30, 0]}}"#);
    assert_eq!(odd.window_state, WindowState { maximized: true, ..WindowState::default() });
    odd.restore(r#"{"window": "nonsense"}"#);
    assert!(odd.window_state.maximized, "unreadable: unchanged");
}

/// eframe's settings file: a RON map whose "printcraft" entry is the app's JSON.
fn settings_file(app_json: &str) -> String {
    let map: std::collections::BTreeMap<&str, &str> =
        [("printcraft", app_json), ("window", "(maximized:true)"), ("egui", "()")].into_iter().collect();
    ron::ser::to_string_pretty(&map, ron::ser::PrettyConfig::default()).expect("serializes")
}

#[test]
fn the_window_state_is_read_from_the_settings_file_before_the_window_exists() {
    let dir = std::env::temp_dir().join(format!("pedeefe-window-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("app.ron");
    let mut app = PrintCraftApp::new();
    app.window_state = SAVED;
    std::fs::write(&path, settings_file(&app.persist())).expect("write");
    assert_eq!(WindowState::read_saved(&path), Some(SAVED));

    // Damaged or missing files open the window as on the first start, without failing.
    let damaged = [
        String::new(),
        "not ron at all {{{".to_string(),
        r#"{"printcraft": 5}"#.to_string(),
        settings_file("not json"),
        settings_file(r#"{"recent": []}"#),
        settings_file(r#"{"window": [1, 2]}"#),
        "\u{0}\u{ff}garbage".to_string(),
    ];
    for text in damaged {
        std::fs::write(&path, &text).expect("write");
        assert_eq!(WindowState::read_saved(&path), None, "{text:?}");
    }
    // Damaged fields are dropped one by one.
    std::fs::write(&path, settings_file(r#"{"window": {"size": [1e9, 1], "pos": [10, 20], "maximized": "yes"}}"#)).expect("write");
    assert_eq!(WindowState::read_saved(&path), Some(WindowState { pos: Some([10.0, 20.0]), ..WindowState::default() }));
    std::fs::write(&path, [0xff_u8, 0xfe, 0x00]).expect("write");
    assert_eq!(WindowState::read_saved(&path), None, "not UTF-8");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(WindowState::read_saved(&path), None, "missing");
    assert_eq!(WindowState::read_saved(&path).unwrap_or_default().startup(), WindowState::default().startup());
}
