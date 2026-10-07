//! The app window is remembered between sessions: normal size and position, and maximized.

use egui::ViewportCommand;
use printcraft_ui_egui::PrintCraftApp;
use printcraft_ui_egui::window_state::WindowState;

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

#[test]
fn the_normal_size_survives_maximizing() {
    let mut s = WindowState::default();
    s.observe(&info(false, [108.0, 140.0, 1100.0, 700.0], [100.0, 100.0]));
    assert_eq!(s, WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 100.0]), maximized: false });
    // Maximized: the normal size and position are kept for un-maximizing.
    s.observe(&info(true, [0.0, 0.0, 1920.0, 1040.0], [-8.0, -8.0]));
    assert_eq!(s, WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 100.0]), maximized: true });
    // Minimized and full screen aren't remembered.
    s.observe(&egui::ViewportInfo { minimized: Some(true), maximized: Some(false), ..Default::default() });
    assert!(s.maximized);
}

#[test]
fn restoring_maximizes_after_putting_back_the_normal_geometry() {
    let monitor = Some(egui::vec2(1920.0, 1080.0));
    let s = WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 100.0]), maximized: true };
    let cmds = s.restore_commands(monitor);
    assert_eq!(
        cmds,
        [
            ViewportCommand::InnerSize(egui::vec2(1100.0, 700.0)),
            ViewportCommand::OuterPosition(egui::pos2(100.0, 100.0)),
            ViewportCommand::Maximized(true)
        ]
    );
    // A normal window: eframe puts it back itself.
    assert!(WindowState { maximized: false, ..s }.restore_commands(monitor).is_empty());
    // The first start (nothing saved) opens maximized.
    assert_eq!(WindowState::default().restore_commands(monitor), [ViewportCommand::Maximized(true)]);
    // A position off this monitor (one that was unplugged) isn't used; a size is capped to it.
    let off = WindowState { size: Some([3000.0, 700.0]), pos: Some([2500.0, 100.0]), maximized: true };
    assert_eq!(off.restore_commands(monitor), [ViewportCommand::InnerSize(egui::vec2(1920.0, 700.0)), ViewportCommand::Maximized(true)]);
}

#[test]
fn window_state_is_saved_with_the_settings_and_checked_on_the_way_back() {
    let mut app = PrintCraftApp::new();
    app.window_state = WindowState { size: Some([1200.0, 800.0]), pos: Some([40.0, 30.0]), maximized: true };
    let saved = app.persist();
    let mut again = PrintCraftApp::new();
    again.restore(&saved);
    assert_eq!(again.window_state, app.window_state);
    // Untrusted settings: impossible sizes and positions are dropped.
    let mut odd = PrintCraftApp::new();
    odd.restore(r#"{"window": {"size": [1e9, 5], "pos": [null, 3], "maximized": true}}"#);
    assert_eq!(odd.window_state, WindowState { size: None, pos: None, maximized: true });
    odd.restore(r#"{"window": "nonsense"}"#);
    assert!(odd.window_state.maximized, "unreadable: unchanged");
}
