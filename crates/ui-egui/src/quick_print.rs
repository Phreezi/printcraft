//! Quick Print: print a file straight away with the default settings, without the Print dialog.
//!
//! This is what the shell's **print** and **printto** verbs run (`pedeefe --print FILE`,
//! `pedeefe --print-to PRINTER FILE`): Explorer's Print command and Outlook's Quick Print on an
//! attachment. The settings are the Print dialog's defaults:
//! - every page, **Fit** (the page fills the sheet edge to edge, keeping its proportions),
//!   **auto orientation** (each sheet turns to its page), comments and forms as in the dialog;
//! - **A4**, unless a page is larger than A4 (by more than [`OVERSIZE`]), then **A3**: a drawing
//!   on A3, A2 or A1 prints on A3, a US Letter page still on A4;
//! - one copy, one-sided, in colour, at the print quality remembered from the Print dialog;
//! - the system's default printer, or the printer named by `printto`.
//!
//! The desktop app calls [`print_file`] for each file before any window opens, then exits.

use pdfcraft_engine::print::{self, A3, A4, Content, Layout, Orientation, SizeMode, spool};

/// How much bigger than A4 (on either side) a page may be and still print on A4. US Letter is
/// 2.8 % wider than A4 and prints on A4; Legal (20 % taller), Tabloid, A3 and up print on A3.
pub const OVERSIZE: f64 = 1.05;

/// A mistake on Quick Print's command line (a broken shell verb), reported to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// `--print` or `--print-to` without a file.
    NoFile,
    /// `--print-to` without a printer name.
    NoPrinter,
}

impl Usage {
    /// The message, in the UI language.
    pub fn message(self) -> &'static str {
        match self {
            Usage::NoFile => tl!("Quick Print needs a PDF file to print."),
            Usage::NoPrinter => tl!("Quick Print needs the name of a printer."),
        }
    }
}

/// The paper for pages of these display sizes (points): A4, or A3 when any page is larger than
/// A4 by more than [`OVERSIZE`] on either side, whichever way it is turned. Sizes that aren't
/// finite and positive are ignored.
pub fn paper_for(sizes: &[(f64, f64)]) -> (f64, f64) {
    let larger = sizes.iter().any(|&(w, h)| {
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return false;
        }
        let (short, long) = (w.min(h), w.max(h));
        short > A4.0 * OVERSIZE || long > A4.1 * OVERSIZE
    });
    if larger { A3 } else { A4 }
}

/// Quick Print's settings for a document whose pages have these display sizes: every page, Fit,
/// auto orientation, on [`paper_for`] the pages.
pub fn settings(sizes: &[(f64, f64)]) -> print::Settings {
    print::Settings {
        pages: (0..sizes.len()).collect(),
        paper: paper_for(sizes),
        orientation: Orientation::Auto,
        layout: Layout::Size(SizeMode::Fit),
        content: Content::DocumentAndMarkups,
        region: None,
    }
}

/// The print quality remembered by the Print dialog (`"print": {"dpi": …}` in the app's saved
/// JSON), or the standard quality when there is none or it isn't one of [`spool::QUALITIES`].
pub fn saved_quality(app: Option<&serde_json::Value>) -> u32 {
    app.and_then(|v| v["print"]["dpi"].as_u64())
        .and_then(|d| spool::QUALITIES.iter().find(|q| u64::from(q.0) == d))
        .map_or(spool::QUALITIES[0].0, |q| q.0)
}

/// The job: one copy, one-sided, in colour, at `dpi`, on `printer` (`None`: the default printer).
pub fn job(printer: Option<&str>, dpi: u32, title: &str) -> spool::Job {
    spool::Job {
        printer: printer.map(str::to_string),
        copies: 1,
        collate: true,
        duplex: spool::Duplex::Off,
        grayscale: false,
        title: title.to_string(),
        dpi,
        print_to_file: None,
    }
}

/// The print-ready PDF (sheets) for a document's bytes, with Quick Print's settings, and how
/// many sheets it has. Errors are messages for the user (untranslated engine words, like the
/// Print dialog's).
pub fn prepare(name: &str, bytes: Vec<u8>) -> Result<(Vec<u8>, usize), String> {
    let mut session = pdfcraft_engine::Session::new();
    let id = session.open(name, None, std::sync::Arc::new(bytes), None).map_err(|e| e.to_string())?;
    let doc = session.get(id).ok_or("the document didn't open")?;
    let sizes: Vec<(f64, f64)> = doc.info.pages.iter().map(|p| (f64::from(p.width), f64::from(p.height))).collect();
    let settings = settings(&sizes);
    let sheets = print::layout(&sizes, &settings).map_err(|e| e.to_string())?.len();
    let pdf = session.print_pdf(id, &settings).map_err(|e| e.to_string())?;
    Ok((pdf, sheets))
}

/// Print the file at `path` with Quick Print's settings on `printer` (`None`: the default
/// printer) at `dpi`, and wait until the spooler has it. Returns the spooler's message (a job
/// id, notes worth logging). Errors are messages for the user, in the UI language.
#[cfg(not(target_arch = "wasm32"))]
pub fn print_file(path: &std::path::Path, printer: Option<&str>, dpi: u32) -> Result<String, String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let failed = |e: &str| crate::i18n::fmt(tl!("Couldn't print {name}: {e}"), &[("name", &name), ("e", e)]);
    let bytes = std::fs::read(path).map_err(|e| failed(&e.to_string()))?;
    // The engine catches panics on open; imposing and spooling are plain `Result`s.
    let (pdf, _) = prepare(&name, bytes).map_err(|e| failed(&e))?;
    spool::submit(&pdf, &job(printer, dpi, &name)).map_err(|e| failed(&e.to_string()))
}

/// Tell the user something went wrong when there is no window to show it in (Quick Print): a
/// system message box with the app's name as its title. Returns once it is dismissed, or at
/// once when no message box can be shown (the message is logged by the caller either way).
#[cfg(not(target_arch = "wasm32"))]
pub fn alert(text: &str) {
    let _ = rfd::MessageDialog::new().set_title(crate::APP_NAME).set_description(text).set_level(rfd::MessageLevel::Error).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: (f64, f64) = (612.0, 792.0);
    const LEGAL: (f64, f64) = (612.0, 1008.0);
    const A1: (f64, f64) = (1683.78, 2383.94);

    #[test]
    fn a4_unless_a_page_is_larger() {
        assert_eq!(paper_for(&[A4]), A4);
        assert_eq!(paper_for(&[(A4.1, A4.0)]), A4, "a landscape A4 page");
        assert_eq!(paper_for(&[LETTER, (419.53, 595.28)]), A4, "Letter and A5 print on A4");
        assert_eq!(paper_for(&[A3]), A3);
        assert_eq!(paper_for(&[(A3.1, A3.0)]), A3, "a landscape A3 drawing");
        assert_eq!(paper_for(&[LEGAL]), A3, "Legal is a fifth taller than A4");
        assert_eq!(paper_for(&[A1]), A3, "larger than A3 still prints on A3 (Fit)");
        assert_eq!(paper_for(&[A4, A4, A1]), A3, "one large page takes the document to A3");
        assert_eq!(paper_for(&[]), A4);
        assert_eq!(paper_for(&[(f64::NAN, 5000.0), (f64::INFINITY, 1.0), (-3000.0, -3000.0), (0.0, 9000.0)]), A4, "junk sizes are ignored");
    }

    #[test]
    fn the_settings_are_the_dialogs_defaults() {
        let s = settings(&[A4, A4, A4]);
        assert_eq!(s.pages, vec![0, 1, 2]);
        assert_eq!(s.paper, A4);
        assert_eq!(s.orientation, Orientation::Auto);
        assert_eq!(s.layout, Layout::Size(SizeMode::Fit));
        assert_eq!(s.content, Content::DocumentAndMarkups);
        assert_eq!(s.region, None);
        assert_eq!(settings(&[A1]).paper, A3);
        // The same as a fresh Print dialog, apart from the paper chosen for the pages.
        let dialog = crate::PrintDraft::default().settings(3, &[]).map_err(|e| e.to_string());
        assert_eq!(
            dialog.map(|d| (d.pages, d.orientation, d.layout, d.content, d.region)),
            Ok((s.pages, s.orientation, s.layout, s.content, s.region))
        );
    }

    #[test]
    fn the_remembered_quality_is_used_when_it_is_valid() {
        let standard = spool::QUALITIES[0].0;
        assert_eq!(saved_quality(None), standard);
        assert_eq!(saved_quality(Some(&serde_json::json!({}))), standard);
        assert_eq!(saved_quality(Some(&serde_json::json!({ "print": { "dpi": 600 } }))), 600);
        assert_eq!(saved_quality(Some(&serde_json::json!({ "print": { "dpi": 123 } }))), standard, "not a choice");
        assert_eq!(saved_quality(Some(&serde_json::json!({ "print": { "dpi": "600" } }))), standard);
        assert_eq!(saved_quality(Some(&serde_json::json!({ "print": 7 }))), standard);
        // What the Print dialog saves is what Quick Print reads.
        let draft = crate::PrintDraft { dpi: 600, ..Default::default() };
        assert_eq!(saved_quality(Some(&serde_json::json!({ "print": draft.prefs() }))), 600);
    }

    #[test]
    fn one_colour_copy_on_the_chosen_printer() {
        let j = job(None, 600, "a.pdf");
        assert_eq!((j.printer, j.copies, j.duplex, j.grayscale, j.dpi, j.title.as_str()), (None, 1, spool::Duplex::Off, false, 600, "a.pdf"));
        assert_eq!(job(Some("EPSON ET-16650"), 300, "b.pdf").printer.as_deref(), Some("EPSON ET-16650"));
        assert_eq!(job(None, 300, "c").print_to_file, None);
    }

    #[test]
    fn unreadable_documents_are_errors_not_crashes() {
        assert!(prepare("junk.pdf", b"not a pdf at all".to_vec()).is_err());
        assert!(prepare("empty.pdf", Vec::new()).is_err());
        assert!(prepare("cut.pdf", b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R".to_vec()).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_missing_file_is_reported_with_its_name() {
        let path = std::env::temp_dir().join(format!("pedeefe-quick-print-missing-{}.pdf", std::process::id()));
        let err = print_file(&path, None, 300).err().unwrap_or_default();
        assert!(err.starts_with("Couldn't print pedeefe-quick-print-missing-"), "{err}");
    }
}
