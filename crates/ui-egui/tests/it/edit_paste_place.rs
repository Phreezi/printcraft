//! PeDeeFe: where Edit text & images pastes, what a click takes when a picture lies over a
//! paragraph, and a click off the pages. Synthetic text and pictures only.
use egui::{Modifiers, Pos2};
use egui_kittest::Harness;
use pdfcraft_engine::{Edit, ImageEdit, PageImage};
use pdfcraft_ui_egui::{Clip, PdfCraftApp};

/// A 400 × 500 page with one long paragraph (eight lines of Helvetica from y 430 down).
fn paragraph_pdf() -> Vec<u8> {
    let stream = |body: &str| format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
    let lines: String =
        (0..8).map(|n| format!("{}(Line {n} of a long paragraph that fills the box) Tj ", if n == 0 { "" } else { "0 -14 Td " })).collect();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        stream(&format!("BT /F1 12 Tf 14 TL 40 430 Td {lines}ET")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("paragraph.pdf", None, paragraph_pdf()).unwrap();
        app.set_option("zoom", "100").unwrap();
        assert!(app.execute("edit.edit_text"));
        app
    });
    crate::support::settle(&mut h);
    h
}

/// The screen point of page point (`x`, `y`), from the page's top-left corner as shown.
fn at(h: &Harness<'static, PdfCraftApp>, x: f32, y: f32) -> Pos2 {
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    r.min + egui::vec2(x, y) * (r.width() / 400.0)
}

fn images(h: &Harness<'static, PdfCraftApp>) -> Vec<PageImage> {
    let app = h.state();
    app.session.get(app.views[0].id).unwrap().page_images(0)
}

fn click(h: &mut Harness<'static, PdfCraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(4);
}

/// ⌘V (Ctrl+V) with the pointer at `p`.
fn paste_at(h: &mut Harness<'static, PdfCraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(1);
    }
    h.run_steps(3);
}

fn green(w: usize, h: usize) -> Clip {
    Clip::Image { width: w, height: h, rgba: [40u8, 200, 90, 255].repeat(w * h) }
}

fn centre(r: [f64; 4]) -> (f64, f64) {
    ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
}

/// ⌘V in Edit text & images with a picture on the clipboard pastes it centred on the pointer; by
/// the page's edge it moves onto the page, whole, where it can be edited.
#[test]
fn command_v_pastes_a_picture_centred_on_the_pointer_and_keeps_it_on_the_page() {
    let mut h = harness();
    h.state_mut().clipboard_override = Some(green(40, 20));
    let p = at(&h, 300.0, 100.0);
    paste_at(&mut h, p);
    let pasted = images(&h);
    assert_eq!(pasted.len(), 1, "pasted");
    let (cx, cy) = centre(pasted[0].rect);
    assert!((cx - 300.0).abs() < 1.5 && (cy - 400.0).abs() < 1.5, "centred on the pointer (300, 400 up): {:?}", pasted[0].rect);
    assert!(h.state().views[0].image_selection.is_some(), "and selected");

    // At the page's bottom-right corner it stays on the page.
    h.state_mut().clipboard_override = Some(green(40, 20));
    let p = at(&h, 399.0, 499.0);
    paste_at(&mut h, p);
    let pasted = images(&h);
    assert_eq!(pasted.len(), 2);
    let r = pasted[1].rect;
    assert!(r[0] >= -0.5 && r[1] >= -0.5 && r[2] <= 400.5 && r[3] <= 500.5, "on the page: {r:?}");
    assert!((r[2] - 400.0).abs() < 1.5 && r[1].abs() < 1.5, "in its corner: {r:?}");
}

/// A picture copied with Copy Image after it was resized pastes at the size it was resized to.
#[test]
fn a_copied_picture_pastes_at_the_size_it_shows() {
    let mut h = harness();
    h.state_mut().clipboard_override = Some(green(40, 20));
    let p = at(&h, 100.0, 300.0);
    paste_at(&mut h, p);
    assert_eq!(images(&h).len(), 1);
    // Resized to 200 × 100.
    let id = h.state().views[0].id;
    h.state_mut().session.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([20.0, 40.0, 220.0, 140.0]) }).unwrap();
    h.run_steps(3);
    h.state_mut().clipboard_override = None;
    assert!(h.state_mut().copy_page_image(0, 0));
    let p = at(&h, 250.0, 150.0);
    paste_at(&mut h, p);
    let pasted = images(&h);
    assert_eq!(pasted.len(), 2, "pasted again");
    let r = pasted[1].rect;
    assert!(((r[2] - r[0]) - 200.0).abs() < 1.0 && ((r[3] - r[1]) - 100.0).abs() < 1.0, "200 × 100, as it was resized: {r:?}");
    let (cx, cy) = centre(r);
    assert!((cx - 250.0).abs() < 1.5 && (cy - 350.0).abs() < 1.5, "centred on the pointer: {r:?}");
}

/// The page menu's Paste puts the picture's top-left corner where the menu opened.
#[test]
fn paste_from_the_page_menu_puts_the_top_left_corner_at_the_point() {
    let mut h = harness();
    h.state_mut().clipboard_override = Some(green(60, 40));
    assert!(h.state_mut().paste_image_at(0, [120.0, 180.0]));
    let r = images(&h)[0].rect;
    assert!((r[0] - 120.0).abs() < 1.0 && (r[3] - 180.0).abs() < 1.0, "top-left corner at (120, 180): {r:?}");
    assert!(((r[2] - r[0]) - 60.0).abs() < 1.0 && ((r[3] - r[1]) - 40.0).abs() < 1.0, "{r:?}");
}

/// A picture placed over a paragraph larger than itself takes the click: it is selected and
/// moves, rather than the paragraph opening for typing under it.
#[test]
fn a_picture_over_a_long_paragraph_takes_the_click() {
    let mut h = harness();
    h.state_mut().clipboard_override = Some(green(40, 20));
    // Over the paragraph's second and third lines.
    let p = at(&h, 120.0, 92.0);
    paste_at(&mut h, p);
    assert_eq!(images(&h).len(), 1);
    // Nothing selected, then a click on the picture.
    let off = at(&h, -60.0, 250.0);
    click(&mut h, off);
    assert!(h.state().views[0].image_selection.is_none() && h.state().views[0].line_editor.is_none());
    click(&mut h, p);
    assert!(h.state().views[0].line_editor.is_none(), "the paragraph under the picture stays closed");
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(0), "the picture is selected");
    // Off the picture, on the paragraph: the paragraph opens instead.
    let text = at(&h, 250.0, 120.0);
    click(&mut h, text);
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph opens");
    assert!(h.state().views[0].image_selection.is_none(), "and the picture lets go");
}

/// A click off the page (the grey beside it) lets go of what is selected.
#[test]
fn a_click_off_the_page_lets_go_of_the_selection() {
    let mut h = harness();
    h.state_mut().clipboard_override = Some(green(40, 20));
    let p = at(&h, 300.0, 300.0);
    paste_at(&mut h, p);
    assert!(h.state().views[0].image_selection.is_some(), "the pasted picture is selected");
    let off = at(&h, -60.0, 250.0);
    assert!(!h.state().views[0].page_screen_rect(0).unwrap().contains(off), "beside the page");
    click(&mut h, off);
    assert!(h.state().views[0].image_selection.is_none(), "nothing selected");
    assert!(h.state().views[0].edit_selection.is_none());
}
