//! The Print dialog (Acrobat's File ▸ Print, execution plan M10.5): printer, copies, grayscale,
//! print quality; pages to print (all, current, range with labels; odd/even, reverse); page
//! sizing & handling (Size, Poster, Multiple with its cut-and-stack order for single-sided
//! printing, Booklet), with PeDeeFe's print area (Full page or Window) applying to all of them;
//! orientation; comments & forms; and a live preview of the sheets, drawn sharp at the screen's
//! resolution.
//!
//! **Window** works like AutoCAD's plot window: the user drags a rectangle over the page (in a
//! large picker with zoom and pan) and only that area of each page prints, exactly as drawn, in
//! whichever sizing mode is chosen: on one sheet with Size (Fit fills the A4/A3 sheet with it), as
//! a poster over several sheets with Poster, several areas per sheet with Multiple, or as a
//! booklet. Holding Shift while dragging keeps the area in the sheet's proportions, so it fills
//! the sheet.
//!
//! The dialog is grouped like Acrobat's: each section in its own titled panel, the sizing modes
//! as a segmented control with the print area choice to its right, and the preview with the
//! scale and sheet count above it and, below, the sheet and (in brackets) the printed size of the
//! page at that scale. A poster previews the whole page (or area) with its tiles over it.
//!
//! Enter prints (like the Print button) from anywhere in the dialog, also while the Pages,
//! Copies or scale fields have focus, but not while a menu is open or the area picker shows;
//! Escape cancels.
//!
//! The printer list is fetched in the background (on Windows it takes a moment), and jobs print
//! in the background (on Windows every sheet is drawn first): the dialog closes at once and a
//! notice says when the job reached the printer. Printer, paper, two-sided, colour and quality
//! are remembered between sessions; the print area and the window, for the session. "Save as PDF" writes the print-ready PDF
//! (in a browser it downloads it).

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};
use pdfcraft_engine::print::{self, A3, A4, Binding, BookletSubset, Content, Layout, Orientation, PageOrder, SizeMode, Subset, spool};

use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, widgets};

/// The papers the dialog offers (PeDeeFe prints on A4 or A3), A4 first and by default.
pub const PAPER_CHOICES: [(&str, (f64, f64)); 2] = [("A4", A4), ("A3", A3)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    All,
    Current,
    Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handling {
    Size,
    Poster,
    Multiple,
    Booklet,
}

/// What part of each page prints, whatever the sizing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PrintArea {
    /// The whole page.
    #[default]
    FullPage,
    /// An area of the page, chosen like AutoCAD's plot window ([`PrintDraft::region`]).
    Window,
}

/// A gesture in the window picker, in page display space (points, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickDrag {
    /// Drawing a new area from this corner.
    Draw { start: (f64, f64) },
    /// Moving the area: where it was grabbed, and the area then.
    Move { grab: (f64, f64), from: [f64; 4] },
}

/// The window picker's view: zoom (1 = the whole page fits), pan (screen points) and gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickView {
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub drag: Option<PickDrag>,
    /// The window before picking started (Cancel puts it back).
    pub before: Option<[f64; 4]>,
}

impl Default for PickView {
    fn default() -> Self {
        PickView { zoom: 1.0, pan: egui::Vec2::ZERO, drag: None, before: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrintDraft {
    pub printers: Vec<spool::Printer>,
    /// The printer list is being fetched.
    pub listing: bool,
    /// Why the printers couldn't be listed (shown in the printer menu).
    pub list_error: Option<String>,
    /// `None` = Save as PDF.
    pub printer: Option<String>,
    /// The printer (or Save as PDF) was picked by the user, now or in an earlier session: the
    /// list arriving doesn't replace it with the default.
    pub printer_chosen: bool,
    pub copies: u32,
    pub collate: bool,
    pub grayscale: bool,
    pub duplex: spool::Duplex,
    /// Resolution of the sheet images where sheets print as images (Windows).
    pub dpi: u32,
    pub which: Which,
    pub range: String,
    pub subset: Subset,
    pub reverse: bool,
    pub handling: Handling,
    pub size: SizeMode,
    pub custom_scale: f64,
    pub per_sheet: usize,
    pub order: PageOrder,
    pub border: bool,
    pub auto_rotate: bool,
    pub booklet_subset: BookletSubset,
    pub binding: Binding,
    pub poster_scale: f64,
    pub overlap: f64,
    pub cut_marks: bool,
    pub orientation: Orientation,
    pub content: Content,
    /// Index into [`PAPER_CHOICES`].
    pub paper: usize,
    /// The sheet shown in the preview (0-based).
    pub sheet: usize,
    pub current_page: usize,
    /// Whole pages or the window.
    pub area: PrintArea,
    /// The window: the area to print, `[x0, y0, x1, y1]` in page display space (points from the
    /// bottom-left), used when [`Self::area`] is [`PrintArea::Window`]. `None` prints the whole
    /// page.
    pub region: Option<[f64; 4]>,
    /// The page the window picker shows.
    pub region_page: usize,
    /// The window picker is showing (instead of the settings).
    pub picking: bool,
    pub pick: PickView,
}

impl Default for PrintDraft {
    fn default() -> Self {
        PrintDraft {
            printers: Vec::new(),
            listing: false,
            list_error: None,
            printer: None,
            printer_chosen: false,
            copies: 1,
            collate: true,
            grayscale: false,
            duplex: spool::Duplex::Off,
            dpi: spool::QUALITIES[0].0,
            which: Which::All,
            range: String::new(),
            subset: Subset::All,
            reverse: false,
            handling: Handling::Size,
            size: SizeMode::Fit,
            custom_scale: 100.0,
            per_sheet: 2,
            order: PageOrder::Horizontal,
            border: false,
            auto_rotate: true,
            booklet_subset: BookletSubset::BothSides,
            binding: Binding::Left,
            poster_scale: 200.0,
            overlap: 18.0,
            cut_marks: true,
            orientation: Orientation::Auto,
            content: Content::DocumentAndMarkups,
            paper: 0,
            sheet: 0,
            current_page: 0,
            area: PrintArea::FullPage,
            region: None,
            region_page: 0,
            picking: false,
            pick: PickView::default(),
        }
    }
}

/// The paper (points, portrait) of choice `i`, A4 for anything out of range.
pub fn paper(i: usize) -> (f64, f64) {
    PAPER_CHOICES.get(i).map_or(A4, |p| p.1)
}

/// Width ÷ height of `paper` turned for `landscape` (Fit adds no margin, so the whole sheet is
/// printable): the shape a window drawn with Shift keeps, so that it fills the sheet exactly.
pub fn printable_aspect(paper: (f64, f64), landscape: bool) -> f64 {
    let (w, h) = (paper.0.min(paper.1), paper.0.max(paper.1));
    let r = if w > 0.0 && h > 0.0 { w / h } else { 1.0 };
    if landscape { 1.0 / r } else { r }
}

/// Whether a window of shape `w × h` prints on a landscape sheet under `orientation`.
pub fn window_landscape(orientation: Orientation, w: f64, h: f64) -> bool {
    match orientation {
        Orientation::Portrait => false,
        Orientation::Landscape => true,
        Orientation::Auto => w > h,
    }
}

fn norm(r: [f64; 4]) -> [f64; 4] {
    [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
}

/// The window dragged from corner `a` to `b` on a page of display size `page`. With `aspect`
/// (width ÷ height) it keeps that shape, covering the dragged extent, and shrinks (anchored at
/// `a`) to stay on the page; without, it is the dragged box clamped to the page.
pub fn drag_window(a: (f64, f64), b: (f64, f64), page: (f64, f64), aspect: Option<f64>) -> [f64; 4] {
    let clamp = |p: (f64, f64)| (p.0.clamp(0.0, page.0), p.1.clamp(0.0, page.1));
    let (a, b) = (clamp(a), clamp(b));
    let Some(r) = aspect.filter(|r| r.is_finite() && *r > 0.0) else { return norm([a.0, a.1, b.0, b.1]) };
    let (sx, sy) = (if b.0 >= a.0 { 1.0 } else { -1.0 }, if b.1 >= a.1 { 1.0 } else { -1.0 });
    let (mut w, mut h) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
    if w / r >= h {
        h = w / r;
    } else {
        w = h * r;
    }
    let room_x = if sx > 0.0 { page.0 - a.0 } else { a.0 };
    let room_y = if sy > 0.0 { page.1 - a.1 } else { a.1 };
    if w > 0.0 && h > 0.0 {
        let k = (room_x / w).min(room_y / h).clamp(0.0, 1.0);
        w *= k;
        h *= k;
    }
    norm([a.0, a.1, a.0 + sx * w, a.1 + sy * h])
}

/// `from` moved by `(dx, dy)`, kept on the page.
pub fn move_window(from: [f64; 4], dx: f64, dy: f64, page: (f64, f64)) -> [f64; 4] {
    let r = norm(from);
    let (w, h) = ((r[2] - r[0]).min(page.0), (r[3] - r[1]).min(page.1));
    let x0 = (r[0] + dx).clamp(0.0, (page.0 - w).max(0.0));
    let y0 = (r[1] + dy).clamp(0.0, (page.1 - h).max(0.0));
    [x0, y0, x0 + w, y0 + h]
}

/// The scale (percent) a window prints at with Fit on one sheet of `paper` (edge to edge: Fit
/// adds no margin).
pub fn window_fit_percent(region: [f64; 4], paper: (f64, f64), orientation: Orientation) -> f64 {
    let r = norm(region);
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if w <= 0.0 || h <= 0.0 {
        return 100.0;
    }
    let (pw, ph) = (paper.0.min(paper.1), paper.0.max(paper.1));
    let (sw, sh) = if window_landscape(orientation, w, h) { (ph, pw) } else { (pw, ph) };
    (sw / w).min(sh / h) * 100.0
}

fn mm(pt: f64) -> f64 {
    pt / 72.0 * 25.4
}

/// "297 × 210 mm".
pub fn area_label(region: [f64; 4]) -> String {
    let r = norm(region);
    format!("{:.0} × {:.0} mm", mm(r[2] - r[0]), mm(r[3] - r[1]))
}

/// "1188,04": two decimals with a decimal comma, as the preview writes printed sizes; "—" for a
/// number that isn't finite.
pub fn decimal_comma(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    format!("{v:.2}").replace('.', ",")
}

/// The caption under the preview: the sheet, then in brackets the printed size of the page (or
/// window) at the scale it prints at, e.g. "A4 - 210 × 297 mm [197,30 × 263,07 mm]".
pub fn sheet_label(name: &str, sheet: (f64, f64), printed: Option<(f64, f64)>) -> String {
    let mut s = format!("{name} - {:.0} × {:.0} mm", mm(sheet.0), mm(sheet.1));
    if let Some((w, h)) = printed {
        s.push_str(&format!(" [{} × {} mm]", decimal_comma(mm(w)), decimal_comma(mm(h))));
    }
    s
}

/// "Scale: 186%" for a scale of 1.864 (1 = 100 %); "Scale: —" when there is none.
pub fn scale_label(scale: Option<f64>) -> String {
    match scale.filter(|s| s.is_finite() && *s > 0.0) {
        Some(s) => crate::i18n::fmt(tl!("Scale: {pct}%"), &[("pct", &format!("{:.0}", s * 100.0))]),
        None => tl!("Scale: —").to_string(),
    }
}

/// The scale (1 = 100 %) the preview reports for sheet `i`, and the printed size (points) of the
/// page content at that scale: the window (or the whole page) times the scale, in the page's
/// own orientation (also when Multiple turns it on the sheet). Exact in every mode, because it
/// comes from the sheet's own placement: Fit, Actual size, Shrink, Custom, Multiple, Booklet,
/// Poster and Window. Pages of different sizes on one sheet print at different scales: this is
/// the first page on the sheet. A sheet with no page (a booklet's blank back) borrows the first
/// placed page's numbers.
pub fn preview_numbers(sheets: &[print::Sheet], i: usize, sizes: &[(f64, f64)], region: Option<[f64; 4]>) -> Option<(f64, (f64, f64))> {
    let pl = sheets.get(i).and_then(|s| s.placed.first()).or_else(|| sheets.iter().find_map(|s| s.placed.first()))?;
    let s = pl.scale();
    let printed = print::printed_size(*sizes.get(pl.page)?, region, s)?;
    Some((s, printed))
}

/// The sheets of the poster that sheet `i` belongs to (one page's grid of tiles), or `None` when
/// sheet `i` isn't a poster tile. Correct when a page prints twice: runs are found by position.
pub fn poster_run(sheets: &[print::Sheet], i: usize) -> Option<std::ops::Range<usize>> {
    let t = sheets.get(i)?.tile?;
    let start = i.checked_sub(t.row.checked_mul(t.cols)?.checked_add(t.col)?)?;
    let end = start.checked_add(t.cols.checked_mul(t.rows)?)?.min(sheets.len());
    Some(start..end)
}

/// What the poster preview draws, in screen points (see [`poster_preview`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PosterPreview {
    /// The page the poster prints.
    pub page: usize,
    /// The whole page (or the window), fitted to the area with a 12-point margin and centred.
    pub page_rect: Rect,
    /// The part of the page's texture inside `page_rect` (texture y runs down from the page top).
    pub uv: Rect,
    /// Screen points per page point.
    pub zoom: f32,
    /// The grid of tiles, columns × rows.
    pub grid: (usize, usize),
    /// Each tile over `page_rect`, in sheet order: its number (from 1) and where it is.
    pub tiles: Vec<(usize, Rect)>,
    /// The index in `tiles` of the current sheet's tile.
    pub current: Option<usize>,
}

/// Where the poster preview draws the sheets of `run` (one page's tiles, from [`poster_run`]) in
/// `area`, like Acrobat: the whole page (or the window `region`) fitted to the area, not one
/// zoomed tile, with each sheet's tile over it and the tile of sheet `current` marked. `None`
/// when there is nothing sensible to draw.
pub fn poster_preview(
    sheets: &[print::Sheet],
    run: std::ops::Range<usize>,
    sizes: &[(f64, f64)],
    region: Option<[f64; 4]>,
    area: Rect,
    current: usize,
) -> Option<PosterPreview> {
    let first = sheets.get(run.start)?;
    let page = first.placed.first()?.page;
    let &(dw, dh) = sizes.get(page)?;
    let ((ox, oy), (vw, vh)) = print::page_view((dw, dh), region)?;
    if !(dw > 0.0 && dh > 0.0 && vw > 0.0 && vh > 0.0 && dw.is_finite() && dh.is_finite()) {
        return None;
    }
    let zoom = ((area.width() - 24.0) / vw as f32).min((area.height() - 24.0) / vh as f32).max(0.001);
    let page_rect = Rect::from_center_size(area.center(), vec2(vw as f32 * zoom, vh as f32 * zoom));
    if !(zoom.is_finite() && page_rect.is_finite()) {
        return None;
    }
    let to_screen = |x: f64, y: f64| pos2(page_rect.left() + (x - ox) as f32 * zoom, page_rect.bottom() - (y - oy) as f32 * zoom);
    let uv = Rect::from_min_max(pos2((ox / dw) as f32, (1.0 - (oy + vh) / dh) as f32), pos2(((ox + vw) / dw) as f32, (1.0 - oy / dh) as f32));
    let mut tiles = Vec::new();
    let mut marked = None;
    for (number, j) in (run.start..run.end.min(sheets.len())).enumerate() {
        let Some(c) = sheets.get(j).and_then(|s| s.placed.first()).map(|pl| pl.clip) else { continue };
        if j == current {
            marked = Some(tiles.len());
        }
        tiles.push((number + 1, Rect::from_two_pos(to_screen(c[0], c[1]), to_screen(c[2], c[3]))));
    }
    let grid = first.tile.map_or((1, 1), |t| (t.cols, t.rows));
    Some(PosterPreview { page, page_rect, uv, zoom, grid, tiles, current: marked })
}

impl PrintDraft {
    /// The engine settings for this draft (page count and labels from the document).
    pub fn settings(&self, count: usize, labels: &[String]) -> Result<print::Settings, String> {
        if self.handling == Handling::Multiple && self.order == PageOrder::CutStack && self.duplex != spool::Duplex::Off {
            return Err("Cut and stack needs Two-sided: Off. Print single-sided sheets.".into());
        }
        let range = match self.which {
            Which::All => None,
            Which::Current => Some((self.current_page + 1).to_string()),
            Which::Range => Some(self.range.clone()),
        };
        let pages = print::select_pages(count, range.as_deref(), labels, self.subset, self.reverse).map_err(|e| e.to_string())?;
        let layout = match self.handling {
            Handling::Size => Layout::Size(match self.size {
                SizeMode::Custom(_) => SizeMode::Custom(self.custom_scale),
                m => m,
            }),
            Handling::Multiple => match Layout::multiple(self.per_sheet) {
                Layout::Multiple { cols, rows, .. } => {
                    Layout::Multiple { cols, rows, order: self.order, border: self.border, auto_rotate: self.auto_rotate }
                }
                other => other,
            },
            Handling::Booklet => Layout::Booklet { subset: self.booklet_subset, binding: self.binding },
            Handling::Poster => Layout::Poster { scale: self.poster_scale, overlap: self.overlap, cut_marks: self.cut_marks },
        };
        // The window applies to every sizing mode.
        let region = if self.area == PrintArea::Window { self.region } else { None };
        Ok(print::Settings { pages, paper: paper(self.paper), orientation: self.orientation, layout, content: self.content, region })
    }

    pub fn job(&self, title: &str) -> spool::Job {
        spool::Job {
            printer: self.printer.clone(),
            copies: self.copies.max(1),
            collate: self.collate,
            duplex: self.duplex,
            grayscale: self.grayscale,
            title: title.to_string(),
            dpi: self.dpi,
            print_to_file: None,
        }
    }

    /// The printer list arrived: keep the chosen printer if it is still there; otherwise (or
    /// when nothing was chosen yet) pick the system default, else the first printer.
    pub fn printers_arrived(&mut self, list: Vec<spool::Printer>) {
        let fallback = list.iter().find(|p| p.default).or(list.first()).map(|p| p.name.clone());
        let still_there = |n: &str| list.iter().any(|p| p.name == n);
        match &self.printer {
            Some(n) if still_there(n) => {}
            Some(_) => self.printer = fallback,
            None if !self.printer_chosen => self.printer = fallback,
            None => {}
        }
        self.printers = list;
        self.listing = false;
    }

    /// The options remembered between sessions.
    pub fn prefs(&self) -> serde_json::Value {
        serde_json::json!({
            "printer": self.printer,
            "printer_chosen": self.printer_chosen,
            "paper": PAPER_CHOICES.get(self.paper).map_or("A4", |p| p.0),
            "duplex": match self.duplex { spool::Duplex::Off => "off", spool::Duplex::LongEdge => "long-edge", spool::Duplex::ShortEdge => "short-edge" },
            "grayscale": self.grayscale,
            "collate": self.collate,
            "dpi": self.dpi,
        })
    }

    /// Restore [`Self::prefs`]. Settings are untrusted: anything unknown keeps the default.
    /// Only the keys read here count, so older files still load: their `lock_aspect` (the
    /// "Keep the sheet's proportions" check box, now Shift while dragging) and `window_poster`
    /// (the Window tab's "one sheet / as a poster" choice, now the Size and Poster tabs with the
    /// Window area) are ignored.
    pub fn restore_prefs(&mut self, v: &serde_json::Value) {
        if let Some(name) = v["printer"].as_str().filter(|n| !n.is_empty() && n.len() <= 512) {
            self.printer = Some(name.to_string());
        }
        if let Some(chosen) = v["printer_chosen"].as_bool() {
            self.printer_chosen = chosen;
        }
        if let Some(i) = v["paper"].as_str().and_then(|p| PAPER_CHOICES.iter().position(|c| c.0 == p)) {
            self.paper = i;
        }
        match v["duplex"].as_str() {
            Some("off") => self.duplex = spool::Duplex::Off,
            Some("long-edge") => self.duplex = spool::Duplex::LongEdge,
            Some("short-edge") => self.duplex = spool::Duplex::ShortEdge,
            _ => {}
        }
        if let Some(g) = v["grayscale"].as_bool() {
            self.grayscale = g;
        }
        if let Some(c) = v["collate"].as_bool() {
            self.collate = c;
        }
        if let Some(dpi) = v["dpi"].as_u64().and_then(|d| spool::QUALITIES.iter().find(|q| u64::from(q.0) == d)) {
            self.dpi = dpi.0;
        }
    }
}

/// Background work for the Print dialog: the printer list being fetched, and jobs printing.
#[derive(Default)]
pub(crate) struct PrintJobs {
    #[cfg(not(target_arch = "wasm32"))]
    listing: Option<std::sync::mpsc::Receiver<Result<Vec<spool::Printer>, String>>>,
    #[cfg(not(target_arch = "wasm32"))]
    running: Vec<(String, std::sync::mpsc::Receiver<Result<String, String>>)>,
}

impl PdfCraftApp {
    pub fn open_print(&mut self) {
        let Some((i, _)) = self.active_ids() else { return };
        let current = self.views[i].current;
        let keep = std::mem::take(&mut self.print_draft);
        self.print_draft = PrintDraft { current_page: current, sheet: 0, picking: false, pick: PickView::default(), ..keep };
        self.list_printers();
        self.dialog = Some(crate::Dialog::Print);
    }

    /// Fetch the printer list in the background (on Windows it takes a moment).
    fn list_printers(&mut self) {
        if !spool::available() {
            // No printers here (the web): Save as PDF, which downloads the file there, even when
            // a printer was remembered from elsewhere.
            self.print_draft.printers_arrived(Vec::new());
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.print_jobs.listing.is_some() {
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                // The receiver may be gone (the app quit): nothing to report to then.
                let _ = tx.send(spool::list_printers().map_err(|e| e.to_string()));
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
            self.print_jobs.listing = Some(rx);
            self.print_draft.listing = true;
        }
    }

    /// Pick up the printer list and finished jobs (each frame).
    pub(crate) fn poll_print(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::mpsc::TryRecvError;
            if let Some(rx) = &self.print_jobs.listing {
                match rx.try_recv() {
                    Ok(Ok(list)) => {
                        self.print_jobs.listing = None;
                        self.print_draft.list_error = None;
                        self.print_draft.printers_arrived(list);
                    }
                    Ok(Err(e)) => {
                        self.print_jobs.listing = None;
                        self.print_draft.list_error = Some(e);
                        self.print_draft.printers_arrived(Vec::new());
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.print_jobs.listing = None;
                        self.print_draft.listing = false;
                    }
                }
            }
            let mut finished = Vec::new();
            self.print_jobs.running.retain(|(printer, rx)| match rx.try_recv() {
                Ok(r) => {
                    finished.push((printer.clone(), r));
                    false
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => {
                    finished.push((printer.clone(), Err("the print job stopped unexpectedly".into())));
                    false
                }
            });
            for (printer, r) in finished {
                match r {
                    Ok(msg) if msg.is_empty() => self.notify_fmt("Sent to {printer}", &[("printer", &printer)]),
                    Ok(msg) => self.notify_fmt("Sent to {printer}: {msg}", &[("printer", &printer), ("msg", &msg)]),
                    // The spooler's own words stay untranslated (see `notify_error`).
                    Err(e) => self.notify_fmt("Couldn't print on {printer}: {e}", &[("printer", &printer), ("e", &e)]),
                }
            }
            if (self.print_jobs.listing.is_some() || !self.print_jobs.running.is_empty())
                && let Some(ctx) = &self.ctx
            {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }

    /// Print (or save) with the dialog's settings. Returns `true` when the job started (printing
    /// carries on in the background) or the PDF was saved. Save as PDF without a preset path
    /// returns `true` once the save picker is showing; the file is written on a later frame, when
    /// the user has chosen where.
    pub fn print_now(&mut self) -> bool {
        let Some((_, id)) = self.active_ids() else { return false };
        let Some(doc) = self.session.get(id) else { return false };
        let labels: Vec<String> = doc.info.pages.iter().map(|p| p.label.clone()).collect();
        let settings = match self.print_draft.settings(doc.info.pages.len(), &labels) {
            Ok(s) => s,
            Err(e) => {
                self.notify_error(e);
                return false;
            }
        };
        let name = doc.name.clone();
        let bytes = match self.session.print_pdf(id, &settings) {
            Ok(b) => b,
            Err(e) => {
                self.notify_error(e);
                return false;
            }
        };
        match self.print_draft.printer.clone() {
            Some(printer) => {
                let job = self.print_draft.job(&name);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(spool::submit(&bytes, &job).map_err(|e| e.to_string()));
                        if let Some(ctx) = ctx {
                            ctx.request_repaint();
                        }
                    });
                    self.print_jobs.running.push((printer.clone(), rx));
                    self.notify_fmt("Printing on {printer}…", &[("printer", &printer)]);
                    true
                }
                #[cfg(target_arch = "wasm32")]
                {
                    match spool::submit(&bytes, &job) {
                        Ok(msg) => {
                            if msg.is_empty() {
                                self.notify_fmt("Sent to {printer}", &[("printer", &printer)]);
                            } else {
                                self.notify_fmt("Sent to {printer}: {msg}", &[("printer", &printer), ("msg", &msg)]);
                            }
                            true
                        }
                        Err(e) => {
                            self.notify_error(e);
                            false
                        }
                    }
                }
            }
            None => self.save_print_pdf(&name, bytes),
        }
    }

    /// Print ▸ Save as PDF on the desktop: write to `save_override` (tests and automation), or
    /// ask where and write on a later frame. Returns `true` once written, or once the save picker
    /// is showing.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_print_pdf(&mut self, name: &str, bytes: Vec<u8>) -> bool {
        let write = move |app: &mut Self, path: std::path::PathBuf| match crate::editing::write_atomically(&path.to_string_lossy(), &bytes) {
            Ok(()) => {
                app.notify_fmt("Saved the print-ready PDF to {path}", &[("path", &path.display().to_string())]);
                true
            }
            Err(e) => {
                app.notify_fmt("Could not save: {e}", &[("e", &e.to_string())]);
                false
            }
        };
        match self.save_override.clone() {
            Some(p) => write(self, p.into()),
            None => {
                let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF");
                let dialog = rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{stem} (print).pdf"));
                self.ask_one(crate::pickers::Ask::Save(dialog), None, move |app, path| {
                    write(app, path);
                })
            }
        }
    }

    /// Print ▸ Save as PDF in a browser: download it, as Save does (#170). There is no folder to
    /// choose and no file system to write to, so `save_override` doesn't apply.
    #[cfg(target_arch = "wasm32")]
    fn save_print_pdf(&mut self, name: &str, bytes: Vec<u8>) -> bool {
        let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF");
        let file = format!("{stem} (print).pdf");
        match crate::editing::download(&file, &bytes) {
            Ok(()) => {
                self.notify_fmt("Downloaded {name}", &[("name", &file)]);
                true
            }
            Err(e) => {
                self.notify_fmt("Couldn't download {name}: {e}", &[("name", &file), ("e", &e)]);
                false
            }
        }
    }
}

fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, value: &mut T, choices: &[(T, &str)], width: f32) {
    let shown = choices.iter().find(|c| c.0 == *value).map_or("", |c| c.1);
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(width).show_ui(ui, |ui| {
        for (v, label) in choices {
            ui.selectable_value(value, *v, *label);
        }
    });
}

fn poster_controls(ui: &mut egui::Ui, d: &mut PrintDraft) {
    ui.horizontal(|ui| {
        ui.label(tl!("Tile scale:"));
        ui.add(egui::DragValue::new(&mut d.poster_scale).range(10.0..=1000.0).suffix(" %"));
        ui.label(tl!("Overlap:"));
        ui.add(egui::DragValue::new(&mut d.overlap).range(0.0..=144.0).suffix(" pt"));
        ui.checkbox(&mut d.cut_marks, tl!("Cut marks"));
    });
}

/// The Print dialog's width (the window picker takes most of the app window instead).
pub(crate) const DIALOG_WIDTH: f32 = 880.0;
/// The preview column's width.
const PREVIEW_WIDTH: f32 = 340.0;
/// The height of the preview's drawing (less on a short screen).
const PREVIEW_HEIGHT: f32 = 530.0;
/// Every sizing tab's body is at least this tall, so switching tabs doesn't move the dialog.
const TAB_BODY_HEIGHT: f32 = 112.0;

/// The page sizing & handling modes, as the segmented control shows them (in English; drawn
/// through `tl!`).
const TABS: [(Handling, &str); 4] =
    [(Handling::Size, "Size"), (Handling::Poster, "Poster"), (Handling::Multiple, "Multiple"), (Handling::Booklet, "Booklet")];

/// The print area choices, shown to the right of the sizing modes (in English; drawn through
/// `tl!`).
const AREAS: [(PrintArea, &str); 2] = [(PrintArea::FullPage, "Full page"), (PrintArea::Window, "Window")];

/// What each Comments & Forms choice prints.
fn content_note(c: Content) -> &'static str {
    match c {
        Content::Document => "The pages and their form fields, without comments.",
        Content::DocumentAndMarkups => "The pages, form fields and every comment set to print.",
        Content::DocumentAndStamps => "The pages, form fields and stamps, without other comments.",
        Content::FormFieldsOnly => "Only what is in the form fields.",
    }
}

/// What the preview shows: the pages' display sizes, the sheets laid out, the window (when the
/// print area is Window) and the pages' sharpest textures.
struct Shown<'a> {
    sizes: &'a [(f64, f64)],
    sheets: &'a [print::Sheet],
    region: Option<[f64; 4]>,
    tex: &'a dyn Fn(usize) -> Option<egui::TextureId>,
}

/// Draw the dialog. `tex` gives a page's sharpest texture so far; `wants` collects the pages
/// shown and the device pixels per point they are shown at, so sharper rasters can be rendered.
/// Returns (print, cancel).
pub(crate) fn body(
    ui: &mut egui::Ui,
    d: &mut PrintDraft,
    t: &Tokens,
    sizes: &[(f64, f64)],
    labels: &[String],
    tex: &dyn Fn(usize) -> Option<egui::TextureId>,
    wants: &mut Vec<(usize, f32)>,
) -> (bool, bool) {
    d.paper = d.paper.min(PAPER_CHOICES.len() - 1);
    d.region_page = d.region_page.min(sizes.len().saturating_sub(1));
    if d.picking {
        picker(ui, d, t, sizes, tex, wants);
        return (false, false);
    }
    ui.set_width(DIALOG_WIDTH);
    let enter = enter_pressed(ui);
    ui.label(egui::RichText::new(tl!("Print")).font(theme::semibold(18.0)));
    ui.add_space(6.0);
    let settings = d.settings(sizes.len(), labels);
    // Laid out once a frame: the preview and the Print button both use it.
    let (sheets, error) = match &settings {
        Ok(s) => match print::layout(sizes, s) {
            Ok(sheets) => (sheets, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        },
        Err(e) => (Vec::new(), Some(e.clone())),
    };
    d.sheet = d.sheet.min(sheets.len().saturating_sub(1));
    let shown = Shown { sizes, sheets: &sheets, region: settings.as_ref().ok().and_then(|s| s.region), tex };
    // A short screen (a 1080p Windows screen at 150 % is 720 points high) scrolls the settings
    // instead of cutting the dialog off.
    let max_height = (ui.ctx().content_rect().height() - 150.0).max(320.0);
    // The scroll area gets its full height now, not what the dialog had last frame: otherwise
    // the dialog grows by the button row each frame until it fits (it seems to unfold on open).
    let width = ui.available_width();
    ui.allocate_ui(vec2(width, max_height), |ui| {
        egui::ScrollArea::vertical().id_salt("print-body").max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let left = (ui.available_width() - PREVIEW_WIDTH - 14.0).max(320.0);
                ui.vertical(|ui| {
                    ui.set_width(left);
                    settings_column(ui, d, t, sizes.len());
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_width(PREVIEW_WIDTH);
                    // On a short screen the preview shrinks so its navigation stays in view.
                    let height = PREVIEW_HEIGHT.min(max_height - 130.0).max(240.0);
                    preview_panel(ui, d, t, &shown, error, height, wants);
                });
            });
        });
    });
    ui.add_space(10.0);
    let (mut go, mut cancel) = (false, false);
    let can_print = settings.is_ok() && !sheets.is_empty();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let label = if d.printer.is_some() { tl!("Print") } else { tl!("Save as PDF") };
        if ui.add_enabled_ui(can_print, |ui| widgets::pill_button(ui, label, true)).inner.clicked() {
            go = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    // Enter is the Print button, unless this frame's Enter opened a menu (a focused combo box)
    // or the area picker (a focused "Select area…"). The fields saw the key first, so a value
    // being typed (Copies, a scale) is already in the draft.
    if enter && can_print && !cancel && !d.picking && !egui::Popup::is_any_open(ui.ctx()) {
        go = true;
    }
    (go, cancel)
}

/// Whether Enter (alone) was pressed this frame for the Print dialog: not while a menu or combo
/// box is open (Enter picks its item there). Read before the dialog's widgets: unless a text field
/// (Pages, or Copies or a scale being typed) has focus, the key is taken here, so a focused check
/// box, radio button or button doesn't also act on it; a focused field keeps it, so it commits
/// what was typed.
fn enter_pressed(ui: &egui::Ui) -> bool {
    let ctx = ui.ctx();
    if egui::Popup::is_any_open(ctx) || !ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none()) {
        return false;
    }
    if !ctx.text_edit_focused() {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    }
    true
}

/// The settings, one titled panel per section (like Acrobat's grouped boxes).
fn settings_column(ui: &mut egui::Ui, d: &mut PrintDraft, t: &Tokens, page_count: usize) {
    widgets::group(ui, tl!("Printer"), t.section[0], |ui| {
        egui::Grid::new("print-top").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(tl!("Printer:"));
            let shown = match (&d.printer, d.listing) {
                (Some(p), _) => p.clone(),
                (None, true) if !d.printer_chosen => tl!("Looking for printers…").to_string(),
                (None, _) => tl!("Save as PDF").to_string(),
            };
            let before = d.printer.clone();
            egui::ComboBox::from_id_salt("printer").selected_text(shown).width(280.0).show_ui(ui, |ui| {
                for p in &d.printers {
                    let label = if p.default { crate::i18n::fmt(tl!("{name} (default)"), &[("name", &p.name)]) } else { p.name.clone() };
                    ui.selectable_value(&mut d.printer, Some(p.name.clone()), label);
                }
                if d.listing {
                    ui.label(egui::RichText::new(tl!("Looking for printers…")).color(t.text_muted));
                } else if let Some(e) = &d.list_error {
                    let why = crate::i18n::fmt(tl!("Couldn't list the printers: {e}"), &[("e", e)]);
                    ui.label(egui::RichText::new(why).color(t.text_muted));
                } else if d.printers.is_empty() && spool::available() {
                    ui.label(egui::RichText::new(tl!("No printers found")).color(t.text_muted));
                }
                ui.selectable_value(&mut d.printer, None, tl!("Save as PDF"));
            });
            if d.printer != before {
                d.printer_chosen = true;
            }
            ui.end_row();
            ui.label(tl!("Copies:"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut d.copies).range(1..=999));
                ui.checkbox(&mut d.collate, tl!("Collate"));
                ui.checkbox(&mut d.grayscale, tl!("Print in grayscale"));
            });
            ui.end_row();
            ui.label(tl!("Two-sided:"));
            combo(
                ui,
                "duplex",
                &mut d.duplex,
                &[
                    (spool::Duplex::Off, tl!("Off")),
                    (spool::Duplex::LongEdge, tl!("Flip on long edge")),
                    (spool::Duplex::ShortEdge, tl!("Flip on short edge")),
                ],
                160.0,
            );
            ui.end_row();
            ui.label(tl!("Paper:"));
            ui.horizontal(|ui| {
                for (i, (name, _)) in PAPER_CHOICES.iter().enumerate() {
                    ui.radio_value(&mut d.paper, i, *name);
                }
                if spool::prints_images() {
                    ui.add_space(12.0);
                    ui.label(tl!("Quality:"));
                    let choices: Vec<(u32, &str)> = spool::QUALITIES.iter().map(|&(dpi, label)| (dpi, tl!(label))).collect();
                    combo(ui, "quality", &mut d.dpi, &choices, 140.0);
                }
            });
            ui.end_row();
        });
    });
    widgets::group(ui, tl!("Pages to Print"), t.section[1], |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut d.which, Which::All, tl!("All"));
            ui.radio_value(&mut d.which, Which::Current, tl!("Current page"));
            let pages = ui.radio_value(&mut d.which, Which::Range, tl!("Pages"));
            let r = ui
                .add_enabled(
                    d.which == Which::Range,
                    egui::TextEdit::singleline(&mut d.range).hint_text(format!("1-{page_count}")).desired_width(110.0),
                )
                // Screen readers name the field after its radio button.
                .labelled_by(pages.id);
            if r.gained_focus() {
                d.which = Which::Range;
            }
        });
        ui.horizontal(|ui| {
            ui.label(tl!("More options:"));
            combo(
                ui,
                "subset",
                &mut d.subset,
                &[(Subset::All, tl!("All pages in range")), (Subset::Odd, tl!("Odd pages only")), (Subset::Even, tl!("Even pages only"))],
                150.0,
            );
            ui.checkbox(&mut d.reverse, tl!("Reverse pages"));
        });
    });
    widgets::group(ui, tl!("Page Sizing & Handling"), t.section[2], |ui| {
        let tabs = TABS.map(|tab| tl!(tab.1));
        let areas = AREAS.map(|a| tl!(a.1));
        // The print area sits to the right of the sizing modes, or under them when a language's
        // longer words don't leave room.
        let beside = widgets::segmented_width(ui, &tabs) + 16.0 + widgets::segmented_width(ui, &areas) <= ui.available_width();
        let mut area_choice = None;
        ui.horizontal(|ui| {
            let selected = TABS.iter().position(|(h, _)| *h == d.handling).unwrap_or(0);
            if let Some(&(h, _)) = widgets::segmented(ui, "print-handling", &tabs, selected).and_then(|i| TABS.get(i)) {
                d.handling = h;
                d.sheet = 0;
            }
            if beside {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| area_choice = area_segments(ui, d, &areas));
            }
        });
        if !beside {
            ui.horizontal(|ui| area_choice = area_segments(ui, d, &areas));
        }
        if let Some(a) = area_choice {
            if a == PrintArea::Window && d.area != PrintArea::Window && d.which == Which::All && page_count > 1 {
                // A window is usually wanted from the page being looked at.
                d.which = Which::Current;
            }
            d.area = a;
            d.sheet = 0;
        }
        area_row(ui, d, t);
        ui.add_space(2.0);
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.divider))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(TAB_BODY_HEIGHT);
                tab_body(ui, d, t);
            });
    });
    // Side by side, the same height; columns justify their contents, so lay out from the left.
    let ragged = egui::Layout::top_down(egui::Align::Min);
    ui.columns_const(|[left, right]| {
        widgets::group(left, tl!("Orientation"), t.section[3], |ui| {
            ui.set_min_height(72.0);
            ui.radio_value(&mut d.orientation, Orientation::Auto, tl!("Auto portrait/landscape"));
            ui.radio_value(&mut d.orientation, Orientation::Portrait, tl!("Portrait"));
            ui.radio_value(&mut d.orientation, Orientation::Landscape, tl!("Landscape"));
        });
        widgets::group(right, tl!("Comments & Forms"), t.section[4], |ui| {
            ui.set_min_height(72.0);
            let width = (ui.available_width() - 8.0).max(120.0);
            combo(
                ui,
                "content",
                &mut d.content,
                &[
                    (Content::Document, tl!("Document")),
                    (Content::DocumentAndMarkups, tl!("Document and markups")),
                    (Content::DocumentAndStamps, tl!("Document and stamps")),
                    (Content::FormFieldsOnly, tl!("Form fields only")),
                ],
                width,
            );
            ui.with_layout(ragged, |ui| ui.label(egui::RichText::new(tl!(content_note(d.content))).size(12.0).color(t.text_muted)));
        });
    });
}

/// The controls of the sizing tab chosen.
fn tab_body(ui: &mut egui::Ui, d: &mut PrintDraft, t: &Tokens) {
    match d.handling {
        Handling::Size => {
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.size, SizeMode::Fit, tl!("Fit"));
                ui.radio_value(&mut d.size, SizeMode::Actual, tl!("Actual size"));
                ui.radio_value(&mut d.size, SizeMode::Shrink, tl!("Shrink oversized pages"));
            });
            ui.horizontal(|ui| {
                let custom = matches!(d.size, SizeMode::Custom(_));
                if ui.radio(custom, tl!("Custom scale:")).clicked() {
                    d.size = SizeMode::Custom(d.custom_scale);
                }
                ui.add_enabled(custom, egui::DragValue::new(&mut d.custom_scale).range(1.0..=1000.0).suffix(" %"));
            });
        }
        Handling::Poster => {
            poster_controls(ui, d);
            ui.label(
                egui::RichText::new(tl!("Each page is enlarged and printed in tiles, one per sheet; the preview shows the tiles over the page."))
                    .size(12.0)
                    .color(t.text_muted),
            );
        }
        Handling::Multiple => {
            ui.horizontal(|ui| {
                ui.label(tl!("Pages per sheet:"));
                combo(ui, "per-sheet", &mut d.per_sheet, &[(2, "2"), (4, "4"), (6, "6"), (9, "9"), (16, "16")], 60.0);
                ui.label(tl!("Page order:"));
                combo(
                    ui,
                    "order",
                    &mut d.order,
                    &[
                        (PageOrder::Horizontal, tl!("Horizontal")),
                        (PageOrder::HorizontalReversed, tl!("Horizontal reversed")),
                        (PageOrder::Vertical, tl!("Vertical")),
                        (PageOrder::VerticalReversed, tl!("Vertical reversed")),
                        (PageOrder::CutStack, tl!("Cut and stack")),
                    ],
                    150.0,
                );
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut d.border, tl!("Print page border"));
                ui.checkbox(&mut d.auto_rotate, tl!("Auto-rotate pages"));
            });
            if d.order == PageOrder::CutStack {
                ui.label(
                    egui::RichText::new(tl!(
                        "Print single-sided. Keep the sheets in order, cut at the marks, then stack the piles left to right, top to bottom."
                    ))
                    .size(12.0)
                    .color(t.text_muted),
                );
            }
        }
        Handling::Booklet => {
            ui.horizontal(|ui| {
                ui.label(tl!("Booklet subset:"));
                combo(
                    ui,
                    "booklet",
                    &mut d.booklet_subset,
                    &[
                        (BookletSubset::BothSides, tl!("Both sides")),
                        (BookletSubset::FrontOnly, tl!("Front side only")),
                        (BookletSubset::BackOnly, tl!("Back side only")),
                    ],
                    130.0,
                );
                ui.label(tl!("Binding:"));
                combo(ui, "binding", &mut d.binding, &[(Binding::Left, tl!("Left")), (Binding::Right, tl!("Right"))], 80.0);
            });
        }
    }
}

/// The print area control ("Full page" | "Window"): the choice clicked, if any.
fn area_segments(ui: &mut egui::Ui, d: &PrintDraft, labels: &[&str]) -> Option<PrintArea> {
    let selected = AREAS.iter().position(|(a, _)| *a == d.area).unwrap_or(0);
    widgets::segmented(ui, "print-area", labels, selected).and_then(|i| AREAS.get(i)).map(|a| a.0)
}

/// The preview, like Acrobat's: the scale and the number of sheets above the drawing; the sheet
/// navigation and the sheet (with the printed size of the page at that scale) below it.
fn preview_panel(
    ui: &mut egui::Ui,
    d: &mut PrintDraft,
    t: &Tokens,
    shown: &Shown,
    error: Option<String>,
    height: f32,
    wants: &mut Vec<(usize, f32)>,
) {
    let numbers = preview_numbers(shown.sheets, d.sheet, shown.sizes, shown.region);
    let n = shown.sheets.len();
    widgets::group_with(
        ui,
        t.icon,
        |ui| {
            ui.label(egui::RichText::new(scale_label(numbers.map(|v| v.0))).font(theme::semibold(13.5)).color(t.text));
            ui.add_space(14.0);
            let count = crate::i18n::fmt(tl!("Sheets: {n}"), &[("n", &n.to_string())]);
            ui.label(egui::RichText::new(count).font(theme::semibold(13.5)).color(t.text));
        },
        |ui| {
            let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
            ui.painter().rect_filled(area, 6.0, t.preview_fill);
            match (error, shown.sheets.get(d.sheet)) {
                (Some(e), _) => {
                    ui.put(area.shrink(16.0), egui::Label::new(egui::RichText::new(e).color(t.text_muted)).wrap());
                }
                (None, Some(sheet)) => match poster_run(shown.sheets, d.sheet) {
                    Some(run) => draw_poster(ui, area, d, t, shown, run, wants),
                    None => draw_sheet(ui, area, sheet, d.grayscale, t, shown, wants),
                },
                _ => {}
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(d.sheet > 0, egui::Button::new("‹")).on_hover_text(tl!("Previous sheet")).clicked() {
                    d.sheet = d.sheet.saturating_sub(1);
                }
                ui.label(if n == 0 {
                    tl!("No sheets").to_string()
                } else {
                    crate::i18n::fmt(tl!("Sheet {s} of {n}"), &[("s", &(d.sheet + 1).to_string()), ("n", &n.to_string())])
                });
                if ui.add_enabled(d.sheet + 1 < n, egui::Button::new("›")).on_hover_text(tl!("Next sheet")).clicked() {
                    d.sheet += 1;
                }
            });
            if let Some(s) = shown.sheets.get(d.sheet) {
                let name = PAPER_CHOICES.get(d.paper).map_or("", |p| p.0);
                ui.label(egui::RichText::new(sheet_label(name, s.size, numbers.map(|v| v.1))).size(12.5).color(t.text))
                    .on_hover_text(tl!("The sheet, and in brackets the size the page prints at at this scale"));
            }
        },
    );
}

/// One sheet as it prints: the paper, each page placed on it, page borders and cut marks.
fn draw_sheet(ui: &egui::Ui, area: Rect, sheet: &print::Sheet, grayscale: bool, t: &Tokens, shown: &Shown, wants: &mut Vec<(usize, f32)>) {
    let ppp = ui.ctx().pixels_per_point();
    let (sw, sh) = (sheet.size.0 as f32, sheet.size.1 as f32);
    if !(sw > 0.0 && sh > 0.0 && sw.is_finite() && sh.is_finite()) {
        return;
    }
    let k = ((area.width() - 24.0) / sw).min((area.height() - 24.0) / sh).max(0.001);
    let paper = Rect::from_center_size(area.center(), vec2(sw * k, sh * k));
    ui.painter().rect_filled(paper.translate(vec2(0.0, 2.0)).expand(1.5), 2.0, t.page_shadow);
    ui.painter().rect_filled(paper, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(paper, 0.0, Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
    let to_screen = |x: f64, y: f64| -> Pos2 { pos2(paper.left() + x as f32 * k, paper.bottom() - y as f32 * k) };
    let painter = ui.painter().with_clip_rect(paper);
    for pl in &sheet.placed {
        let Some(&(dw, dh)) = shown.sizes.get(pl.page) else { continue };
        // Device pixels per point of the page, as drawn here.
        wants.push((pl.page, pl.scale() as f32 * k * ppp));
        let [x0, y0, x1, y1] = pl.clip;
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
            let (sx, sy) = pl.matrix.apply(x, y);
            to_screen(sx, sy)
        });
        // UVs of the visible part (texture y runs down from the page top).
        let uv = |x: f64, y: f64| pos2((x / dw) as f32, (1.0 - y / dh) as f32);
        let uvs = [uv(x0, y0), uv(x1, y0), uv(x1, y1), uv(x0, y1)];
        match (shown.tex)(pl.page) {
            Some(tex) => {
                let mut mesh = egui::Mesh::with_texture(tex);
                for (p, u) in corners.iter().zip(uvs) {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: *p,
                        uv: u,
                        color: if grayscale { Color32::from_gray(235) } else { Color32::WHITE },
                    });
                }
                mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                painter.add(egui::Shape::mesh(mesh));
            }
            None => {
                painter.add(egui::Shape::convex_polygon(corners.to_vec(), Color32::from_gray(245), Stroke::new(0.5, Color32::from_gray(180))));
                let c = corners.iter().fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2()) / 4.0;
                painter.text(c.to_pos2(), egui::Align2::CENTER_CENTER, (pl.page + 1).to_string(), theme::regular(11.0), Color32::from_gray(120));
            }
        }
    }
    for b in &sheet.borders {
        painter.rect_stroke(
            Rect::from_two_pos(to_screen(b[0], b[1]), to_screen(b[2], b[3])),
            0.0,
            Stroke::new(0.6, Color32::BLACK),
            egui::StrokeKind::Middle,
        );
    }
    for l in &sheet.lines {
        painter.line_segment([to_screen(l[0], l[1]), to_screen(l[2], l[3])], Stroke::new(0.6, Color32::BLACK));
    }
}

/// A poster, like Acrobat previews one: the whole page (or the window) with its tiles over it as
/// dashed lines, numbered, and the tile of the current sheet highlighted. The geometry comes
/// from [`poster_preview`]; screen readers (and the tests) read the grid and the highlighted
/// tile from the drawing's label.
fn draw_poster(ui: &egui::Ui, area: Rect, d: &PrintDraft, t: &Tokens, shown: &Shown, run: std::ops::Range<usize>, wants: &mut Vec<(usize, f32)>) {
    let Some(g) = poster_preview(shown.sheets, run, shown.sizes, shown.region, area, d.sheet) else { return };
    let (p, page) = (g.page, g.page_rect);
    wants.push((p, g.zoom * ui.ctx().pixels_per_point()));
    ui.painter().rect_filled(page.translate(vec2(0.0, 2.0)).expand(1.5), 2.0, t.page_shadow);
    ui.painter().rect_filled(page, 0.0, Color32::WHITE);
    let painter = ui.painter().with_clip_rect(area);
    match (shown.tex)(p) {
        Some(id) => {
            painter.image(id, page, g.uv, if d.grayscale { Color32::from_gray(235) } else { Color32::WHITE });
        }
        None => {
            painter.text(page.center(), egui::Align2::CENTER_CENTER, (p + 1).to_string(), theme::regular(13.0), Color32::from_gray(120));
        }
    }
    painter.rect_stroke(page, 0.0, Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
    let current = g.current.and_then(|i| g.tiles.get(i));
    let (cols, rows) = (g.grid.0.to_string(), g.grid.1.to_string());
    let label = match current {
        Some((n, _)) => crate::i18n::fmt(
            tl!("Poster preview: {cols} × {rows} tiles, tile {n} highlighted"),
            &[("cols", &cols), ("rows", &rows), ("n", &n.to_string())],
        ),
        None => crate::i18n::fmt(tl!("Poster preview: {cols} × {rows} tiles"), &[("cols", &cols), ("rows", &rows)]),
    };
    ui.interact(page, ui.id().with("poster-preview"), egui::Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
    for &(number, r) in &g.tiles {
        if current.is_some_and(|c| c.0 == number) {
            painter.rect_filled(r, 0.0, t.accent.gamma_multiply(0.18));
        }
        let ring = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        // A light line under the dashes keeps them visible on dark drawings.
        painter.add(egui::Shape::line(ring.to_vec(), Stroke::new(1.5, Color32::from_white_alpha(170))));
        painter.extend(egui::Shape::dashed_line(&ring, Stroke::new(1.0, Color32::from_gray(60)), 5.0, 3.0));
        // Numbers only where they leave the drawing readable.
        if r.width() >= 44.0 && r.height() >= 30.0 {
            let galley = painter.layout_no_wrap(number.to_string(), theme::medium(10.0), Color32::from_gray(50));
            let chip = Rect::from_min_size(r.left_top() + vec2(3.0, 3.0), galley.size() + vec2(6.0, 2.0));
            painter.rect_filled(chip, 3.0, Color32::from_white_alpha(215));
            painter.galley(chip.min + vec2(3.0, 1.0), galley, Color32::from_gray(50));
        }
    }
    if let Some(&(_, r)) = current {
        painter.rect_stroke(r, 0.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
    }
}

/// The row under the sizing modes: with Window, "Select area…" and the area's size; with Full
/// page, a note (so the dialog keeps its height when the choice changes).
fn area_row(ui: &mut egui::Ui, d: &mut PrintDraft, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        if d.area == PrintArea::FullPage {
            ui.label(egui::RichText::new(tl!("Whole pages print. Choose Window to print an area of each page.")).size(12.0).color(t.text_muted));
            return;
        }
        let hint = tl!("Drag a rectangle over the page. The area prints exactly as drawn; to fill the sheet, hold Shift while drawing it.");
        if widgets::pill_button(ui, tl!("Select area…"), d.region.is_none()).on_hover_text(hint).clicked() {
            d.region_page = d.current_page;
            d.pick = PickView { before: d.region, ..PickView::default() };
            d.picking = true;
        }
        ui.add_space(4.0);
        match d.region {
            Some(r) => {
                ui.label(crate::i18n::fmt(tl!("Area: {area}"), &[("area", &area_label(r))]));
            }
            None => {
                ui.label(egui::RichText::new(tl!("No area yet: whole pages print")).color(t.text_muted));
            }
        }
    });
}

/// The line under the window picker: the area's size and how it prints with the sizing mode
/// chosen (its scale on one sheet with Size, the poster's scale with Poster).
pub fn pick_note(d: &PrintDraft, region: [f64; 4]) -> String {
    let area = area_label(region);
    let one_sheet =
        |pct: f64| crate::i18n::fmt(tl!("Area: {area} · prints at {pct}% on one sheet"), &[("area", &area), ("pct", &format!("{pct:.0}"))]);
    let fit = || window_fit_percent(region, paper(d.paper), d.orientation);
    match d.handling {
        Handling::Size => match d.size {
            SizeMode::Fit => one_sheet(fit()),
            SizeMode::Shrink => one_sheet(fit().min(100.0)),
            SizeMode::Actual => one_sheet(100.0),
            SizeMode::Custom(_) => one_sheet(d.custom_scale),
        },
        Handling::Poster => {
            crate::i18n::fmt(tl!("Area: {area} · prints as a poster at {pct}%"), &[("area", &area), ("pct", &format!("{:.0}", d.poster_scale))])
        }
        Handling::Multiple | Handling::Booklet => crate::i18n::fmt(tl!("Area: {area}"), &[("area", &area)]),
    }
}

/// Leave the picker, putting back the area from before.
fn cancel_pick(d: &mut PrintDraft) {
    d.region = d.pick.before;
    d.picking = false;
    d.pick.drag = None;
}

/// A key drawn as a keycap ("Shift"); filled while `down`.
fn keycap(ui: &mut egui::Ui, t: &Tokens, key: &str, down: bool) {
    let (fill, text) = if down { t.selected_pair() } else { (t.field, t.text) };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, if down { t.accent } else { t.border }))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(key).font(theme::medium(11.5)).color(text));
        });
}

/// The window picker: the page large, with zoom and pan; drag to draw the area, drag inside it
/// to move it, drag a corner to resize it; Shift keeps the sheet's proportions.
fn picker(
    ui: &mut egui::Ui,
    d: &mut PrintDraft,
    t: &Tokens,
    sizes: &[(f64, f64)],
    tex: &dyn Fn(usize) -> Option<egui::TextureId>,
    wants: &mut Vec<(usize, f32)>,
) {
    let Some(&(dw, dh)) = sizes.get(d.region_page) else {
        d.picking = false;
        return;
    };
    // Escape leaves the picker (as Cancel does), not the whole dialog: the key is taken here,
    // before the dialog looks for it.
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        cancel_pick(d);
        return;
    }
    let width = ui.available_width().max(600.0);
    ui.set_width(width);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Select the area to print")).font(theme::semibold(18.0)));
        ui.add_space(12.0);
        if sizes.len() > 1 {
            if ui.add_enabled(d.region_page > 0, egui::Button::new("‹")).on_hover_text(tl!("Previous page")).clicked() {
                d.region_page = d.region_page.saturating_sub(1);
            }
            ui.label(crate::i18n::fmt(tl!("Page {p} of {n}"), &[("p", &(d.region_page + 1).to_string()), ("n", &sizes.len().to_string())]));
            if ui.add_enabled(d.region_page + 1 < sizes.len(), egui::Button::new("›")).on_hover_text(tl!("Next page")).clicked() {
                d.region_page += 1;
            }
        }
    });
    ui.label(
        egui::RichText::new(tl!(
            "Drag to draw the area; drag inside it to move it, or drag a corner to resize it. Scroll to zoom; drag with the right or middle button to pan."
        ))
        .color(t.text_muted),
    );
    let shift = ui.input(|i| i.modifiers.shift);
    let paper_name = PAPER_CHOICES.get(d.paper).map_or("A4", |p| p.0);
    ui.horizontal(|ui| {
        keycap(ui, t, "Shift", shift);
        ui.label(
            egui::RichText::new(crate::i18n::fmt(
                tl!("Hold Shift while dragging to keep the {paper} sheet's proportions, so the area fills the sheet."),
                &[("paper", paper_name)],
            ))
            .color(if shift { t.text } else { t.text_muted }),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tl!("Fit page")).on_hover_text(tl!("Show the whole page")).clicked() {
                d.pick.zoom = 1.0;
                d.pick.pan = egui::Vec2::ZERO;
            }
        });
    });
    ui.add_space(6.0);
    let screen_h = ui.ctx().content_rect().height();
    let height = (screen_h - 300.0).clamp(320.0, 1100.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tl!("Print area")));
    ui.painter().rect_filled(rect, 6.0, t.preview_fill);
    // Zoom (1 = the page fits) and pan.
    let fit = ((rect.width() - 32.0) / dw as f32).min((rect.height() - 32.0) / dh as f32).max(0.01);
    if resp.hovered() {
        let (scroll, pinch, pointer) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
        let factor = pinch * (scroll * 0.002).exp();
        if (factor - 1.0).abs() > 1e-4 && factor.is_finite() {
            let old = d.pick.zoom;
            let new = (old * factor).clamp(1.0, 64.0);
            // Keep the point under the pointer where it is.
            if let Some(p) = pointer {
                let c = rect.center() + d.pick.pan;
                d.pick.pan += (p - c) * (1.0 - new / old);
            }
            d.pick.zoom = new;
        }
    }
    if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
        d.pick.pan += resp.drag_delta();
    }
    if d.pick.zoom <= 1.0 {
        d.pick.pan = egui::Vec2::ZERO;
    }
    let z = fit * d.pick.zoom;
    let page_rect = Rect::from_center_size(rect.center() + d.pick.pan, vec2(dw as f32 * z, dh as f32 * z));
    let to_screen = |x: f64, y: f64| pos2(page_rect.left() + x as f32 * z, page_rect.top() + (dh - y) as f32 * z);
    let to_page = |p: Pos2| (((p.x - page_rect.left()) / z) as f64, dh - ((p.y - page_rect.top()) / z) as f64);
    wants.push((d.region_page, z * ui.ctx().pixels_per_point()));
    // Gestures (primary button). Shift is read every frame, so pressing or releasing it during a
    // drag reshapes the area at once.
    let orientation = d.orientation;
    let paper_size = paper(d.paper);
    let aspect_for = |w: f64, h: f64| shift.then(|| printable_aspect(paper_size, window_landscape(orientation, w, h)));
    // The area's corners on screen, each with the opposite corner (page space) and its cursor.
    let corners = |region: Option<[f64; 4]>| {
        region.map(norm).map(|r| {
            [
                (to_screen(r[0], r[3]), (r[2], r[1]), egui::CursorIcon::ResizeNwSe),
                (to_screen(r[2], r[3]), (r[0], r[1]), egui::CursorIcon::ResizeNeSw),
                (to_screen(r[0], r[1]), (r[2], r[3]), egui::CursorIcon::ResizeNeSw),
                (to_screen(r[2], r[1]), (r[0], r[3]), egui::CursorIcon::ResizeNwSe),
            ]
        })
    };
    let corner_at = |region: Option<[f64; 4]>, p: Pos2| corners(region).and_then(|cs| cs.into_iter().find(|c| c.0.distance(p) <= 8.0));
    let inside_at =
        |region: Option<[f64; 4]>, at: (f64, f64)| region.map(norm).is_some_and(|r| at.0 >= r[0] && at.0 <= r[2] && at.1 >= r[1] && at.1 <= r[3]);
    // Where the button went down: the drag starts there, not where it passed egui's threshold.
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
    {
        let at = to_page(p);
        d.pick.drag = Some(match (corner_at(d.region, p), inside_at(d.region, at), d.region) {
            // A corner: draw again from the opposite one (a resize).
            (Some((_, opposite, _)), _, _) => PickDrag::Draw { start: opposite },
            (None, true, Some(from)) => PickDrag::Move { grab: at, from },
            _ => PickDrag::Draw { start: (at.0.clamp(0.0, dw), at.1.clamp(0.0, dh)) },
        });
    }
    if resp.dragged_by(egui::PointerButton::Primary)
        && let (Some(drag), Some(p)) = (d.pick.drag, resp.interact_pointer_pos())
    {
        let at = to_page(p);
        match drag {
            PickDrag::Draw { start } => {
                let (w, h) = ((at.0 - start.0).abs(), (at.1 - start.1).abs());
                // A click (no drag yet) doesn't replace the area.
                if w * z as f64 > 3.0 || h * z as f64 > 3.0 {
                    d.region = Some(drag_window(start, at, (dw, dh), aspect_for(w, h)));
                }
            }
            PickDrag::Move { grab, from } => d.region = Some(move_window(from, at.0 - grab.0, at.1 - grab.1, (dw, dh))),
        }
    }
    if resp.drag_stopped() {
        d.pick.drag = None;
        if let Some(r) = d.region
            && (r[2] - r[0] < print::MIN_REGION || r[3] - r[1] < print::MIN_REGION)
        {
            d.region = None;
        }
    }
    // The page (on its shadow), the area and the dimmed rest.
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(page_rect.translate(vec2(0.0, 3.0)).expand(2.0), 3.0, t.page_shadow);
    match tex(d.region_page) {
        Some(id) => {
            painter.image(id, page_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            painter.rect_filled(page_rect, 0.0, Color32::WHITE);
            painter.text(page_rect.center(), egui::Align2::CENTER_CENTER, tl!("Drawing the page…"), theme::regular(13.0), Color32::from_gray(140));
        }
    }
    painter.rect_stroke(page_rect, 0.0, Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
    if let Some(r) = d.region {
        let sel = Rect::from_two_pos(to_screen(r[0], r[1]), to_screen(r[2], r[3]));
        let dim = Color32::from_black_alpha(140);
        for part in [
            Rect::from_min_max(page_rect.min, pos2(page_rect.right(), sel.top())),
            Rect::from_min_max(pos2(page_rect.left(), sel.bottom()), page_rect.max),
            Rect::from_min_max(pos2(page_rect.left(), sel.top()), pos2(sel.left(), sel.bottom())),
            Rect::from_min_max(pos2(sel.right(), sel.top()), pos2(page_rect.right(), sel.bottom())),
        ] {
            if part.is_positive() {
                painter.rect_filled(part, 0.0, dim);
            }
        }
        // White inside, accent outside: the outline shows on any drawing.
        painter.rect_stroke(sel, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
        painter.rect_stroke(sel, 0.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Outside);
        for (c, _, _) in corners(Some(r)).into_iter().flatten() {
            painter.rect(Rect::from_center_size(c, vec2(10.0, 10.0)), 2.0, Color32::WHITE, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
        }
        // The area's size on a badge above it (inside it at the top of the canvas).
        let (fill, text) = t.selected_pair();
        let galley = painter.layout_no_wrap(area_label(r), theme::medium(11.5), text);
        let size = galley.size() + vec2(12.0, 6.0);
        let mut at = pos2(sel.left(), sel.top() - size.y - 8.0);
        if at.y < rect.top() + 4.0 {
            at = sel.left_top() + vec2(10.0, 10.0);
        }
        let chip = Rect::from_min_size(at, size);
        painter.rect(chip, 4.0, fill, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        painter.galley(chip.min + vec2(6.0, 3.0), galley, text);
    }
    if resp.hovered() {
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let icon = match (d.pick.drag, pointer.and_then(|p| corner_at(d.region, p))) {
            (Some(PickDrag::Move { .. }), _) => egui::CursorIcon::Grabbing,
            (None, Some((_, _, icon))) => icon,
            (None, None) if pointer.is_some_and(|p| inside_at(d.region, to_page(p))) => egui::CursorIcon::Move,
            _ => egui::CursorIcon::Crosshair,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        match d.region {
            Some(r) => {
                ui.label(pick_note(d, r));
            }
            None => {
                ui.label(egui::RichText::new(tl!("No area: drag over the page")).color(t.text_muted));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled_ui(d.region.is_some(), |ui| widgets::pill_button(ui, tl!("Use this area"), true)).inner.clicked() {
                d.picking = false;
                d.pick.drag = None;
            }
            if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
                cancel_pick(d);
            }
        });
    });
}
