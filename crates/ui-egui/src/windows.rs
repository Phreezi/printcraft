//! Several windows in one process, each with its own ordered tabs; tabs move between them.
//!
//! The rest of the UI is written for one window: it reads the tabs from [`PdfCraftApp::views`]
//! and [`PdfCraftApp::active`]. So at any moment one window, the *current* one, has its tabs
//! there, and the other windows' tabs are parked in [`Windows`]. Each frame the root viewport
//! (eframe's own window) is drawn, then every other window through
//! [`egui::Context::show_viewport_immediate`], each with its tabs swapped in first.
//!
//! Between frames the current window is the *focus* window, the one the user worked in last:
//! files from the operating system or from a second launch of the app, the control channel and
//! background jobs act on it, and it is the only window that draws dialogs, the command palette and
//! notices. While a modal is open there, the other windows are disabled.
//!
//! Tab moves and window changes asked for while a window is drawn (a tab dropped, a focus change)
//! are queued and applied once every window has been drawn ([`PdfCraftApp::tidy_windows`]), never
//! in the middle of a window's pass, where code holds indices into `views`.
//!
//! The rules (Acrobat's):
//! - A tab dragged out of the tab strip and released over another window's tab strip moves there;
//!   released anywhere else it opens in a new window at that place. A window's only tab moves its
//!   window instead.
//! - A window left without tabs closes, unless it is the only window (it shows Home, as before).
//!   The root viewport can't close without ending the app, so when it empties while other windows
//!   remain, it takes over one of them (its tabs and its place) and that window closes.
//! - Closing a window closes its tabs, asking about unsaved changes like File ▸ Close all. Closing
//!   the only window quits, as before.
//! - Where the platform reports no window positions (Wayland), a tab can still be torn off into a
//!   new window, but not dropped onto another window. Where viewports are embedded (the web, tests)
//!   tabs aren't torn off at all.

use egui::{Pos2, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportId};
use pdfcraft_engine::DocId;

use crate::{DocView, PdfCraftApp};

/// Identifies a window for as long as it is open. The root viewport is [`ROOT_WINDOW`].
pub type WindowKey = u64;

/// eframe's own window, open as long as the app runs.
pub const ROOT_WINDOW: WindowKey = 0;

/// How far around a tab strip a released tab still counts as dropped on it (points).
pub const STRIP_SLACK: f32 = 14.0;

/// More windows than anyone works with; the cap only bounds a runaway (a script, a stuck drag).
const MAX_WINDOWS: usize = 64;

/// A new window's size when the window the tab came from reports none.
const DEFAULT_SIZE: Vec2 = Vec2::new(1100.0, 760.0);

/// The viewport that shows window `key`.
pub fn viewport_id(key: WindowKey) -> ViewportId {
    if key == ROOT_WINDOW { ViewportId::ROOT } else { ViewportId::from_hash_of(("pdfcraft-window", key)) }
}

/// One window's tabs while another window's are in [`PdfCraftApp::views`].
#[derive(Default)]
struct Tabs {
    views: Vec<DocView>,
    active: Option<usize>,
    title: String,
    full_screen: bool,
}

/// Where a window is on screen, as last reported (screen coordinates, in points). Every field is
/// `None` where the platform doesn't say (Wayland reports no positions).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    /// The content area.
    pub inner: Option<Rect>,
    /// The window with its frame.
    pub outer: Option<Rect>,
    pub maximized: bool,
    pub minimized: bool,
}

impl Geometry {
    pub fn from_info(info: &egui::ViewportInfo) -> Self {
        Self {
            inner: info.inner_rect.filter(|r| r.is_finite()),
            outer: info.outer_rect.filter(|r| r.is_finite()),
            maximized: info.maximized == Some(true),
            minimized: info.minimized == Some(true),
        }
    }

    /// A point in this window's own coordinates, on the screen.
    pub fn to_screen(&self, local: Pos2) -> Option<Pos2> {
        let p = local + self.inner?.min.to_vec2();
        p.is_finite().then_some(p)
    }

    /// The offset from the window's outer corner to its content's corner (title bar, borders).
    fn frame_offset(&self) -> Vec2 {
        match (self.inner, self.outer) {
            (Some(i), Some(o)) => (i.min - o.min).max(Vec2::ZERO),
            _ => Vec2::ZERO,
        }
    }
}

/// An open window.
struct Window {
    key: WindowKey,
    /// The window's tabs; `None` while they are in [`PdfCraftApp::views`] (the current window).
    parked: Option<Tabs>,
    /// How a window other than the root is created (kept unchanged, so egui never re-applies it).
    builder: ViewportBuilder,
    geometry: Geometry,
    /// The tab strip and each tab, in the window's own coordinates, as last drawn.
    strip: Option<Rect>,
    tabs: Vec<(DocId, Rect)>,
}

impl Window {
    fn new(key: WindowKey, builder: ViewportBuilder) -> Self {
        Self { key, parked: Some(Tabs::default()), builder, geometry: Geometry::default(), strip: None, tabs: Vec::new() }
    }

    /// The tab strip on screen, with the slack a drop is allowed.
    fn drop_zone(&self) -> Option<Rect> {
        let strip = self.strip?;
        let min = self.geometry.to_screen(strip.min)?;
        Some(Rect::from_min_size(min, strip.size()).expand2(Vec2::new(0.0, STRIP_SLACK)))
    }

    /// Where a tab dropped at `screen` goes among this window's tabs (`None`: after the last).
    fn insert_index(&self, screen: Pos2) -> Option<usize> {
        let inner = self.geometry.inner?;
        let x = screen.x - inner.min.x;
        self.tabs.iter().position(|(_, r)| x < r.center().x)
    }
}

/// A tab being dragged by its label.
#[derive(Clone, Debug)]
pub(crate) struct TabDrag {
    /// The window it started in.
    pub(crate) window: WindowKey,
    pub(crate) doc: DocId,
    /// The tab's label (for the label that follows the pointer).
    pub(crate) name: String,
    /// The pointer's offset from the tab's top-left corner when the drag started, and that corner
    /// in the window's coordinates.
    pub(crate) grab: Vec2,
    pub(crate) tab_min: Pos2,
    /// The last pointer position seen, in the window's coordinates and on screen. The pointer can
    /// leave the window during the drag; the last position seen inside or outside it is kept.
    pub(crate) pointer: Option<Pos2>,
    pub(crate) screen: Option<Pos2>,
}

/// Where a moved tab goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabTarget {
    /// Into this window's tab strip, before tab `index` (`None`: after the last one).
    Window(WindowKey, Option<usize>),
    /// Into a new window whose outer top-left corner is here (screen points; `None`: wherever the
    /// system puts it). A window's only tab moves its window there instead.
    NewWindow(Option<Pos2>),
}

/// The open windows. See the module docs.
pub struct Windows {
    /// The root first, then the others in the order they opened.
    list: Vec<Window>,
    current: WindowKey,
    focus: WindowKey,
    next_key: WindowKey,
    /// Bring this window to the front once every window has been drawn.
    pending_focus: Option<WindowKey>,
    pending_moves: Vec<(DocId, TabTarget)>,
    pub(crate) drag: Option<TabDrag>,
    /// The app is quitting (the system's Quit), not just closing the root window.
    pub(crate) quitting: bool,
}

impl Default for Windows {
    fn default() -> Self {
        let mut root = Window::new(ROOT_WINDOW, ViewportBuilder::default());
        root.parked = None;
        Self {
            list: vec![root],
            current: ROOT_WINDOW,
            focus: ROOT_WINDOW,
            next_key: ROOT_WINDOW + 1,
            pending_focus: None,
            pending_moves: Vec::new(),
            drag: None,
            quitting: false,
        }
    }
}

impl Windows {
    fn get(&self, key: WindowKey) -> Option<&Window> {
        self.list.iter().find(|w| w.key == key)
    }

    fn get_mut(&mut self, key: WindowKey) -> Option<&mut Window> {
        self.list.iter_mut().find(|w| w.key == key)
    }

    /// The window the user worked in last (dialogs, opened files and commands go there).
    pub fn focus(&self) -> WindowKey {
        self.focus
    }

    /// The window whose tabs are in [`PdfCraftApp::views`] right now.
    pub fn current(&self) -> WindowKey {
        self.current
    }

    /// Every open window, the root first.
    pub fn keys(&self) -> Vec<WindowKey> {
        self.list.iter().map(|w| w.key).collect()
    }

    /// More than one window is open.
    pub fn several(&self) -> bool {
        self.list.len() > 1
    }

    /// Where window `key` was last seen on screen.
    pub fn geometry(&self, key: WindowKey) -> Option<Geometry> {
        self.get(key).map(|w| w.geometry)
    }

    /// Record where window `key` is on screen (each frame, from its viewport's info; tests).
    pub fn set_geometry(&mut self, key: WindowKey, geometry: Geometry) {
        if let Some(w) = self.get_mut(key) {
            w.geometry = geometry;
        }
    }

    /// Record the current window's tab strip and tabs, in its own coordinates, as just drawn.
    pub(crate) fn set_strip(&mut self, strip: Rect, tabs: Vec<(DocId, Rect)>) {
        let current = self.current;
        if let Some(w) = self.get_mut(current) {
            w.strip = Some(strip);
            w.tabs = tabs;
        }
    }

    /// Record a window's tab strip in its own coordinates (tests: as if it had been drawn).
    pub fn set_strip_of(&mut self, key: WindowKey, strip: Rect, tabs: Vec<(DocId, Rect)>) {
        if let Some(w) = self.get_mut(key) {
            w.strip = Some(strip);
            w.tabs = tabs;
        }
    }

    /// Bring window `key` to the front once every window has been drawn.
    pub fn request_focus(&mut self, key: WindowKey) {
        self.pending_focus = Some(key);
    }

    /// Move a tab once every window has been drawn.
    pub(crate) fn queue_move(&mut self, doc: DocId, target: TabTarget) {
        self.pending_moves.push((doc, target));
    }

    /// Where a tab released at `screen` (from window `from`) goes: the first other window whose
    /// tab strip is under that point, else a new window there. `tab_corner` is where the tab's
    /// top-left corner was relative to the pointer, so the new window's tab lands under it.
    pub fn drop_target(&self, from: WindowKey, screen: Option<Pos2>, tab_corner: Vec2) -> TabTarget {
        let Some(p) = screen else { return TabTarget::NewWindow(None) };
        if let Some(w) = self.list.iter().find(|w| w.key != from && w.drop_zone().is_some_and(|z| z.contains(p))) {
            return TabTarget::Window(w.key, w.insert_index(p));
        }
        let frame = self.get(from).map_or(Vec2::ZERO, |w| w.geometry.frame_offset());
        let corner = p - tab_corner - frame;
        TabTarget::NewWindow(corner.is_finite().then_some(corner))
    }

    /// The window (other than `from`) whose tab strip is under `screen`, and where the tab would go.
    pub(crate) fn hovered_strip(&self, from: WindowKey, screen: Pos2) -> Option<(WindowKey, Option<usize>)> {
        self.list.iter().find(|w| w.key != from && w.drop_zone().is_some_and(|z| z.contains(screen))).map(|w| (w.key, w.insert_index(screen)))
    }

    /// The views of every window but the current one.
    pub(crate) fn parked_views(&self) -> impl Iterator<Item = &DocView> {
        self.list.iter().filter_map(|w| w.parked.as_ref()).flat_map(|t| t.views.iter())
    }

    /// The views of every window but the current one, to change.
    pub(crate) fn parked_views_mut(&mut self) -> impl Iterator<Item = &mut DocView> {
        self.list.iter_mut().filter_map(|w| w.parked.as_mut()).flat_map(|t| t.views.iter_mut())
    }

    fn tabs_of(&self, key: WindowKey) -> Option<&Tabs> {
        self.get(key).and_then(|w| w.parked.as_ref())
    }
}

impl PdfCraftApp {
    /// The documents in window `key`, in tab order (empty for a window that isn't open).
    pub fn window_tabs(&self, key: WindowKey) -> Vec<DocId> {
        if key == self.windows.current {
            self.views.iter().map(|v| v.id).collect()
        } else {
            self.windows.tabs_of(key).map(|t| t.views.iter().map(|v| v.id).collect()).unwrap_or_default()
        }
    }

    /// The document window `key` shows (`None`: its Home tab, or no such window).
    pub fn window_active(&self, key: WindowKey) -> Option<DocId> {
        let (views, active) = if key == self.windows.current {
            (&self.views, self.active)
        } else {
            let t = self.windows.tabs_of(key)?;
            (&t.views, t.active)
        };
        active.and_then(|i| views.get(i)).map(|v| v.id)
    }

    /// The window that has document `doc` as a tab.
    pub fn window_of(&self, doc: DocId) -> Option<WindowKey> {
        self.windows.keys().into_iter().find(|k| self.window_tabs(*k).contains(&doc))
    }

    /// Every open document's view, in whichever window it is.
    pub(crate) fn all_views_mut(&mut self) -> impl Iterator<Item = &mut DocView> {
        self.views.iter_mut().chain(self.windows.parked_views_mut())
    }

    /// Document `doc`'s view, in whichever window it is (results of background work and saves
    /// that finish after the user moved on to another window).
    pub(crate) fn view_of_mut(&mut self, doc: DocId) -> Option<&mut DocView> {
        self.all_views_mut().find(|v| v.id == doc)
    }

    /// A modal (dialog, save prompt, password, palette, link prompt) is open in the focus window.
    pub(crate) fn modal_open(&self) -> bool {
        self.dialog.is_some() || self.close_request.is_some() || self.password_prompt.is_some() || self.pending_link.is_some() || self.palette_open
    }

    /// The window being drawn shows the dialogs, palette and notices (it is the focus window).
    pub(crate) fn draws_overlays(&self) -> bool {
        self.windows.current == self.windows.focus
    }

    /// Make window `key`'s tabs the ones in [`Self::views`]. `false` if there is no such window.
    pub(crate) fn enter_window(&mut self, key: WindowKey) -> bool {
        if key == self.windows.current {
            return true;
        }
        let Some(incoming) = self.windows.get_mut(key).and_then(|w| w.parked.take()) else { return false };
        let outgoing = Tabs {
            views: std::mem::replace(&mut self.views, incoming.views),
            active: std::mem::replace(&mut self.active, incoming.active),
            title: std::mem::replace(&mut self.window_title, incoming.title),
            full_screen: std::mem::replace(&mut self.full_screen, incoming.full_screen),
        };
        let current = self.windows.current;
        match self.windows.get_mut(current) {
            Some(w) => w.parked = Some(outgoing),
            // The current window always stays in the list (`tidy_windows` leaves it before closing
            // it); should it be missing, its documents join this window rather than vanish.
            None => {
                let views = outgoing.views;
                self.views.extend(views);
            }
        }
        self.windows.current = key;
        true
    }

    /// Take document `doc`'s view out of the current window, keeping the active tab where it was.
    fn detach_view(&mut self, doc: DocId) -> Option<DocView> {
        let i = self.views.iter().position(|v| v.id == doc)?;
        let active_doc = self.active.and_then(|a| self.views.get(a)).map(|v| v.id);
        let view = self.views.remove(i);
        self.active = if self.views.is_empty() {
            None
        } else if active_doc == Some(doc) {
            Some(i.min(self.views.len() - 1))
        } else {
            active_doc.and_then(|d| self.views.iter().position(|v| v.id == d))
        };
        Some(view)
    }

    /// Put `view` into the current window before tab `at` (`None`: last) and make it active.
    fn attach_view(&mut self, view: DocView, at: Option<usize>) {
        let at = at.unwrap_or(self.views.len()).min(self.views.len());
        self.views.insert(at, view);
        self.active = Some(at);
    }

    /// Move document `doc`'s tab (see [`TabTarget`]). Returns the window it ends up in, which is
    /// brought to the front; `None` if the document or the window isn't open (or too many windows
    /// are). A window left without tabs closes in [`Self::tidy_windows`].
    pub fn move_tab(&mut self, doc: DocId, target: TabTarget) -> Option<WindowKey> {
        let from = self.window_of(doc)?;
        let back = self.windows.current;
        let to = match target {
            TabTarget::Window(key, _) if self.windows.get(key).is_none() => return None,
            TabTarget::Window(key, at) if key == from => {
                // Within the window: a new place in its tab strip.
                self.enter_window(from);
                if let Some(view) = self.detach_view(doc) {
                    self.attach_view(view, at);
                }
                self.enter_window(back);
                return Some(from);
            }
            TabTarget::Window(key, _) => key,
            TabTarget::NewWindow(at) if self.window_tabs(from).len() <= 1 => {
                // Its window's only tab: the window goes there instead.
                if let (Some(at), Some(ctx)) = (at, &self.ctx) {
                    ctx.send_viewport_cmd_to(viewport_id(from), ViewportCommand::OuterPosition(at));
                }
                self.windows.request_focus(from);
                return Some(from);
            }
            TabTarget::NewWindow(at) => self.new_window(from, at)?,
        };
        let at = match target {
            TabTarget::Window(_, at) => at,
            TabTarget::NewWindow(_) => None,
        };
        self.enter_window(from);
        let view = self.detach_view(doc);
        self.enter_window(to);
        if let Some(view) = view {
            self.attach_view(view, at);
        }
        self.enter_window(back);
        self.windows.request_focus(to);
        Some(to)
    }

    /// Open an empty window (it closes again unless a tab moves into it before the frame ends),
    /// the size of window `like`, with its outer corner at `at`.
    fn new_window(&mut self, like: WindowKey, at: Option<Pos2>) -> Option<WindowKey> {
        if self.windows.list.len() >= MAX_WINDOWS {
            self.notify_tr("Too many windows are open: close some first");
            return None;
        }
        let key = self.windows.next_key;
        self.windows.next_key = key.checked_add(1)?;
        let size = self.windows.geometry(like).and_then(|g| g.inner).map_or(DEFAULT_SIZE, |r| r.size());
        let mut builder = self.window_template.clone().with_title(crate::APP_NAME).with_inner_size(size);
        if let Some(at) = at {
            builder = builder.with_position(at);
        }
        self.windows.list.push(Window::new(key, builder));
        Some(key)
    }

    /// Close the current window: its documents close, each one with unsaved changes asking first
    /// (in this window, which comes to the front). The window closes once it has no tabs left.
    pub(crate) fn close_current_window(&mut self) {
        self.close_all();
        if self.close_request.is_some() {
            self.windows.request_focus(self.windows.current);
        }
    }

    /// Close window `key` as its close button would (between frames: tests, automation).
    pub fn close_window(&mut self, key: WindowKey) {
        let back = self.windows.current;
        if self.enter_window(key) {
            self.close_current_window();
            self.enter_window(back);
        }
    }

    /// The first window, the current one first, with a document that has unsaved changes.
    pub(crate) fn window_with_dirty(&self) -> Option<WindowKey> {
        let current = self.windows.current;
        std::iter::once(current).chain(self.windows.keys().into_iter().filter(|k| *k != current)).find(|k| {
            let dirty = |views: &[DocView]| views.iter().any(|v| self.session.get(v.id).is_some_and(|d| d.dirty));
            if *k == current { dirty(&self.views) } else { self.windows.tabs_of(*k).is_some_and(|t| dirty(&t.views)) }
        })
    }

    /// Note the current window's place on screen and whether the user is working in it.
    pub(crate) fn note_window(&mut self, ctx: &egui::Context) {
        let info = ctx.input(|i| i.viewport().clone());
        let current = self.windows.current;
        self.windows.set_geometry(current, Geometry::from_info(&info));
        // A modal stays where it opened: the focus doesn't follow the user away from it.
        if info.focused == Some(true) && !self.modal_open() {
            self.windows.focus = current;
        }
    }

    /// Draw every window other than the root, each with its own tabs (from the root's pass).
    pub(crate) fn show_other_windows(&mut self, ctx: &egui::Context) {
        let others: Vec<(WindowKey, ViewportBuilder)> =
            self.windows.list.iter().filter(|w| w.key != ROOT_WINDOW).map(|w| (w.key, w.builder.clone())).collect();
        for (key, builder) in others {
            ctx.show_viewport_immediate(viewport_id(key), builder, |ui, class| {
                if !self.enter_window(key) {
                    return;
                }
                let ctx = ui.ctx().clone();
                if class == egui::ViewportClass::EmbeddedWindow {
                    self.draw_embedded_window(ui);
                } else {
                    self.note_window(&ctx);
                    self.window_input(&ctx);
                    self.draw_window(ui);
                }
            });
        }
    }

    /// A window drawn inside the root, where there are no native windows (the web, tests; tabs
    /// aren't torn off there, so only automation opens one): its tabs as a list, plus the
    /// dialogs when it is the focus window. The whole interface would draw a second copy of every
    /// widget into the root's viewport.
    fn draw_embedded_window(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = self.views.iter().filter_map(|v| self.session.get(v.id)).map(|d| d.display_name()).collect();
        for (i, name) in names.iter().enumerate() {
            if ui.selectable_label(self.active == Some(i), name).clicked() {
                self.active = Some(i);
            }
        }
        if self.draws_overlays() {
            let ctx = ui.ctx().clone();
            crate::palette::show(self, &ctx);
            crate::dialogs::show(self, &ctx);
            crate::widgets::toast(self, &ctx);
        }
    }

    /// A window's own input before it is drawn: files dropped on it, its close button, its keys.
    pub(crate) fn window_input(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            self.open_dropped(f, ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close_current_window();
        }
        self.window_keys(ctx);
    }

    /// Apply what the windows asked for while they were drawn: tab moves, windows left without
    /// tabs closing, the root taking over a window, the focus. Then the focus window's tabs are
    /// in [`Self::views`] until the next frame.
    pub fn tidy_windows(&mut self, ctx: &egui::Context) {
        for (doc, target) in std::mem::take(&mut self.windows.pending_moves) {
            self.move_tab(doc, target);
        }
        self.enter_window(ROOT_WINDOW);
        // Windows other than the root close once they have no tabs.
        self.windows.list.retain(|w| w.key == ROOT_WINDOW || w.parked.as_ref().is_some_and(|t| !t.views.is_empty()));
        if self.views.is_empty() && self.windows.several() {
            self.root_takes_over(ctx);
        }
        if self.windows.get(self.windows.focus).is_none() {
            self.windows.focus = ROOT_WINDOW;
        }
        if let Some(key) = self.windows.pending_focus.take().filter(|k| self.windows.get(*k).is_some()) {
            self.windows.focus = key;
            let id = viewport_id(key);
            if self.windows.geometry(key).is_some_and(|g| g.minimized) {
                ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
            }
            ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
        }
        if self.windows.drag.as_ref().is_some_and(|d| self.windows.get(d.window).is_none()) {
            self.windows.drag = None;
        }
        self.enter_window(self.windows.focus);
    }

    /// The root window has no tabs but other windows are open: it takes the tabs and the place of
    /// one of them (the focus window if it is one), which closes. To the user, the empty window
    /// closed. Called with the root current.
    fn root_takes_over(&mut self, ctx: &egui::Context) {
        let focus = self.windows.focus;
        let Some(pos) =
            self.windows.list.iter().position(|w| w.key == focus && w.key != ROOT_WINDOW).or_else(|| (self.windows.list.len() > 1).then_some(1))
        else {
            return;
        };
        if pos >= self.windows.list.len() {
            return;
        }
        let w = self.windows.list.remove(pos);
        let tabs = w.parked.unwrap_or_default();
        self.views = tabs.views;
        self.active = tabs.active;
        self.full_screen = tabs.full_screen;
        // Sent again in the root's next pass.
        self.window_title.clear();
        let root = ViewportId::ROOT;
        let was_maximized = self.windows.geometry(ROOT_WINDOW).is_some_and(|g| g.maximized);
        // The start-up maximizing is over: it must not undo the move.
        self.window_restore = None;
        if was_maximized {
            ctx.send_viewport_cmd_to(root, ViewportCommand::Maximized(false));
        }
        if let Some(outer) = w.geometry.outer {
            ctx.send_viewport_cmd_to(root, ViewportCommand::OuterPosition(outer.min));
        }
        if let Some(inner) = w.geometry.inner {
            ctx.send_viewport_cmd_to(root, ViewportCommand::InnerSize(inner.size()));
        }
        if w.geometry.maximized {
            ctx.send_viewport_cmd_to(root, ViewportCommand::Maximized(true));
        }
        ctx.send_viewport_cmd_to(root, ViewportCommand::Fullscreen(tabs.full_screen));
        if let Some(root_window) = self.windows.get_mut(ROOT_WINDOW) {
            root_window.geometry = w.geometry;
        }
        if self.windows.focus == w.key || self.windows.focus == ROOT_WINDOW {
            self.windows.focus = ROOT_WINDOW;
        }
        if self.windows.pending_focus.is_none_or(|k| k == w.key) {
            self.windows.pending_focus = Some(ROOT_WINDOW);
        }
        if let Some(d) = self.windows.drag.as_mut().filter(|d| d.window == w.key) {
            d.window = ROOT_WINDOW;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    #[test]
    fn a_drop_on_another_windows_strip_goes_there_and_elsewhere_opens_a_new_window() {
        let mut w = Windows::default();
        w.list.push(Window::new(1, ViewportBuilder::default()));
        let geometry =
            |x: f32, y: f32| Geometry { inner: Some(rect(x, y + 30.0, 800.0, 600.0)), outer: Some(rect(x, y, 800.0, 630.0)), ..Default::default() };
        w.set_geometry(ROOT_WINDOW, geometry(0.0, 0.0));
        w.set_geometry(1, geometry(1000.0, 100.0));
        let tabs = vec![(DocId(7), rect(40.0, 4.0, 120.0, 30.0)), (DocId(8), rect(164.0, 4.0, 120.0, 30.0))];
        w.set_strip_of(1, rect(0.0, 0.0, 800.0, 38.0), tabs);
        // Window 1's strip spans x 1000..1800, y 130..168 on screen (plus the slack).
        assert_eq!(w.drop_target(ROOT_WINDOW, Some(egui::pos2(1050.0, 140.0)), Vec2::ZERO), TabTarget::Window(1, Some(0)));
        assert_eq!(w.drop_target(ROOT_WINDOW, Some(egui::pos2(1200.0, 175.0)), Vec2::ZERO), TabTarget::Window(1, Some(1)));
        assert_eq!(w.drop_target(ROOT_WINDOW, Some(egui::pos2(1500.0, 140.0)), Vec2::ZERO), TabTarget::Window(1, None));
        // Below the strip: a new window, its tab under the pointer (the root's 30-point title bar).
        assert_eq!(
            w.drop_target(ROOT_WINDOW, Some(egui::pos2(1200.0, 400.0)), egui::vec2(50.0, 10.0)),
            TabTarget::NewWindow(Some(egui::pos2(1150.0, 360.0)))
        );
        // A window's own strip isn't a target, and without positions (Wayland) only a new window is.
        assert!(matches!(w.drop_target(1, Some(egui::pos2(1050.0, 140.0)), Vec2::ZERO), TabTarget::NewWindow(Some(_))));
        assert_eq!(w.drop_target(ROOT_WINDOW, None, Vec2::ZERO), TabTarget::NewWindow(None));
        w.set_geometry(1, Geometry::default());
        assert!(matches!(w.drop_target(ROOT_WINDOW, Some(egui::pos2(1050.0, 140.0)), Vec2::ZERO), TabTarget::NewWindow(_)));
    }

    #[test]
    fn viewport_ids_are_distinct_and_the_root_is_eframes() {
        assert_eq!(viewport_id(ROOT_WINDOW), ViewportId::ROOT);
        assert_ne!(viewport_id(1), viewport_id(2));
        assert_ne!(viewport_id(1), ViewportId::ROOT);
    }

    #[test]
    fn geometry_ignores_non_finite_rects() {
        let info = egui::ViewportInfo { inner_rect: Some(rect(f32::NAN, 0.0, 10.0, 10.0)), ..Default::default() };
        let g = Geometry::from_info(&info);
        assert_eq!(g.inner, None);
        assert_eq!(g.to_screen(egui::pos2(1.0, 1.0)), None);
    }
}
