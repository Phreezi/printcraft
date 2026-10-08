//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

use egui::{Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::print::{self, A4, Orientation, SizeMode};
use pdfcraft_ui_egui::print_ui::{
    PAPER_CHOICES, PickDrag, area_label, decimal_comma, drag_window, move_window, pick_note, poster_preview, poster_run, printable_aspect,
    scale_label, sheet_label, window_fit_percent, window_landscape,
};
use pdfcraft_ui_egui::{Dialog, PdfCraftApp, PrintArea, PrintDraft, PrintHandling};

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn print_dialog_lays_out_sheets_and_saves_a_pdf() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    // Each section is a titled panel.
    for title in ["Printer", "Pages to Print", "Page Sizing & Handling", "Orientation", "Comments & Forms"] {
        h.get_by_label(title);
    }
    h.get_by_label("Sheet 1 of 1");
    // Two copies of the one page, as a 2-up poster-free layout: Multiple.
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 1");
    // No printer on a test machine: Save as PDF.
    let out = std::env::temp_dir().join(format!("pdfcraft-print-test-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    // Chosen, so a printer list arriving late doesn't pick a real printer instead.
    h.state_mut().print_draft.printer_chosen = true;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let bytes = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
    let _ = std::fs::remove_file(out);
}

#[test]
fn invalid_ranges_are_explained_in_the_preview() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(2);
    h.state_mut().print_draft.which = pdfcraft_ui_egui::PrintWhich::Range;
    h.state_mut().print_draft.range = "7".into();
    h.run_steps(2);
    h.get_by_label_contains("out of range");
}

#[test]
fn paper_is_a4_or_a3_and_a4_by_default() {
    assert_eq!(PAPER_CHOICES.map(|p| p.0), ["A4", "A3"]);
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("A4");
    h.get_by_label("A3");
    assert!(h.query_by_label("US Letter").is_none(), "no other papers");
    let s = h.state().print_draft.settings(1, &[]).unwrap();
    assert_eq!(s.paper, pdfcraft_engine::print::A4);
    h.get_by_label("A3").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.settings(1, &[]).unwrap().paper, pdfcraft_engine::print::A3);
}

#[test]
fn window_is_a_print_area_for_every_mode() {
    use pdfcraft_engine::print::Layout;
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    // Four sizing modes, no Window tab: Window is the print area beside them, Full page first.
    for tab in ["Size", "Poster", "Multiple", "Booklet", "Full page", "Window"] {
        h.get_by_label(tab);
    }
    assert_eq!(h.state().print_draft.area, PrintArea::FullPage);
    h.get_by_label_contains("Whole pages print");
    assert!(h.query_by_label("Select area…").is_none(), "no picker button for whole pages");
    // The tabs and the area choice share one row, the area on the right.
    let (size, window) = (h.get_by_label("Size").rect(), h.get_by_label("Window").rect());
    assert!((size.center().y - window.center().y).abs() < 1.0 && window.left() > h.get_by_label("Booklet").rect().right(), "{size:?} {window:?}");
    h.get_by_label("Window").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.area, PrintArea::Window);
    assert_eq!(h.state().print_draft.handling, PrintHandling::Size, "the sizing mode stays");
    h.get_by_label_contains("No area yet");
    // The old Window-only output choice is gone (Size and Poster cover it), and so are "Whole
    // page" and the proportions check box (Shift while drawing).
    for gone in ["On one sheet", "As a poster", "Print the area:", "Whole page", "Keep the sheet"] {
        assert!(h.query_by_label_contains(gone).is_none(), "{gone}");
    }
    // An area (as the picker would leave it): Size prints it alone, Fit filling the sheet.
    h.state_mut().print_draft.region = Some([0.0, 0.0, 200.0, 141.0]);
    h.run_steps(2);
    h.get_by_label_contains("Area: 71 × 50 mm");
    let s = h.state().print_draft.settings(1, &[]).unwrap();
    assert_eq!(s.region, Some([0.0, 0.0, 200.0, 141.0]));
    assert!(matches!(s.layout, Layout::Size(SizeMode::Fit)));
    // Poster tiles the area over several sheets (a window over 3 sheets, say).
    h.get_by_label("Poster").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.area, PrintArea::Window, "the area stays chosen across tabs");
    let s = h.state().print_draft.settings(1, &[]).unwrap();
    assert!(matches!(s.layout, Layout::Poster { .. }) && s.region.is_some());
    h.get_by_label_contains("Area: 71 × 50 mm");
    // Multiple and Booklet print each page's area too.
    for tab in ["Multiple", "Booklet"] {
        h.get_by_label(tab).click();
        h.run_steps(2);
        assert_eq!(h.state().print_draft.settings(1, &[]).unwrap().region, Some([0.0, 0.0, 200.0, 141.0]), "{tab}");
    }
    // Full page prints whole pages again, in every mode; the area is kept for later.
    h.get_by_label("Full page").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.settings(1, &[]).unwrap().region, None);
    assert!(h.state().print_draft.region.is_some());
    // A window over several sheets: Poster at 500 % of the 71 × 50 mm area.
    let d = PrintDraft {
        handling: PrintHandling::Poster,
        area: PrintArea::Window,
        region: Some([0.0, 0.0, 200.0, 141.0]),
        poster_scale: 500.0,
        ..Default::default()
    };
    let sheets = print::layout(&[(300.0, 400.0)], &d.settings(1, &[]).unwrap()).unwrap();
    assert!(sheets.len() >= 2 && sheets.iter().all(|s| s.tile.is_some()), "{} sheets", sheets.len());
    for sheet in &sheets {
        let c = sheet.placed[0].clip;
        assert!(c[2] <= 200.0 + 1e-9 && c[3] <= 141.0 + 1e-9, "only the area prints: {c:?}");
    }
}

#[test]
fn the_picker_line_follows_the_sizing_mode() {
    let r = [0.0, 0.0, 200.0, 141.0];
    let mut d = PrintDraft { area: PrintArea::Window, region: Some(r), ..Default::default() };
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm · prints at 421% on one sheet");
    d.size = SizeMode::Actual;
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm · prints at 100% on one sheet");
    d.size = SizeMode::Shrink;
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm · prints at 100% on one sheet");
    d.size = SizeMode::Custom(250.0);
    d.custom_scale = 250.0;
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm · prints at 250% on one sheet");
    d.handling = PrintHandling::Poster;
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm · prints as a poster at 200%");
    d.handling = PrintHandling::Booklet;
    assert_eq!(pick_note(&d, r), "Area: 71 × 50 mm");
}

#[test]
fn window_picker_draws_and_moves_the_area() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Window").click();
    h.run_steps(2);
    h.get_by_label("Select area…").click();
    h.run_steps(4);
    assert!(h.state().print_draft.picking);
    h.get_by_label("Select the area to print");
    assert!(h.query_by_label("Whole page").is_none());
    assert!(h.query_by_label_contains("Keep the sheet").is_none());
    h.get_by_label_contains("Hold Shift while dragging");
    // Simulate the gestures' results through the draft (pointer drags are covered by the
    // geometry tests below), then accept.
    h.state_mut().print_draft.region = Some([10.0, 10.0, 110.0, 80.0]);
    h.run_steps(2);
    h.get_by_label("Use this area").click();
    // The dialog narrows again and takes a frame or two to re-centre.
    h.run_steps(4);
    assert!(!h.state().print_draft.picking, "back to the settings");
    assert!(h.state().print_draft.region.is_some());
    // Cancel puts back the area from before.
    let before = h.state().print_draft.region;
    h.get_by_label("Select area…").click();
    h.run_steps(4);
    h.state_mut().print_draft.region = Some([0.0, 0.0, 50.0, 50.0]);
    h.state_mut().print_draft.pick.drag = Some(PickDrag::Draw { start: (0.0, 0.0) });
    h.run_steps(1);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.region, before);
    assert!(!h.state().print_draft.picking);
    // Escape leaves the picker the same way, and only the picker: the dialog stays open.
    h.run_steps(3);
    h.get_by_label("Select area…").click();
    h.run_steps(4);
    assert!(h.state().print_draft.picking);
    h.state_mut().print_draft.region = Some([0.0, 0.0, 60.0, 60.0]);
    h.run_steps(1);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().print_draft.picking, "Escape cancels the picker");
    assert_eq!(h.state().print_draft.region, before, "and puts the area back");
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the Print dialog stays open");
    h.get_by_label("Select area…");
}

/// Drag in the harness from `from` to `to` (screen points), with the given modifiers held.
fn drag(h: &mut Harness<'static, PdfCraftApp>, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers });
    h.run_steps(1);
    for k in 1..=4 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 4.0)));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers });
    h.run_steps(2);
}

fn near(a: [f64; 4], b: [f64; 4], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

#[test]
fn window_picker_shift_drag_keeps_the_sheet_shape() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Window").click();
    h.run_steps(2);
    h.get_by_label("Select area…").click();
    h.run_steps(4);
    assert!(h.state().print_draft.picking);
    // form.pdf is one 300 × 400 page, fitted into the canvas less 16 points a side.
    let rect = h.get_by_label("Print area").rect();
    let fit = ((rect.width() - 32.0) / 300.0).min((rect.height() - 32.0) / 400.0);
    let page = Rect::from_center_size(rect.center(), vec2(300.0 * fit, 400.0 * fit));
    let at = |x: f32, y: f32| pos2(page.left() + x * fit, page.top() + (400.0 - y) * fit);
    let tol = 1.5 / fit as f64 + 0.5;
    // A plain drag prints exactly the box drawn.
    drag(&mut h, at(50.0, 350.0), at(250.0, 250.0), egui::Modifiers::NONE);
    let r = h.state().print_draft.region.expect("an area");
    assert!(near(r, [50.0, 250.0, 250.0, 350.0], tol), "{r:?}");
    // With Shift: the sheet's proportions (a wide box: a landscape A4 sheet). (Drawn
    // afresh: a drag from the area's corner would resize it.)
    h.state_mut().print_draft.region = None;
    h.run_steps(1);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    drag(&mut h, at(50.0, 350.0), at(250.0, 250.0), egui::Modifiers::SHIFT);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(1);
    let r = h.state().print_draft.region.expect("an area");
    let aspect = printable_aspect(A4, true);
    assert!((((r[2] - r[0]) / (r[3] - r[1])) - aspect).abs() < 0.01, "{r:?}: {} vs {aspect}", (r[2] - r[0]) / (r[3] - r[1]));
    assert!(near(r, [50.0, 350.0 - 200.0 / aspect, 250.0, 350.0], tol), "{r:?}");
    // A corner resizes: the opposite corner stays put.
    let bottom_right = (r[2] as f32, r[1] as f32);
    drag(&mut h, at(bottom_right.0, bottom_right.1), at(280.0, 150.0), egui::Modifiers::NONE);
    let r = h.state().print_draft.region.expect("an area");
    assert!(near(r, [50.0, 150.0, 280.0, 350.0], tol), "{r:?}");
}

#[test]
fn window_geometry() {
    let a4 = pdfcraft_engine::print::A4;
    // Fit adds no margin: the shape that fills the sheet is the sheet's own.
    let r = printable_aspect(a4, false);
    assert!((r - 595.28 / 841.89).abs() < 1e-9);
    assert!((printable_aspect(a4, true) - 1.0 / r).abs() < 1e-9);
    assert!(window_landscape(Orientation::Auto, 300.0, 200.0));
    assert!(!window_landscape(Orientation::Portrait, 300.0, 200.0));
    // With Shift: the dragged box grows to the sheet's shape, covering the drag.
    let page = (1000.0, 1000.0);
    let w = drag_window((100.0, 100.0), (400.0, 150.0), page, Some(1.0 / r));
    assert!(((w[2] - w[0]) / (w[3] - w[1]) - 1.0 / r).abs() < 1e-6, "{w:?}");
    assert!((w[2] - w[0] - 300.0).abs() < 1e-6, "covers the drag");
    // Dragging up and left works too, and the box stays on the page.
    let w = drag_window((100.0, 100.0), (-500.0, -500.0), page, Some(1.0));
    assert_eq!(w, [0.0, 0.0, 100.0, 100.0]);
    // Without: just the box, clamped.
    assert_eq!(drag_window((10.0, 20.0), (1200.0, 5.0), page, None), [10.0, 5.0, 1000.0, 20.0]);
    // Moving stays on the page.
    assert_eq!(move_window([0.0, 0.0, 100.0, 100.0], 950.0, -20.0, page), [900.0, 0.0, 1000.0, 100.0]);
    // A window the size of an A4 sheet prints at 100% on A4, and fills A3 at about 141%.
    let full = [0.0, 0.0, 595.28, 841.89];
    assert!((window_fit_percent(full, a4, Orientation::Auto) - 100.0).abs() < 1e-6);
    let a3 = 100.0 * (841.89 / 595.28_f64).min(1190.55 / 841.89);
    assert!((window_fit_percent(full, pdfcraft_engine::print::A3, Orientation::Auto) - a3).abs() < 1e-6, "about 141% on A3");
    assert_eq!(area_label([0.0, 0.0, 841.89, 595.28]), "297 × 210 mm");
}

#[test]
fn print_settings_are_remembered() {
    let mut app = PdfCraftApp::new();
    app.print_draft.printer = Some("Office".into());
    app.print_draft.printer_chosen = true;
    app.print_draft.paper = 1;
    app.print_draft.grayscale = true;
    app.print_draft.duplex = pdfcraft_engine::print::spool::Duplex::LongEdge;
    let saved = app.persist();
    let mut again = PdfCraftApp::new();
    again.restore(&saved);
    let d = &again.print_draft;
    assert_eq!(d.printer.as_deref(), Some("Office"));
    assert!(d.printer_chosen && d.grayscale);
    // The proportions check box and the Window tab's output choice are gone (Shift while
    // dragging; the Size and Poster tabs): not saved any more, and a file from before, which has
    // them, still loads.
    assert!(app.print_draft.prefs().get("lock_aspect").is_none());
    assert!(app.print_draft.prefs().get("window_poster").is_none());
    let mut old = PdfCraftApp::new();
    old.restore(r#"{"print": {"paper": "A3", "lock_aspect": false, "window_poster": true}}"#);
    assert_eq!(old.print_draft.paper, 1);
    assert_eq!((old.print_draft.area, old.print_draft.handling), (PrintArea::FullPage, PrintHandling::Size));
    // The print area and the window last for the session, not between sessions.
    let mut window = PdfCraftApp::new();
    window.print_draft.area = PrintArea::Window;
    window.print_draft.region = Some([1.0, 2.0, 30.0, 40.0]);
    let mut next = PdfCraftApp::new();
    next.restore(&window.persist());
    assert_eq!((next.print_draft.area, next.print_draft.region), (PrintArea::FullPage, None));
    assert_eq!(d.paper, 1);
    assert_eq!(d.duplex, pdfcraft_engine::print::spool::Duplex::LongEdge);
    // Untrusted settings: nonsense keeps the defaults.
    let mut odd = PdfCraftApp::new();
    odd.restore(r#"{"print": {"paper": "Letter", "dpi": 7, "printer": 12, "duplex": "sideways"}}"#);
    assert_eq!(odd.print_draft.paper, 0);
    assert_eq!(odd.print_draft.dpi, 300);
    assert_eq!(odd.print_draft.printer, None);
}

#[test]
fn a_printer_list_arriving_keeps_the_users_choice() {
    use pdfcraft_engine::print::spool::Printer;
    let list = || vec![Printer { name: "A".into(), default: false }, Printer { name: "B".into(), default: true }];
    let mut d = pdfcraft_ui_egui::PrintDraft { listing: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("B"), "nothing chosen: the default");
    assert!(!d.listing);
    let mut d = pdfcraft_ui_egui::PrintDraft { printer: Some("A".into()), printer_chosen: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("A"), "the chosen printer stays");
    let mut d = pdfcraft_ui_egui::PrintDraft { printer: Some("Gone".into()), printer_chosen: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("B"), "a printer that is gone falls back to the default");
    let mut d = pdfcraft_ui_egui::PrintDraft { printer: None, printer_chosen: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer, None, "Save as PDF, chosen, stays");
}

#[test]
fn preview_labels_format_like_acrobat() {
    assert_eq!(decimal_comma(1188.04), "1188,04");
    assert_eq!(decimal_comma(0.5), "0,50");
    assert_eq!(decimal_comma(f64::NAN), "—");
    assert_eq!(sheet_label("A4", A4, Some((720.0, 1440.0))), "A4 - 210 × 297 mm [254,00 × 508,00 mm]");
    assert_eq!(sheet_label("A3", (1190.55, 841.89), None), "A3 - 420 × 297 mm");
    assert_eq!(scale_label(Some(1.864)), "Scale: 186%");
    assert_eq!(scale_label(Some(5.0)), "Scale: 500%");
    assert_eq!(scale_label(None), "Scale: —");
    assert_eq!(scale_label(Some(f64::INFINITY)), "Scale: —");
}

/// The dialog open on form.pdf (one 300 × 400 page), with `set` applied to the draft.
fn dialog_with(set: impl FnOnce(&mut PrintDraft)) -> Harness<'static, PdfCraftApp> {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    set(&mut h.state_mut().print_draft);
    h.run_steps(3);
    h
}

#[test]
fn preview_reports_the_real_scale_and_printed_size() {
    // Fit: the 300 × 400 page fills the A4 sheet (595.28 × 841.89, no margin) at 198 %: edge to
    // edge across.
    let h = dialog_with(|_| {});
    h.get_by_label("Scale: 198%");
    h.get_by_label("Sheets: 1");
    h.get_by_label("A4 - 210 × 297 mm [210,00 × 280,00 mm]");
    let h = dialog_with(|d| d.size = SizeMode::Actual);
    h.get_by_label("Scale: 100%");
    h.get_by_label("A4 - 210 × 297 mm [105,83 × 141,11 mm]");
    let h = dialog_with(|d| d.size = SizeMode::Shrink);
    h.get_by_label("Scale: 100%");
    let h = dialog_with(|d| {
        d.size = SizeMode::Custom(50.0);
        d.custom_scale = 50.0;
    });
    h.get_by_label("Scale: 50%");
    h.get_by_label("A4 - 210 × 297 mm [52,92 × 70,56 mm]");
    // Two per sheet: side by side on a landscape sheet.
    let h = dialog_with(|d| d.handling = PrintHandling::Multiple);
    h.get_by_label("Scale: 133%");
    h.get_by_label("A4 - 297 × 210 mm [141,09 × 188,12 mm]");
    // A booklet: the blank back borrows the front's scale.
    let mut h = dialog_with(|d| d.handling = PrintHandling::Booklet);
    h.get_by_label("Scale: 134%");
    h.get_by_label("Sheets: 2");
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 2");
    h.get_by_label("Scale: 134%");
    // A window, fitted to the sheet (landscape, edge to edge across).
    let h = dialog_with(|d| {
        d.area = PrintArea::Window;
        d.region = Some([0.0, 0.0, 200.0, 141.0]);
    });
    h.get_by_label("Scale: 421%");
    h.get_by_label("A4 - 297 × 210 mm [297,00 × 209,39 mm]");
    // A page the sheet's size prints at 100 % with Fit, the same as Actual size (R3).
    let mut h = harness();
    let bytes = h.state().session.create_blank(A4.0, A4.1, 1).unwrap();
    h.state_mut().open_bytes("a4.pdf", None, bytes.as_ref().clone()).unwrap();
    h.run_steps(2);
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    assert_eq!(h.state().print_draft.size, SizeMode::Fit, "Fit is the default");
    h.get_by_label("Scale: 100%");
    h.get_by_label("A4 - 210 × 297 mm [210,00 × 297,00 mm]");
    // On A3 it fills the sheet.
    h.get_by_label("A3").click();
    h.run_steps(2);
    h.get_by_label("Scale: 141%");
}

#[test]
fn poster_preview_shows_the_whole_page_grid() {
    // 500 %: 1500 × 2000 points in 3 × 3 A4 tiles.
    let mut h = dialog_with(|d| {
        d.handling = PrintHandling::Poster;
        d.poster_scale = 500.0;
    });
    h.get_by_label("Scale: 500%");
    h.get_by_label("Sheets: 9");
    h.get_by_label("A4 - 210 × 297 mm [529,17 × 705,56 mm]");
    h.get_by_label("Sheet 1 of 9");
    // The drawing is the whole page with the grid over it, not one zoomed tile.
    h.get_by_label("Poster preview: 3 × 3 tiles, tile 1 highlighted");
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 9");
    h.get_by_label("Scale: 500%");
    h.get_by_label("Poster preview: 3 × 3 tiles, tile 2 highlighted");
    // The default 200 %: two tiles.
    let h = dialog_with(|d| d.handling = PrintHandling::Poster);
    h.get_by_label("Sheets: 2");
    h.get_by_label("A4 - 210 × 297 mm [211,67 × 282,22 mm]");
    // The tiles of one page form one run, which covers the whole page; a page printed twice
    // gives two runs.
    let draft = PrintDraft { handling: PrintHandling::Poster, poster_scale: 500.0, ..Default::default() };
    let settings = draft.settings(1, &[]).unwrap();
    let sheets = print::layout(&[(300.0, 400.0)], &settings).unwrap();
    assert_eq!(poster_run(&sheets, 4), Some(0..9));
    let union = sheets
        .iter()
        .map(|s| s.placed[0].clip)
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, c| [u[0].min(c[0]), u[1].min(c[1]), u[2].max(c[2]), u[3].max(c[3])]);
    assert_eq!(union, [0.0, 0.0, 300.0, 400.0]);
    let twice = print::layout(&[(300.0, 400.0)], &print::Settings { pages: vec![0, 0], ..settings }).unwrap();
    assert_eq!(twice.len(), 18);
    assert_eq!(poster_run(&twice, 10), Some(9..18));
    assert_eq!(poster_run(&twice, 99), None);
    let size = print::layout(&[(300.0, 400.0)], &PrintDraft::default().settings(1, &[]).unwrap()).unwrap();
    assert_eq!(poster_run(&size, 0), None);

    // What the preview draws. The whole 300 × 400 page fits 424 × 524 less the 12-point margin
    // at 1.25 points per point, centred: 375 × 500.
    let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(424.0, 524.0));
    let g = poster_preview(&sheets, 0..9, &[(300.0, 400.0)], None, area, 0).unwrap();
    assert_eq!(g.page, 0);
    assert_eq!(g.zoom, 1.25);
    assert!(close(g.page_rect, Rect::from_center_size(area.center(), vec2(375.0, 500.0))), "{:?}", g.page_rect);
    assert!(close(g.uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))), "{:?}", g.uv);
    // Nine tiles, numbered in sheet order, in a 3 × 3 grid from the top left, row by row, that
    // together cover exactly the page.
    assert_eq!(g.grid, (3, 3));
    assert_eq!(g.tiles.iter().map(|t| t.0).collect::<Vec<_>>(), (1..=9).collect::<Vec<_>>());
    assert!(close(g.tiles.iter().fold(Rect::NOTHING, |u, t| u.union(t.1)), g.page_rect));
    for (i, &(_, r)) in g.tiles.iter().enumerate() {
        assert!(g.page_rect.expand(1e-3).contains_rect(r), "tile {i} {r:?}");
        let (row, col) = (i / 3, i % 3);
        if col > 0 {
            assert!(r.left() > g.tiles[i - 1].1.left() && (r.top() - g.tiles[i - 1].1.top()).abs() < 1e-3, "tile {i} {r:?}");
        }
        if row > 0 {
            assert!(r.top() > g.tiles[i - 3].1.top() && (r.left() - g.tiles[i - 3].1.left()).abs() < 1e-3, "tile {i} {r:?}");
        }
    }
    assert!((g.tiles[0].1.min - g.page_rect.min).length() < 1e-3);
    assert!((g.tiles[8].1.max - g.page_rect.max).length() < 1e-3);
    assert!(g.tiles[4].1.contains(g.page_rect.center()));
    // The highlighted tile is the current sheet's; a sheet outside the poster marks none.
    for j in 0..9 {
        assert_eq!(poster_preview(&sheets, 0..9, &[(300.0, 400.0)], None, area, j).unwrap().current, Some(j));
    }
    assert_eq!(poster_preview(&sheets, 0..9, &[(300.0, 400.0)], None, area, 9).unwrap().current, None);
    // The second copy of a page printed twice: the same picture, numbered from 1 again.
    let again = poster_preview(&twice, 9..18, &[(300.0, 400.0)], None, area, 10).unwrap();
    assert_eq!((again.page_rect, again.current, again.tiles.len(), again.tiles[0].0), (g.page_rect, Some(1), 9, 1));
    // A window printed as a poster: the window, not the page, fills the preview.
    let window = PrintDraft {
        handling: PrintHandling::Poster,
        area: PrintArea::Window,
        region: Some([0.0, 0.0, 200.0, 141.0]),
        poster_scale: 500.0,
        ..Default::default()
    };
    let wsheets = print::layout(&[(300.0, 400.0)], &window.settings(1, &[]).unwrap()).unwrap();
    let run = poster_run(&wsheets, 0).unwrap();
    let w = poster_preview(&wsheets, run.clone(), &[(300.0, 400.0)], window.region, area, 0).unwrap();
    assert_eq!(w.zoom, 2.0);
    assert!(close(w.page_rect, Rect::from_center_size(area.center(), vec2(400.0, 282.0))), "{:?}", w.page_rect);
    assert!(close(w.uv, Rect::from_min_max(pos2(0.0, 1.0 - 141.0 / 400.0), pos2(200.0 / 300.0, 1.0))), "{:?}", w.uv);
    assert_eq!(w.tiles.len(), run.len());
    assert!(close(w.tiles.iter().fold(Rect::NOTHING, |u, t| u.union(t.1)), w.page_rect));
    // Nothing to draw: no poster preview, no panic.
    assert_eq!(poster_preview(&sheets, 0..9, &[], None, area, 0), None);
    assert_eq!(poster_preview(&sheets, 50..60, &[(300.0, 400.0)], None, area, 0), None);
    assert!(poster_preview(&sheets, 0..usize::MAX, &[(300.0, 400.0)], None, Rect::NOTHING, 0).is_none_or(|g| g.tiles.len() == 9));
}

fn close(a: Rect, b: Rect) -> bool {
    (a.min - b.min).length() < 1e-3 && (a.max - b.max).length() < 1e-3
}

#[test]
fn control_verbs_set_the_print_area_and_poster_scale() {
    let mut app = PdfCraftApp::new();
    app.set_option("print-area", "window").unwrap();
    assert_eq!(app.print_draft.area, PrintArea::Window);
    app.set_option("print-area", "full").unwrap();
    assert_eq!(app.print_draft.area, PrintArea::FullPage);
    assert!(app.set_option("print-area", "sideways").is_err());
    // The old Window tab: a script asking for it gets the Window area, the sizing mode unchanged.
    app.set_option("print-tab", "poster").unwrap();
    app.set_option("print-tab", "window").unwrap();
    assert_eq!((app.print_draft.area, app.print_draft.handling), (PrintArea::Window, PrintHandling::Poster));
    assert!(app.set_option("print-window-output", "poster").is_err(), "the Window-only output choice is gone");
    app.set_option("print-poster-scale", "350%").unwrap();
    assert_eq!(app.print_draft.poster_scale, 350.0);
    for bad in ["5", "5000", "NaN", "inf", "big"] {
        assert!(app.set_option("print-poster-scale", bad).is_err(), "{bad}");
    }
    assert_eq!(app.print_draft.poster_scale, 350.0);
    assert!(app.set_option("print-lock", "on").is_err(), "the proportions lock is gone");
}

#[test]
fn print_dialog_opens_at_its_final_size() {
    // The settings scroll on a short screen; the scroll area must not make the dialog unfold
    // frame by frame (it then also moves under the pointer).
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    let first = (h.get_by_label("A3").rect(), h.get_by_label("Sheets: 1").rect());
    for _ in 0..6 {
        h.run_steps(1);
        assert_eq!((h.get_by_label("A3").rect(), h.get_by_label("Sheets: 1").rect()), first);
    }
}

#[test]
fn cut_stack_dialog_previews_saves_and_refuses_duplex() {
    use pdfcraft_engine::print::{PageOrder, spool::Duplex};
    let mut h = harness();
    // Ten source pages, generated using the headless engine.
    let bytes = h.state().session.create_blank(200.0, 300.0, 10).unwrap();
    h.state_mut().open_bytes("numbered.pdf", None, bytes.as_ref().clone()).unwrap();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    // Pick the new order through the actual widgets.
    h.get_by_value("Horizontal").click();
    h.run_steps(2);
    h.get_by_label("Cut and stack").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.order, PageOrder::CutStack);
    h.state_mut().print_draft.per_sheet = 4;
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 3");
    h.get_by_label_contains("Keep the sheets in order");
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 3");
    h.state_mut().print_draft.duplex = Duplex::LongEdge;
    h.run_steps(2);
    h.get_by_label_contains("Cut and stack needs Two-sided: Off");
    assert!(!h.state_mut().print_now());
    h.state_mut().print_draft.duplex = Duplex::Off;
    h.state_mut().print_draft.printer = None;
    // Chosen, so a printer list arriving late doesn't pick a real printer instead.
    h.state_mut().print_draft.printer_chosen = true;
    let out = std::env::temp_dir().join(format!("pdfcraft-cut-stack-ui-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let printed = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(&out).unwrap())).unwrap();
    assert_eq!(pdfcraft_model::pages(&printed).len(), 3);
    let _ = std::fs::remove_file(out);
}

#[test]
fn a_failed_save_as_pdf_keeps_the_dialog_open() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().print_draft.printer = None;
    h.state_mut().print_draft.printer_chosen = true;
    // A folder that doesn't exist: the write fails.
    let out = std::env::temp_dir().join(format!("pdfcraft-missing-{}", std::process::id())).join("x.pdf");
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the dialog stays open, with its settings");
    let toast = h.state().toast.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert!(toast.contains("Could not save"), "the user is told why: {toast:?}");
}

/// The dialog on form.pdf printing to `out` (Save as PDF, as there is no printer here).
fn saving_dialog(out: &std::path::Path) -> Harness<'static, PdfCraftApp> {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.state_mut().print_draft.printer_chosen = true;
    h.run_steps(2);
    h
}

/// R2: "I type 1-2 in Pages and press Enter: Enter doesn't print."
#[test]
fn enter_prints_from_the_pages_field() {
    let out = std::env::temp_dir().join(format!("pdfcraft-enter-pages-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let mut h = saving_dialog(&out);
    // Choose Pages, click into its field, type, press Enter.
    h.get_by(|n| n.role() == egui::accesskit::Role::RadioButton && n.label().as_deref() == Some("Pages")).click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.which, pdfcraft_ui_egui::PrintWhich::Range);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Pages").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Pages").type_text("1");
    h.run_steps(1);
    assert_eq!(h.state().print_draft.range, "1");
    assert!(h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Pages").is_focused(), "typing in the field");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialog, None, "Enter printed and closed the dialog");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(&out).expect("printed"))).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
    let _ = std::fs::remove_file(out);
}

#[test]
fn enter_prints_with_nothing_focused_and_commits_a_typed_number() {
    let out = std::env::temp_dir().join(format!("pdfcraft-enter-copies-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let mut h = saving_dialog(&out);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialog, None, "Enter is the Print button");
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
    // A custom scale typed into its field, then Enter: printed with the new value.
    let mut h = saving_dialog(&out);
    h.state_mut().print_draft.size = SizeMode::Custom(100.0);
    h.state_mut().print_draft.custom_scale = 100.0;
    h.run_steps(2);
    h.get_by_value("100 %").click();
    h.run_steps(2);
    h.get_by(|n| n.role() == egui::accesskit::Role::SpinButton && n.is_focused()).type_text("50");
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    assert_eq!(h.state().print_draft.custom_scale, 50.0, "the typed value counts");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(&out).expect("printed"))).unwrap();
    let page = &pdfcraft_model::pages(&doc)[0];
    let contents = page.dict.get(b"Contents").expect("contents").clone();
    let content = match &*doc.resolve(&contents) {
        pdfcraft_cos::Object::Stream(st) => String::from_utf8_lossy(&st.decoded().unwrap()).into_owned(),
        other => panic!("{other:?}"),
    };
    assert!(content.starts_with("q 0.5 0 0 0.5 "), "printed at 50 %: {content}");
    let _ = std::fs::remove_file(out);
}

#[test]
fn enter_does_not_print_while_a_menu_or_the_picker_is_open() {
    let out = std::env::temp_dir().join(format!("pdfcraft-enter-menu-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let mut h = saving_dialog(&out);
    // An open combo box takes Enter (to pick its item).
    h.get_by_value("Document and markups").click();
    h.run_steps(2);
    h.get_by_label("Form fields only");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "a menu was open: no print");
    assert!(!out.exists());
    // The window picker: Enter does nothing there either.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.state_mut().print_draft.area = PrintArea::Window;
    h.run_steps(2);
    h.get_by_label("Select area…").click();
    h.run_steps(3);
    assert!(h.state().print_draft.picking);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    assert!(h.state().print_draft.picking, "still picking");
    assert!(!out.exists());
    // Escape still cancels: the picker first, then the dialog.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().dialog, None);
    assert!(!out.exists(), "cancelled, not printed");
}

#[test]
fn quick_print_prepares_fit_sheets_on_a4_without_the_dialog() {
    let (pdf, sheets) = pdfcraft_ui_egui::quick_print::prepare("form.pdf", include_bytes!("data/form.pdf").to_vec()).unwrap();
    assert_eq!(sheets, 1);
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(pdf)).unwrap();
    let pages = pdfcraft_model::pages(&doc);
    assert_eq!(pages.len(), 1);
    // The form's 300 × 400 pt page prints on A4; an A3 drawing (landscape) on A3, Fit, turned.
    let settings = pdfcraft_ui_egui::quick_print::settings(&[(300.0, 400.0)]);
    assert_eq!((settings.paper, settings.layout, settings.orientation), (A4, print::Layout::Size(SizeMode::Fit), Orientation::Auto));
    let drawing = pdfcraft_ui_egui::quick_print::settings(&[(1190.55, 841.89)]);
    let sheets = print::layout(&[(1190.55, 841.89)], &drawing).unwrap();
    let sheet = sheets[0].size;
    assert!((sheet.0 - print::A3.1).abs() < 0.01 && (sheet.1 - print::A3.0).abs() < 0.01, "a landscape A3 sheet: {sheet:?}");
    assert!((sheets[0].placed[0].scale() - 1.0).abs() < 1e-6, "A3 on A3 prints at 100 %");
}
