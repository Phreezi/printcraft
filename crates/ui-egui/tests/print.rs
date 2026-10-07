//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use printcraft_engine::print::Orientation;
use printcraft_ui_egui::print_ui::{
    PAPER_CHOICES, PickDrag, area_label, drag_window, fit_aspect, move_window, printable_aspect, window_fit_percent, window_landscape,
};
use printcraft_ui_egui::{Dialog, PrintCraftApp, PrintHandling, PrintWindowOutput};

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
    h.get_by_label("Pages to Print".to_uppercase().as_str());
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
    h.state_mut().print_draft.lock_aspect = false;
    h.run_steps(2);
    h.get_by_label_contains("Area: 71 × 50 mm");
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
}

#[test]
fn window_geometry() {
    let a4 = printcraft_engine::print::A4;
    let r = printable_aspect(a4, false);
    assert!((r - (595.28 - 36.0) / (841.89 - 36.0)).abs() < 1e-9);
    assert!((printable_aspect(a4, true) - 1.0 / r).abs() < 1e-9);
    assert!(window_landscape(Orientation::Auto, 300.0, 200.0));
    assert!(!window_landscape(Orientation::Portrait, 300.0, 200.0));
    // Locked: the dragged box grows to the sheet's shape, covering the drag.
    let page = (1000.0, 1000.0);
    let w = drag_window((100.0, 100.0), (400.0, 150.0), page, Some(1.0 / r));
    assert!(((w[2] - w[0]) / (w[3] - w[1]) - 1.0 / r).abs() < 1e-6, "{w:?}");
    assert!((w[2] - w[0] - 300.0).abs() < 1e-6, "covers the drag");
    // Dragging up and left works too, and the box stays on the page.
    let w = drag_window((100.0, 100.0), (-500.0, -500.0), page, Some(1.0));
    assert_eq!(w, [0.0, 0.0, 100.0, 100.0]);
    // Unlocked: just the box, clamped.
    assert_eq!(drag_window((10.0, 20.0), (1200.0, 5.0), page, None), [10.0, 5.0, 1000.0, 20.0]);
    // Reshaping keeps the centre and stays on the page; already the shape: unchanged.
    let f = fit_aspect([400.0, 400.0, 600.0, 500.0], 1.0, page);
    assert!(((f[2] - f[0]) - (f[3] - f[1])).abs() < 1e-6 && ((f[0] + f[2]) / 2.0 - 500.0).abs() < 1e-6, "{f:?}");
    assert_eq!(fit_aspect([0.0, 0.0, 200.0, 100.0], 2.0, page), [0.0, 0.0, 200.0, 100.0]);
    let big = fit_aspect([0.0, 0.0, 1000.0, 1000.0], 2.0, (1000.0, 300.0));
    assert!(big[2] <= 1000.0 + 1e-9 && big[3] <= 300.0 + 1e-9, "{big:?}");
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
    app.print_draft.lock_aspect = false;
    app.print_draft.duplex = printcraft_engine::print::spool::Duplex::LongEdge;
    let saved = app.persist();
    let mut again = PrintCraftApp::new();
    again.restore(&saved);
    let d = &again.print_draft;
    assert_eq!(d.printer.as_deref(), Some("Office"));
    assert!(d.printer_chosen && d.grayscale && !d.lock_aspect);
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
