//! Project links: the Help menu, About dialog and home screen open this fork's pages, and say in
//! plain text what PeDeeFe is based on. No ArtCraft marks or community links (PdfCraft's brand
//! licence asks forks to remove them).

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_engine::links;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
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
        ("Based on PdfCraft", "https://github.com/storytold/pdfcraft"),
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
    for c in pdfcraft_engine::commands::COMMANDS {
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
    h.get_by_label("PeDeeFe icon");
    h.get_by_label_contains("Based on PdfCraft (formerly PrintCraft) by the ArtCraft team");
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
        assert_eq!(pdfcraft_engine::commands::command(l.command).unwrap().menu, Some("Help"));
    }
}

#[test]
fn about_dialog_has_contributors_and_models_tabs() {
    let mut h = harness(|app| app.dialog = Some(Dialog::About));
    h.get_by_label("Contributors").click();
    h.run_steps(2);
    // The owner is always in the compiled-in credits (contributors/contributors.json), shown by username.
    h.get_by_label("@echelon");
    h.get_by_label("Table").click();
    h.run_steps(2);
    h.get_by_label("PRs");
    h.get_by_label("Display name").click();
    h.run_steps(2);
    h.get_by_label("Brandon Thomas");
    h.get_by_label("Models").click();
    h.run_steps(2);
    assert!(h.query_all_by_label("Anthropic").count() >= 1);
}

/// How a scene sets up the app before its first frame.
type Setup = Box<dyn FnOnce(&mut PdfCraftApp)>;

/// Names that only the credit may show: upstream's wording (kept in its catalogs, so they stay
/// shared) reaches the screen through `branded`, which puts PeDeeFe in their place.
const UPSTREAM_NAMES: [&str; 4] = ["PdfCraft", "PrintCraft", "ArtCraft", "Discord"];

/// Labels and values on screen that name the original project or its community, apart from the
/// credit and the link to the original (in `lang`).
fn upstream_names_on_screen(h: &Harness<'static, PdfCraftApp>, lang: pdfcraft_ui_egui::i18n::Lang) -> Vec<String> {
    let link = pdfcraft_ui_egui::i18n::tr(lang, "Based on PdfCraft").to_string();
    h.query_all_by(|n| {
        let text = format!("{} {}", n.label().unwrap_or_default(), n.value().unwrap_or_default());
        UPSTREAM_NAMES.iter().any(|name| text.contains(name))
    })
    .map(|n| {
        let n = n.accesskit_node();
        format!("{} {}", n.label().unwrap_or_default(), n.value().unwrap_or_default()).trim().to_string()
    })
    .filter(|text| !text.contains(links::CREDIT) && *text != link)
    .collect()
}

#[test]
fn people_see_pedeefe_in_every_language_and_the_original_only_in_the_credit() {
    let pdf: &'static [u8] = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    for lang in pdfcraft_ui_egui::i18n::Lang::all() {
        let code = lang.code();
        let in_lang = move |app: &mut PdfCraftApp| app.set_option("language", code).unwrap();
        let with_doc = move |app: &mut PdfCraftApp| {
            in_lang(app);
            app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        };
        let scenes: Vec<(&str, Setup)> = vec![
            ("home", Box::new(in_lang)),
            (
                "about",
                Box::new(move |app: &mut PdfCraftApp| {
                    with_doc(app);
                    app.dialog = Some(Dialog::About);
                }),
            ),
            (
                "print",
                Box::new(move |app: &mut PdfCraftApp| {
                    with_doc(app);
                    app.set_option("dialog", "print").unwrap();
                }),
            ),
            (
                "preferences",
                Box::new(move |app: &mut PdfCraftApp| {
                    with_doc(app);
                    app.set_option("dialog", "preferences").unwrap();
                }),
            ),
            (
                "recovery",
                Box::new(move |app: &mut PdfCraftApp| {
                    in_lang(app);
                    app.recoverable =
                        vec![pdfcraft_ui_egui::RecoveryMeta { key: "k".into(), name: "draft.pdf".into(), path: None, saved_at: 0, encrypted: false }];
                    app.dialog = Some(Dialog::Recovery);
                }),
            ),
            (
                "update",
                Box::new(move |app: &mut PdfCraftApp| {
                    in_lang(app);
                    app.update_source = Some(std::sync::Arc::new(|| {
                        Ok(pdfcraft_ui_egui::updates::Release { version: "v99.0.0".into(), url: format!("{}/tag/v99.0.0", links::RELEASES) })
                    }));
                }),
            ),
        ];
        for (scene, setup) in scenes {
            let mut h = harness(setup);
            if scene == "update" {
                h.state_mut().execute("help.check_updates");
                for _ in 0..200 {
                    h.run_steps(2);
                    if h.query_all_by_label_contains("99.0.0").next().is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert!(h.query_all_by_label_contains("PeDeeFe 99.0.0").next().is_some(), "{code}: the update names PeDeeFe");
            }
            if scene == "home" {
                // The Help menu too.
                h.get_by_label(pdfcraft_ui_egui::i18n::tr(lang, "Menu")).click();
                h.run_steps(2);
                h.get_by_label(&format!("{} ⏵", pdfcraft_ui_egui::i18n::tr(lang, "Help"))).hover();
                h.run_steps(3);
                h.get_by_label_contains(&pdfcraft_ui_egui::i18n::menu_label("help.check_updates", "Check for updates…"));
            }
            // Each scene is on screen.
            let marker = match scene {
                "about" => links::CREDIT,
                "print" => "A4",
                "preferences" => pdfcraft_ui_egui::i18n::tr(lang, "Interface language"),
                "recovery" => "draft.pdf",
                _ => links::APP_NAME,
            };
            assert!(h.query_all_by_label_contains(marker).next().is_some(), "{code}: {scene} shows {marker:?}");
            assert_eq!(upstream_names_on_screen(&h, lang), Vec::<String>::new(), "{code}: {scene}");
        }
    }
}
