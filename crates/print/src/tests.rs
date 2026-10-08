use std::sync::Arc;

use super::*;
use crate::spool::{Duplex, Job, lp_args, parse_lpstat};

/// `n` pages of 200×300 (page 3 is landscape 300×200 when n ≥ 3); page i shows "(Page i+1)".
/// Page 1 carries a printable square comment, a non-printing note, and a stamp.
fn fixture(n: usize) -> Document {
    let mut objs: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 6 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")).into_bytes());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    // 4: an appearance stream; 5: unused.
    let ap = b"0 0 1 rg 0 0 10 10 re f";
    objs.push(
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length {} >>\nstream\n{}\nendstream", ap.len(), String::from_utf8_lossy(ap))
            .into_bytes(),
    );
    objs.push(b"null".to_vec());
    for i in 0..n {
        let annots = if i == 0 {
            " /Annots [<< /Type /Annot /Subtype /Square /F 4 /Rect [10 10 50 50] /AP << /N 4 0 R >> >> << /Type /Annot /Subtype /Text /F 0 /Rect [60 10 80 30] /AP << /N 4 0 R >> >> << /Type /Annot /Subtype /Stamp /F 4 /Rect [100 10 140 50] /AP << /N 4 0 R >> >>]"
        } else {
            ""
        };
        let mb = if i == 2 { " /MediaBox [0 0 300 200]" } else { "" };
        objs.push(
            format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >>{mb}{annots} >>", 7 + 2 * i).into_bytes(),
        );
        let body = format!("BT /F1 24 Tf 20 150 Td (Page {}) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()).into_bytes());
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn settings(pages: Vec<usize>, layout: Layout) -> Settings {
    Settings { pages, paper: (612.0, 792.0), layout, ..Settings::default() }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn page_selection() {
    let labels: Vec<String> = ["i", "ii", "1", "2", "A-1"].iter().map(|s| s.to_string()).collect();
    assert_eq!(select_pages(5, None, &labels, Subset::All, false).unwrap(), [0, 1, 2, 3, 4]);
    assert_eq!(select_pages(5, Some("1-2, 5"), &[], Subset::All, false).unwrap(), [0, 1, 4]);
    assert_eq!(select_pages(5, Some("4-"), &[], Subset::All, false).unwrap(), [3, 4]);
    assert_eq!(select_pages(5, Some("-2"), &[], Subset::All, false).unwrap(), [0, 1]);
    assert_eq!(select_pages(5, Some("ii-2, A-1"), &labels, Subset::All, false).unwrap(), [1, 2, 3, 4], "labels, even with a dash");
    assert_eq!(select_pages(5, None, &[], Subset::Even, true).unwrap(), [3, 1]);
    assert_eq!(select_pages(5, None, &[], Subset::Odd, false).unwrap(), [0, 2, 4]);
    assert!(matches!(select_pages(5, Some("7"), &[], Subset::All, false), Err(PrintError::Invalid(_))));
    assert!(matches!(select_pages(5, Some("x"), &[], Subset::All, false), Err(PrintError::Invalid(_))));
    assert_eq!(select_pages(1, None, &[], Subset::Even, false), Err(PrintError::NoPages));
}

#[test]
fn size_modes() {
    let sizes = [(200.0, 300.0), (1000.0, 1500.0)];
    let fit = layout(&sizes, &settings(vec![0, 1], Layout::Size(SizeMode::Fit))).unwrap();
    let s0 = fit[0].placed[0].matrix.0[0];
    assert!(close(s0, 2.64), "the whole sheet, no margin: min(612 / 200, 792 / 300): {s0}");
    let actual = layout(&sizes, &settings(vec![0], Layout::Size(SizeMode::Actual))).unwrap();
    assert_eq!(actual[0].placed[0].matrix.0, [1.0, 0.0, 0.0, 1.0, 206.0, 246.0], "centred at 100%");
    let shrink = layout(&sizes, &settings(vec![0, 1], Layout::Size(SizeMode::Shrink))).unwrap();
    assert_eq!(shrink[0].placed[0].matrix.0[0], 1.0, "small pages stay at 100%");
    assert!(shrink[1].placed[0].matrix.0[0] < 1.0, "big pages shrink");
    let custom = layout(&sizes, &settings(vec![0], Layout::Size(SizeMode::Custom(50.0)))).unwrap();
    assert_eq!(custom[0].placed[0].matrix.0[0], 0.5);
    // Auto orientation turns the sheet for landscape pages.
    let land = layout(&[(300.0, 200.0)], &settings(vec![0], Layout::Size(SizeMode::Fit))).unwrap();
    assert_eq!(land[0].size, (792.0, 612.0));
}

/// Issue R3 (round 3): Fit adds no margin. A page the size of the sheet prints at exactly
/// 100 % (as Actual size does), a page smaller or larger than the sheet scales to fill it edge
/// to edge, keeping its proportions; Shrink leaves a page the sheet's size alone.
#[test]
fn fit_fills_the_sheet_without_margins() {
    let fit = |page: (f64, f64), paper: (f64, f64), mode: SizeMode| {
        let s = Settings { pages: vec![0], paper, layout: Layout::Size(mode), ..Settings::default() };
        layout(&[page], &s).unwrap().remove(0)
    };
    // A4 on A4: 100 %, from the sheet's corner, the same placement as Actual size.
    let a4 = fit(A4, A4, SizeMode::Fit);
    assert_eq!(a4.size, A4);
    assert_eq!(a4.placed[0].matrix.0, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "A4 on A4 prints at 100 %");
    assert_eq!(a4.placed[0].matrix, fit(A4, A4, SizeMode::Actual).placed[0].matrix);
    assert_eq!(fit(A4, A4, SizeMode::Shrink).placed[0].matrix.0[0], 1.0, "nothing to shrink");
    // A4 on A3: fills the A3 sheet (A3 is √2 × A4, to the rounding of the point sizes).
    let a3 = fit(A4, A3, SizeMode::Fit);
    let b = a3.placed[0].matrix.bbox([0.0, 0.0, A4.0, A4.1]);
    assert!(close(b[1], 0.0) && close(b[3], A3.1), "edge to edge vertically: {b:?}");
    assert!(b[0] >= -0.01 && b[2] <= A3.0 + 0.01 && (A3.0 - (b[2] - b[0])) < 1.0, "and (to under a point) across: {b:?}");
    assert!(close(a3.placed[0].scale(), A3.1 / A4.1));
    // A landscape A4 page turns the sheet (auto orientation) and prints at 100 % too.
    let land = fit((A4.1, A4.0), A4, SizeMode::Fit);
    assert_eq!(land.size, (A4.1, A4.0));
    assert!(close(land.placed[0].scale(), 1.0));
    // Letter on A4: as large as fits, touching both sides, centred top to bottom.
    let letter = fit((612.0, 792.0), A4, SizeMode::Fit);
    let b = letter.placed[0].matrix.bbox([0.0, 0.0, 612.0, 792.0]);
    assert!(close(b[0], 0.0) && close(b[2], A4.0), "{b:?}");
    assert!(close(b[1], A4.1 - b[3]), "centred: {b:?}");
    // A small page grows to the sheet; a big one shrinks to it.
    assert!(close(fit((100.0, 141.42), A4, SizeMode::Fit).placed[0].scale(), A4.0 / 100.0));
    assert!(close(fit((1190.55, 1683.78), A4, SizeMode::Fit).placed[0].scale(), A4.0 / 1190.55));
}

#[test]
fn multiple_pages_per_sheet() {
    let sizes = vec![(200.0, 300.0); 5];
    let sheets = layout(&sizes, &settings((0..5).collect(), Layout::multiple(4))).unwrap();
    assert_eq!(sheet_pages(&sheets), [vec![0, 1, 2, 3], vec![4]]);
    // Horizontal order: 0 top-left, 1 top-right, 2 bottom-left.
    let o = |k: usize| (sheets[0].placed[k].matrix.0[4], sheets[0].placed[k].matrix.0[5]);
    assert!(o(1).0 > o(0).0 && close(o(1).1, o(0).1) && o(2).1 < o(0).1 && close(o(2).0, o(0).0));
    // Two per sheet prints side by side on landscape paper.
    let two = layout(&sizes, &settings(vec![0, 1], Layout::multiple(2))).unwrap();
    assert_eq!(two[0].size, (792.0, 612.0));
    assert!(two[0].placed[1].matrix.0[4] > two[0].placed[0].matrix.0[4]);
    // Vertical order and borders.
    let v =
        layout(&sizes, &settings(vec![0, 1, 2], Layout::Multiple { cols: 2, rows: 2, order: PageOrder::Vertical, border: true, auto_rotate: false }))
            .unwrap();
    assert!(close(v[0].placed[1].matrix.0[4], v[0].placed[0].matrix.0[4]) && v[0].placed[1].matrix.0[5] < v[0].placed[0].matrix.0[5]);
    assert_eq!(v[0].borders.len(), 3);
    // Auto-rotate turns a landscape page in a portrait cell.
    let r = layout(&[(300.0, 200.0), (200.0, 300.0)], &settings(vec![0, 1], Layout::multiple(4))).unwrap();
    // The first (landscape) page sets a landscape sheet; the portrait page is turned to fit.
    assert_eq!(r[0].size, (792.0, 612.0));
    assert!(r[0].placed[0].matrix.0[0] > 0.0);
    assert_eq!(r[0].placed[1].matrix.0[0], 0.0, "rotated");
}

#[test]
fn booklets_pair_pages_for_folding() {
    let sizes = vec![(200.0, 300.0); 6];
    let sheets = layout(&sizes, &settings((0..6).collect(), Layout::Booklet { subset: BookletSubset::BothSides, binding: Binding::Left })).unwrap();
    // 6 pages pad to 8: sheet 1 front [8,1] → [blank, 0], back [1, 6]; sheet 2 front [5, 2], back [3, 4].
    assert_eq!(sheet_pages(&sheets), [vec![0], vec![1], vec![5, 2], vec![3, 4]]);
    assert!(sheets[0].placed[0].matrix.0[4] >= 395.9, "page 1 on the right half");
    assert_eq!(sheets[0].size, (792.0, 612.0));
    let front = layout(&sizes, &settings((0..6).collect(), Layout::Booklet { subset: BookletSubset::FrontOnly, binding: Binding::Right })).unwrap();
    assert_eq!(sheet_pages(&front), [vec![0], vec![2, 5]], "right binding mirrors the spread");
}

#[test]
fn posters_tile_with_overlap() {
    // A 200×300 page at 400% = 800×1200 on Letter with 18 pt margins (576×756 tiles), overlap 36.
    let sheets = layout(&[(200.0, 300.0)], &settings(vec![0], Layout::Poster { scale: 400.0, overlap: 36.0, cut_marks: true })).unwrap();
    assert_eq!(sheets.len(), 4, "2 × 2 tiles");
    assert_eq!(sheets[0].lines.len(), 8, "cut marks");
    let c0 = sheets[0].placed[0].clip;
    let c1 = sheets[1].placed[0].clip;
    assert!(close(c0[2] - c1[0], 36.0 / 4.0), "neighbouring tiles share the overlap");
    assert!(layout(&[(200.0, 300.0)], &settings(vec![0], Layout::Poster { scale: 400.0, overlap: 400.0, cut_marks: false })).is_err());
    // Each sheet knows its tile, row by row from the top-left; other layouts have none.
    for (k, s) in sheets.iter().enumerate() {
        assert_eq!(s.tile, Some(Tile { col: k % 2, row: k / 2, cols: 2, rows: 2 }), "sheet {k}");
    }
    // Together the tiles cover the whole page, and the first is its top-left corner.
    let union = sheets
        .iter()
        .map(|s| s.placed[0].clip)
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, c| [u[0].min(c[0]), u[1].min(c[1]), u[2].max(c[2]), u[3].max(c[3])]);
    assert_eq!(union, [0.0, 0.0, 200.0, 300.0]);
    assert!(close(sheets[0].placed[0].clip[0], 0.0) && close(sheets[0].placed[0].clip[3], 300.0));
    let size = layout(&[(200.0, 300.0)], &settings(vec![0], Layout::Size(SizeMode::Fit))).unwrap();
    assert_eq!(size[0].tile, None);
}

#[test]
fn placement_scale_and_printed_size() {
    // Fit of 200 × 300 on Letter (612 × 792, no margin): min(3.06, 2.64) = 2.64.
    let fit = layout(&[(200.0, 300.0)], &settings(vec![0], Layout::Size(SizeMode::Fit))).unwrap();
    let s = fit[0].placed[0].scale();
    assert!(close(s, 2.64), "{s}");
    let p = printed_size((200.0, 300.0), None, s).unwrap();
    assert!(close(p.0, 528.0) && close(p.1, 792.0), "{p:?}");
    // A page turned on its cell (Multiple, auto-rotate) prints at the same scale as unturned.
    let sizes = [(200.0, 300.0), (300.0, 200.0)];
    let multi = layout(&sizes, &settings(vec![0, 1], Layout::multiple(2))).unwrap();
    let (upright, turned) = (multi[0].placed[0], multi[0].placed[1]);
    assert_eq!(turned.matrix.0[0], 0.0, "the landscape page is rotated");
    assert!(turned.scale() > 0.0 && close(turned.scale(), upright.scale()), "{} {}", turned.scale(), upright.scale());
    // A window: its own size times the scale; one off the page has no size.
    let p = printed_size((200.0, 300.0), Some([100.0, 150.0, 200.0, 300.0]), 5.04).unwrap();
    assert!(close(p.0, 504.0) && close(p.1, 756.0), "{p:?}");
    assert_eq!(printed_size((200.0, 300.0), Some([300.0, 300.0, 400.0, 400.0]), 1.0), None);
    assert_eq!(printed_size((200.0, 300.0), None, f64::INFINITY), None);
    // A poster at 400%: every tile prints at 4×.
    let poster = layout(&[(200.0, 300.0)], &settings(vec![0], Layout::Poster { scale: 400.0, overlap: 36.0, cut_marks: false })).unwrap();
    assert!(poster.iter().all(|s| close(s.placed[0].scale(), 4.0)));
    let degenerate = Placement { page: 0, matrix: Matrix([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]), clip: [0.0; 4] };
    assert_eq!(degenerate.scale(), 0.0);
}

#[test]
fn imposed_pdf_has_the_sheets_and_honours_comments_and_forms() {
    let doc = fixture(3);
    let out = impose(&doc, &settings(vec![0, 1, 2], Layout::multiple(2))).unwrap();
    let printed = Document::open(Arc::new(out.clone())).unwrap();
    let pages = pdfcraft_model::pages(&printed);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].crop(&printed), [0.0, 0.0, 792.0, 612.0]);
    // The source pages are form XObjects holding their content.
    let streams = |d: &Document| -> Vec<String> {
        d.object_numbers()
            .into_iter()
            .filter_map(|n| match &*d.get(ObjRef::new(n, d.generation(n))) {
                Object::Stream(s) => s.decoded().ok().map(|b| String::from_utf8_lossy(&b).into_owned()),
                _ => None,
            })
            .collect()
    };
    let all = streams(&printed).join("\n");
    assert!(all.contains("(Page 1)") && all.contains("(Page 3)"));
    // Markups: the printable square and the stamp, not the note without the Print flag.
    let page1 = streams(&printed).into_iter().find(|s| s.contains("(Page 1)")).unwrap();
    assert_eq!(page1.matches(" Do Q").count(), 2, "{page1}");
    let doc_only = impose(&doc, &Settings { content: Content::Document, ..settings(vec![0], Layout::Size(SizeMode::Fit)) }).unwrap();
    let p = streams(&Document::open(Arc::new(doc_only)).unwrap()).into_iter().find(|s| s.contains("(Page 1)")).unwrap();
    assert_eq!(p.matches(" Do Q").count(), 0);
    let stamps = impose(&doc, &Settings { content: Content::DocumentAndStamps, ..settings(vec![0], Layout::Size(SizeMode::Fit)) }).unwrap();
    let p = streams(&Document::open(Arc::new(stamps)).unwrap()).into_iter().find(|s| s.contains("(Page 1)")).unwrap();
    assert_eq!(p.matches(" Do Q").count(), 1);
    let fields = impose(&doc, &Settings { content: Content::FormFieldsOnly, ..settings(vec![1], Layout::Size(SizeMode::Fit)) }).unwrap();
    assert!(!streams(&Document::open(Arc::new(fields)).unwrap()).join("").contains("(Page 2)"));
    // The output is a fresh file: the unused source pages are gone.
    let one = impose(&doc, &settings(vec![1], Layout::Size(SizeMode::Fit))).unwrap();
    let s = streams(&Document::open(Arc::new(one)).unwrap()).join("\n");
    assert!(s.contains("(Page 2)") && !s.contains("(Page 1)"));
}

#[test]
fn spooler_arguments_and_printer_list() {
    let out = "printer Office_Laser is idle.  enabled since Thu Oct  1 09:00:00 2026\nprinter Label_Writer disabled since …\nsystem default destination: Office_Laser\n";
    assert_eq!(
        parse_lpstat(out),
        [spool::Printer { name: "Office_Laser".into(), default: true }, spool::Printer { name: "Label_Writer".into(), default: false }]
    );
    assert!(parse_lpstat("lpstat: No destinations added.\nno system default destination\n").is_empty());
    let job = Job {
        printer: Some("Office_Laser".into()),
        copies: 3,
        collate: false,
        duplex: Duplex::LongEdge,
        grayscale: true,
        title: "memo.pdf".into(),
        ..Job::default()
    };
    assert_eq!(
        lp_args(&job, "/tmp/x.pdf").join(" "),
        "-d Office_Laser -n 3 -t memo.pdf -o collate=false -o sides=two-sided-long-edge -o print-color-mode=monochrome -o fit-to-page=false -- /tmp/x.pdf"
    );
    assert_eq!(lp_args(&Job::default(), "f.pdf")[0], "-n", "no -d: the default printer");
}

#[test]
fn a_window_prints_only_that_area() {
    // A 200 × 300 page; the window is its top-right quarter.
    let sizes = [(200.0, 300.0)];
    let region = Some([100.0, 150.0, 200.0, 300.0]);
    let fit = layout(&sizes, &Settings { region, ..settings(vec![0], Layout::Size(SizeMode::Fit)) }).unwrap();
    let pl = fit[0].placed[0];
    assert_eq!(pl.clip, [100.0, 150.0, 200.0, 300.0], "clipped to the window");
    // The window (100 × 150) fills the sheet: min(612 / 100, 792 / 150) = 5.28.
    assert!(close(pl.matrix.0[0], 5.28), "{:?}", pl.matrix);
    // Its corners land on the sheet, centred.
    let (x0, y0) = pl.matrix.apply(100.0, 150.0);
    let (x1, y1) = pl.matrix.apply(200.0, 300.0);
    assert!(close(x0 + x1, 612.0) && close(y0 + y1, 792.0), "centred: {x0} {y0} {x1} {y1}");
    assert!(close(y1 - y0, 792.0), "the full height of the sheet");
    // Poster: the window, enlarged, tiles over several sheets; nothing outside it shows.
    let poster = layout(&sizes, &Settings { region, ..settings(vec![0], Layout::Poster { scale: 800.0, overlap: 0.0, cut_marks: false }) }).unwrap();
    assert!(poster.len() > 1);
    for sheet in &poster {
        let c = sheet.placed[0].clip;
        assert!(c[0] >= 100.0 && c[1] >= 150.0 && c[2] <= 200.0 && c[3] <= 300.0, "{c:?}");
    }
    // A window larger than the page is cut to the page; one off the page is an error.
    assert_eq!(page_view((200.0, 300.0), Some([-50.0, -50.0, 500.0, 500.0])), Some(((0.0, 0.0), (200.0, 300.0))));
    let off = layout(&sizes, &Settings { region: Some([300.0, 300.0, 400.0, 400.0]), ..settings(vec![0], Layout::Size(SizeMode::Fit)) });
    assert!(matches!(off, Err(PrintError::Invalid(_))));
    assert_eq!(page_view((200.0, 300.0), Some([f64::NAN, 0.0, 10.0, 10.0])), None);
    // The print-ready PDF clips to the window.
    let doc = fixture(1);
    let bytes = impose(&doc, &Settings { region, ..settings(vec![0], Layout::Size(SizeMode::Fit)) }).unwrap();
    let out = Document::open(Arc::new(bytes)).unwrap();
    let sheet = &pdfcraft_model::pages(&out)[0];
    let content = String::from_utf8_lossy(&decoded_contents(&out, &sheet.dict)).into_owned();
    assert!(content.contains("100 150 100 150 re W n"), "{content}");
}

#[test]
fn sheets_are_drawn_as_images() {
    let doc = fixture(3);
    // Pages 1 and 3 (landscape): two sheets, the second turned.
    let pdf = impose(&doc, &settings(vec![0, 2], Layout::Size(SizeMode::Fit))).unwrap();
    let dir = std::env::temp_dir().join(format!("pdfcraft-raster-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sheets = crate::raster::write_sheets(&pdf, 72, false, &dir).unwrap();
    assert_eq!(sheets.len(), 2);
    assert!(!sheets[0].landscape && sheets[1].landscape);
    assert_eq!(sheets[0].pixels, (612, 792), "72 dpi: one pixel per point");
    assert!(sheets[1].path.to_string_lossy().ends_with("sheet-00002-l.png"));
    let png = std::fs::read(&sheets[0].path).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // Grayscale at 150 dpi.
    let gray = crate::raster::write_sheets(&pdf, 150, true, &dir).unwrap();
    let (w, h) = gray[0].pixels;
    assert!(w.abs_diff(1275) <= 1 && h.abs_diff(1650) <= 1, "8.5 × 11 in at 150 dpi: {w} × {h}");
    let decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(&gray[0].path).unwrap()));
    let reader = decoder.read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Grayscale);
    let _ = std::fs::remove_dir_all(&dir);
    // A malformed image buffer is refused, not written.
    assert!(crate::raster::write_png(&std::env::temp_dir().join("never.png"), 10, 10, &[0; 7], false, 300).is_err());
}

#[test]
fn windows_spooler_scripts_and_answers() {
    use crate::spool::windows::{base64_decode, job_env, paper_name, parse_printers, parse_report};
    assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello");
    assert_eq!(base64_decode("SGVsbG8").unwrap(), b"Hello", "padding is optional");
    assert_eq!(base64_decode("").unwrap(), b"");
    assert!(base64_decode("not base64!").is_none());
    // "HP LaserJet (escritório)" and a network printer, as Windows PowerShell reports them.
    let enc = |s: &str| {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let b = s.as_bytes();
        let mut o = String::new();
        for c in b.chunks(3) {
            let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
            for (i, sh) in [18, 12, 6, 0].iter().enumerate() {
                o.push(if i <= c.len() { A[((n >> sh) & 63) as usize] as char } else { '=' });
            }
        }
        o
    };
    let out = format!("printer {}\r\ndefault {}\r\nnoise\r\nprinter !!!\r\n", enc("HP LaserJet (escritório)"), enc(r"\\server\Plotter A3"));
    assert_eq!(
        parse_printers(&out),
        [
            spool::Printer { name: "HP LaserJet (escritório)".into(), default: false },
            spool::Printer { name: r"\\server\Plotter A3".into(), default: true }
        ]
    );
    // Reports: notes, errors (Windows' own words, any language), and PowerShell failing outright.
    assert_eq!(parse_report(&format!("note {}\ndone 2\n", enc("no A3")), "", true), Ok(vec!["no A3".to_string()]));
    assert_eq!(
        parse_report(&format!("error {}\n", enc("A impressora não está disponível")), "", false),
        Err("A impressora não está disponível".into())
    );
    assert_eq!(
        parse_report("", "File cannot be loaded because running scripts is disabled", false),
        Err("File cannot be loaded because running scripts is disabled".into())
    );
    assert!(parse_report("", "", false).is_err());
    // The job's environment: everything the script reads, nothing it has to parse.
    let job =
        Job { printer: Some("Office".into()), copies: 2, duplex: Duplex::ShortEdge, grayscale: true, title: "plan\n.pdf".into(), ..Job::default() };
    let env: std::collections::HashMap<_, _> = job_env(&job, std::path::Path::new("C:/t"), (842.0, 595.0)).into_iter().collect();
    assert_eq!(env["PDFCRAFT_PRINTER"], "Office");
    assert_eq!(env["PDFCRAFT_COPIES"], "2");
    assert_eq!(env["PDFCRAFT_DUPLEX"], "short");
    assert_eq!(env["PDFCRAFT_COLOR"], "0");
    assert_eq!(env["PDFCRAFT_TITLE"], "plan.pdf", "no control characters");
    assert_eq!((env["PDFCRAFT_PAPER_W"].as_str(), env["PDFCRAFT_PAPER_H"].as_str()), ("826", "1169"), "A4 portrait in 1/100 in");
    assert_eq!(env["PDFCRAFT_PAPER_NAME"], "A4");
    assert_eq!(paper_name(crate::A3), "A3");
    assert_eq!(paper_name((300.0, 400.0)), "106 × 141 mm");
    // The scripts never interpolate job values: they only read the environment.
    for script in [crate::spool::windows::PRINT_SCRIPT, crate::spool::windows::LIST_SCRIPT] {
        assert!(!script.contains("{}") && script.contains("$ErrorActionPreference = 'Stop'"));
    }
}

/// Windows only: print two sheets to a PDF file through "Microsoft Print to PDF", the whole way
/// (sheet images, PowerShell, the spooler). Skips where that printer isn't installed.
#[cfg(windows)]
#[test]
fn windows_prints_through_microsoft_print_to_pdf() {
    let printers = match spool::list_printers() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipped: the printers couldn't be listed here ({e})");
            return;
        }
    };
    let Some(pdf_printer) = printers.iter().find(|p| p.name == "Microsoft Print to PDF") else {
        eprintln!("skipped: no Microsoft Print to PDF printer here ({printers:?})");
        return;
    };
    let doc = fixture(3);
    let pdf = impose(&doc, &Settings { paper: crate::A4, ..settings(vec![0, 2], Layout::Size(SizeMode::Fit)) }).unwrap();
    let out = std::env::temp_dir().join(format!("pedeefe-print-test-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let job = Job {
        printer: Some(pdf_printer.name.clone()),
        dpi: 150,
        print_to_file: Some(out.to_string_lossy().into_owned()),
        title: "PeDeeFe test".into(),
        ..Job::default()
    };
    spool::submit(&pdf, &job).expect("printed");
    let bytes = std::fs::read(&out).expect("Microsoft Print to PDF wrote the file");
    assert!(bytes.starts_with(b"%PDF"), "a PDF");
    let printed = Document::open(Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_model::pages(&printed).len(), 2, "both sheets printed");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn lpstat_output_is_untranslated() {
    // A localized lpstat (here Polish) is unreadable to parse_lpstat...
    assert!(parse_lpstat("drukarka Office_Laser jest bezczynna.\ndomyślny cel systemowy: Office_Laser\n").is_empty());
    // ...so the command must force the C locale, including the SOFTWARE switch macOS CUPS needs.
    let cmd = spool::lpstat_command();
    let envs: Vec<_> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
    for key in ["LC_ALL", "LANG"] {
        assert!(envs.contains(&(key.into(), Some("C".into()))), "{key}=C missing: {envs:?}");
    }
    assert!(envs.iter().any(|(k, v)| k == "SOFTWARE" && v.as_deref().is_some_and(|v| !v.is_empty())), "SOFTWARE missing: {envs:?}");
}

fn cut_stack(cols: usize, rows: usize) -> Layout {
    Layout::Multiple { cols, rows, order: PageOrder::CutStack, border: false, auto_rotate: false }
}

#[test]
fn cut_stack_keeps_piles_in_order_and_blanks_in_place() {
    let sizes = vec![(200.0, 300.0); 10];
    let sheets = layout(&sizes, &settings((0..10).collect(), cut_stack(2, 2))).unwrap();
    assert_eq!(sheet_pages(&sheets), [vec![0, 3, 6, 9], vec![1, 4, 7], vec![2, 5, 8]]);
    for s in &sheets {
        assert_eq!(s.lines, sheets[0].lines, "identical cuts even with blank cells");
        assert_eq!(s.lines.len(), 4);
        for (pl, first) in s.placed.iter().zip(&sheets[0].placed) {
            assert_eq!(pl.matrix, first.matrix, "every pile stays in its cell");
        }
    }
    assert!(sheets[0].placed[1].matrix.0[4] > sheets[0].placed[0].matrix.0[4]);
    assert!(sheets[0].placed[2].matrix.0[5] < sheets[0].placed[0].matrix.0[5]);
    // Selection and reverse order are preserved, including repeated source pages.
    let selected = vec![9, 5, 5, 2, 0];
    let sheets = layout(&sizes, &settings(selected, cut_stack(2, 2))).unwrap();
    assert_eq!(sheet_pages(&sheets), [vec![9, 5, 0], vec![5, 2]]);
    assert_eq!(sheets[1].placed[1].matrix, sheets[0].placed[1].matrix);
}

#[test]
fn cutting_and_stacking_recovers_every_selected_page() {
    // Simulate the actual operation using cell positions, not the imposition formula.
    for (cols, rows) in [(1, 1), (1, 2), (2, 2), (2, 3), (3, 3), (4, 4)] {
        for orientation in [Orientation::Auto, Orientation::Portrait, Orientation::Landscape] {
            for n in 1..=37 {
                let sizes = vec![(200.0, 300.0); n];
                let selected: Vec<usize> = (0..n).rev().collect();
                let s = Settings { orientation, ..settings(selected.clone(), cut_stack(cols, rows)) };
                let sheets = layout(&sizes, &s).unwrap();
                let mut piles: std::collections::BTreeMap<(i64, i64), Vec<usize>> = Default::default();
                for sheet in &sheets {
                    for pl in &sheet.placed {
                        // Equal source sizes, so the page origins identify row/column.
                        let [_, _, _, _, x, y] = pl.matrix.0;
                        piles.entry((-(y * 100.0).round() as i64, (x * 100.0).round() as i64)).or_default().push(pl.page);
                    }
                }
                let restacked: Vec<usize> = piles.into_values().flatten().collect();
                assert_eq!(restacked, selected, "{cols}x{rows}, {n} pages, {orientation:?}");
            }
        }
    }
}

#[test]
fn multiple_rejects_hostile_grids_without_panicking() {
    let sizes = [(200.0, 300.0)];
    for (cols, rows) in [(usize::MAX, 2), (2, usize::MAX), (0, 2), (1, 0), (257, 1)] {
        let mode = Layout::Multiple { cols, rows, order: PageOrder::Horizontal, border: false, auto_rotate: false };
        assert!(matches!(layout(&sizes, &settings(vec![0], mode)), Err(PrintError::Invalid(_))));
    }
    let tiny = Settings { paper: (72.0, 72.0), ..settings(vec![0], cut_stack(16, 16)) };
    assert!(matches!(layout(&sizes, &tiny), Err(PrintError::Invalid(_))));
    for size in [(0.0, 300.0), (200.0, f64::NAN), (f64::INFINITY, 300.0), (f64::from_bits(1), f64::from_bits(1))] {
        assert!(matches!(layout(&[size], &settings(vec![0], cut_stack(2, 2))), Err(PrintError::Invalid(_))));
    }
}

#[test]
fn cut_stack_prints_the_window() {
    // Window and cut and stack together: each cell's pile holds the window (the page's top-right
    // quarter), and the cut marks stay.
    let sizes = vec![(200.0, 300.0); 5];
    let region = Some([100.0, 150.0, 200.0, 300.0]);
    let sheets = layout(&sizes, &Settings { region, ..settings((0..5).collect(), cut_stack(2, 2)) }).unwrap();
    assert_eq!(sheet_pages(&sheets), [vec![0, 2, 4], vec![1, 3]]);
    for sheet in &sheets {
        assert_eq!(sheet.lines.len(), 4);
        for pl in &sheet.placed {
            assert_eq!(pl.clip, [100.0, 150.0, 200.0, 300.0], "clipped to the window");
        }
    }
}
