# printcraft-print

Layer L4: printing (execution plan M10.5).

```rust
let pages = select_pages(count, Some("1-3, ii, 9-"), &labels, Subset::All, false)?;
let settings = Settings { pages, paper: A4, layout: Layout::multiple(4), ..Default::default() };
let sheets = layout(&display_sizes, &settings)?;   // geometry only (previews)
let pdf = impose(&doc, &settings)?;                // the print-ready PDF
spool::submit(&pdf, &spool::Job { printer: None, copies: 2, ..Default::default() })?;
```

- **Size**: fit (to the sheet minus an 18 pt margin), actual size, shrink oversized, custom %;
  centred; auto orientation turns the sheet for landscape pages.
- **Multiple**: 2/4/6/9/16 (or any n) pages per sheet, horizontal/vertical (reversed) order,
  page borders, auto-rotation of pages that don't match the cell.
- **Booklet**: saddle-stitch imposition padded to a multiple of 4; both sides, front or back
  only; left or right binding.
- **Poster**: tile scale, overlap shared by neighbouring tiles, cut marks.
- **Comments & forms**: document, + markups, + stamps, or form fields only; annotations print
  only with their Print flag, drawn from their appearance streams (Algorithm 8.1).

Each source page becomes a Form XObject (its content wrapped in q/Q, plus the printable
annotations); sheets place them with a clip. The result is a fresh, unencrypted,
garbage-collected file (callers check the print permission).

- **Window** (`Settings::region`): print only an area of each page; every layout treats the
  area as the page (Fit fills the sheet with it, Poster tiles it). PeDeeFe's Print dialog picks it
  like AutoCAD's plot window.

`spool` talks to CUPS on macOS and Linux (`lpstat -p -d`, `lp` with copies, collation, duplex and
monochrome options). On Windows, `raster` draws each sheet as a PNG at the job's resolution
(300 or 600 dpi) and `spool::windows` prints them through `System.Drawing.Printing`, driven by
Windows PowerShell with a fixed script (no `unsafe`, nothing from the document in the script's
text): the printer's matching paper, per-sheet orientation, copies, collation, duplex and colour.
The web reports that printing to a printer isn't available; the print-ready PDF can always be
saved.

Not yet: the printer's own properties dialog, paper trays, poster labels, PostScript output,
colour conversion for grayscale on CUPS.
