//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

use egui::{Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use printcraft_engine::print::{self, A4, Orientation, SizeMode};
use printcraft_ui_egui::print_ui::{
    PAPER_CHOICES, PickDrag, area_label, decimal_comma, drag_window, move_window, poster_run, printable_aspect, scale_label, sheet_label,
    window_fit_percent, window_landscape,
};
use printcraft_ui_egui::{Dialog, PrintCraftApp, PrintDraft, PrintHandling, PrintWindowOutput};

fn harness() -> Harness<'static, PrintCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PrintCraftApp::new();
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
    let out = std::env::temp_dir().join(format!("printcraft-print-test-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    // Chosen, so a printer list arriving late doesn't pick a real printer instead.
    h.state_mut().print_draft.printer_chosen = true;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let bytes = std::fs::read(&out).expect("saved");
    let doc = printcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert_eq!(printcraft_model::pages(&doc).len(), 1);
    let _ = std::fs::remove_file(out);
}

#[test]
fn invalid_ranges_are_explained_in_the_preview() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(2);
    h.state_mut().print_draft.which = printcraft_ui_egui::PrintWhich::Range;
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
    assert_eq!(s.paper, printcraft_engine::print::A4);
    h.get_by_label("A3").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.settings(1, &[]).unwrap().paper, printcraft_engine::print::A3);
}

#[test]
fn window_tab_prints_the_selected_area() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Window").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.handling, PrintHandling::Window);
    h.get_by_label_contains("No area yet");
    // An area (as the picker would leave it): printed alone, filling the sheet.
    h.state_mut().print_draft.region = Some([0.0, 0.0, 200.0, 141.0]);
    h.run_steps(2);
    h.get_by_label_contains("Area: 71 × 50 mm");
    // No "Whole page" (the Size tab prints whole pages) and no proportions check box (Shift).
    assert!(h.query_by_label("Whole page").is_none());
    assert!(h.query_by_label_contains("Keep the sheet").is_none());
    h.get_by_label_contains("hold Shift while drawing");
    let s = h.state().print_draft.settings(1, &[]).unwrap();
    assert_eq!(s.region, Some([0.0, 0.0, 200.0, 141.0]));
    assert!(matches!(s.layout, printcraft_engine::print::Layout::Size(printcraft_engine::print::SizeMode::Fit)));
    h.get_by_label_contains("Prints at");
    // As a poster instead: tiles.
    h.get_by_label("As a poster").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.window_output, PrintWindowOutput::Poster);
    assert!(matches!(h.state().print_draft.settings(1, &[]).unwrap().layout, printcraft_engine::print::Layout::Poster { .. }));
    // Other tabs print whole pages.
    h.get_by_label("Size").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.settings(1, &[]).unwrap().region, None);
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
fn drag(h: &mut Harness<'static, PrintCraftApp>, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
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
    // With Shift: the sheet's proportions (a wide box: a landscape A4's printable area). (Drawn
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
    let a4 = printcraft_engine::print::A4;
    let r = printable_aspect(a4, false);
    assert!((r - (595.28 - 36.0) / (841.89 - 36.0)).abs() < 1e-9);
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
    // A window the sheet's shape fills the printable area: an A4 page's printable area prints at 100%.
    let full = [0.0, 0.0, 595.28 - 36.0, 841.89 - 36.0];
    assert!((window_fit_percent(full, a4, Orientation::Auto) - 100.0).abs() < 1e-6);
    let a3 = 100.0 * ((841.89 - 36.0) / (595.28_f64 - 36.0)).min((1190.55 - 36.0) / (841.89 - 36.0));
    assert!((window_fit_percent(full, printcraft_engine::print::A3, Orientation::Auto) - a3).abs() < 1e-6, "about 143% on A3");
    assert_eq!(area_label([0.0, 0.0, 841.89, 595.28]), "297 × 210 mm");
}

#[test]
fn print_settings_are_remembered() {
    let mut app = PrintCraftApp::new();
    app.print_draft.printer = Some("Office".into());
    app.print_draft.printer_chosen = true;
    app.print_draft.paper = 1;
    app.print_draft.grayscale = true;
    app.print_draft.duplex = printcraft_engine::print::spool::Duplex::LongEdge;
    let saved = app.persist();
    let mut again = PrintCraftApp::new();
    again.restore(&saved);
    let d = &again.print_draft;
    assert_eq!(d.printer.as_deref(), Some("Office"));
    assert!(d.printer_chosen && d.grayscale);
    // The proportions check box is gone (Shift while dragging): not saved any more, and a file
    // from before, which has it, still loads.
    assert!(app.print_draft.prefs().get("lock_aspect").is_none());
    let mut old = PrintCraftApp::new();
    old.restore(r#"{"print": {"paper": "A3", "lock_aspect": false, "window_poster": true}}"#);
    assert_eq!(old.print_draft.paper, 1);
    assert_eq!(old.print_draft.window_output, PrintWindowOutput::Poster);
    assert_eq!(d.paper, 1);
    assert_eq!(d.duplex, printcraft_engine::print::spool::Duplex::LongEdge);
    // Untrusted settings: nonsense keeps the defaults.
    let mut odd = PrintCraftApp::new();
    odd.restore(r#"{"print": {"paper": "Letter", "dpi": 7, "printer": 12, "duplex": "sideways"}}"#);
    assert_eq!(odd.print_draft.paper, 0);
    assert_eq!(odd.print_draft.dpi, 300);
    assert_eq!(odd.print_draft.printer, None);
}

#[test]
fn a_printer_list_arriving_keeps_the_users_choice() {
    use printcraft_engine::print::spool::Printer;
    let list = || vec![Printer { name: "A".into(), default: false }, Printer { name: "B".into(), default: true }];
    let mut d = printcraft_ui_egui::PrintDraft { listing: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("B"), "nothing chosen: the default");
    assert!(!d.listing);
    let mut d = printcraft_ui_egui::PrintDraft { printer: Some("A".into()), printer_chosen: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("A"), "the chosen printer stays");
    let mut d = printcraft_ui_egui::PrintDraft { printer: Some("Gone".into()), printer_chosen: true, ..Default::default() };
    d.printers_arrived(list());
    assert_eq!(d.printer.as_deref(), Some("B"), "a printer that is gone falls back to the default");
    let mut d = printcraft_ui_egui::PrintDraft { printer: None, printer_chosen: true, ..Default::default() };
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
fn dialog_with(set: impl FnOnce(&mut PrintDraft)) -> Harness<'static, PrintCraftApp> {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    set(&mut h.state_mut().print_draft);
    h.run_steps(3);
    h
}

#[test]
fn preview_reports_the_real_scale_and_printed_size() {
    // Fit: the 300 × 400 page fills A4's printable 559.28 × 805.89 at 186 %.
    let h = dialog_with(|_| {});
    h.get_by_label("Scale: 186%");
    h.get_by_label("Sheets: 1");
    h.get_by_label("A4 - 210 × 297 mm [197,30 × 263,07 mm]");
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
    // A window, fitted to the sheet: the same number as "Prints at".
    let h = dialog_with(|d| {
        d.handling = PrintHandling::Window;
        d.region = Some([0.0, 0.0, 200.0, 141.0]);
        d.window_output = PrintWindowOutput::Fit;
    });
    h.get_by_label("Scale: 397%");
    h.get_by_label("A4 - 297 × 210 mm [279,86 × 197,30 mm]");
    h.get_by_label_contains("Prints at 397%");
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
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 9");
    h.get_by_label("Scale: 500%");
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
}

#[test]
fn control_verbs_set_the_window_output_and_poster_scale() {
    let mut app = PrintCraftApp::new();
    app.set_option("print-window-output", "poster").unwrap();
    assert_eq!(app.print_draft.window_output, PrintWindowOutput::Poster);
    app.set_option("print-window-output", "fit").unwrap();
    assert_eq!(app.print_draft.window_output, PrintWindowOutput::Fit);
    assert!(app.set_option("print-window-output", "sideways").is_err());
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
