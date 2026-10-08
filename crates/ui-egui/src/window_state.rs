//! Remember the app window between sessions (its normal size and position, and whether it was
//! maximized) and bring it back without flicker.
//!
//! The window must appear once, already where it belongs and painted. Each step below removes a
//! visible jump:
//!
//! - **Geometry before the window exists.** eframe records the window's geometry when the app
//!   quits and applies it while creating the window, keeping it on a connected screen (on Windows
//!   it pulls a window left on an unplugged monitor back onto one). The window starts hidden and
//!   eframe shows it only after its first frame is painted. Commands sent from the app take effect
//!   only after that first show, so the app moves or resizes nothing at start.
//! - **Never maximized while hidden.** Asked to maximize a hidden window, winit (0.30) on Windows
//!   calls `ShowWindow(SW_MAXIMIZE)` and then hides it again: the empty window flashes on screen
//!   (egui issue 5975). It can also end up drawn at full size without really being maximized,
//!   where un-maximizing does nothing (seen on Windows with eframe's own restore). So the hook in
//!   [`Startup::adjust`] creates the window *not* maximized, at the exact place and size the
//!   maximized window had last time, and the app maximizes it right after the first painted frame
//!   is shown ([`PrintCraftApp::restore_window`]). The window already covers that area, so
//!   maximizing changes nothing visible.
//! - **Un-maximizing still returns to the normal size.** Windows un-maximizes to where the window
//!   was before maximizing, which is now that full-screen area. The first time the window is
//!   un-maximized, the app puts back the remembered normal size and position
//!   ([`WindowState::unmaximize_commands`]).
//! - **The first frame is the real interface.** The desktop app installs fonts and the theme before
//!   the first frame (`PrintCraftApp::prepare`), so the frame the window is shown with is the
//!   interface, not an empty near-black one.
//!
//! The very first start (nothing saved) opens centred at a default size and maximizes once shown:
//! with no record of the screen yet, that one start grows into place.

use std::path::Path;

use egui::{Pos2, ViewportBuilder, ViewportCommand, pos2, vec2};

use crate::PrintCraftApp;

/// The window as last seen.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WindowState {
    /// Inner size of the normal window, in points.
    pub size: Option<[f32; 2]>,
    /// Outer position of the normal window, in points.
    pub pos: Option<[f32; 2]>,
    pub maximized: bool,
    /// Outer position of the maximized window, in points: where the window is created when it
    /// opens maximized (see the module docs).
    pub max_pos: Option<[f32; 2]>,
}

/// Inner size of the window on the first start, in points.
pub const DEFAULT_SIZE: [f32; 2] = [1440.0, 920.0];

/// eframe nudges a maximized window's frame (it hangs past the screen's edges by the border width)
/// back inside the screen; differences up to this many points are that nudge, not a changed screen.
const BORDER_SLACK: f32 = 16.0;

/// Frames to wait for the window to report being maximized before following it anyway (a window
/// manager may refuse).
const MAXIMIZE_WAIT: u32 = 120;

/// Settings files bigger than this aren't read before the window opens (they hold signatures and
/// stamps; the window state is a few bytes).
const MAX_SETTINGS_BYTES: u64 = 64 << 20;

impl WindowState {
    /// Read saved settings field by field, so one damaged field doesn't lose the others.
    /// `None` when there is no window object at all.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let o = v.as_object()?;
        let pair = |k: &str| -> Option<[f32; 2]> {
            let a = o.get(k)?.as_array()?;
            match a.as_slice() {
                [x, y] => Some([x.as_f64()? as f32, y.as_f64()? as f32]),
                _ => None,
            }
        };
        let maximized = o.get("maximized").and_then(serde_json::Value::as_bool).unwrap_or(false);
        Some(WindowState { size: pair("size"), pos: pair("pos"), maximized, max_pos: pair("max_pos") }.sanitized())
    }

    /// The window state inside the app's settings as eframe stores them (`app.ron`: a RON map
    /// whose `"printcraft"` entry is the app's JSON, see `PrintCraftApp::persist`). `None` when any
    /// layer is missing or damaged.
    pub fn from_settings(ron_text: &str) -> Option<Self> {
        let map: std::collections::HashMap<String, String> = ron::from_str(ron_text).ok()?;
        let app: serde_json::Value = serde_json::from_str(map.get("printcraft")?).ok()?;
        Self::from_json(app.get("window")?)
    }

    /// Read the window state from the settings file before the window is created (the desktop
    /// app calls this ahead of `eframe::run_native`). `None` when the file is missing, too big or
    /// damaged: the window then opens as on the first start.
    pub fn read_saved(path: &Path) -> Option<Self> {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        let mut text = String::new();
        file.take(MAX_SETTINGS_BYTES.saturating_add(1)).read_to_string(&mut text).ok()?;
        if text.len() as u64 > MAX_SETTINGS_BYTES {
            return None;
        }
        Self::from_settings(&text)
    }

    /// Settings are untrusted: sizes and positions must be finite and sane, or they are dropped.
    pub fn sanitized(self) -> Self {
        let size = self.size.filter(|s| s.iter().all(|v| v.is_finite() && (200.0..=16384.0).contains(v)));
        let sane_pos = |p: &[f32; 2]| p.iter().all(|v| v.is_finite() && (-32768.0..=32768.0).contains(v));
        WindowState { size, pos: self.pos.filter(sane_pos), maximized: self.maximized, max_pos: self.max_pos.filter(sane_pos) }
    }

    /// Nothing saved yet.
    pub fn is_first_start(&self) -> bool {
        self.size.is_none() && self.pos.is_none() && !self.maximized
    }

    /// The window opens maximized: it was maximized last time, or this is the first start.
    pub fn opens_maximized(&self) -> bool {
        self.maximized || self.is_first_start()
    }

    /// How the window is created, decided before it exists.
    pub fn startup(&self) -> Startup {
        Startup { size: self.size.unwrap_or(DEFAULT_SIZE), centered: self.is_first_start(), maximized: self.opens_maximized(), max_pos: self.max_pos }
    }

    /// Follow the window (each frame). Minimized and full-screen states aren't remembered.
    pub fn observe(&mut self, v: &egui::ViewportInfo) {
        if v.minimized == Some(true) || v.fullscreen == Some(true) {
            return;
        }
        match v.maximized {
            Some(true) => {
                self.maximized = true;
                if let Some(r) = v.outer_rect.filter(|r| r.is_finite()) {
                    self.max_pos = Some([r.min.x, r.min.y]);
                }
            }
            Some(false) => {
                self.maximized = false;
                if let Some(r) = v.inner_rect.filter(|r| r.is_finite() && r.width() >= 200.0 && r.height() >= 200.0) {
                    self.size = Some([r.width(), r.height()]);
                }
                if let Some(r) = v.outer_rect.filter(|r| r.is_finite()) {
                    self.pos = Some([r.min.x, r.min.y]);
                }
            }
            None => {}
        }
    }

    /// The commands that give a window, just un-maximized for the first time since it opened
    /// maximized, its normal size and position back (see the module docs). `created_at` is the
    /// outer position the window was created at.
    ///
    /// Un-maximized in place (the restore button, a double click, Win+Down), the window covers the
    /// screen it was maximized on, which is where the saved position is checked: one that isn't on
    /// that screen (it was on another monitor, perhaps one unplugged since) is replaced by the
    /// screen's centre. Dragged off the top of the screen, it keeps the place the drag gave it and
    /// only gets its size back.
    pub fn unmaximize_commands(&self, info: &egui::ViewportInfo, created_at: Option<Pos2>) -> Vec<ViewportCommand> {
        let (Some([w, h]), Some(inner), Some(outer)) = (self.size, info.inner_rect, info.outer_rect) else {
            return Vec::new();
        };
        if !(inner.is_finite() && outer.is_finite()) || inner.width() <= 0.0 || inner.height() <= 0.0 {
            return Vec::new();
        }
        // Never bigger than the area it was maximized in.
        let size = vec2(w.min(inner.width()), h.min(inner.height()));
        if (inner.width() - size.x).abs() <= 2.0 && (inner.height() - size.y).abs() <= 2.0 {
            // Already its normal size: Windows knew where to return.
            return Vec::new();
        }
        let mut cmds = vec![ViewportCommand::InnerSize(size)];
        let in_place = created_at.is_some_and(|c| (outer.min - c).abs().max_elem() <= 2.0);
        if in_place {
            // Borders and title bar.
            let frame = (outer.size() - inner.size()).max(egui::Vec2::ZERO);
            let outer_size = size + frame;
            let screen = outer;
            let on_screen = |p: Pos2| {
                p.x >= screen.min.x - BORDER_SLACK && p.y >= screen.min.y - BORDER_SLACK && p.x + 100.0 <= screen.max.x && p.y + 50.0 <= screen.max.y
            };
            let pos = match self.pos.map(|[x, y]| pos2(x, y)) {
                Some(p) if on_screen(p) => p,
                _ => (screen.center() - outer_size / 2.0).max(screen.min),
            };
            cmds.push(ViewportCommand::OuterPosition(pos));
        }
        cmds
    }
}

/// How the window is created, decided before it exists from the saved [`WindowState`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Startup {
    /// Inner size when eframe has no usable geometry of its own (points).
    pub size: [f32; 2],
    /// Nothing saved yet: let eframe centre the window on the main screen.
    pub centered: bool,
    /// The window opens maximized (the app maximizes it once its first frame is on screen).
    pub maximized: bool,
    /// Where the maximized window was (outer position, points).
    pub max_pos: Option<[f32; 2]>,
}

impl Startup {
    /// The size the window is created with when eframe has saved none.
    pub fn builder(&self, b: ViewportBuilder) -> ViewportBuilder {
        b.with_inner_size(self.size)
    }

    /// eframe's window-builder hook: runs after eframe applied the geometry it saved (already kept
    /// on a connected screen) and before the window is created.
    ///
    /// - The window is never created maximized or full screen (see the module docs).
    /// - A geometry eframe recorded from a minimized window (0 × 0, far off screen) or a full-screen
    ///   one is replaced by the normal size, placed by the system.
    /// - A maximized window's geometry is used only when the window opens maximized, and then at
    ///   exactly the place it had (undoing eframe's nudge of its frame), so that maximizing it once
    ///   shown changes nothing visible.
    pub fn adjust(&self, mut b: ViewportBuilder) -> ViewportBuilder {
        let saved_maximized = b.maximized == Some(true);
        let usable = b.fullscreen != Some(true)
            && b.inner_size.is_some_and(|s| s.x.is_finite() && s.y.is_finite() && s.x >= 200.0 && s.y >= 200.0)
            && !(saved_maximized && !self.maximized);
        if !usable {
            b.inner_size = Some(vec2(self.size[0], self.size[1]));
            b.position = None;
        } else if saved_maximized
            && let (Some(p), Some([x, y])) = (b.position, self.max_pos)
            && (p - pos2(x, y)).abs().max_elem() <= BORDER_SLACK
        {
            b.position = Some(pos2(x, y));
        }
        b.maximized = Some(false);
        b.fullscreen = Some(false);
        b
    }
}

/// Bringing the window back at start (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Restore {
    /// Frames seen since the window was created.
    frame: u32,
    /// Maximize once the first frame is on screen.
    maximize: bool,
    /// The window has reported being maximized since.
    maximized_seen: bool,
    /// Outer position the window was created at (where Windows un-maximizes it to).
    created_at: Option<Pos2>,
    /// Give back the normal size and position on the first un-maximize.
    fixup: bool,
}

impl PrintCraftApp {
    /// Put the window back as the last session left it (the desktop app calls this at start, with
    /// the window created by [`Startup`]): maximize it once its first frame is on screen if it was
    /// maximized (or on the first start).
    pub fn restore_window(&mut self) {
        self.window_restore = Some(Restore { maximize: self.window_state.opens_maximized(), ..Restore::default() });
    }

    /// Restore the window once it is on screen, then follow it (each frame).
    pub(crate) fn window_tick(&mut self, ctx: &egui::Context) {
        let info = ctx.input(|i| i.viewport().clone());
        let Some(mut r) = self.window_restore else {
            self.window_state.observe(&info);
            return;
        };
        if r.frame == 0 {
            // The window is still hidden: eframe shows it after this frame is painted, and only
            // then applies commands sent now.
            r.created_at = info.outer_rect.filter(|o| o.is_finite()).map(|o| o.min);
            if r.maximize {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                r.fixup = self.window_state.size.is_some();
            }
        }
        r.frame = r.frame.saturating_add(1);
        if r.maximize && !r.maximized_seen {
            if info.maximized == Some(true) {
                r.maximized_seen = true;
            } else if r.frame < MAXIMIZE_WAIT {
                // Not maximized yet: don't take the full-screen area for the normal geometry.
                self.window_restore = Some(r);
                ctx.request_repaint();
                return;
            } else {
                r.fixup = false;
            }
        }
        if r.fixup && info.maximized == Some(false) && info.minimized != Some(true) && info.fullscreen != Some(true) {
            for cmd in self.window_state.unmaximize_commands(&info, r.created_at) {
                ctx.send_viewport_cmd(cmd);
            }
            // The geometry changes after this frame; follow it from the next.
            self.window_restore = None;
            self.window_state.maximized = false;
            return;
        }
        self.window_state.observe(&info);
        self.window_restore = r.fixup.then_some(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(maximized: bool, outer: [f32; 2], inner: [f32; 2]) -> egui::ViewportInfo {
        egui::ViewportInfo {
            maximized: Some(maximized),
            minimized: Some(false),
            fullscreen: Some(false),
            outer_rect: Some(egui::Rect::from_min_size(pos2(outer[0], outer[1]), vec2(inner[0] + 16.0, inner[1] + 39.0))),
            inner_rect: Some(egui::Rect::from_min_size(pos2(outer[0] + 8.0, outer[1] + 31.0), vec2(inner[0], inner[1]))),
            ..Default::default()
        }
    }

    /// One app frame as eframe runs it, with the window described by `viewport`: the commands
    /// that move, resize or maximize the window.
    fn frame(app: &mut PrintCraftApp, ctx: &egui::Context, viewport: egui::ViewportInfo) -> Vec<ViewportCommand> {
        let mut input = egui::RawInput::default();
        input.viewports.insert(egui::ViewportId::ROOT, viewport);
        let mut out = ctx.run_ui(input, |ui| app.window_tick(ui.ctx()));
        out.textures_delta.clear();
        let commands = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        commands
            .into_iter()
            .filter(|c| {
                matches!(
                    c,
                    ViewportCommand::InnerSize(_)
                        | ViewportCommand::OuterPosition(_)
                        | ViewportCommand::Maximized(_)
                        | ViewportCommand::Fullscreen(_)
                        | ViewportCommand::Minimized(_)
                )
            })
            .collect()
    }

    #[test]
    fn a_window_that_opens_maximized_is_maximized_after_the_first_frame_and_nothing_else() {
        let mut app = PrintCraftApp::new();
        app.window_state = WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 80.0]), maximized: true, max_pos: Some([-8.0, -8.0]) };
        app.restore_window();
        let ctx = egui::Context::default();
        // Created hidden at the maximized area, not maximized.
        let created = info(false, [-8.0, -8.0], [1920.0, 1009.0]);
        assert_eq!(frame(&mut app, &ctx, created.clone()), [ViewportCommand::Maximized(true)]);
        // Until the window reports being maximized, the full-screen area isn't taken for its normal size.
        assert!(frame(&mut app, &ctx, created).is_empty());
        assert_eq!(app.window_state.size, Some([1100.0, 700.0]));
        let maximized = info(true, [-8.0, -8.0], [1920.0, 1009.0]);
        for _ in 0..3 {
            assert!(frame(&mut app, &ctx, maximized.clone()).is_empty(), "no resizing or moving while maximized");
        }
        assert!(app.window_state.maximized);
        // Un-maximized: Windows puts it back at the full-screen area it was created at; the app gives
        // it its normal size and position, once.
        let restored = info(false, [-8.0, -8.0], [1920.0, 1009.0]);
        assert_eq!(
            frame(&mut app, &ctx, restored),
            [ViewportCommand::InnerSize(vec2(1100.0, 700.0)), ViewportCommand::OuterPosition(pos2(100.0, 80.0))]
        );
        let normal = info(false, [100.0, 80.0], [1100.0, 700.0]);
        assert!(frame(&mut app, &ctx, normal.clone()).is_empty());
        assert_eq!(
            app.window_state,
            WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 80.0]), maximized: false, max_pos: Some([-8.0, -8.0]) }
        );
        // From then on Windows knows the normal geometry: maximizing and un-maximizing again is left to it.
        assert!(frame(&mut app, &ctx, maximized).is_empty());
        assert!(frame(&mut app, &ctx, normal).is_empty());
        assert!(app.window_restore.is_none());
    }

    #[test]
    fn a_normal_window_is_followed_from_the_first_frame_without_commands() {
        let mut app = PrintCraftApp::new();
        app.window_state = WindowState { size: Some([1100.0, 700.0]), pos: Some([100.0, 80.0]), maximized: false, max_pos: None };
        app.restore_window();
        let ctx = egui::Context::default();
        assert!(frame(&mut app, &ctx, info(false, [120.0, 90.0], [1000.0, 600.0])).is_empty());
        assert_eq!(app.window_state.size, Some([1000.0, 600.0]));
        assert_eq!(app.window_state.pos, Some([120.0, 90.0]));
        assert!(app.window_restore.is_none());
    }

    #[test]
    fn a_window_manager_that_never_maximizes_is_followed_after_a_while() {
        let mut app = PrintCraftApp::new();
        app.restore_window();
        let ctx = egui::Context::default();
        let shown = info(false, [200.0, 100.0], [1440.0, 920.0]);
        assert_eq!(frame(&mut app, &ctx, shown.clone()), [ViewportCommand::Maximized(true)], "the first start opens maximized");
        for _ in 0..MAXIMIZE_WAIT {
            assert!(frame(&mut app, &ctx, shown.clone()).is_empty());
        }
        assert_eq!(app.window_state.size, Some([1440.0, 920.0]));
        assert!(app.window_restore.is_none());
    }

    #[test]
    fn prepared_apps_draw_the_interface_in_their_first_frame() {
        let ctx = egui::Context::default();
        let mut app = PrintCraftApp::new();
        app.prepare(&ctx);
        let mut f = eframe::Frame::_new_kittest();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            eframe::App::logic(&mut app, ui.ctx(), &mut f);
            eframe::App::ui(&mut app, ui, &mut f);
        });
        out.textures_delta.clear();
        fn texts(s: &egui::Shape) -> usize {
            match s {
                egui::Shape::Text(_) => 1,
                egui::Shape::Vec(v) => v.iter().map(texts).sum(),
                _ => 0,
            }
        }
        let text: usize = out.shapes.iter().map(|s| texts(&s.shape)).sum();
        assert!(text > 5, "the first frame shows the interface, not an empty window ({text} text shapes)");
    }
}
