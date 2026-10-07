//! Printing on Windows.
//!
//! The workspace forbids `unsafe`, so PeDeeFe doesn't call the Win32 printing API itself. It
//! runs Windows PowerShell (part of every Windows 10 and 11) with a fixed script that uses .NET's
//! `System.Drawing.Printing`:
//! - [`LIST_SCRIPT`] lists the installed printers and the default one;
//! - [`PRINT_SCRIPT`] prints the sheet images [`crate::raster`] wrote, one per page, at the
//!   sheet's paper size (the printer's matching paper, or a custom size), turned per sheet, with
//!   copies, collation, two-sided printing and colour mode.
//!
//! Nothing from the document reaches the script's text: the job's options (printer name, title,
//! folder) travel in environment variables, and the script only reads them as values. Names and
//! messages come back Base64-encoded UTF-8, so printer names and Windows' own messages in any
//! language survive the pipe. Each run uses a fresh temporary folder that is removed afterwards.
//!
//! The parsing and the job's environment are plain functions, tested on every platform; only
//! running PowerShell is Windows-only.

use std::path::Path;

use super::{Duplex, Job, Printer};

/// Lists the printers: one `default <base64 name>` or `printer <base64 name>` line each.
pub const LIST_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
function Enc([string] $s) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($s)) }
try {
  Add-Type -AssemblyName System.Drawing
  $default = (New-Object System.Drawing.Printing.PrinterSettings).PrinterName
  foreach ($name in [System.Drawing.Printing.PrinterSettings]::InstalledPrinters) {
    if ($name -eq $default) { Write-Output ('default ' + (Enc $name)) } else { Write-Output ('printer ' + (Enc $name)) }
  }
  exit 0
} catch {
  Write-Output ('error ' + (Enc $_.Exception.Message))
  exit 1
}
"#;

/// Prints the `sheet-*.png` files of `PRINTCRAFT_JOB_DIR` in name order. Reports `note <base64>`
/// lines (worth showing the user), `done <sheets>`, or `error <base64>` with exit code 1.
pub const PRINT_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
function Enc([string] $s) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($s)) }
try {
  Add-Type -AssemblyName System.Drawing
  $files = @(Get-ChildItem -LiteralPath $env:PRINTCRAFT_JOB_DIR -Filter 'sheet-*.png' | Sort-Object Name | ForEach-Object { $_.FullName })
  if ($files.Count -eq 0) { throw 'There are no sheets to print.' }
  $doc = New-Object System.Drawing.Printing.PrintDocument
  $doc.PrintController = New-Object System.Drawing.Printing.StandardPrintController
  $doc.DocumentName = $env:PRINTCRAFT_TITLE
  $ps = $doc.PrinterSettings
  if ($env:PRINTCRAFT_PRINTER) { $ps.PrinterName = $env:PRINTCRAFT_PRINTER }
  if (-not $ps.IsValid) { throw ('The printer "' + $ps.PrinterName + '" is not available.') }
  if ($env:PRINTCRAFT_OUTPUT) { $ps.PrintToFile = $true; $ps.PrintFileName = $env:PRINTCRAFT_OUTPUT }
  $ps.Copies = [int16]$env:PRINTCRAFT_COPIES
  $ps.Collate = ($env:PRINTCRAFT_COLLATE -eq '1')
  if ($env:PRINTCRAFT_DUPLEX -ne 'off') {
    if ($ps.CanDuplex) {
      if ($env:PRINTCRAFT_DUPLEX -eq 'short') { $ps.Duplex = [System.Drawing.Printing.Duplex]::Horizontal } else { $ps.Duplex = [System.Drawing.Printing.Duplex]::Vertical }
    } else {
      Write-Output ('note ' + (Enc ($ps.PrinterName + ' cannot print on both sides; the sheets print one-sided.')))
      $ps.Duplex = [System.Drawing.Printing.Duplex]::Simplex
    }
  } else {
    $ps.Duplex = [System.Drawing.Printing.Duplex]::Simplex
  }
  # Paper: the printer's own size closest to the sheet (hundredths of an inch, portrait).
  $w = [int]$env:PRINTCRAFT_PAPER_W
  $h = [int]$env:PRINTCRAFT_PAPER_H
  $paper = $null
  foreach ($p in $ps.PaperSizes) {
    $pw = [Math]::Min($p.Width, $p.Height); $ph = [Math]::Max($p.Width, $p.Height)
    if ([Math]::Abs($pw - $w) -le 6 -and [Math]::Abs($ph - $h) -le 6) { $paper = $p; break }
  }
  if ($null -eq $paper) {
    Write-Output ('note ' + (Enc ($ps.PrinterName + ' does not list ' + $env:PRINTCRAFT_PAPER_NAME + ' paper; check the printout.')))
    $paper = New-Object System.Drawing.Printing.PaperSize($env:PRINTCRAFT_PAPER_NAME, $w, $h)
  }
  $doc.DefaultPageSettings.PaperSize = $paper
  $doc.DefaultPageSettings.Color = ($env:PRINTCRAFT_COLOR -eq '1')
  $doc.DefaultPageSettings.Margins = New-Object System.Drawing.Printing.Margins(0, 0, 0, 0)
  $state = @{ i = 0 }
  $doc.add_QueryPageSettings({
    param($sender, $e)
    $e.PageSettings.Landscape = $files[$state.i].EndsWith('-l.png')
  }.GetNewClosure())
  $doc.add_PrintPage({
    param($sender, $e)
    $img = [System.Drawing.Image]::FromFile($files[$state.i])
    try {
      $g = $e.Graphics
      $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
      $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
      # The sheet starts at the paper's corner, not at the printable area's.
      $g.TranslateTransform(-$e.PageSettings.HardMarginX, -$e.PageSettings.HardMarginY)
      $b = $e.PageBounds
      $g.DrawImage($img, 0, 0, $b.Width, $b.Height)
    } finally {
      $img.Dispose()
    }
    $state.i++
    $e.HasMorePages = ($state.i -lt $files.Count)
  }.GetNewClosure())
  $doc.Print()
  Write-Output ('done ' + $files.Count)
  exit 0
} catch {
  $err = $_.Exception
  while ($null -ne $err.InnerException) { $err = $err.InnerException }
  Write-Output ('error ' + (Enc $err.Message))
  exit 1
}
"#;

/// Decode standard Base64 (with or without padding). `None` for anything else.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.trim().bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn decode_text(b64: &str) -> Option<String> {
    base64_decode(b64).map(|b| String::from_utf8_lossy(&b).trim().to_string())
}

/// Parse [`LIST_SCRIPT`]'s output. Lines that don't decode are skipped.
pub fn parse_printers(out: &str) -> Vec<Printer> {
    out.lines()
        .filter_map(|l| {
            let (kind, name) = l.trim().split_once(' ')?;
            let default = match kind {
                "default" => true,
                "printer" => false,
                _ => return None,
            };
            let name = decode_text(name).filter(|n| !n.is_empty())?;
            Some(Printer { name, default })
        })
        .collect()
}

/// What a script reported: its notes, or the error to show.
pub fn parse_report(stdout: &str, stderr: &str, success: bool) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let mut error = None;
    for l in stdout.lines() {
        match l.trim().split_once(' ') {
            Some(("note", b)) => notes.extend(decode_text(b)),
            Some(("error", b)) => error = decode_text(b),
            _ => {}
        }
    }
    match (success, error) {
        (true, None) => Ok(notes),
        (_, Some(e)) if !e.is_empty() => Err(e),
        _ => {
            // PowerShell itself failed (no report): show what it said.
            let said = stderr.trim();
            Err(if said.is_empty() { "Windows refused the print job".into() } else { said.chars().take(500).collect() })
        }
    }
}

/// Points → hundredths of an inch.
fn hundredths(pt: f64) -> i64 {
    if pt.is_finite() { (pt / 72.0 * 100.0).round().clamp(1.0, 100_000.0) as i64 } else { 1 }
}

/// The paper's name for messages: A3, A4 or the size in millimetres.
pub fn paper_name(paper: (f64, f64)) -> String {
    let (w, h) = (paper.0.min(paper.1), paper.0.max(paper.1));
    for (name, (pw, ph)) in crate::PAPERS {
        if (pw - w).abs() < 3.0 && (ph - h).abs() < 3.0 {
            return name.to_string();
        }
    }
    let mm = |pt: f64| (pt / 72.0 * 25.4).round() as i64;
    format!("{} × {} mm", mm(w), mm(h))
}

/// The environment [`PRINT_SCRIPT`] reads for `job`, with its sheets in `dir` on `paper`
/// (points, either way round).
pub fn job_env(job: &Job, dir: &Path, paper: (f64, f64)) -> Vec<(&'static str, String)> {
    let (w, h) = (paper.0.min(paper.1), paper.0.max(paper.1));
    let mut env = vec![
        ("PRINTCRAFT_JOB_DIR", dir.to_string_lossy().into_owned()),
        ("PRINTCRAFT_PRINTER", job.printer.clone().unwrap_or_default()),
        ("PRINTCRAFT_TITLE", job.title.chars().filter(|c| !c.is_control()).take(200).collect()),
        ("PRINTCRAFT_COPIES", job.copies.clamp(1, 999).to_string()),
        ("PRINTCRAFT_COLLATE", if job.collate { "1" } else { "0" }.into()),
        (
            "PRINTCRAFT_DUPLEX",
            match job.duplex {
                Duplex::Off => "off",
                Duplex::LongEdge => "long",
                Duplex::ShortEdge => "short",
            }
            .into(),
        ),
        ("PRINTCRAFT_COLOR", if job.grayscale { "0" } else { "1" }.into()),
        ("PRINTCRAFT_PAPER_W", hundredths(w).to_string()),
        ("PRINTCRAFT_PAPER_H", hundredths(h).to_string()),
        ("PRINTCRAFT_PAPER_NAME", paper_name((w, h))),
        ("PRINTCRAFT_OUTPUT", job.print_to_file.clone().unwrap_or_default()),
    ];
    env.retain(|(_, v)| !v.contains('\0'));
    env
}

/// A fresh, private temporary folder for one run.
#[cfg(windows)]
fn job_dir(what: &str) -> Result<std::path::PathBuf, crate::PrintError> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("pedeefe-{what}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| crate::PrintError::Spool(format!("could not create {}: {e}", dir.display())))?;
    Ok(dir)
}

/// Run `script` with Windows PowerShell (no window, no profile), its file in `dir`, giving up
/// after `timeout`. Returns (succeeded, stdout, stderr).
#[cfg(windows)]
fn run_powershell(
    script: &str,
    dir: &Path,
    env: &[(&'static str, String)],
    timeout: std::time::Duration,
) -> Result<(bool, String, String), crate::PrintError> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    /// CREATE_NO_WINDOW: no console window flashes up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let spool = |m: String| crate::PrintError::Spool(m);
    let file = dir.join("job.ps1");
    std::fs::write(&file, script).map_err(|e| spool(format!("could not write {}: {e}", file.display())))?;
    // The system's own PowerShell, not whatever a PATH entry might shadow it with.
    let exe = std::env::var_os("SystemRoot")
        .map(|r| std::path::PathBuf::from(r).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "powershell.exe".into());
    let mut cmd = Command::new(exe);
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&file)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| spool(format!("Windows PowerShell could not be started: {e}")))?;
    // Read both pipes on their own threads, so a chatty script can't block on a full pipe.
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = Vec::new();
            if let Some(mut p) = pipe {
                // At most 1 MiB: the scripts write a few short lines.
                let _ = p.by_ref().take(1 << 20).read_to_end(&mut s);
            }
            String::from_utf8_lossy(&s).into_owned()
        })
    };
    let out = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(spool(format!("Windows didn't answer within {} seconds", timeout.as_secs())));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(spool(format!("lost track of Windows PowerShell: {e}"))),
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    Ok((status.success(), out, err))
}

/// The installed printers.
#[cfg(windows)]
pub fn list() -> Result<Vec<Printer>, crate::PrintError> {
    let dir = job_dir("printers")?;
    let run = run_powershell(LIST_SCRIPT, &dir, &[], std::time::Duration::from_secs(60));
    let _ = std::fs::remove_dir_all(&dir);
    let (ok, out, err) = run?;
    parse_report(&out, &err, ok).map_err(crate::PrintError::Spool)?;
    Ok(parse_printers(&out))
}

/// Draw the sheets of `pdf` and print them. Returns the notes worth showing (empty when all went
/// as asked).
#[cfg(windows)]
pub fn submit(pdf: &[u8], job: &Job) -> Result<String, crate::PrintError> {
    let dir = job_dir("print")?;
    let result = (|| {
        let sheets = crate::raster::write_sheets(pdf, job.dpi, job.grayscale, &dir)?;
        let paper = sheets.first().map(|s| s.size).ok_or(crate::PrintError::NoPages)?;
        let env = job_env(job, &dir, paper);
        // Big jobs take a while to draw into the spooler; a stuck driver doesn't hang forever.
        let (ok, out, err) = run_powershell(PRINT_SCRIPT, &dir, &env, std::time::Duration::from_secs(30 * 60))?;
        parse_report(&out, &err, ok).map(|notes| notes.join(" ")).map_err(crate::PrintError::Spool)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
