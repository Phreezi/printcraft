//! Window chrome: tab strip (with the integrated macOS title bar), mode bar, right rail.

use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, vec2};

use crate::canvas::{DocView, Fit, PageLayout};
use crate::theme::{self, ThemePreference, Tokens};
use crate::windows::{TabDrag, TabTarget};
use crate::{Dialog, Mode, PdfCraftApp, PropsTab, RightPanel, icons, widgets};
use pdfcraft_engine::DocId;

pub fn tab_strip(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let left = if cfg!(target_os = "macos") && app.integrated_titlebar { 80 } else { 8 };
    egui::Panel::top("tab_strip")
        .exact_size(38.0)
        .frame(egui::Frame::NONE.fill(t.titlebar).inner_margin(egui::Margin { left, right: 10, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            let drag = ui.interact(full, ui.id().with("titledrag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            let mut tabs: Vec<(DocId, Rect)> = Vec::new();
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if icons::button(ui, "house", 28.0, app.active.is_none(), tl!("Home")).clicked() {
                    app.active = None;
                }
                let mut close = None;
                for i in 0..app.views.len() {
                    let id = app.views[i].id;
                    let Some(doc) = app.session.get(id) else { continue };
                    let (name, dirty) = (doc.display_name(), doc.dirty);
                    let resp = tab(ui, &t, &name, dirty, app.active == Some(i), &mut close, i, id);
                    tabs.push((id, resp.rect));
                    if resp.clicked() {
                        app.active = Some(i);
                    }
                }
                if let Some(i) = close {
                    app.request_close_tab(i);
                }
                ui.add_space(4.0);
                if widgets::ghost_button(ui, "plus", tl!("Open")).on_hover_text(tl!("Open a PDF (⌘O)")).clicked() {
                    app.open_dialog();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (icon, label) = match app.theme_preference {
                        ThemePreference::System => ("settings", tl!("Use system setting")),
                        ThemePreference::Light => ("sun", tl!("Light gray")),
                        ThemePreference::Dark => ("moon", tl!("Dark gray")),
                    };
                    let tip = format!("{}: {label}", tl!("Display theme"));
                    let response = icons::button(ui, icon, 28.0, false, &tip);
                    egui::Popup::menu(&response).show(|ui| theme_menu(app, ui));
                    if icons::button(ui, "circle-help", 28.0, false, tl!("Keyboard shortcuts")).clicked() {
                        app.dialog = Some(Dialog::Shortcuts);
                    }
                });
            });
            tab_drag(app, ui, full, &tabs);
            drop_marker(app, ui, &t, &tabs);
            app.windows.set_strip(full, tabs);
        });
}

/// The widget id of document `doc`'s tab: the same wherever the tab is, so a drag survives the
/// tab moving along the strip.
fn tab_id(doc: DocId) -> egui::Id {
    egui::Id::new(("pdfcraft-tab", doc.0))
}

/// Dragging a tab by its label: along the strip it changes places; released away from the strip
/// it moves to the window whose tab strip is under the pointer, or to a new window there (see
/// [`crate::windows`]). Not where windows can't be created (the web, tests).
fn tab_drag(app: &mut PdfCraftApp, ui: &egui::Ui, strip: Rect, tabs: &[(DocId, Rect)]) {
    let ctx = ui.ctx().clone();
    let current = app.windows.current();
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    for (doc, rect) in tabs {
        if ctx.is_being_dragged(tab_id(*doc)) && app.windows.drag.as_ref().is_none_or(|d| d.doc != *doc || d.window != current) {
            let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer).unwrap_or(rect.min);
            let name = app.session.get(*doc).map(|d| d.display_name()).unwrap_or_default();
            app.windows.drag =
                Some(TabDrag { window: current, doc: *doc, name, grab: origin - rect.min, tab_min: rect.min, pointer: None, screen: None });
        }
    }
    let Some(mut d) = app.windows.drag.clone().filter(|d| d.window == current) else { return };
    let dragging = ctx.is_being_dragged(tab_id(d.doc));
    let stopped = ctx.drag_stopped_id() == Some(tab_id(d.doc));
    if !dragging && !stopped {
        // The drag ended without a release seen here (the tab closed, the window lost the pointer).
        app.windows.drag = None;
        return;
    }
    // The pointer may be outside the window (the system lets the window follow it while the
    // button is down); the last position seen is kept for when it is gone.
    if let Some(p) = pointer {
        d.pointer = Some(p);
        d.screen = app.windows.geometry(current).and_then(|g| g.to_screen(p));
    }
    let in_strip = d.pointer.is_none_or(|p| strip.expand2(vec2(0.0, crate::windows::STRIP_SLACK)).contains(p));
    let can_tear_off = !ctx.embed_viewports();
    if in_strip {
        // Along the strip: before the first other tab whose middle is right of the pointer.
        if let Some(p) = d.pointer {
            let at = tabs.iter().filter(|(doc, _)| *doc != d.doc).filter(|(_, r)| r.center().x < p.x).count();
            let now = tabs.iter().position(|(doc, _)| *doc == d.doc);
            if now.is_some_and(|n| n != at) {
                app.move_tab(d.doc, TabTarget::Window(current, Some(at)));
            }
        }
    } else if can_tear_off {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(p) = d.pointer {
            tab_ghost(&ctx, &d.name, p);
        }
    }
    if stopped {
        app.windows.drag = None;
        if !in_strip && can_tear_off {
            let first = tabs.first().map_or(d.tab_min, |(_, r)| r.min);
            let target = app.windows.drop_target(current, d.screen, d.grab + first.to_vec2());
            app.windows.queue_move(d.doc, target);
        }
    } else {
        app.windows.drag = Some(d);
    }
}

/// The dragged tab's label, following the pointer.
fn tab_ghost(ctx: &egui::Context, name: &str, at: egui::Pos2) {
    let t = Tokens::get(ctx);
    egui::Area::new(egui::Id::new("pdfcraft-tab-ghost")).order(egui::Order::Tooltip).interactable(false).fixed_pos(at + vec2(12.0, 10.0)).show(
        ctx,
        |ui| {
            egui::Frame::NONE
                .fill(t.chrome)
                .stroke(Stroke::new(1.0, t.divider))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(icons::image("file-text", 15.0, t.accent));
                        ui.label(egui::RichText::new(name).font(theme::regular(13.0)).color(t.text));
                    });
                });
        },
    );
}

/// A tab dragged from another window over this window's tab strip: where it would land, and its
/// label (it left the window it came from).
fn drop_marker(app: &PdfCraftApp, ui: &egui::Ui, t: &Tokens, tabs: &[(DocId, Rect)]) {
    let current = app.windows.current();
    let Some(d) = app.windows.drag.as_ref().filter(|d| d.window != current) else { return };
    let Some(screen) = d.screen else { return };
    let Some((key, at)) = app.windows.hovered_strip(d.window, screen) else { return };
    if key != current {
        return;
    }
    let x = match at.and_then(|i| tabs.get(i)) {
        Some((_, r)) => r.left() - 2.0,
        None => tabs.last().map_or(ui.max_rect().left() + 40.0, |(_, r)| r.right() + 2.0),
    };
    let y = tabs.first().map_or(ui.max_rect().y_range(), |(_, r)| r.y_range());
    ui.painter().vline(x, y, Stroke::new(2.0, t.accent));
    if let Some(local) = app.windows.geometry(current).and_then(|g| g.inner).map(|inner| screen - inner.min.to_vec2()) {
        tab_ghost(ui.ctx(), &d.name, local);
    }
}

/// Both theme entry points use the same choices and command path.
fn theme_menu(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    for (preference, command, label) in [
        (ThemePreference::System, "view.theme.system", "Use system setting"),
        (ThemePreference::Light, "view.theme.light", "Light gray"),
        (ThemePreference::Dark, "view.theme.dark", "Dark gray"),
    ] {
        if ui.radio(app.theme_preference == preference, tl!(label)).clicked() {
            app.execute(command);
            ui.close();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tab(ui: &mut egui::Ui, t: &Tokens, name: &str, dirty: bool, active: bool, close: &mut Option<usize>, index: usize, doc: DocId) -> egui::Response {
    let font = theme::regular(13.0);
    let label: String = if name.chars().count() > 28 { format!("{}…", name.chars().take(27).collect::<String>()) } else { name.to_string() };
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), t.text).size().x);
    let (rect, _) = ui.allocate_exact_size(vec2(text_w + 64.0, 30.0), Sense::hover());
    // Clicked to show, dragged to move (along the strip, to another window or out to a new one).
    let resp = ui.interact(rect, tab_id(doc), Sense::click_and_drag());
    let a11y = if dirty { format!("{name} (edited)") } else { name.to_string() };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, &a11y));
    let bg = if active {
        t.chrome
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, bg);
    icons::paint(
        ui,
        Rect::from_min_size(rect.min + vec2(6.0, 7.0), vec2(16.0, 16.0)),
        "file-text",
        15.0,
        if active { t.accent } else { t.text_muted },
    );
    ui.painter().text(rect.min + vec2(28.0, rect.height() / 2.0), Align2::LEFT_CENTER, label, font, if active { t.text } else { t.text_muted });
    let x_rect = Rect::from_center_size(rect.right_center() - vec2(16.0, 0.0), vec2(20.0, 20.0));
    let x = ui.interact(x_rect, tab_id(doc).with("close"), Sense::click());
    if x.hovered() {
        ui.painter().rect_filled(x_rect, CornerRadius::same(4), t.pressed);
    }
    // Unsaved changes: a dot where the close button sits, until the tab is hovered.
    if dirty && !resp.hovered() && !x.hovered() {
        ui.painter().circle_filled(x_rect.center(), 4.0, if active { t.text } else { t.text_muted });
    } else if active || resp.hovered() || x.hovered() {
        icons::paint(ui, x_rect, "x", 13.0, t.text_muted);
    }
    if x.clicked() {
        *close = Some(index);
    }
    resp.on_hover_text(if dirty { crate::i18n::fmt(tl!("{name} — unsaved changes"), &[("name", name)]) } else { name.to_string() })
}

pub fn mode_bar(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("mode_bar")
        .exact_size(48.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                main_menu(app, ui);
                ui.add_space(6.0);
                ui.painter().vline(ui.cursor().left(), ui.max_rect().y_range().shrink(12.0), Stroke::new(1.0, t.divider));
                ui.add_space(10.0);
                for (mode, label) in
                    [(Mode::AllTools, "All tools"), (Mode::Read, "Read"), (Mode::Edit, "Edit"), (Mode::Convert, "Convert"), (Mode::Sign, "E-Sign")]
                {
                    if widgets::mode_tab(ui, tl!(label), app.mode == mode).clicked() {
                        app.select_mode(mode);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let has_doc = app.active.is_some();
                    ui.add_enabled_ui(has_doc, |ui| {
                        if icons::button(ui, "printer", 32.0, false, tl!("Print (⌘P)")).clicked() {
                            app.run_command("print.dialog");
                        }
                        if icons::button(ui, "save", 32.0, false, tl!("Save (M4)")).clicked() {
                            app.run_command("file.save");
                        }
                        if icons::button(ui, "info", 32.0, false, tl!("Document properties (⌘D)")).clicked() {
                            app.dialog = Some(Dialog::Properties(PropsTab::Description));
                        }
                    });
                    ui.add_space(8.0);
                    if widgets::search_box(ui, tl!("Find tools and commands"), 260.0).clicked() {
                        app.palette_open = true;
                    }
                });
            });
        });
}

fn main_menu(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let resp = widgets::ghost_button(ui, "panel-left", tl!("Menu"));
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(230.0);
        ui.menu_button(tl!("File"), |ui| crate::commands::registry_menu(app, ui, "File"));
        ui.menu_button(tl!("Edit"), |ui| crate::commands::registry_menu(app, ui, "Edit"));
        ui.menu_button(tl!("Pages"), |ui| crate::commands::registry_menu(app, ui, "Pages"));
        ui.menu_button(tl!("View"), |ui| {
            if let Some(i) = app.active {
                let v = &mut app.views[i];
                ui.label(egui::RichText::new(tl!("Zoom")).color(t.text_faint).small());
                if widgets::menu_item(ui, tl!("Actual size"), "⌘1").clicked() {
                    v.set_zoom(1.0);
                }
                if widgets::menu_item(ui, tl!("Zoom to page level"), "⌘0").clicked() {
                    v.fit = Fit::Page;
                }
                if widgets::menu_item(ui, tl!("Fit to width"), "⌘2").clicked() {
                    v.fit = Fit::Width;
                }
                if widgets::menu_item(ui, tl!("Fit to height"), "").clicked() {
                    v.fit = Fit::Height;
                    v.goto = Some((v.current, 0.0));
                }
                if widgets::menu_item(ui, tl!("Fit visible"), "⌘3").clicked() {
                    ui.close();
                    app.execute("view.fit_visible");
                    return;
                }
                if widgets::menu_item(ui, tl!("Zoom in"), "⌘+").clicked() {
                    v.zoom_step(true);
                }
                if widgets::menu_item(ui, tl!("Zoom out"), "⌘−").clicked() {
                    v.zoom_step(false);
                }
                if widgets::menu_item(ui, tl!("Rotate view clockwise"), "⇧⌘+").clicked() {
                    v.rotate_view(true);
                }
                if widgets::menu_item(ui, tl!("Rotate view counterclockwise"), "⇧⌘−").clicked() {
                    v.rotate_view(false);
                }
                ui.separator();
                ui.label(egui::RichText::new(tl!("Page navigation")).color(t.text_faint).small());
                if ui.add_enabled(!v.back.is_empty(), egui::Button::new(tl!("Previous view")).shortcut_text("⌘[")).clicked() {
                    v.view_history(false);
                }
                if ui.add_enabled(!v.forward.is_empty(), egui::Button::new(tl!("Next view")).shortcut_text("⌘]")).clicked() {
                    v.view_history(true);
                }
                ui.separator();
                if let Some(id) = page_display_menu(ui, &app.views[i]) {
                    app.execute(id);
                }
                ui.separator();
            }
            crate::commands::registry_menu(app, ui, "View");
            ui.menu_button(tl!("Display theme"), |ui| theme_menu(app, ui));
            ui.menu_button(tl!("Side panels"), |ui| {
                for (p, label) in [
                    (RightPanel::Comments, tl!("Comments")),
                    (RightPanel::Bookmarks, tl!("Bookmarks")),
                    (RightPanel::Pages, tl!("Pages")),
                    (RightPanel::Fields, tl!("Fields")),
                    (RightPanel::Layers, tl!("Layers")),
                    (RightPanel::Attachments, tl!("Attachments")),
                    (RightPanel::Signatures, tl!("Signatures")),
                    (RightPanel::Accessibility, tl!("Accessibility Checker")),
                    (RightPanel::Search, tl!("Search")),
                    (RightPanel::Compare, tl!("Compare")),
                ] {
                    if ui.radio(app.right == Some(p), tl!(label)).clicked() {
                        app.right = Some(p);
                    }
                }
            });
        });
        ui.menu_button(tl!("Help"), |ui| crate::commands::registry_menu(app, ui, "Help"));
    });
}

pub fn right_rail(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some((index, id)) = app.active_ids() else { return };
    let Some(doc) = app.session.get(id) else { return };
    let has_signatures = doc.is_signed();
    let has_check = app.a11y.report.as_ref().is_some_and(|(d, _)| *d == id);
    let (has_comments, has_outline, has_fields, has_layers, has_files) = (
        !doc.info.annotations.is_empty(),
        !doc.info.outline.is_empty(),
        !doc.info.fields.is_empty(),
        !doc.info.layers.is_empty(),
        !doc.info.attachments.is_empty(),
    );
    let page_count = doc.info.pages.len();
    let labels: Vec<String> = doc.info.pages.iter().map(|p| p.label.clone()).collect();
    egui::Panel::right("rail")
        .resizable(false)
        .exact_size(48.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(6, 8)).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let mut rail_button = |ui: &mut egui::Ui, panel: RightPanel, icon: &str, tip: &str, has: bool| {
                let selected = app.right == Some(panel);
                let r = icons::button(ui, icon, 34.0, selected, tl!(tip));
                if has && !selected {
                    let c = r.rect.right_top() + vec2(-8.0, 8.0);
                    ui.painter().circle_filled(c, 3.0, t.accent);
                }
                if r.clicked() {
                    app.right = if selected { None } else { Some(panel) };
                }
            };
            rail_button(ui, RightPanel::Comments, "message-square-text", "Comments", has_comments);
            rail_button(ui, RightPanel::Bookmarks, "bookmark", "Bookmarks", has_outline);
            rail_button(ui, RightPanel::Pages, "files", "Page thumbnails", false);
            rail_button(ui, RightPanel::Fields, "text-cursor-input", "Form fields", has_fields);
            rail_button(ui, RightPanel::Layers, "layers", "Layers", has_layers);
            rail_button(ui, RightPanel::Attachments, "paperclip", "Attachments", has_files);
            rail_button(ui, RightPanel::Signatures, "signature", "Signatures", has_signatures);
            if has_check {
                rail_button(ui, RightPanel::Accessibility, "accessibility", "Accessibility Checker", false);
            }

            // Page navigation cluster at the bottom (as in Acrobat's rail).
            let view = &mut app.views[index];
            let mut page_display = None;
            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if icons::button(ui, "zoom-out", 32.0, false, tl!("Zoom out (⌘−)")).clicked() {
                    view.zoom_step(false);
                }
                if icons::button(ui, "zoom-in", 32.0, false, tl!("Zoom in (⌘+)")).clicked() {
                    view.zoom_step(true);
                }
                // Between rotate and zoom, as in Acrobat. Its command runs once `view` is free.
                let tip = crate::i18n::fmt(tl!("Page display: {layout}"), &[("layout", tl!(view.layout.label()))]);
                let resp = icons::button(ui, view.layout.icon(), 32.0, false, &tip);
                page_display = egui::Popup::menu(&resp)
                    .show(|ui| {
                        ui.set_min_width(220.0);
                        rail_view_menu(ui, view)
                    })
                    .and_then(|r| r.inner);
                if icons::button(ui, "rotate-cw", 32.0, false, tl!("Rotate view clockwise (⇧⌘+)")).clicked() {
                    view.rotate_view(true);
                }
                // Not `columns-2` while fitting the page: that is the two-page view's icon.
                let fit_icon = if view.fit == Fit::Width { "maximize" } else { "maximize-2" };
                if icons::button(ui, fit_icon, 32.0, false, tl!("Toggle fit page / fit width")).clicked() {
                    view.fit = if view.fit == Fit::Width { Fit::Page } else { Fit::Width };
                    view.goto = Some((view.current, 0.0));
                }
                ui.label(egui::RichText::new(format!("{:.0}%", view.zoom * 100.0)).font(theme::regular(10.5)).color(t.text_faint));
                ui.add_space(6.0);
                if icons::button(ui, "chevron-down", 30.0, false, tl!("Next page")).clicked() {
                    view.step_page(true);
                }
                if icons::button(ui, "chevron-up", 30.0, false, tl!("Previous page")).clicked() {
                    view.step_page(false);
                }
                ui.label(egui::RichText::new(page_count.to_string()).font(theme::regular(11.0)).color(t.text_muted));
                let edit = egui::TextEdit::singleline(&mut view.page_input)
                    .id(egui::Id::new("page-input"))
                    .desired_width(34.0)
                    .horizontal_align(Align::Center)
                    .font(theme::medium(12.0))
                    .margin(vec2(2.0, 4.0));
                let r = egui::Frame::NONE
                    .fill(t.field)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(CornerRadius::same(5))
                    .show(ui, |ui| ui.add(edit))
                    .inner
                    .on_hover_text(tl!("Current page — type a page number or label (such as iv) and press Enter"));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // A page label first (logical page numbers, as Acrobat), then a number.
                    let typed = view.page_input.clone();
                    if !view.go_to_typed(&typed, &labels) {
                        view.page_input = (view.current + 1).to_string();
                    }
                }
            });
            if let Some(id) = page_display {
                app.execute(id);
            }
        });
}

/// View ▸ Page display and the rail's Page display menu: the layouts and the cover page.
/// Returns the command a click asks for, for the caller to run.
fn page_display_menu(ui: &mut egui::Ui, view: &DocView) -> Option<&'static str> {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tl!("Page display")).color(t.text_faint).small());
    let mut picked = None;
    for l in PageLayout::ORDER {
        if ui.radio(view.layout == l, tl!(l.label())).clicked() {
            picked = Some(l.command());
        }
    }
    let mut cover = view.cover;
    if ui.add_enabled(view.cover_applies(), egui::Checkbox::new(&mut cover, tl!("Show cover page in two-page view"))).changed() {
        picked = Some("view.layout.cover");
    }
    if picked.is_some() {
        ui.close();
    }
    picked
}

/// The rail's Page display menu: Acrobat's view choices around the page display. Zoom changes
/// apply at once; the command a click asks for is returned for the caller to run.
fn rail_view_menu(ui: &mut egui::Ui, view: &mut DocView) -> Option<&'static str> {
    let mut picked = None;
    for (id, label) in [("view.fit_width_scrolling", "Fit to width scrolling"), ("view.fit_one_page", "Fit one full page")] {
        if ui.button(tl!(label)).clicked() {
            picked = Some(id);
        }
    }
    ui.separator();
    if ui.button(tl!("Actual size")).clicked() {
        view.set_zoom(1.0);
        ui.close();
    }
    if ui.button(tl!("Zoom to page level")).clicked() {
        view.set_fit(Fit::Page);
        ui.close();
    }
    if ui.button(tl!("Fit visible")).clicked() {
        picked = Some("view.fit_visible");
    }
    ui.separator();
    picked = page_display_menu(ui, view).or(picked);
    ui.separator();
    for (id, label) in [("view.read_mode", "Read mode"), ("view.full_screen", "Full screen mode")] {
        if ui.button(tl!(label)).clicked() {
            picked = Some(id);
        }
    }
    if picked.is_some() {
        ui.close();
    }
    picked
}
