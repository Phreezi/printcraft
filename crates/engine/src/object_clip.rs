//! Edit ▸ Copy, Cut and Paste of objects selected together in Edit a PDF (#5989).
//!
//! A copy ([`ObjectClip`]) holds what the objects draw and the resources that drawing names
//! ([`pdfcraft_edit::copy_objects`]), with a snapshot of the document they came from, so it can
//! be pasted into any open document, also after the original changes or closes. Within the same
//! document, while the objects it refers to are unchanged, a paste draws them again from the same
//! fonts and images; otherwise it first copies them over ([`pdfcraft_organize::import_value`]):
//! each once, through cycles, sharing what the target already holds, and within a size limit.

use std::sync::Arc;

use pdfcraft_cos::Object;

use crate::{DocId, Document, Edit, EditError, EditableObject, ObjectTarget};

/// The most objects a paste copies into another document.
const MAX_IMPORT_OBJECTS: usize = 20_000;
/// The most stream data (fonts, images…) a paste copies into another document.
const MAX_IMPORT_BYTES: usize = 256 << 20;
/// How far each further paste onto the same page steps down and to the right, in points, as
/// pasted added content steps ([`pdfcraft_edit::added::PASTE_STEP`]).
const STEP: f64 = pdfcraft_edit::added::PASTE_STEP;

/// Objects copied from one page of a document.
#[derive(Clone)]
pub struct ObjectClip {
    copied: pdfcraft_edit::CopiedObjects,
    /// The document as it was when copied: what the copy's resources refer to.
    source: pdfcraft_cos::Document,
    doc: DocId,
    page: usize,
}

impl PartialEq for ObjectClip {
    fn eq(&self, other: &Self) -> bool {
        self.copied == other.copied
            && self.doc == other.doc
            && self.page == other.page
            && self.source.source_identity() == other.source.source_identity()
    }
}

impl std::fmt::Debug for ObjectClip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectClip").field("doc", &self.doc).field("page", &self.page).field("objects", &self.copied.objects.len()).finish()
    }
}

impl ObjectClip {
    /// The objects as they were listed when copied ([`Document::editable_objects`]).
    pub fn objects(&self) -> &[EditableObject] {
        &self.copied.objects
    }

    /// The text of the copied paragraphs and text boxes, one per line.
    pub fn text(&self) -> String {
        self.copied.text()
    }

    /// The union of the objects' boxes, in the user space of the page they came from.
    pub fn rect(&self) -> [f64; 4] {
        self.copied.rect
    }

    /// The document and page the objects were copied from.
    pub fn source(&self) -> (DocId, usize) {
        (self.doc, self.page)
    }
}

impl Document {
    /// Edit ▸ Copy of `targets` (references from one [`Document::editable_objects`] inventory of
    /// `page`). Refused when the document doesn't allow copying. Changes nothing.
    pub fn copy_objects(&self, page: usize, targets: &[ObjectTarget]) -> Result<ObjectClip, EditError> {
        self.check_copying()?;
        let editor = self.editor.as_ref().ok_or_else(|| EditError::ReadOnly("the document can't be read".into()))?;
        let copied = pdfcraft_edit::copy_objects(&editor.cos, page, targets)?;
        Ok(ObjectClip { copied, source: editor.cos.clone(), doc: self.id, page })
    }

    /// Where Paste puts `clip` on `page`: the user-space offset that moves it `steps` × 18 pt down
    /// and to the right as displayed, or, when that leaves nothing of it on the page's crop box,
    /// the offset that centres it there.
    pub fn object_paste_offset(&self, page: usize, clip: &ObjectClip, steps: u32) -> Result<[f64; 2], String> {
        let step = STEP * f64::from(steps);
        let offset = self.object_move_offset(page, [step, step])?;
        let p = self.info.pages.get(page).ok_or("the page no longer exists")?;
        let crop = p.crop.map(f64::from);
        let r = clip.rect();
        let moved = [r[0] + offset[0], r[1] + offset[1], r[2] + offset[0], r[3] + offset[1]];
        if moved[0] < crop[2] && moved[2] > crop[0] && moved[1] < crop[3] && moved[3] > crop[1] {
            return Ok(offset);
        }
        let centre = |a: f64, b: f64| (a + b) / 2.0;
        Ok([centre(crop[0], crop[2]) - centre(r[0], r[2]), centre(crop[1], crop[3]) - centre(r[1], r[3])])
    }

    /// Where Paste puts `clip` on `page` when it is pasted at a point (`place`): the user-space
    /// offset that moves the copy's box, as displayed, to [`PastePlace::top_left`].
    pub fn object_place_offset(&self, page: usize, clip: &ObjectClip, place: PastePlace) -> Result<[f64; 2], String> {
        let p = self.info.pages.get(page).ok_or("the page no longer exists")?;
        let r = clip.rect().map(|v| v as f32);
        let corners = [p.user_to_view(r[0], r[1]), p.user_to_view(r[2], r[1]), p.user_to_view(r[2], r[3]), p.user_to_view(r[0], r[3])];
        let xs = corners.map(|c| f64::from(c[0]));
        let ys = corners.map(|c| f64::from(c[1]));
        let min = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
        let max = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let shown = [min(xs), min(ys), max(xs), max(ys)];
        if !shown.iter().all(|v| v.is_finite()) {
            return Err("the copied objects have no place on the page".into());
        }
        let to = place.top_left((shown[2] - shown[0], shown[3] - shown[1]), (f64::from(p.width), f64::from(p.height)));
        self.object_move_offset(page, [to[0] - shown[0], to[1] - shown[1]])
    }
}

/// Where a paste puts what it pastes, as displayed on the page (view space: points from the
/// page's top-left corner as shown, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PastePlace {
    /// The point the paste is anchored at.
    pub at: [f64; 2],
    /// Which of the pasted box's points goes there.
    pub anchor: PasteAnchor,
    /// Where the pasted box has to stay, as far as it fits (`[x0, y0, x1, y1]`, view space): the
    /// part of the page on screen, so what is pasted can be seen and edited. `None` keeps it on
    /// the page.
    pub keep_in: Option<[f64; 4]>,
}

/// Which point of a pasted box goes to [`PastePlace::at`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteAnchor {
    /// Its top-left corner as displayed (a paste from the page's right-click menu).
    TopLeft,
    /// Its centre (⌘V with the pointer over the page).
    Centre,
}

impl PastePlace {
    /// Where a box `size` (width and height as displayed, in points) lands on a page of display
    /// size `page` (`(width, height)`): its top-left corner, in view space. The anchor point goes
    /// to `at`, then the box moves the least it must to lie inside [`Self::keep_in`] (cut to the
    /// page), or the page, as far as it fits; a box larger than that keeps its top-left corner in.
    pub fn top_left(&self, size: (f64, f64), page: (f64, f64)) -> [f64; 2] {
        let finite = |v: f64, or: f64| if v.is_finite() { v } else { or };
        let (w, h) = (finite(size.0, 0.0).max(0.0), finite(size.1, 0.0).max(0.0));
        let (pw, ph) = (finite(page.0, 0.0).max(0.0), finite(page.1, 0.0).max(0.0));
        let at = [finite(self.at[0], 0.0), finite(self.at[1], 0.0)];
        let mut x = match self.anchor {
            PasteAnchor::TopLeft => at[0],
            PasteAnchor::Centre => at[0] - w / 2.0,
        };
        let mut y = match self.anchor {
            PasteAnchor::TopLeft => at[1],
            PasteAnchor::Centre => at[1] - h / 2.0,
        };
        let page_box = [0.0, 0.0, pw, ph];
        let keep = self
            .keep_in
            .filter(|k| k.iter().all(|v| v.is_finite()))
            .map(|k| [k[0].min(k[2]).max(0.0), k[1].min(k[3]).max(0.0), k[0].max(k[2]).min(pw), k[1].max(k[3]).min(ph)])
            .filter(|k| k[2] > k[0] && k[3] > k[1])
            .unwrap_or(page_box);
        // The box fits where it must stay: inside it. Else inside the page, as far as it fits.
        let fit = |b: [f64; 4]| w <= b[2] - b[0] + 1e-6 && h <= b[3] - b[1] + 1e-6;
        let bounds = if fit(keep) { keep } else { page_box };
        let clamp = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.clamp(lo, hi) };
        x = clamp(x, bounds[0], bounds[2] - w);
        y = clamp(y, bounds[1], bounds[3] - h);
        [x, y]
    }
}

/// The objects copied in an application or a script, and how many times each page has had them
/// pasted, which sets how far the next paste there steps ([`Document::object_paste_offset`]).
#[derive(Clone, Debug)]
pub struct ObjectClipboard {
    clip: Arc<ObjectClip>,
    pastes: Vec<(DocId, usize, u32)>,
}

impl ObjectClipboard {
    pub fn new(clip: ObjectClip) -> Self {
        ObjectClipboard { clip: Arc::new(clip), pastes: Vec::new() }
    }

    pub fn clip(&self) -> &ObjectClip {
        &self.clip
    }

    /// The edit that pastes the copy onto `page` of `doc`: where it was, or on the page it came
    /// from one step down and to the right (as Acrobat offsets a paste onto the same page), and
    /// one step further for each earlier paste onto that page.
    pub fn paste(&mut self, doc: &Document, page: usize) -> Result<Edit, String> {
        let at = self.pastes.iter().position(|(d, p, _)| *d == doc.id && *p == page);
        let earlier = at.and_then(|i| self.pastes.get(i)).map_or(0, |(_, _, n)| *n);
        let own = u32::from(self.clip.source() == (doc.id, page));
        let offset = doc.object_paste_offset(page, &self.clip, earlier.saturating_add(own))?;
        match at.and_then(|i| self.pastes.get_mut(i)) {
            Some(entry) => entry.2 = entry.2.saturating_add(1),
            None => self.pastes.push((doc.id, page, 1)),
        }
        Ok(Edit::PasteObjects { page, clip: Arc::clone(&self.clip), offset })
    }

    /// The edit that pastes the copy onto `page` of `doc` at a point (PeDeeFe: the right-click
    /// menu's Paste puts its top-left corner there, ⌘V centres it on the pointer), in the part of
    /// the page `place` keeps it in. Further pastes count as [`Self::paste`]'s do.
    pub fn paste_at(&mut self, doc: &Document, page: usize, place: PastePlace) -> Result<Edit, String> {
        let offset = doc.object_place_offset(page, &self.clip, place)?;
        match self.pastes.iter_mut().find(|(d, p, _)| *d == doc.id && *p == page) {
            Some(entry) => entry.2 = entry.2.saturating_add(1),
            None => self.pastes.push((doc.id, page, 1)),
        }
        Ok(Edit::PasteObjects { page, clip: Arc::clone(&self.clip), offset })
    }
}

impl Edit {
    /// The edit that deletes `targets` (indexes into one inventory of `page`) as one undo step:
    /// what Delete and Cut do to objects selected together. One object is deleted by its own
    /// edit, several by [`Edit::DeleteObjects`]; none is no edit.
    pub fn delete_objects(page: usize, mut targets: Vec<ObjectTarget>) -> Option<Edit> {
        targets.sort();
        targets.dedup();
        match targets.as_slice() {
            [] => None,
            [t] => Some(delete_one(page, *t)),
            _ => Some(Edit::DeleteObjects { page, objects: targets }),
        }
    }
}

fn delete_one(page: usize, t: ObjectTarget) -> Edit {
    use crate::ObjectKind;
    match t.kind {
        ObjectKind::Text => Edit::EditTextBlock { page, block: t.index, text: String::new(), style: crate::BlockStyle::default() },
        ObjectKind::Image => Edit::EditPageImage { page, index: t.index, change: crate::ImageEdit::Delete },
        ObjectKind::Added => Edit::DeleteContent { page, index: t.index },
    }
}

/// [`Edit::DeleteObjects`], each object by its own edit (`run`). A deletion renumbers what follows
/// it in its own inventory, so pictures go from the last to the first, and added content, which
/// the other inventories leave out, after them. Deleting a paragraph also removes a copy drawn
/// exactly over it (fake bold, a paste onto itself), which renumbers paragraphs before it too: so
/// each paragraph is found again by its text and box, and one already gone with another is done.
pub(crate) fn delete(
    doc: &mut pdfcraft_cos::Document,
    page: usize,
    objects: &[ObjectTarget],
    mut run: impl FnMut(&mut pdfcraft_cos::Document, &Edit) -> Result<(), EditError>,
) -> Result<(), EditError> {
    use crate::ObjectKind;
    let mut targets = objects.to_vec();
    targets.sort_by_key(|t| (t.kind == ObjectKind::Added, std::cmp::Reverse(t.index)));
    targets.dedup();
    let blocks = pdfcraft_edit::text_blocks(doc, page)?;
    for t in targets {
        let edit = match t.kind {
            ObjectKind::Text => {
                let key = blocks.get(t.index).map(crate::BlockKey::of).ok_or_else(|| {
                    EditError::Edit(pdfcraft_edit::EditError::Invalid("a selected paragraph no longer exists; select the objects again".into()))
                })?;
                let Some(block) = key.find(&pdfcraft_edit::text_blocks(doc, page)?, t.index) else { continue };
                delete_one(page, ObjectTarget { kind: t.kind, index: block })
            }
            _ => delete_one(page, t),
        };
        run(doc, &edit)?;
    }
    Ok(())
}

/// [`Edit::PasteObjects`]: the copy's resources as `doc` holds them, then the paste.
pub(crate) fn paste(doc: &mut pdfcraft_cos::Document, page: usize, clip: &ObjectClip, offset: [f64; 2]) -> Result<(), EditError> {
    let resources = Object::Dict(clip.copied.resources.clone());
    let refused =
        |e: pdfcraft_organize::OrganizeError| EditError::Edit(pdfcraft_edit::EditError::Invalid(format!("the copied objects can't be pasted: {e}")));
    let used = pdfcraft_organize::reachable(&clip.source, &resources, MAX_IMPORT_OBJECTS, MAX_IMPORT_BYTES).map_err(refused)?;
    // Still the objects the copy was made from: the same document, and each of them unchanged.
    let same = doc.source_identity() == clip.source.source_identity()
        && used.iter().all(|r| {
            let (here, there) = (doc.get(*r), clip.source.get(*r));
            Arc::ptr_eq(&here, &there) || here == there
        });
    let resources = if same { resources } else { pdfcraft_organize::import_value(doc, &clip.source, &resources).map_err(refused)? };
    let Object::Dict(resources) = resources else {
        return Err(EditError::Edit(pdfcraft_edit::EditError::Invalid("the copied objects' resources could not be copied".into())));
    };
    let copied = pdfcraft_edit::CopiedObjects { resources, ..clip.copied.clone() };
    pdfcraft_edit::paste_objects(doc, page, &copied, offset)?;
    Ok(())
}

#[cfg(test)]
#[path = "object_clip_tests.rs"]
mod tests;
