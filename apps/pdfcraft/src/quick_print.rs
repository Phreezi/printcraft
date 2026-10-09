//! `--print FILE…` and `--print-to PRINTER FILE…`: the shell's print and printto verbs.
//!
//! Explorer's Print command and Outlook's Quick Print on a PDF attachment run the verb registered
//! for PeDeeFe's ProgID (packaging/windows/pdfcraft.wxs). The files print with Quick Print's
//! default settings (`pdfcraft_ui_egui::quick_print`: Fit, A4 or A3 for larger pages, auto
//! orientation, the remembered quality), with no dialog and no window, and the process exits.
//!
//! The job is printed by this process, never handed to a running PeDeeFe: Outlook saves the
//! attachment to a temporary file that it may delete once the verb's process exits, so the file
//! is read and spooled before exiting; printing also doesn't depend on the running app being idle
//! or on its version, and this process never takes part in the single-instance election.
//!
//! Errors never crash it: each is logged (standard error and `logs/quick-print.log` in the
//! settings folder, appended, so the running app's own log is left alone) and shown in a system
//! message box, in the UI language, and the exit code is 1.

use std::path::{Path, PathBuf};

use pdfcraft_ui_egui::quick_print::{self, Usage};
use pdfcraft_ui_egui::{i18n, window_state};

use crate::logging::AppLogger;

/// Quick Print's log file inside the settings folder's `logs` folder.
pub const LOG_FILE: &str = "quick-print.log";

/// Where the files go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The system's default printer (`--print`, the shell's print verb).
    Default,
    /// The named printer (`--print-to`, the shell's printto verb).
    Printer(String),
}

impl Target {
    fn printer(&self) -> Option<&str> {
        match self {
            Target::Default => None,
            Target::Printer(name) => Some(name),
        }
    }
}

/// What the command line asked Quick Print to do.
pub type Request = Result<Target, Usage>;

/// Set up the log and the language from the settings in `settings_dir`, print `files` to the
/// request's target (or report its usage mistake), and tell the user about failures. Returns the
/// process exit code: 0 when every file reached the spooler.
pub fn run(logger: Option<&AppLogger>, settings_dir: Option<&Path>, request: &Request, files: &[String]) -> i32 {
    if let (Some(logger), Some(dir)) = (logger, settings_dir)
        && let Err(e) = logger.attach_append(&dir.join("logs").join(LOG_FILE))
    {
        log::warn!("no Quick Print log file: {e}");
    }
    let app = settings_dir.and_then(|d| window_state::read_app_settings(&d.join("app.ron")));
    let language = app.as_ref().and_then(|v| v["language"].as_str()).and_then(i18n::normalize_pref).unwrap_or(i18n::AUTO);
    i18n::set_current(i18n::Lang::from_pref(language));
    let dpi = quick_print::saved_quality(app.as_ref());
    let failures = failures(request, files, dpi, quick_print::print_file);
    if failures.is_empty() {
        return 0;
    }
    quick_print::alert(&failures.join("\n\n"));
    1
}

/// Print each file with `print` (the real [`quick_print::print_file`], or a stand-in in tests),
/// logging every outcome; returns the messages to show for what failed (a usage mistake prints
/// nothing).
pub fn failures(
    request: &Request,
    files: &[String],
    dpi: u32,
    mut print: impl FnMut(&Path, Option<&str>, u32) -> Result<String, String>,
) -> Vec<String> {
    let target = match request {
        Ok(target) => target,
        Err(usage) => {
            let message = usage.message().to_string();
            log::error!("Quick Print: {message}");
            return vec![message];
        }
    };
    let printer = target.printer();
    let shown = printer.unwrap_or("the default printer");
    let mut failed = Vec::new();
    for file in files {
        // The verbs hand over absolute paths; a relative one is taken from the working folder.
        let path = std::path::absolute(file).unwrap_or_else(|_| PathBuf::from(file));
        match print(&path, printer, dpi) {
            Ok(msg) if msg.is_empty() => log::info!("Quick Print: {} sent to {shown} at {dpi} dpi", path.display()),
            Ok(msg) => log::info!("Quick Print: {} sent to {shown} at {dpi} dpi: {msg}", path.display()),
            Err(e) => {
                log::error!("Quick Print: {} on {shown}: {e}", path.display());
                failed.push(e);
            }
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_tried_and_the_failures_are_collected() {
        let mut seen = Vec::new();
        let files = vec!["a.pdf".to_string(), "missing.pdf".to_string(), "c.pdf".to_string()];
        let failed = failures(&Ok(Target::Printer("EPSON ET-16650".into())), &files, 600, |path, printer, dpi| {
            seen.push((path.is_absolute(), printer.map(str::to_string), dpi));
            if path.ends_with("missing.pdf") { Err("Couldn't print missing.pdf: not found".into()) } else { Ok(String::new()) }
        });
        assert_eq!(failed, vec!["Couldn't print missing.pdf: not found".to_string()]);
        assert_eq!(seen.len(), 3, "a failure doesn't stop the other files");
        assert!(seen.iter().all(|s| *s == (true, Some("EPSON ET-16650".to_string()), 600)), "{seen:?}");
        let failed = failures(&Ok(Target::Default), &files[..1], 300, |_, printer, _| {
            assert_eq!(printer, None, "the default printer");
            Ok("request id is EPSON-12".into())
        });
        assert!(failed.is_empty());
    }

    #[test]
    fn a_usage_mistake_is_reported_without_printing() {
        for usage in [Usage::NoFile, Usage::NoPrinter] {
            let failed = failures(&Err(usage), &["a.pdf".to_string()], 300, |_, _, _| panic!("nothing prints"));
            assert_eq!(failed, vec![usage.message().to_string()]);
        }
    }
}
