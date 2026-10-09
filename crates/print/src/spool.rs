//! The system print spooler.
//!
//! - **macOS and Linux**: CUPS. Printers come from `lpstat`, jobs go to `lp` with the job options
//!   (copies, collation, duplex, colour).
//! - **Windows**: Windows' own printing. The sheets are drawn as images at the job's resolution
//!   ([`crate::raster`]) and printed through `System.Drawing.Printing`, driven by Windows
//!   PowerShell, which every Windows 10 and 11 has ([`windows`]). Every printer driver takes an
//!   image; only some understand PDF.
//! - Elsewhere (the web): printing to a printer isn't available; the print-ready PDF can still be
//!   saved.

use crate::PrintError;

pub mod windows;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Printer {
    pub name: String,
    pub default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Duplex {
    #[default]
    Off,
    LongEdge,
    ShortEdge,
}

/// Print quality choices (dots per inch) for spoolers that print sheets as images (Windows).
pub const QUALITIES: [(u32, &str); 2] = [(300, "Standard (300 dpi)"), (600, "High (600 dpi)")];

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// `None` = the system default printer.
    pub printer: Option<String>,
    pub copies: u32,
    pub collate: bool,
    pub duplex: Duplex,
    pub grayscale: bool,
    pub title: String,
    /// Resolution of the sheet images, where sheets print as images (Windows); see [`QUALITIES`].
    pub dpi: u32,
    /// Windows: print into this file instead of through the printer's port (for example with
    /// "Microsoft Print to PDF"). Ignored by CUPS.
    pub print_to_file: Option<String>,
}

impl Default for Job {
    fn default() -> Self {
        Job {
            printer: None,
            copies: 1,
            collate: true,
            duplex: Duplex::Off,
            grayscale: false,
            title: "PeDeeFe".into(),
            dpi: QUALITIES[0].0,
            print_to_file: None,
        }
    }
}

/// Whether this platform can send jobs to printers (otherwise only Save as PDF).
pub const fn available() -> bool {
    cfg!(any(windows, all(unix, not(target_arch = "wasm32"))))
}

/// Whether sheets go to the printer as images (and print quality matters).
pub const fn prints_images() -> bool {
    cfg!(windows)
}

/// Parse `lpstat -p -d` output.
pub fn parse_lpstat(out: &str) -> Vec<Printer> {
    let default = out.lines().find_map(|l| l.strip_prefix("system default destination:")).map(|s| s.trim().to_string());
    out.lines()
        .filter_map(|l| l.strip_prefix("printer "))
        .filter_map(|l| l.split_whitespace().next())
        .map(|n| Printer { name: n.to_string(), default: default.as_deref() == Some(n) })
        .collect()
}

/// The `lp` arguments for a job printing `file`.
pub fn lp_args(job: &Job, file: &str) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(p) = &job.printer {
        a.extend(["-d".to_string(), p.clone()]);
    }
    a.extend(["-n".to_string(), job.copies.clamp(1, 999).to_string()]);
    a.extend(["-t".to_string(), job.title.clone()]);
    let mut opt = |o: &str| a.extend(["-o".to_string(), o.to_string()]);
    opt(if job.collate { "collate=true" } else { "collate=false" });
    opt(match job.duplex {
        Duplex::Off => "sides=one-sided",
        Duplex::LongEdge => "sides=two-sided-long-edge",
        Duplex::ShortEdge => "sides=two-sided-short-edge",
    });
    if job.grayscale {
        opt("print-color-mode=monochrome");
    }
    // The sheets are already laid out at their final size.
    opt("fit-to-page=false");
    a.push("--".into());
    a.push(file.to_string());
    a
}

/// `lpstat -p -d`, forced to print untranslated messages so [`parse_lpstat`] can read them.
///
/// `LC_ALL`/`LANG=C` is enough on Linux. macOS CUPS ignores them and follows the user's
/// interface language (`AppleLanguages`) unless `SOFTWARE` is set, in which case it uses `LANG`.
pub fn lpstat_command() -> std::process::Command {
    let mut c = std::process::Command::new("lpstat");
    c.args(["-p", "-d"]).env("LC_ALL", "C").env("LANG", "C").env("SOFTWARE", "PeDeeFe");
    c
}

/// The printers the system knows (empty when there are none or no spooler). This can take a
/// second on Windows: call it off the UI thread.
pub fn printers() -> Vec<Printer> {
    list_printers().unwrap_or_default()
}

/// The printers the system knows, or why they couldn't be listed.
pub fn list_printers() -> Result<Vec<Printer>, PrintError> {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        match lpstat_command().output() {
            Ok(o) => Ok(parse_lpstat(&String::from_utf8_lossy(&o.stdout))),
            Err(e) => Err(PrintError::Spool(format!("the print spooler is not available: {e}"))),
        }
    }
    #[cfg(windows)]
    {
        windows::list()
    }
    #[cfg(not(any(windows, all(unix, not(target_arch = "wasm32")))))]
    {
        Ok(Vec::new())
    }
}

/// Send a print-ready PDF to the spooler. Returns the spooler's message (the job id, or notes
/// worth showing). Blocking: on Windows it draws every sheet first, so run it off the UI thread.
pub fn submit(pdf: &[u8], job: &Job) -> Result<String, PrintError> {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        let dir = std::env::temp_dir().join(format!("pdfcraft-print-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| PrintError::Spool(e.to_string()))?;
        let file = dir.join(format!("job-{}.pdf", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        std::fs::write(&file, pdf).map_err(|e| PrintError::Spool(e.to_string()))?;
        let out = std::process::Command::new("lp").args(lp_args(job, &file.to_string_lossy())).output();
        let _ = std::fs::remove_file(&file);
        let out = out.map_err(|e| PrintError::Spool(format!("the print spooler is not available: {e}")))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(PrintError::Spool(if err.is_empty() { "the print job was refused".into() } else { err }))
        }
    }
    #[cfg(windows)]
    {
        windows::submit(pdf, job)
    }
    #[cfg(not(any(windows, all(unix, not(target_arch = "wasm32")))))]
    {
        let _ = (pdf, job);
        Err(PrintError::Spool("printing to a printer isn't available here; save the print-ready PDF instead".into()))
    }
}
