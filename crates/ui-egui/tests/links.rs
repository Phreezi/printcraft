//! Project links: the Help menu, About dialog and home screen open this fork's pages, and say in
//! plain text what PeDeeFe is based on. No ArtCraft marks or community links (PrintCraft's brand
//! licence asks forks to remove them).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use printcraft_engine::links;
use printcraft_ui_egui::{Dialog, PrintCraftApp};

fn harness(setup: impl FnOnce(&mut PrintCraftApp) + 'static) -> Harness<'static, PrintCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PrintCraftApp::new();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn home_screen_links() {
    for (label, url) in [
        ("Report a problem", "https://github.com/Phreezi/printcraft/issues"),
        ("PeDeeFe on GitHub", "https://github.com/Phreezi/printcraft"),
        ("Based on PrintCraft", "https://github.com/storytold/printcraft"),
    ] {
        let mut h = harness(|_| {});
        h.get_by_label("Help and feedback");
        h.get_by_label(label).click();
        h.run_steps(2);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(url), "{label}");
    }
}

#[test]
fn no_artcraft_marks_or_community_links() {
    let h = harness(|_| {});
    assert_eq!(h.query_all_by_label("ArtCraft").count(), 0, "no ArtCraft mark (alt text)");
    assert!(h.query_by_label("Discord").is_none(), "no Discord button");
    for c in printcraft_engine::commands::COMMANDS {
        assert!(!c.label.contains("ArtCraft") && !c.label.contains("Discord"), "{}", c.label);
    }
}

#[test]
fn about_dialog_names_the_app_and_its_origin() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    // With a document open, so the home screen's own links are not on screen.
    let mut h = harness(move |app| {
        app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        app.dialog = Some(Dialog::About);
    });
    h.get_by_label("PeDeeFe");
    h.get_by_label_contains("Based on PrintCraft by the ArtCraft team");
    assert_eq!(h.query_all_by_label("ArtCraft").count(), 0, "no ArtCraft mark (alt text)");
    h.get_by_label("Report a problem").click();
    h.run_steps(2);
    assert_eq!(h.state().last_opened_url.as_deref(), Some(links::ISSUES));
}

#[test]
fn help_commands_open_each_link() {
    for l in links::LINKS {
        let mut h = harness(|_| {});
        assert!(h.state_mut().execute(l.command), "{}", l.command);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(l.url));
        assert_eq!(printcraft_engine::commands::command(l.command).unwrap().menu, Some("Help"));
    }
}
