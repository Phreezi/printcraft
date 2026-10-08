//! The interface language: the first-run "Choose your language / Escolha o idioma" prompt and the
//! Preferences choice, which offer exactly English and European Portuguese.

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

const TITLE: &str = "Choose your language / Escolha o idioma";

/// The app as the desktop app starts it: settings restored (`None`: a first start), then asked.
fn first_start(settings: Option<&str>) -> Harness<'static, PdfCraftApp> {
    let settings = settings.map(str::to_string);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        if let Some(json) = &settings {
            app.restore(json);
        }
        app.ask_language_if_unset();
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn first_start_asks_for_the_language_and_applies_it_at_once() {
    let mut h = first_start(None);
    assert!(h.state().language_prompt);
    h.get_by_label(TITLE);
    // Both hints, each in its own language.
    h.get_by_label("You can change it later in Preferences.");
    h.get_by_label("Pode alterá-lo mais tarde nas Preferências.");
    // Escape and the app's shortcuts don't get past it.
    h.key_press(Key::Escape);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run_steps(2);
    assert!(h.state().language_prompt, "Escape doesn't dismiss the prompt");
    assert_eq!(h.state().dialog, None, "no shortcut runs under the prompt");

    h.get_by_role_and_label(Role::Button, "Português (Portugal)").click();
    h.run_steps(3);
    assert!(!h.state().language_prompt);
    assert_eq!(h.state().language, "pt-pt");
    assert!(h.query_by_label(TITLE).is_none(), "the prompt is gone");
    // The home screen is in Portuguese in the same run, not after a restart.
    h.get_by_label("Bem-vindo ao PeDeeFe");
    h.get_by_label("Ferramentas recomendadas");
    assert!(h.query_by_label("Recommended tools").is_none());

    // Saved: the next start neither asks again nor forgets the choice.
    let saved = h.state().persist();
    let next = first_start(Some(&saved));
    assert!(!next.state().language_prompt);
    assert_eq!(next.state().language, "pt-pt");
    next.get_by_label("Bem-vindo ao PeDeeFe");
}

#[test]
fn choosing_english_keeps_the_english_interface() {
    let mut h = first_start(None);
    h.get_by_role_and_label(Role::Button, "English").click();
    h.run_steps(3);
    assert!(!h.state().language_prompt);
    assert_eq!(h.state().language, "en");
    h.get_by_label("Welcome to PeDeeFe");
    let next = first_start(Some(&h.state().persist()));
    assert!(!next.state().language_prompt);
}

#[test]
fn only_an_explicit_choice_of_an_offered_language_skips_the_prompt() {
    for (settings, asks) in [
        ("{}", true),
        ("not json", true),
        (r#"{"language":"auto"}"#, true),
        // Languages upstream PdfCraft offers but PeDeeFe doesn't.
        (r#"{"language":"ja"}"#, true),
        (r#"{"language":"pt-br"}"#, true),
        (r#"{"language":"en"}"#, false),
        (r#"{"language":"pt-pt"}"#, false),
    ] {
        let mut app = PdfCraftApp::new();
        app.restore(settings);
        app.ask_language_if_unset();
        assert_eq!(app.language_prompt, asks, "{settings}");
    }
    // `--language` (and the control channel) answers it too.
    let mut app = PdfCraftApp::new();
    app.ask_language_if_unset();
    app.set_option("language", "pt-pt").unwrap();
    assert!(!app.language_prompt);
    // Screenshots and scripts can show and hide it.
    app.set_option("language-prompt", "show").unwrap();
    assert!(app.language_prompt);
    app.set_option("language-prompt", "hide").unwrap();
    assert!(!app.language_prompt);
    assert!(app.set_option("language-prompt", "maybe").is_err());
    // Unknown codes change nothing.
    assert!(!app.choose_language("xx"));
    assert_eq!(app.language, "pt-pt");
}

/// Open the Preferences language selector, which shows `current`.
fn open_language_menu(h: &mut Harness<'static, PdfCraftApp>, current: &str) {
    let shows = |text: Option<String>| text.is_some_and(|t| t.contains(current));
    h.get_by(|n| n.role() == Role::ComboBox && (shows(n.label()) || shows(n.value()))).click();
    h.run_steps(2);
}

#[test]
fn preferences_offer_english_and_european_portuguese_only() {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        // A language set elsewhere (`--language ja`) still shows as what it is.
        app.set_option("language", "ja").unwrap();
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(4);
    open_language_menu(&mut h, "日本語");
    h.get_by_label("English");
    h.get_by_label("Português (Portugal)");
    for not_offered in ["Auto", "Español", "Čeština", "Português (Brasil)", "简体中文", "繁體中文"] {
        assert!(h.query_by_label(not_offered).is_none(), "{not_offered} is offered");
    }
    h.get_by_label("Português (Portugal)").click();
    h.run_steps(3);
    assert_eq!(h.state().language, "pt-pt");
    assert_eq!(h.state().dialog, Some(Dialog::Preferences));
    // The dialog is relabelled at once.
    h.get_by_label("Idioma da interface");
    h.get_by_label("Identidade");

    open_language_menu(&mut h, "Português (Portugal)");
    h.get_by_label("English").click();
    h.run_steps(3);
    assert_eq!(h.state().language, "en");
    h.get_by_label("Interface language");
}
