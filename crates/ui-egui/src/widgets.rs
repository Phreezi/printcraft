//! Small custom widgets built on the design tokens.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, vec2};

use crate::theme::{self, Tokens};
use crate::{PrintCraftApp, icons};

/// A mode-bar tab: text with an underline when active.
pub fn mode_tab(ui: &mut egui::Ui, label: &str, active: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = if active { theme::semibold(13.5) } else { theme::medium(13.5) };
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), t.text).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 22.0, 48.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, label));
    if resp.hovered() && !active {
        ui.painter().rect_filled(rect.shrink2(vec2(2.0, 9.0)), CornerRadius::same(6), t.hover);
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, if active { t.text } else { t.text_muted });
    if active {
        let r = Rect::from_min_max(rect.left_bottom() + vec2(11.0, -3.0), rect.right_bottom() - vec2(11.0, 0.0));
        ui.painter().rect_filled(r, CornerRadius::same(1), t.text);
    }
    resp
}

/// Rounded pill button; `primary` fills with the accent.
pub fn pill_button(ui: &mut egui::Ui, label: &str, primary: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::medium(12.5);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), t.text).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 26.0, 28.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    let (fill, stroke, text) = if primary {
        (if resp.hovered() { t.accent_text } else { t.accent }, Stroke::NONE, Color32::WHITE)
    } else {
        (if resp.hovered() { t.hover } else { t.card }, Stroke::new(1.2, t.text_muted), t.text)
    };
    ui.painter().rect(rect, CornerRadius::same(14), fill, stroke, egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, text);
    resp
}

/// Icon + label, transparent until hovered.
pub fn ghost_button(ui: &mut egui::Ui, icon: &str, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::medium(13.0);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), t.text).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 38.0, 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
    }
    icons::paint(ui, Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(18.0, 18.0)), icon, 17.0, t.icon);
    ui.painter().text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, label, font, t.text);
    resp
}

/// A search-field lookalike that opens the command palette.
pub fn search_box(ui: &mut egui::Ui, placeholder: &str, width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), placeholder));
    let fill = if resp.hovered() { t.hover } else { t.field };
    ui.painter().rect(rect, CornerRadius::same(16), fill, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    icons::paint(ui, Rect::from_min_size(rect.min + vec2(10.0, 8.0), vec2(16.0, 16.0)), "search", 15.0, t.text_muted);
    ui.painter().text(rect.left_center() + vec2(34.0, 0.0), Align2::LEFT_CENTER, placeholder, theme::regular(13.0), t.text_faint);
    ui.painter().text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "⌘K", theme::regular(11.5), t.text_faint);
    resp.on_hover_cursor(egui::CursorIcon::Text)
}

pub fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> Response {
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
}

pub fn section_title(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.to_uppercase()).font(theme::semibold(10.5)).color(t.text_faint).extra_letter_spacing(0.6));
    ui.add_space(2.0);
}

/// A titled panel for one section of a dialog: a header band tinted with the section's `hue`
/// (with a marker bar in that hue and the title in Title Case), then the content on the group
/// fill, inside a rounded border. Returns what `add` returns.
pub fn group<R>(ui: &mut egui::Ui, title: &str, hue: Color32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    group_with(
        ui,
        hue,
        |ui| {
            ui.label(egui::RichText::new(title).font(theme::semibold(13.5)).color(t.text));
        },
        add,
    )
}

/// [`group`] with its own header row (after the marker bar).
pub fn group_with<R>(ui: &mut egui::Ui, hue: Color32, header: impl FnOnce(&mut egui::Ui), add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    let r = 8;
    egui::Frame::new()
        .fill(t.group_fill)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(r))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .fill(t.section_band(hue))
                .corner_radius(CornerRadius { nw: r - 1, ne: r - 1, sw: 0, se: 0 })
                .inner_margin(egui::Margin { left: 10, right: 12, top: 6, bottom: 6 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = spacing;
                        let (bar, _) = ui.allocate_exact_size(vec2(4.0, 15.0), Sense::hover());
                        ui.painter().rect_filled(bar, CornerRadius::same(2), hue);
                        header(ui);
                    });
                });
            egui::Frame::new()
                .inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 10 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = spacing;
                    add(ui)
                })
                .inner
        })
        .inner
}

/// A segmented control: one strip of buttons, the selected one filled. Returns the index of
/// the segment clicked. Each segment is a selectable button labelled with its text.
pub fn segmented(ui: &mut egui::Ui, id_salt: &str, labels: &[&str], selected: usize) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let font = theme::medium(13.0);
    let bold = theme::semibold(13.0);
    let widths: Vec<f32> = labels.iter().map(|l| ui.fonts_mut(|f| f.layout_no_wrap((*l).to_owned(), bold.clone(), t.text).size().x) + 28.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (strip, _) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    ui.painter().rect(strip, CornerRadius::same(8), t.field, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let (sel_fill, sel_text) = t.selected_pair();
    let mut clicked = None;
    let mut x = strip.left() + 2.0;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let seg = Rect::from_min_size(egui::pos2(x, strip.top() + 2.0), vec2(*w, strip.height() - 4.0));
        x += w;
        let resp = ui.interact(seg, egui::Id::new((id_salt, i)), Sense::click());
        let on = i == selected;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, *label));
        if on {
            let stroke = if t.dark() { Stroke::new(1.0, t.accent) } else { Stroke::NONE };
            ui.painter().rect(seg, CornerRadius::same(6), sel_fill, stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(seg, CornerRadius::same(6), t.hover);
        }
        // A thin separator between two unselected neighbours.
        if i + 1 < labels.len() && !on && i + 1 != selected {
            let sx = seg.right();
            ui.painter().line_segment([egui::pos2(sx, seg.top() + 7.0), egui::pos2(sx, seg.bottom() - 7.0)], Stroke::new(1.0, t.border));
        }
        let (f, c) = if on { (bold.clone(), sel_text) } else { (font.clone(), if resp.hovered() { t.text } else { t.text_muted }) };
        ui.painter().text(seg.center(), Align2::CENTER_CENTER, *label, f, c);
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Transient message at the bottom centre.
pub fn toast(app: &mut PrintCraftApp, ctx: &egui::Context) {
    let Some((msg, start)) = app.toast.clone() else { return };
    let now = ctx.input(|i| i.time);
    let start = if start == 0.0 { now } else { start };
    app.toast = Some((msg.clone(), start));
    if now - start > 3.5 {
        app.toast = None;
        return;
    }
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("toast"))
        .order(egui::Order::Tooltip)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(screen.center_bottom() - vec2(0.0, 28.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(if t.dark() { Color32::from_rgb(0xEC, 0xEC, 0xEF) } else { Color32::from_rgb(0x2A, 0x2A, 0x2F) })
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(msg).color(if t.dark() { Color32::from_rgb(0x22, 0x22, 0x26) } else { Color32::WHITE }));
                });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

/// The PeDeeFe app icon (assets/brand/pedeefe/logo/pedeefe-icon.svg), `size` points square.
pub fn app_icon(ui: &mut egui::Ui, size: f32) -> Response {
    ui.add(
        egui::Image::from_bytes("bytes://pedeefe-icon.svg", include_bytes!("../../../assets/brand/pedeefe/logo/pedeefe-icon.svg"))
            .fit_to_exact_size(vec2(size, size))
            .alt_text("PeDeeFe icon"),
    )
}

/// Buttons for every project link (`printcraft_engine::links`), the first one prominent.
/// Returns the registry command of the one clicked.
pub fn community_links(ui: &mut egui::Ui) -> Option<&'static str> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (i, l) in printcraft_engine::links::LINKS.iter().enumerate() {
            let resp = icon_pill(ui, l.icon, l.label, i == 0);
            if resp.on_hover_text(l.url).clicked() {
                clicked = Some(l.command);
            }
        }
    });
    clicked
}

/// A pill button with an icon (primary = filled accent).
pub fn icon_pill(ui: &mut egui::Ui, icon: &str, label: &str, primary: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::medium(12.5);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), t.text).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 46.0, 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    let (fill, stroke, text) = if primary {
        (if resp.hovered() { t.accent_text } else { t.accent }, Stroke::NONE, Color32::WHITE)
    } else {
        (if resp.hovered() { t.hover } else { t.card }, Stroke::new(1.2, t.text_muted), t.text)
    };
    ui.painter().rect(rect, CornerRadius::same(15), fill, stroke, egui::StrokeKind::Inside);
    crate::icons::paint(ui, Rect::from_min_size(rect.min + vec2(12.0, 7.0), vec2(16.0, 16.0)), icon, 15.0, text);
    ui.painter().text(rect.left_center() + vec2(34.0, 0.0), Align2::LEFT_CENTER, label, font, text);
    resp
}
