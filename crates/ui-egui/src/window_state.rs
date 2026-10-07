//! Remember the app window between sessions: its normal (not maximized) size and position, and
//! whether it was maximized.
//!
//! eframe saves the window's geometry as well, and puts back a window that was left at a normal
//! size. A window closed *maximized*, though, came back on Windows at the maximized size without
//! being maximized (a window created hidden and maximized loses that state when shown), and
//! un-maximizing it went nowhere. So the app keeps its own record: once the window is on screen,
//! it restores the normal size and position (where un-maximizing returns to) and then maximizes.
//! The first start, with nothing saved, opens maximized.

use egui::{ViewportCommand, pos2, vec2};

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
}

/// Frames to wait before restoring (the window must be visible), and after (it settles before
/// being followed again).
const RESTORE_AT: u32 = 2;
const FOLLOW_AT: u32 = 10;

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
        Some(WindowState { size: pair("size"), pos: pair("pos"), maximized }.sanitized())
    }

    /// Settings are untrusted: sizes and positions must be finite and sane, or they are dropped.
    pub fn sanitized(self) -> Self {
        let size = self.size.filter(|s| s.iter().all(|v| v.is_finite() && (200.0..=16384.0).contains(v)));
        let pos = self.pos.filter(|p| p.iter().all(|v| v.is_finite() && (-32768.0..=32768.0).contains(v)));
        WindowState { size, pos, maximized: self.maximized }
    }

    /// Follow the window (each frame). Minimized and full-screen states aren't remembered.
    pub fn observe(&mut self, v: &egui::ViewportInfo) {
        if v.minimized == Some(true) || v.fullscreen == Some(true) {
            return;
        }
        match v.maximized {
            Some(true) => self.maximized = true,
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

    /// The commands that put the window back, given the monitor it opened on (points).
    pub fn restore_commands(&self, monitor: Option<egui::Vec2>) -> Vec<ViewportCommand> {
        let first_start = self.size.is_none() && self.pos.is_none() && !self.maximized;
        if !(self.maximized || first_start) {
            // A normal window: eframe has already put it back.
            return Vec::new();
        }
        let mut cmds = Vec::new();
        if let Some([w, h]) = self.size {
            let (w, h) = match monitor {
                Some(m) if m.x > 0.0 && m.y > 0.0 => (w.min(m.x), h.min(m.y)),
                _ => (w, h),
            };
            cmds.push(ViewportCommand::InnerSize(vec2(w, h)));
        }
        // Only a position on this monitor: one left on a monitor that is gone would hide the window.
        if let (Some([x, y]), Some(m)) = (self.pos, monitor)
            && x >= 0.0
            && y >= 0.0
            && x < m.x - 100.0
            && y < m.y - 100.0
        {
            cmds.push(ViewportCommand::OuterPosition(pos2(x, y)));
        }
        cmds.push(ViewportCommand::Maximized(true));
        cmds
    }
}

impl PrintCraftApp {
    /// Put the window back as the last session left it (the desktop app calls this at start).
    pub fn restore_window(&mut self) {
        self.window_restore = Some(0);
    }

    /// Restore the window once it is on screen, then follow it (each frame).
    pub(crate) fn window_tick(&mut self, ctx: &egui::Context) {
        match self.window_restore {
            Some(n) if n < FOLLOW_AT => {
                if n == RESTORE_AT {
                    let monitor = ctx.input(|i| i.viewport().monitor_size);
                    for cmd in self.window_state.restore_commands(monitor) {
                        ctx.send_viewport_cmd(cmd);
                    }
                }
                self.window_restore = Some(n + 1);
                ctx.request_repaint();
            }
            Some(_) => self.window_restore = None,
            None => {
                let info = ctx.input(|i| i.viewport().clone());
                self.window_state.observe(&info);
            }
        }
    }
}
