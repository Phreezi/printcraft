//! System print spooler supplied by craft-print. PDF composition and permission checks
//! stay in PdfCraft; these adapters retain the application's print-error surface.
//!
//! PeDeeFe adds **Print as image** on Windows ([`windows`], [`submit_as_image`]): the sheets are
//! drawn as images at the chosen quality ([`crate::raster`]) and printed through
//! `System.Drawing.Printing`, driven by Windows PowerShell. It is the way PeDeeFe printed before
//! craft-print's native spooler, kept for drivers or files the native path prints badly (as
//! Acrobat's Advanced ▸ Print as image).

pub use craft_print::{
    Duplex, HANDLED_OPTIONS, HAS_PRINTER_PREFERENCES, Job, Printer, PrinterOption, Scaling, lp_args, lpstat_command, parse_lpoptions, parse_lpstat,
    parse_lpstat_e, parse_ppd, parse_windows_printers, ppd_text, printers, printers_parsed,
};

/// The driver's job options for a printer (CUPS: its PPD, with the queue's current defaults);
/// empty when it has none or on other platforms. Reads files and runs `lpoptions`, so call it
/// when the user asks, not every frame.
pub fn printer_options(printer: &str) -> Vec<PrinterOption> {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        printer_options_via(printer, std::path::Path::new("/etc/cups/ppd"), std::process::Command::new("lpoptions"))
    }
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    {
        craft_print::printer_options(printer)
    }
}

/// Read a destination's options with the PPD directory and command supplied, so tests need
/// neither a system queue nor a physical printer. CUPS instances share their queue's PPD but
/// have their own defaults, queried with the complete `queue/instance` destination.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) fn printer_options_via(printer: &str, ppd_dir: &std::path::Path, mut lpoptions: std::process::Command) -> Vec<PrinterOption> {
    let (queue, instance) = printer.split_once('/').map_or((printer, None), |(q, i)| (q, Some(i)));
    // Retain the conservative name checks, applied to each component. Neither component is
    // a path: reject dot components, backslashes and any additional separator before I/O.
    let valid = |name: &str| {
        !name.is_empty()
            && name.len() <= 64
            && !matches!(name, "." | "..")
            && name.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b':' | b'=' | b'"'))
    };
    if !valid(queue) || instance.is_some_and(|name| !valid(name)) {
        return Vec::new();
    }
    const PPD_CAP: u64 = 16 << 20;
    let ppd = std::fs::File::open(ppd_dir.join(format!("{queue}.ppd"))).ok().and_then(|f| {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        f.take(PPD_CAP).read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    let mut options = ppd.map(|b| parse_ppd(&ppd_text(&b))).unwrap_or_default();
    let current = lpoptions.args(["-p", printer, "-l"]).output().map(|o| parse_lpoptions(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    for (key, value) in current {
        if let Some(o) = options.iter_mut().find(|o| o.key == key)
            && o.choices.iter().any(|(c, _)| *c == value)
        {
            o.default = value;
        }
    }
    options
}

pub mod windows;

/// Print quality choices (dots per inch) for Print as image.
pub const QUALITIES: [(u32, &str); 2] = [(300, "Standard (300 dpi)"), (600, "High (600 dpi)")];

/// How Print as image draws and sends the sheets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageJob {
    /// Resolution of the sheet images; see [`QUALITIES`].
    pub dpi: u32,
    /// Print into this file instead of through the printer's port (for example with
    /// "Microsoft Print to PDF").
    pub print_to_file: Option<String>,
}

impl Default for ImageJob {
    fn default() -> Self {
        ImageJob { dpi: QUALITIES[0].0, print_to_file: None }
    }
}

/// Whether this platform can send jobs to printers (otherwise only Save as PDF).
pub const fn available() -> bool {
    cfg!(any(windows, all(unix, not(target_arch = "wasm32"))))
}

/// Whether Print as image is available here (Windows).
pub const fn prints_images() -> bool {
    cfg!(windows)
}

/// Print a print-ready PDF as images ([`windows`]). Blocking: it draws every sheet first, so
/// run it off the UI thread. Elsewhere than Windows it refuses.
pub fn submit_as_image(pdf: &[u8], job: &Job, image: &ImageJob) -> Result<String, crate::PrintError> {
    #[cfg(windows)]
    {
        windows::submit(pdf, job, image)
    }
    #[cfg(not(windows))]
    {
        let _ = (pdf, job, image);
        Err(crate::PrintError::Spool("Print as image is only available on Windows".into()))
    }
}

pub fn submit(pdf: &[u8], job: &Job) -> Result<String, crate::PrintError> {
    craft_print::submit(pdf, job).map_err(|e| crate::PrintError::Spool(e.to_string()))
}

pub fn open_printer_preferences(printer: &str) -> Result<(), crate::PrintError> {
    craft_print::open_printer_preferences(printer).map_err(|e| crate::PrintError::Spool(e.to_string()))
}
