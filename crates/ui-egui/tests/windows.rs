//! Several windows (R5): tabs move between windows and into new ones, a window left without tabs
//! closes, the root window takes over another when it empties, closing a window asks about its
//! unsaved documents, and quitting asks in every window.
//!
//! kittest has no native windows: other windows are drawn embedded in the root (as on the web),
//! which exercises their drawing; the moves themselves are driven through the model.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::DocId;
use pdfcraft_ui_egui::{CloseRequest, PdfCraftApp, ROOT_WINDOW, TabTarget};

/// `n` pages of 200×300.
fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    for _ in 0..n {
        objs.push("<< /Type /Page /Parent 2 0 R >>".into());
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// The root window with a.pdf, b.pdf and c.pdf.
fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        for name in ["a.pdf", "b.pdf", "c.pdf"] {
            app.open_bytes(name, None, fixture(2)).expect("opens");
        }
        app
    });
    h.run_steps(4);
    h
}

fn doc(h: &Harness<'_, PdfCraftApp>, name: &str) -> DocId {
    h.state().session.docs().iter().find(|d| d.name == name).map(|d| d.id).expect("open")
}

fn names(h: &Harness<'_, PdfCraftApp>, key: u64) -> Vec<String> {
    h.state().window_tabs(key).into_iter().filter_map(|id| h.state().session.get(id).map(|d| d.name.clone())).collect()
}

#[test]
fn tabs_move_to_a_new_window_and_back_and_an_empty_window_closes() {
    let mut h = harness();
    let (a, b, c) = (doc(&h, "a.pdf"), doc(&h, "b.pdf"), doc(&h, "c.pdf"));
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    assert_ne!(w, ROOT_WINDOW);
    h.run_steps(3);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW, w]);
    assert_eq!(names(&h, ROOT_WINDOW), ["a.pdf", "c.pdf"]);
    assert_eq!(names(&h, w), ["b.pdf"]);
    assert_eq!(h.state().window_active(w), Some(b), "the moved tab is active in its new window");
    assert_eq!(h.state().windows.focus(), w, "the new window comes to the front");
    assert_eq!(h.state().window_of(b), Some(w));
    // The other window is drawn (embedded here) with its own tab strip.
    assert!(h.query_all_by_label("b.pdf").count() >= 1);

    // Dropped onto the new window's strip, before its first tab.
    assert_eq!(h.state_mut().move_tab(c, TabTarget::Window(w, Some(0))), Some(w));
    h.run_steps(2);
    assert_eq!(names(&h, w), ["c.pdf", "b.pdf"]);
    assert_eq!(names(&h, ROOT_WINDOW), ["a.pdf"]);
    assert_eq!(h.state().window_active(ROOT_WINDOW), Some(a), "the root keeps a tab active");

    // Along a strip: a new place, same window.
    assert_eq!(h.state_mut().move_tab(c, TabTarget::Window(w, None)), Some(w));
    assert_eq!(names(&h, w), ["b.pdf", "c.pdf"]);

    // Back to the root; the window closes once its last tab has left.
    h.state_mut().move_tab(b, TabTarget::Window(ROOT_WINDOW, None));
    h.state_mut().move_tab(c, TabTarget::Window(ROOT_WINDOW, Some(0)));
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["c.pdf", "a.pdf", "b.pdf"]);
    assert_eq!(h.state().views.len(), 3, "the root's tabs are current again");
    assert_eq!(h.state().session.docs().len(), 3, "moving never closes a document");
}

#[test]
fn a_windows_only_tab_moves_the_window_and_unknown_targets_are_refused() {
    let mut h = harness();
    let b = doc(&h, "b.pdf");
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    // Its only tab dragged into empty space: the window itself moves, no third window.
    assert_eq!(h.state_mut().move_tab(b, TabTarget::NewWindow(Some(egui::pos2(300.0, 200.0)))), Some(w));
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW, w]);
    assert_eq!(h.state_mut().move_tab(b, TabTarget::Window(99, None)), None, "no such window");
    assert_eq!(h.state_mut().move_tab(DocId(12345), TabTarget::NewWindow(None)), None, "no such document");
    assert_eq!(names(&h, w), ["b.pdf"]);
}

#[test]
fn the_root_window_takes_over_another_when_its_last_tab_leaves() {
    let mut h = harness();
    let (a, b, c) = (doc(&h, "a.pdf"), doc(&h, "b.pdf"), doc(&h, "c.pdf"));
    let w = h.state_mut().move_tab(c, TabTarget::NewWindow(None)).expect("a new window");
    h.state_mut().move_tab(a, TabTarget::Window(w, None));
    h.state_mut().move_tab(b, TabTarget::Window(w, None));
    h.run_steps(3);
    // To the user the emptied window closed; the root shows the other window's tabs.
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["c.pdf", "a.pdf", "b.pdf"]);
    assert_eq!(h.state().window_active(ROOT_WINDOW), Some(b));
    assert_eq!(h.state().windows.focus(), ROOT_WINDOW);
}

#[test]
fn closing_the_last_tab_of_a_window_closes_it_but_the_only_window_shows_home() {
    let mut h = harness();
    let b = doc(&h, "b.pdf");
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    h.run_steps(2);
    // The window's tabs are current between frames (it is the focus window).
    assert_eq!(h.state().windows.current(), w);
    h.state_mut().request_close_tab(0);
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["a.pdf", "c.pdf"]);
    // The only window: closing its tabs leaves Home, as before.
    h.state_mut().execute("file.close_all");
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert!(h.state().views.is_empty() && h.state().active.is_none());
}

#[test]
fn closing_a_window_asks_about_its_unsaved_documents_there() {
    let mut h = harness();
    let (b, c) = (doc(&h, "b.pdf"), doc(&h, "c.pdf"));
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    h.state_mut().move_tab(c, TabTarget::Window(w, None));
    h.run_steps(2);
    // Change b.pdf (in window w, current between frames).
    h.state_mut().active = Some(0);
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }));
    h.state_mut().close_window(w);
    h.run_steps(2);
    assert_eq!(names(&h, w), ["b.pdf"], "the clean document closed at once");
    assert_eq!(h.state().close_request, Some(CloseRequest::All));
    assert_eq!(h.state().windows.focus(), w, "the question is asked in that window");
    h.get_by_label_contains("Save changes to “b.pdf”");
    // Cancel keeps the window and its document.
    let ctx = h.ctx.clone();
    h.state_mut().resolve_close(&ctx, None);
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW, w]);
    // Don't save: the document closes, and so does its window.
    h.state_mut().close_window(w);
    h.run_steps(1);
    h.state_mut().resolve_close(&ctx, Some(false));
    h.run_steps(2);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["a.pdf"]);
    assert!(h.state().close_request.is_none());
}

#[test]
fn quitting_asks_about_unsaved_documents_in_every_window() {
    let mut h = harness();
    let b = doc(&h, "b.pdf");
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    h.run_steps(2);
    h.state_mut().active = Some(0);
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }));
    // Back in the root (a.pdf and c.pdf are clean): the quit question moves on to window w.
    h.state_mut().windows.request_focus(ROOT_WINDOW);
    h.run_steps(2);
    assert_eq!(h.state().windows.focus(), ROOT_WINDOW);
    h.state_mut().close_request = Some(CloseRequest::Quit);
    h.run_steps(3);
    assert_eq!(h.state().windows.focus(), w, "the window with unsaved changes comes to the front");
    assert_eq!(h.state().close_request, Some(CloseRequest::Quit));
    h.get_by_label_contains("Save changes to “b.pdf”");
    let ctx = h.ctx.clone();
    h.state_mut().resolve_close(&ctx, Some(false));
    h.run_steps(2);
    assert!(h.state().close_request.is_none(), "nothing left to ask: the app quits");
    assert_eq!(h.state().window_of(b), None, "b.pdf was closed without saving");
}

#[test]
fn dragging_a_tab_along_the_strip_reorders_and_never_tears_off_where_windows_are_embedded() {
    let mut h = harness();
    let rect_of = |h: &Harness<'_, PdfCraftApp>, name: &str| h.get_by_label(name).rect();
    let a = rect_of(&h, "a.pdf");
    let c = rect_of(&h, "c.pdf");
    let drag = |h: &mut Harness<'_, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2| {
        h.event(egui::Event::PointerMoved(from));
        h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.run_steps(1);
        for k in 1..=6 {
            h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 6.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.run_steps(2);
    };
    // a.pdf dragged past c.pdf's middle: it goes last.
    drag(&mut h, a.center(), c.center() + egui::vec2(c.width() * 0.4, 0.0));
    assert_eq!(names(&h, ROOT_WINDOW), ["b.pdf", "c.pdf", "a.pdf"]);
    // Dragged down onto the page: no new window here (kittest embeds windows, like the web).
    let b = rect_of(&h, "b.pdf");
    drag(&mut h, b.center(), b.center() + egui::vec2(40.0, 400.0));
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["b.pdf", "c.pdf", "a.pdf"]);
}

/// A queue of [`OsEvent`]s the app polls, and the handle to add to it.
type OsQueue = std::rc::Rc<std::cell::RefCell<Vec<pdfcraft_ui_egui::OsEvent>>>;

fn os_queue(app: &mut PdfCraftApp) -> OsQueue {
    let queue: OsQueue = Default::default();
    let q = queue.clone();
    app.os_events = Some(Box::new(move || q.borrow_mut().drain(..).collect()));
    queue
}

fn temp_pdf(tag: &str, pages: usize) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("pdfcraft-{tag}-{}.pdf", std::process::id()));
    std::fs::write(&path, fixture(pages)).unwrap();
    path
}

/// R5: a file forwarded by another launch (or a relaunch) brings a minimized window back. eframe
/// runs only `App::logic` while every window is minimized, so the window has to be restored and
/// focused from there, not from the `ui` pass that never comes.
#[test]
fn a_forwarded_file_or_a_relaunch_restores_a_minimized_window_from_logic_alone() {
    use eframe::App as _;
    use egui::{ViewportCommand, ViewportId, ViewportInfo};
    let path = temp_pdf("minimized", 1);
    let ctx = egui::Context::default();
    let mut app = PdfCraftApp::new();
    let queue = os_queue(&mut app);
    let mut frame = eframe::Frame::_new_kittest();
    let mut input = egui::RawInput::default();
    input.viewports.insert(ViewportId::ROOT, ViewportInfo { minimized: Some(true), ..Default::default() });
    let raise = [ViewportCommand::Minimized(false), ViewportCommand::Focus];
    for event in [pdfcraft_ui_egui::OsEvent::Open(vec![path.to_string_lossy().into_owned()]), pdfcraft_ui_egui::OsEvent::Activate] {
        queue.borrow_mut().push(event);
        let out = ctx.run_logic(&input, |ctx| app.logic(ctx, &mut frame));
        let commands = out.viewport_commands.get(&ViewportId::ROOT).cloned().unwrap_or_default();
        assert!(commands.windows(2).any(|w| w == raise), "un-minimized, then focused: {commands:?}");
    }
    std::fs::remove_file(&path).ok();
    assert_eq!(app.views.len(), 1, "the forwarded file opened");
}

/// R5: a file forwarded while a dialog is open waits for it to close: the Print dialog keeps
/// printing the document it was opened for, and a password prompt isn't dropped.
#[test]
fn a_forwarded_file_waits_for_an_open_dialog_or_prompt() {
    use pdfcraft_ui_egui::{Dialog, OsEvent};
    let path = temp_pdf("held", 2);
    let mut h = harness();
    let queue = os_queue(h.state_mut());
    let a = doc(&h, "a.pdf");
    h.state_mut().active = Some(0);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    queue.borrow_mut().push(OsEvent::Open(vec![path.to_string_lossy().into_owned()]));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    assert_eq!(h.state().views.len(), 3, "not opened under the dialog");
    assert_eq!(h.state().active_ids().map(|(_, id)| id), Some(a), "Print still prints a.pdf");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    assert_eq!(h.state().views.len(), 4, "opened once the dialog closed");
    // A password prompt for an encrypted file stays while a forwarded file waits.
    h.state_mut().password_prompt = Some(pdfcraft_ui_egui::PasswordPrompt {
        name: "locked.pdf".into(),
        path: None,
        bytes: std::sync::Arc::new(Vec::new()),
        input: "secr".into(),
        error: None,
    });
    queue.borrow_mut().push(OsEvent::Open(vec![path.to_string_lossy().into_owned()]));
    h.run_steps(3);
    assert!(h.state().password_prompt.as_ref().is_some_and(|p| p.input == "secr"), "the prompt and what was typed stay");
    assert_eq!(h.state().views.len(), 4);
    h.state_mut().password_prompt = None;
    h.run_steps(3);
    assert_eq!(h.state().views.len(), 5);
    std::fs::remove_file(&path).ok();
}

fn close_root(h: &mut Harness<'static, PdfCraftApp>) {
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
}

fn root_close_sent(h: &Harness<'static, PdfCraftApp>) -> bool {
    h.output().viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)))
}

/// R5: "Close all windows" (the Windows taskbar) closes the root window and the others at once:
/// the app quits, rather than the root staying open (empty, or with another window's tabs).
#[test]
fn closing_every_window_together_quits_but_closing_the_root_alone_hands_it_another_windows_tabs() {
    let mut h = harness();
    let b = doc(&h, "b.pdf");
    let w = h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    h.run_steps(2);
    close_root(&mut h);
    h.step();
    assert!(h.state().window_tabs(ROOT_WINDOW).is_empty(), "the root's tabs closed");
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW, w], "the other window isn't taken over yet");
    // The other window's close arrives a moment later.
    h.state_mut().close_window(w);
    h.step();
    assert!(root_close_sent(&h), "every window closed: the app quits");

    // The root window closed on its own: after the moment, it takes over the other window.
    let mut h = harness();
    let b = doc(&h, "b.pdf");
    h.state_mut().move_tab(b, TabTarget::NewWindow(None)).expect("a new window");
    h.run_steps(2);
    close_root(&mut h);
    h.step();
    h.run_steps(pdfcraft_ui_egui::windows::CLOSE_ALL_FRAMES as usize + 4);
    assert_eq!(h.state().windows.keys(), vec![ROOT_WINDOW]);
    assert_eq!(names(&h, ROOT_WINDOW), ["b.pdf"]);
    assert!(!root_close_sent(&h), "the app keeps running");
}
