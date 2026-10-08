//! Edit a PDF ▸ Edit text & images: boxes around the paragraphs and images already on the page.
//! Click a paragraph to edit it in place (⌘Enter, clicking away or Esc applies it and rewraps it
//! to the box; ⌘Z takes it back); drag it to move it, or drag the handle on its right edge to
//! rewrap it to a new width. Click an image to select it: drag to move, drag a corner to resize
//! (keeping its proportions), right-click for rotate, flip, replace, save and delete.
//!
//! Several boxes at once, as in Acrobat: drag on empty page space to draw a selection rectangle
//! (every paragraph and image it touches is selected, not only the ones inside it), ⇧- or
//! ⌘/Ctrl-click a box to add or remove it, ⌘A / Ctrl+A selects every box on the page. Delete or
//! Backspace deletes the selection as one undoable step. A selection is on one page. Esc leaves
//! the tool (after applying the text being typed).

use std::collections::BTreeSet;

use egui::{Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Stroke};
use printcraft_engine::Edit;
use printcraft_render::DocInfo;

use crate::canvas::{DocView, PageXform};
use crate::{PrintCraftApp, QuickTool};

const ACCENT: Color32 = Color32::from_rgb(0x14, 0x73, 0xE6);

/// The inline paragraph editor's text field.
const EDITOR_ID: &str = "edit-text-line-input";

/// The inline paragraph editor's widget id (for keyboard focus checks).
pub(crate) fn editor_id() -> egui::Id {
    egui::Id::new(EDITOR_ID)
}

/// Paragraphs and images selected together (a selection rectangle, ⇧/⌘-clicks, ⌘A), all on one
/// page. Indexes are into `text_blocks` and `page_images` as they were when the boxes were
/// picked: any change to the document clears the selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxSelection {
    pub page: usize,
    pub blocks: BTreeSet<usize>,
    pub images: BTreeSet<usize>,
}

impl BoxSelection {
    pub fn new(page: usize) -> Self {
        BoxSelection { page, ..Default::default() }
    }

    /// How many boxes are selected.
    pub fn len(&self) -> usize {
        self.blocks.len().saturating_add(self.images.len())
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.images.is_empty()
    }

    fn toggle_block(&mut self, b: usize) {
        if !self.blocks.remove(&b) {
            self.blocks.insert(b);
        }
    }

    fn toggle_image(&mut self, i: usize) {
        if !self.images.remove(&i) {
            self.images.insert(i);
        }
    }

    /// The image, when the selection is one image and nothing else.
    fn single_image(&self) -> Option<usize> {
        if self.blocks.is_empty() && self.images.len() == 1 { self.images.first().copied() } else { None }
    }
}

/// The selection a ⇧/⌘-click or ⇧-marquee on `page` adds to: the current one when it is on that
/// page (with the image selected alone, if any), else nothing yet.
fn seeded(view: &DocView, page: usize) -> BoxSelection {
    let mut s = view.edit_selection.clone().filter(|s| s.page == page).unwrap_or_else(|| BoxSelection::new(page));
    if let Some(im) = view.image_selection.as_ref().filter(|im| im.page == page) {
        s.images.insert(im.index);
    }
    s
}

/// Make `s` the selection. One image alone also gets the move and resize handles; anything else
/// selected (an added item, a comment) is deselected, so Delete deletes the boxes.
pub(crate) fn set_selection(view: &mut DocView, s: BoxSelection) {
    view.image_selection = s.single_image().map(|index| ImageSelection { page: s.page, index, drag: None });
    view.edit_selection = (!s.is_empty()).then_some(s);
    view.content.selected = None;
    view.comments.selected = None;
}

/// The paragraphs' boxes on screen.
fn block_boxes(xf: &PageXform, info: &DocInfo, page: usize, lines: &[printcraft_engine::TextBlock]) -> Vec<Rect> {
    lines.iter().map(|l| xf.user_rect(info, page, l.rect.map(|v| v as f32)).expand(2.0)).collect()
}

/// The images' boxes on screen.
fn image_boxes(xf: &PageXform, info: &DocInfo, page: usize, images: &[printcraft_engine::PageImage]) -> Vec<Rect> {
    images.iter().map(|im| xf.user_rect(info, page, im.rect.map(|v| v as f32))).collect()
}

/// A selected box: tinted and framed in the accent colour.
fn paint_selected(painter: &egui::Painter, r: Rect) {
    painter.rect_filled(r, CornerRadius::same(2), ACCENT.gamma_multiply(0.16));
    painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(2.0, ACCENT), egui::StrokeKind::Outside);
}

/// The selection rectangle being dragged on `page`, if any (screen; kept on the page).
fn marquee_rect(ui: &egui::Ui, view: &DocView, page: usize, xf: &PageXform) -> Option<Rect> {
    let (_, start) = view.edit_marquee.filter(|(p, _)| *p == page)?;
    let end = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())).unwrap_or(start);
    let end = Pos2::new(end.x.clamp(xf.rect.left(), xf.rect.right()), end.y.clamp(xf.rect.top(), xf.rect.bottom()));
    Some(Rect::from_two_pos(start, end))
}

/// Whether the box at `i` shows as selected: picked, or touched by the rectangle being dragged.
fn shows_selected(picked: Option<&BTreeSet<usize>>, live: Option<Rect>, i: usize, b: Rect) -> bool {
    picked.is_some_and(|set| set.contains(&i)) || live.is_some_and(|r| r.intersects(b))
}

/// ⇧ or ⌘/Ctrl held: clicks and rectangles add to the selection (or take a box out of it).
fn extending(ui: &egui::Ui) -> bool {
    ui.input(|i| i.modifiers.shift || i.modifiers.command)
}

/// Drag on empty page space to select every paragraph and image the rectangle touches; a click
/// there clears the selection. Returns `true` while a rectangle is being dragged on this page
/// (the boxes then only paint).
#[allow(clippy::too_many_arguments)]
pub(crate) fn marquee_input(
    ui: &egui::Ui,
    resp: &egui::Response,
    xf: &PageXform,
    page: usize,
    info: &DocInfo,
    lines: &[printcraft_engine::TextBlock],
    images: &[printcraft_engine::PageImage],
    view: &mut DocView,
) -> bool {
    let blocks = block_boxes(xf, info, page, lines);
    let pics = image_boxes(xf, info, page, images);
    if let Some(r) = marquee_rect(ui, view, page, xf) {
        let painter = ui.painter();
        painter.rect_filled(r, CornerRadius::ZERO, ACCENT.gamma_multiply(0.08));
        painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            view.edit_marquee = None;
            let extend = extending(ui);
            // A rectangle too small to mean anything is a click.
            if r.width() < 3.0 && r.height() < 3.0 && !extend {
                set_selection(view, BoxSelection::new(page));
                return true;
            }
            let mut s = if extend { seeded(view, page) } else { BoxSelection::new(page) };
            // Touching is enough (a box doesn't have to be inside the rectangle), as in Acrobat.
            s.blocks.extend(blocks.iter().enumerate().filter(|(_, b)| r.intersects(**b)).map(|(i, _)| i));
            s.images.extend(pics.iter().enumerate().filter(|(_, b)| r.intersects(**b)).map(|(i, _)| i));
            set_selection(view, s);
        }
        return true;
    }
    // Over a box (a paragraph, its width handle, an image, the selected image's corner handles),
    // the pointer is the box's: it moves, resizes or opens it.
    let upright = info.pages.get(page).is_some_and(|p| p.rotation % 180 == 0);
    let selected_image = view.image_selection.as_ref().filter(|s| s.page == page).and_then(|s| pics.get(s.index)).copied();
    let over_box = |p: Pos2| {
        blocks.iter().any(|b| b.contains(p) || (upright && width_handle(*b).contains(p)))
            || pics.iter().any(|b| b.contains(p))
            || selected_image.is_some_and(|b| [b.left_top(), b.right_top(), b.left_bottom(), b.right_bottom()].iter().any(|c| c.distance(p) < 8.0))
    };
    if resp.drag_started()
        && view.block_drag.is_none()
        && let Some(o) = ui.input(|i| i.pointer.press_origin()).filter(|o| xf.rect.contains(*o) && !over_box(*o))
    {
        view.edit_marquee = Some((page, o));
        return true;
    }
    // A plain click on empty page space: nothing is selected any more.
    if resp.clicked() && !extending(ui) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| xf.rect.contains(p) && !over_box(p)) {
        set_selection(view, BoxSelection::new(page));
    }
    false
}

/// Delete or Backspace (no text field focused, nothing else selected): delete the selected
/// boxes as one step. A finished gesture whose release was missed (the page scrolled away)
/// is dropped here too.
pub(crate) fn keys(ctx: &egui::Context, view: &mut DocView) {
    if view.edit_marquee.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
        view.edit_marquee = None;
    }
    if view.line_editor.is_some()
        || view.edit_marquee.is_some()
        || view.content.selected.is_some()
        || view.comments.selected.is_some()
        || ctx.egui_wants_keyboard_input()
    {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
        && let Some(e) = delete_selected(view)
    {
        view.pending_edit = Some(e);
    }
}

/// The edit that deletes the selected boxes (and clears the selection): one undo step (see
/// [`Edit::delete_boxes`]). The image selected alone counts when it is on the selection's page.
pub(crate) fn delete_selected(view: &mut DocView) -> Option<Edit> {
    let mut sel = view.edit_selection.take().unwrap_or_default();
    if let Some(im) = view.image_selection.take() {
        if sel.is_empty() {
            sel.page = im.page;
        }
        if im.page == sel.page {
            sel.images.insert(im.index);
        }
    }
    Edit::delete_boxes(sel.page, &sel.blocks, &sel.images)
}

/// A paragraph being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct LineEditor {
    pub page: usize,
    pub block: usize,
    pub text: String,
    original: String,
    rect: Rect,
    /// The original PDF rectangle, in user space. It is converted again each frame so the editor
    /// stays attached while the page is zoomed, scrolled, or rotated.
    source_rect: [f32; 4],
    multiline: bool,
    /// How far the editor box may grow to the right (screen pixels): a single-line paragraph
    /// rewraps growing to the page's edge, a multi-line one keeps its width.
    max_width: f32,
    /// Current screen-space size. Recomputed from the current page transform before painting.
    size: f32,
    focus: bool,
    /// The Format text panel's values, and what the paragraph had (to send only changes).
    pub look: printcraft_engine::AddedText,
    look0: printcraft_engine::AddedText,
    /// Underline, line spacing (× size; 0 = the paragraph's own), character spacing (pt) and
    /// horizontal scale (%), and what they were.
    pub extras: Extras,
    extras0: Extras,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extras {
    pub underline: bool,
    pub line_spacing: f64,
    pub char_spacing: f64,
    pub scale: f64,
}

impl Default for Extras {
    fn default() -> Self {
        Extras { underline: false, line_spacing: 0.0, char_spacing: 0.0, scale: 100.0 }
    }
}

/// Underline, line spacing, character spacing and horizontal scale for the paragraph being
/// edited (under Format text). Returns `true` when something changed.
pub(crate) fn extras_panel(ui: &mut egui::Ui, e: &mut Extras) -> bool {
    let before = *e;
    ui.horizontal(|ui| {
        if crate::icons::button(ui, "underline", 26.0, e.underline, "Underline").clicked() {
            e.underline = !e.underline;
        }
        let label = |v: f64| if v == 0.0 { "Line spacing".to_string() } else { format!("{v:.2}×") };
        egui::ComboBox::from_id_salt("line-spacing").selected_text(label(e.line_spacing)).width(110.0).show_ui(ui, |ui| {
            for v in [1.0, 1.15, 1.5, 2.0] {
                ui.selectable_value(&mut e.line_spacing, v, label(v));
            }
        });
    });
    ui.horizontal(|ui| {
        let l = ui.label("Character spacing");
        ui.add(egui::DragValue::new(&mut e.char_spacing).range(-5.0..=50.0).speed(0.1).suffix(" pt")).labelled_by(l.id);
    });
    ui.horizontal(|ui| {
        let l = ui.label("Horizontal scale");
        ui.add(egui::DragValue::new(&mut e.scale).range(10.0..=400.0).speed(1.0).suffix(" %")).labelled_by(l.id);
    });
    *e != before
}

impl LineEditor {
    /// How far the box may grow to the right, when the paragraph is a single line (a multi-line
    /// paragraph rewraps to its own width and the box doesn't grow).
    pub fn growth(&self) -> Option<f32> {
        (self.max_width > self.rect.width()).then_some(self.max_width)
    }

    /// The formatting the panel changed.
    pub fn style(&self) -> printcraft_engine::BlockStyle {
        let (l, o) = (&self.look, &self.look0);
        printcraft_engine::BlockStyle {
            family: (l.family != o.family || l.bold != o.bold || l.italic != o.italic).then_some((l.family, l.bold, l.italic)),
            size: (l.size != o.size).then_some(l.size),
            color: (l.color != o.color).then_some(l.color),
            align: (l.align != o.align).then_some(l.align),
            underline: (self.extras.underline != self.extras0.underline).then_some(self.extras.underline),
            line_spacing: (self.extras.line_spacing != self.extras0.line_spacing && self.extras.line_spacing > 0.0)
                .then_some(self.extras.line_spacing),
            char_spacing: (self.extras.char_spacing != self.extras0.char_spacing).then_some(self.extras.char_spacing),
            scale: (self.extras.scale != self.extras0.scale).then_some(self.extras.scale),
            ..Default::default()
        }
    }

    /// After applying formatting: the paragraph now has it.
    pub fn applied(&mut self) {
        self.look0 = self.look.clone();
        self.extras0 = self.extras;
        self.original = self.text.clone();
        self.focus = true;
    }

    /// Adopt the rewritten paragraph's current geometry before the next overlay frame.
    pub(crate) fn refresh_source(&mut self, block: &printcraft_engine::TextBlock) {
        self.source_rect = block.rect.map(|v| v as f32);
        self.multiline = block.lines.len() > 1;
    }
}

/// The look shown for a paragraph: the family and weight guessed from its PDF font name.
fn source_look(base_font: &str, size: f64, color: [f64; 3], detected_bold: bool, detected_italic: bool) -> printcraft_engine::AddedText {
    use printcraft_engine::FontFamily as F;
    let name = base_font.to_ascii_lowercase();
    let family = if ["courier", "mono", "consolas", "menlo", "monaco", "lucida console"].iter().any(|s| name.contains(s)) {
        F::Courier
    } else if !name.contains("sans") && ["times", "serif", "roman", "cambria", "georgia", "palatino", "garamond"].iter().any(|s| name.contains(s)) {
        F::Times
    } else {
        F::Helvetica
    };
    printcraft_engine::AddedText {
        family,
        bold: detected_bold || ["bold", "black", "heavy", "semibold", "demi"].iter().any(|s| name.contains(s)),
        italic: detected_italic || ["italic", "oblique", "slanted"].iter().any(|s| name.contains(s)),
        size: (size * 10.0).round() / 10.0,
        color,
        ..Default::default()
    }
}

fn look_of(b: &printcraft_engine::TextBlock) -> printcraft_engine::AddedText {
    source_look(&b.base_font, b.size, b.color, b.bold, b.italic)
}

fn editor_font(look: &printcraft_engine::AddedText, size: f32) -> FontId {
    let family = match (look.family, look.bold) {
        (printcraft_engine::FontFamily::Courier, _) => FontFamily::Monospace,
        (_, true) => FontFamily::Name("semibold".into()),
        (_, false) => FontFamily::Proportional,
    };
    FontId::new(size, family)
}

fn color32(color: [f64; 3]) -> Color32 {
    Color32::from_rgb(
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// A selected page image, and what the pointer is doing to it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageSelection {
    pub page: usize,
    pub index: usize,
    /// Dragging: the start point and, for a corner, the opposite corner (screen).
    drag: Option<(Pos2, Option<Pos2>)>,
}

/// A paragraph box being dragged: moved, or (from the handle on its right edge) resized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDrag {
    page: usize,
    block: usize,
    /// Where the drag started (screen).
    start: Pos2,
    resize: bool,
}

/// The resize handle on a paragraph box's right edge (screen).
fn width_handle(b: Rect) -> Rect {
    Rect::from_center_size(Pos2::new(b.right(), b.center().y), egui::vec2(7.0, 14.0))
}

/// What a right-click on a selected image asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ImageAction {
    Replace(usize, usize),
    Save(usize, usize),
}

fn user_box(xf: &PageXform, info: &DocInfo, page: usize, r: Rect) -> [f64; 4] {
    let p = &info.pages[page];
    let (a, b) = (xf.screen_to_view(r.min), xf.screen_to_view(r.max));
    let (u, v) = (p.view_to_user(a.0, a.1), p.view_to_user(b.0, b.1));
    [u[0].min(v[0]) as f64, u[1].min(v[1]) as f64, u[0].max(v[0]) as f64, u[1].max(v[1]) as f64]
}

/// Images on a page: select, move, resize, right-click. Returns `true` when the pointer was used.
/// With `interact` false (a selection rectangle or a paragraph drag is in progress) the boxes
/// only paint.
#[allow(clippy::too_many_arguments)]
pub(crate) fn image_input(
    ui: &egui::Ui,
    resp: &egui::Response,
    xf: &PageXform,
    page: usize,
    info: &DocInfo,
    images: &[printcraft_engine::PageImage],
    view: &mut DocView,
    action: &mut Option<ImageAction>,
    interact: bool,
) -> bool {
    let boxes = image_boxes(xf, info, page, images);
    let painter = ui.painter();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let selected = view.image_selection.as_ref().filter(|s| s.page == page).map(|s| s.index).filter(|i| *i < boxes.len());
    let picked = view.edit_selection.as_ref().filter(|s| s.page == page).map(|s| &s.images);
    let live = marquee_rect(ui, view, page, xf);
    for (i, b) in boxes.iter().enumerate() {
        let hovered = interact && pointer.is_some_and(|p| b.contains(p));
        if selected == Some(i) || shows_selected(picked, live, i, *b) {
            paint_selected(painter, *b);
        } else {
            let stroke = if hovered { Stroke::new(1.5, ACCENT.gamma_multiply(0.7)) } else { Stroke::new(0.75, ACCENT.gamma_multiply(0.35)) };
            painter.rect_stroke(*b, CornerRadius::ZERO, stroke, egui::StrokeKind::Outside);
        }
    }
    // The image selected alone: corner handles, dragging.
    if let Some((i, b)) = selected.and_then(|i| boxes.get(i).map(|b| (i, *b))) {
        let corners = [b.left_top(), b.right_top(), b.left_bottom(), b.right_bottom()];
        for c in corners {
            painter.rect(
                Rect::from_center_size(c, egui::vec2(8.0, 8.0)),
                CornerRadius::ZERO,
                Color32::WHITE,
                Stroke::new(1.0, ACCENT),
                egui::StrokeKind::Middle,
            );
        }
        if !interact {
            return false;
        }
        let origin = ui.input(|i| i.pointer.press_origin());
        if resp.drag_started()
            && !extending(ui)
            && let Some(o) = origin
        {
            let corner = corners.iter().position(|c| c.distance(o) < 8.0);
            if corner.is_some() || b.contains(o) {
                let opposite = corner.and_then(|k| corners.get(3 - k).copied());
                if let Some(s) = view.image_selection.as_mut() {
                    s.drag = Some((o, opposite));
                }
            }
        }
        if let Some((start, opposite)) = view.image_selection.as_ref().and_then(|s| s.drag)
            && let Some(p) = pointer
        {
            let preview = match opposite {
                // Resize from the opposite corner, keeping the aspect ratio.
                Some(fixed) => {
                    let (w0, h0) = (b.width().max(1.0), b.height().max(1.0));
                    let k = ((p.x - fixed.x).abs() / w0).max((p.y - fixed.y).abs() / h0).max(0.05);
                    let (w, h) = (w0 * k, h0 * k);
                    let x = if p.x < fixed.x { fixed.x - w } else { fixed.x };
                    let y = if p.y < fixed.y { fixed.y - h } else { fixed.y };
                    Rect::from_min_size(Pos2::new(x, y), egui::vec2(w, h))
                }
                None => b.translate(p - start),
            };
            painter.rect_stroke(preview, CornerRadius::ZERO, Stroke::new(1.0, ACCENT), egui::StrokeKind::Middle);
            if resp.drag_stopped() {
                if let Some(s) = view.image_selection.as_mut() {
                    s.drag = None;
                }
                if preview != b {
                    view.pending_edit =
                        Some(Edit::EditPageImage { page, index: i, change: printcraft_engine::ImageEdit::Move(user_box(xf, info, page, preview)) });
                }
            }
            return true;
        }
    }
    if !interact {
        return false;
    }
    let Some(p) = pointer.filter(|p| xf.rect.contains(*p)) else { return false };
    let Some((hit, hit_box)) = boxes.iter().enumerate().rev().find(|(_, b)| b.contains(p)).map(|(i, b)| (i, *b)) else { return false };
    if selected != Some(hit) && !shows_selected(picked, None, hit, hit_box) {
        painter.rect_stroke(hit_box, CornerRadius::ZERO, Stroke::new(1.5, ACCENT.gamma_multiply(0.7)), egui::StrokeKind::Outside);
    }
    ui.ctx().set_cursor_icon(if selected == Some(hit) { egui::CursorIcon::Move } else { egui::CursorIcon::PointingHand });
    if resp.clicked() && extending(ui) {
        // ⇧/⌘-click: in or out of the selection.
        let mut s = seeded(view, page);
        s.toggle_image(hit);
        set_selection(view, s);
        return true;
    }
    // Right-clicking an image that is part of a larger selection keeps the selection.
    let in_group = view.edit_selection.as_ref().is_some_and(|s| s.page == page && s.len() > 1 && s.images.contains(&hit));
    if resp.clicked() || (resp.secondary_clicked() && !in_group) {
        let mut s = BoxSelection::new(page);
        s.images.insert(hit);
        set_selection(view, s);
    }
    if selected == Some(hit) || in_group {
        resp.context_menu(|ui| {
            use printcraft_engine::ImageEdit as E;
            if in_group {
                let n = view.edit_selection.as_ref().map_or(0, BoxSelection::len);
                if ui.button(format!("Delete {n} Selected Items")).clicked() {
                    view.pending_edit = delete_selected(view);
                    ui.close();
                }
                return;
            }
            let items: [(&str, Option<E>); 4] = [
                ("Rotate Clockwise", Some(E::Rotate(1))),
                ("Rotate Counterclockwise", Some(E::Rotate(3))),
                ("Flip Horizontal", Some(E::Flip { horizontal: true })),
                ("Flip Vertical", Some(E::Flip { horizontal: false })),
            ];
            for (label, change) in items {
                if ui.button(label).clicked() {
                    view.pending_edit = change.map(|c| Edit::EditPageImage { page, index: hit, change: c });
                    ui.close();
                }
            }
            if ui.button("Replace Image…").clicked() {
                *action = Some(ImageAction::Replace(page, hit));
                ui.close();
            }
            if ui.button("Save Image As…").clicked() {
                *action = Some(ImageAction::Save(page, hit));
                ui.close();
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                view.image_selection = None;
                view.edit_selection = None;
                view.pending_edit = Some(Edit::EditPageImage { page, index: hit, change: E::Delete });
                ui.close();
            }
        });
    }
    true
}

/// Lines and their screen boxes for a page; hover outlines, click opens the editor (⇧/⌘-click
/// selects instead). Returns `true` when the pointer was used. With `interact` false (a
/// selection rectangle is being dragged, or an image has the pointer) the boxes only paint.
#[allow(clippy::too_many_arguments)]
pub(crate) fn page_input(
    ui: &egui::Ui,
    resp: &egui::Response,
    xf: &PageXform,
    page: usize,
    info: &DocInfo,
    lines: &[printcraft_engine::TextBlock],
    view: &mut DocView,
    interact: bool,
) -> bool {
    let boxes = block_boxes(xf, info, page, lines);
    let painter = ui.painter();
    let active = view.line_editor.as_ref().filter(|e| e.page == page).map(|e| e.block);
    let picked = view.edit_selection.as_ref().filter(|s| s.page == page).map(|s| &s.blocks);
    let live = marquee_rect(ui, view, page, xf);
    for (i, b) in boxes.iter().enumerate() {
        if active == Some(i) {
            painter.rect_filled(b.expand(1.0), CornerRadius::same(2), ACCENT.gamma_multiply(0.08));
            painter.rect_stroke(*b, CornerRadius::same(2), Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
        } else if shows_selected(picked, live, i, *b) {
            paint_selected(painter, *b);
        } else {
            painter.rect_stroke(*b, CornerRadius::same(2), Stroke::new(0.75, ACCENT.gamma_multiply(0.35)), egui::StrokeKind::Outside);
        }
    }
    if !interact {
        return false;
    }
    // The width handle needs the page upright (or upside down): on a quarter-turned page the
    // screen's horizontal is the paragraph's vertical.
    let upright = info.pages.get(page).is_some_and(|p| p.rotation % 180 == 0);
    // A drag in progress: the box follows the pointer (or its right edge does); releasing applies it.
    if let Some(d) = view.block_drag.filter(|d| d.page == page)
        && let (Some(b), Some(l)) = (boxes.get(d.block).copied(), lines.get(d.block))
    {
        let p = ui.input(|i| i.pointer.interact_pos()).unwrap_or(d.start);
        let preview = if d.resize {
            Rect::from_min_max(b.min, Pos2::new((b.right() + p.x - d.start.x).max(b.left() + 12.0), b.max.y))
        } else {
            b.translate(p - d.start)
        };
        painter.rect_stroke(preview, CornerRadius::same(2), Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
        ui.ctx().set_cursor_icon(if d.resize { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Grabbing });
        if resp.drag_stopped() || !ui.input(|i| i.pointer.any_down()) {
            view.block_drag = None;
            let (from, to) = (user_box(xf, info, page, b), user_box(xf, info, page, preview));
            let style = if d.resize {
                // The new width in user space: the box's change, added to the paragraph's own.
                let width = (l.rect[2] - l.rect[0]) + (to[2] - to[0]) - (from[2] - from[0]);
                printcraft_engine::BlockStyle { width: Some(width), ..Default::default() }
            } else {
                printcraft_engine::BlockStyle { offset: Some([to[0] - from[0], to[1] - from[1]]), ..Default::default() }
            };
            if preview != b {
                view.pending_edit = Some(Edit::EditTextBlock { page, block: d.block, text: l.text.clone(), style });
            }
        }
        return true;
    }
    let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| xf.rect.contains(*p)) else { return false };
    let Some((hit, hit_box)) =
        boxes.iter().enumerate().rev().find(|(_, b)| b.contains(p) || (upright && width_handle(**b).contains(p))).map(|(i, b)| (i, *b))
    else {
        return false;
    };
    let Some(l) = lines.get(hit) else { return false };
    if active != Some(hit) && !shows_selected(picked, None, hit, hit_box) {
        painter.rect_stroke(hit_box, CornerRadius::same(2), Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
    }
    let on_handle = upright && active.is_none() && width_handle(hit_box).contains(p);
    if upright && active.is_none() {
        painter.rect(width_handle(hit_box), CornerRadius::same(2), Color32::WHITE, Stroke::new(1.0, ACCENT), egui::StrokeKind::Middle);
    }
    ui.ctx().set_cursor_icon(if on_handle { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Text });
    // Dragging a box (not while a paragraph is open for typing) moves it; from the handle, resizes it.
    if resp.drag_started()
        && active.is_none()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some(block) = boxes.iter().rposition(|b| b.contains(o) || (upright && width_handle(*b).contains(o)))
    {
        let resize = upright && boxes.get(block).is_some_and(|b| width_handle(*b).contains(o));
        view.block_drag = Some(BlockDrag { page, block, start: o, resize });
        return true;
    }
    if resp.clicked() && extending(ui) {
        // ⇧/⌘-click: in or out of the selection, without opening it for typing.
        let mut s = seeded(view, page);
        s.toggle_block(hit);
        set_selection(view, s);
        return true;
    }
    if resp.clicked() {
        set_selection(view, BoxSelection::new(page));
        // Screen pixels per point, from the box's width.
        let scale = (hit_box.width() - 4.0) / ((l.rect[2] - l.rect[0]).max(1.0) as f32);
        // How far the editor box may grow: a single-line paragraph's rewrite grows to the
        // page's right edge, a multi-line paragraph rewraps to its own width.
        let right = xf.rect.right().min(view.viewport_rect().right()) - 6.0;
        let max_width = if l.lines.len() == 1 { (right - hit_box.left()).max(hit_box.width()) } else { hit_box.width() };
        view.line_editor = Some(LineEditor {
            page,
            block: hit,
            text: l.text.clone(),
            original: l.text.clone(),
            rect: hit_box,
            source_rect: l.rect.map(|v| v as f32),
            multiline: l.lines.len() > 1,
            max_width,
            size: (l.size as f32 * scale).clamp(8.0, 72.0),
            focus: true,
            look: look_of(l),
            look0: look_of(l),
            extras: Extras::default(),
            extras0: Extras::default(),
        });
    }
    true
}

/// The inline editor; returns the edit once the text is applied.
pub(crate) fn overlay(ctx: &egui::Context, view: &mut DocView, info: &DocInfo) -> Option<Edit> {
    // Clicks outside the document (the Format text panel) keep the paragraph open.
    let outside = ctx.input(|i| i.pointer.latest_pos()).is_some_and(|p| !view.viewport_rect().contains(p));
    let page = view.line_editor.as_ref()?.page;
    let xf = view.page_xform(page)?;
    let viewport_right = view.viewport_rect().right();
    let ed = view.line_editor.as_mut()?;
    // Reproject the source box every frame. The page may have been zoomed, scrolled or rotated
    // while the format panel was open.
    ed.rect = xf.user_rect(info, ed.page, ed.source_rect).expand(2.0);
    let right = xf.rect.right().min(viewport_right) - 6.0;
    ed.max_width = if ed.multiline { ed.rect.width() } else { (right - ed.rect.left()).max(ed.rect.width()) };
    let scale = (ed.rect.width() / (ed.source_rect[2] - ed.source_rect[0]).abs().max(1.0)).max(0.01);
    ed.size = (ed.look.size as f32 * scale).clamp(8.0, 72.0);
    let font = editor_font(&ed.look, ed.size);
    let text_color = color32(ed.look.color);
    let mut done = false;
    egui::Area::new(egui::Id::new("edit-text-line")).order(egui::Order::Foreground).fixed_pos(Pos2::new(ed.rect.left(), ed.rect.top())).show(
        ctx,
        |ui| {
            // The box follows the text as you type: as wide as the longest drafted line needs
            // (up to what the rewrite allows), so new content grows the box instead of wrapping
            // inside the old one.
            let mut width = ed.rect.width().max(120.0);
            if ed.max_width > width {
                let needed = ui.fonts_mut(|f| {
                    ed.text.lines().map(|l| f.layout_no_wrap(l.to_owned(), font.clone(), text_color).size().x).fold(0.0_f32, f32::max)
                }) + 8.0;
                width = width.max(needed).min(ed.max_width);
            }
            egui::Frame::NONE.fill(Color32::WHITE).stroke(Stroke::new(1.5, ACCENT)).inner_margin(egui::Margin::symmetric(2, 0)).show(ui, |ui| {
                let rows = ed.text.lines().count().max(1);
                let r = ui.add(
                    egui::TextEdit::multiline(&mut ed.text)
                        .id(editor_id())
                        .font(font.clone())
                        .text_color(text_color)
                        .frame(egui::Frame::NONE)
                        .desired_width(width)
                        .desired_rows(rows),
                );
                if ed.focus {
                    r.request_focus();
                    ed.focus = false;
                }
                // Esc applies too (and leaves the tool, see `edit_text_keys`): typed text is
                // never thrown away; ⌘Z takes it back.
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                let apply = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                if esc || apply || (r.lost_focus() && !outside) {
                    done = true;
                }
            });
        },
    );
    if done { view.line_editor.take().and_then(finish) } else { None }
}

/// The edit a closed editor makes: its text and formatting, if either changed.
fn finish(ed: LineEditor) -> Option<Edit> {
    let style = ed.style();
    (ed.text != ed.original || style != printcraft_engine::BlockStyle::default()).then_some(Edit::EditTextBlock {
        page: ed.page,
        block: ed.block,
        text: ed.text,
        style,
    })
}

impl PrintCraftApp {
    /// Edit text & images keys, read before the canvas (and before ⌘A becomes "select all
    /// text"): ⌘A / Ctrl+A selects every box on the current page; Esc cancels a drag in
    /// progress, or else leaves the tool. Esc is not consumed: the paragraph editor sees it in
    /// the same frame and applies the typed text.
    pub(crate) fn edit_text_keys(&mut self, ctx: &egui::Context) {
        if self.quick_tool != QuickTool::EditText || self.dialog.is_some() || self.palette_open || egui::Popup::is_any_open(ctx) {
            return;
        }
        let Some(i) = self.active.filter(|i| self.views.get(*i).is_some_and(|v| !v.organize)) else { return };
        let select_all = !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|inp| inp.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::A)));
        if select_all {
            self.select_all_boxes(i);
        }
        let Some(view) = self.views.get_mut(i) else { return };
        // Esc in another text field (the find bar, a panel's field) only leaves that field.
        if self.full_screen || view.typing_elsewhere || !ctx.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            return;
        }
        // One step back: a drag in progress is cancelled first.
        let dragging = view.edit_marquee.take().is_some()
            | view.block_drag.take().is_some()
            | view.image_selection.as_mut().is_some_and(|s| s.drag.take().is_some());
        if dragging {
            return;
        }
        // The overlay applies a paragraph being typed this frame; one on a page scrolled out of
        // view has no overlay, so it is applied here.
        if let Some(page) = view.line_editor.as_ref().map(|ed| ed.page)
            && view.page_xform(page).is_none()
            && let Some(ed) = view.line_editor.take()
        {
            view.pending_edit = finish(ed);
        }
        view.edit_selection = None;
        view.image_selection = None;
        self.quick_tool = QuickTool::Select;
    }

    /// ⌘A in Edit text & images: every paragraph and image on the current page.
    pub(crate) fn select_all_boxes(&mut self, index: usize) {
        let Some(view) = self.views.get_mut(index) else { return };
        let Some(doc) = self.session.get(view.id) else { return };
        if !doc.allows_modification() {
            return;
        }
        let page = view.current;
        let mut s = BoxSelection::new(page);
        s.blocks = (0..doc.text_blocks(page).len()).collect();
        s.images = (0..doc.page_images(page).len()).collect();
        set_selection(view, s);
    }
}
