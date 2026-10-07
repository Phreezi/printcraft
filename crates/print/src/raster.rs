//! Sheets as images: the print-ready PDF drawn page by page at a printer resolution and written
//! as PNG files, one per sheet. Windows prints these (see [`crate::spool::windows`]): every
//! printer driver takes an image, while only some understand PDF.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use printcraft_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};

use crate::PrintError;

/// Resolutions a job may ask for (dots per inch); anything else is clamped into this range.
pub const MIN_DPI: u32 = 72;
pub const MAX_DPI: u32 = 1200;

/// One sheet written as an image.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetImage {
    pub path: PathBuf,
    /// The sheet's size in points, as laid out (width, height).
    pub size: (f64, f64),
    /// Wider than tall: the printer turns the paper for it.
    pub landscape: bool,
    /// Pixels (width, height).
    pub pixels: (u32, u32),
}

/// The file name of sheet `index` (0-based): sorted names are print order, and the suffix says
/// how the paper is turned (`-p` portrait, `-l` landscape), which the Windows script reads.
pub fn file_name(index: usize, landscape: bool) -> String {
    format!("sheet-{:05}-{}.png", index + 1, if landscape { 'l' } else { 'p' })
}

/// Draw every page of the print-ready `pdf` at `dpi` and write it into `dir` as an 8-bit RGB
/// (or, with `gray`, grayscale) PNG. Returns the sheets in print order.
pub fn write_sheets(pdf: &[u8], dpi: u32, gray: bool, dir: &Path) -> Result<Vec<SheetImage>, PrintError> {
    let bytes = Arc::new(pdf.to_vec());
    let cos = printcraft_cos::Document::open(bytes.clone())?;
    let sizes: Vec<(f64, f64)> = printcraft_model::pages(&cos).iter().map(|p| p.display_size(&cos)).collect();
    if sizes.is_empty() {
        return Err(PrintError::NoPages);
    }
    let scale = dpi.clamp(MIN_DPI, MAX_DPI) as f32 / 72.0;
    let mut renderer = PageRenderer::new(bytes, RenderConfig::default());
    let mut out = Vec::with_capacity(sizes.len());
    for (i, &size) in sizes.iter().enumerate() {
        let page = renderer.render(RenderRequest { page: i, kind: RequestKind::Pixels, tile: None, scale, tag: 0 });
        if let Some(e) = page.error {
            return Err(PrintError::Spool(format!("sheet {} could not be drawn: {e}", i + 1)));
        }
        let landscape = size.0 > size.1;
        let path = dir.join(file_name(i, landscape));
        write_png(&path, page.width, page.height, &page.rgba, gray, dpi)?;
        out.push(SheetImage { path, size, landscape, pixels: (page.width, page.height) });
    }
    Ok(out)
}

/// Write premultiplied RGBA (`width` × `height`) as an opaque PNG: pixels are composited over
/// white (renders already have a white background, so this only matters for stray alpha).
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8], gray: bool, dpi: u32) -> Result<(), PrintError> {
    let io = |e: std::io::Error| PrintError::Spool(format!("could not write {}: {e}", path.display()));
    let enc = |e: png::EncodingError| PrintError::Spool(format!("could not write {}: {e}", path.display()));
    let row = (width as usize).checked_mul(4).filter(|r| *r > 0).ok_or_else(|| PrintError::Spool("empty sheet image".into()))?;
    if (height as usize).checked_mul(row) != Some(rgba.len()) {
        return Err(PrintError::Spool("the sheet image has the wrong size".into()));
    }
    let file = std::fs::File::create(path).map_err(io)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(if gray { png::ColorType::Grayscale } else { png::ColorType::Rgb });
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    // Dots per metre, so image viewers show the sheet at its real size.
    let dpm = (f64::from(dpi.clamp(MIN_DPI, MAX_DPI)) / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: dpm, yppu: dpm, unit: png::Unit::Meter }));
    let mut writer = encoder.write_header().map_err(enc)?;
    let mut stream = writer.stream_writer().map_err(enc)?;
    let mut line = Vec::with_capacity(if gray { width as usize } else { width as usize * 3 });
    for px in rgba.chunks_exact(row) {
        line.clear();
        for &[r, g, b, a] in px.as_chunks::<4>().0 {
            // Premultiplied over white: c + (255 − a).
            let white = 255 - u16::from(a);
            let [r, g, b] = [r, g, b].map(|c| (u16::from(c) + white).min(255));
            if gray {
                // Rec. 601 luma, in integers.
                line.push(((77 * r + 150 * g + 29 * b) >> 8) as u8);
            } else {
                line.extend_from_slice(&[r as u8, g as u8, b as u8]);
            }
        }
        stream.write_all(&line).map_err(io)?;
    }
    stream.finish().map_err(enc)?;
    writer.finish().map_err(enc)
}
