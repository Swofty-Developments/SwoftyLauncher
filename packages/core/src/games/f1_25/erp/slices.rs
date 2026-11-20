use std::path::Path;

use swofty_formats::erp::Resource;
use swofty_formats::texarray::Array;

use super::pack;
use super::{ErpBuild, Slot, fail};
use crate::Result;

const VIEW_COUNT: usize = 0x14;

pub(crate) fn view_slices(view: &Resource) -> Result<Option<u32>> {
    let Some(frag) = view.fragments.first() else {
        return Ok(None);
    };
    let data = frag.data()?;
    Ok(data
        .get(VIEW_COUNT..VIEW_COUNT + 4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes"))))
}

// The slice count a texture array view says and the one its image has, when they differ
// and the view doesn't say 0 (the stock single-slice convention).
pub(crate) fn view_mismatch(view: &Resource, image: &Resource) -> Result<Option<(u32, usize)>> {
    let Some(count) = Array::slice_count(image) else {
        return Ok(None);
    };
    Ok(view_slices(view)?
        .filter(|&said| said != 0 && said as usize != count)
        .map(|said| (said, count)))
}

// What one merge-slices op did: the stock slice count (0 without a stock array), the
// slices it wrote, whether it replaced the whole resource or was refused, and why, as
// warnings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SliceMerge {
    pub erp: String,
    pub resource: String,
    pub slices: usize,
    pub changed: Vec<usize>,
    pub whole: bool,
    pub refused: bool,
    pub notes: Vec<String>,
}

impl ErpBuild<'_> {
    // Puts every slice of `resource` in `from` after the slices of the array in `erp`, as
    // the game or earlier addons left it, and sets the slice count in the array's
    // header and in its view (the same name without `.image`, the count at 0x14).
    pub(super) fn append_slices(&mut self, erp: &str, resource: &str, from: &Path) -> Result<()> {
        let packer = self.packer.clone();
        let source = self.load_from(from, resource)?;
        let theirs = Array::read_slices(&source).ok_or_else(|| {
            fail(format!(
                "append-slices: {resource} in {} is not a texture array",
                from.display()
            ))
        })?;
        let mut res = match self.current(erp, resource)? {
            Some(r) => r,
            None => self
                .stock_copy(erp, resource)?
                .ok_or_else(|| fail(format!("append-slices: the game has no {resource}")))?,
        };
        let mut into = Array::read(&res).ok_or_else(|| {
            fail(format!(
                "append-slices: {resource} in {erp} is not a texture array"
            ))
        })?;
        if !theirs.fits(&into) {
            return Err(fail(format!(
                "append-slices: {resource} differs in format or slice size from the array in {erp}"
            )));
        }
        let added = into.append(&theirs);
        let head = res
            .fragments
            .first()
            .ok_or_else(|| fail(format!("{resource}: no fragment 0")))?
            .data()?;
        let head =
            Array::with_count(&head, into.count).ok_or_else(|| fail(format!("{resource}: short header")))?;
        pack::set_fragment(&packer, &mut res, 0, &head).map_err(|e| fail(format!("{erp}: {e}")))?;
        pack::set_fragment(&packer, &mut res, 1, &into.data).map_err(|e| fail(format!("{erp}: {e}")))?;
        self.place(erp, res, true)?;
        if let Some(view) = resource.strip_suffix(".image") {
            let found = match self.current(erp, view)? {
                Some(r) => Some(r),
                None => self.stock_copy(erp, view)?,
            };
            if let Some(mut v) = found {
                let mut d = v
                    .fragments
                    .first()
                    .ok_or_else(|| fail(format!("{view}: no fragment 0")))?
                    .data()?;
                if let Some(at) = d.get_mut(VIEW_COUNT..VIEW_COUNT + 4) {
                    at.copy_from_slice(&(into.count as u32).to_le_bytes());
                    pack::set_fragment(&packer, &mut v, 0, &d).map_err(|e| fail(format!("{erp}: {e}")))?;
                    self.place(erp, v, true)?;
                }
            }
        }
        self.notes.push(format!(
            "{erp}: {} slice(s) appended to {resource} at {}..{}",
            added.len(),
            added.start,
            added.end
        ));
        Ok(())
    }

    pub(super) fn merge_slices(&mut self, erp: &str, resource: &str, from: &Path) -> Result<SliceMerge> {
        let packer = self.packer.clone();
        let mut merge = SliceMerge {
            erp: erp.to_owned(),
            resource: resource.to_owned(),
            ..SliceMerge::default()
        };
        let source = self.load_from(from, resource)?;
        let stock = self.stock_copy(erp, resource)?;
        let Some((stock, base)) = stock.as_ref().and_then(|r| Array::read(r).map(|a| (r, a))) else {
            merge.whole = true;
            merge.notes.push(match stock {
                Some(_) => format!(
                    "the game's {resource} is not a texture array; it replaces the whole resource in {erp}"
                ),
                None => format!("the game has no {resource}; it replaces the whole resource in {erp}"),
            });
            self.place(erp, source, true)?;
            return Ok(merge);
        };
        merge.slices = base.count;
        let current = self.current(erp, resource)?;
        let left = current.as_ref().map(Array::read);
        let by = self
            .writers
            .get(&(erp.to_owned(), resource.to_owned()))
            .map_or("an earlier addon", String::as_str)
            .to_owned();
        let theirs = match Array::read(&source) {
            Some(a) if a.fits(&base) => a,
            _ => {
                if let Some(Some(grown)) = &left
                    && grown.count > base.count
                {
                    merge.refused = true;
                    merge.notes.push(format!(
                        "{resource} differs in format or slice size from the game's array, and {by} grew the array in {erp} to {} slices; it is not merged, so the array keeps them",
                        grown.count
                    ));
                    return Ok(merge);
                }
                merge.whole = true;
                merge.notes.push(format!(
                    "{resource} differs in format or slice size from the game's array; it replaces the whole resource in {erp}"
                ));
                self.place(erp, source, true)?;
                return Ok(merge);
            }
        };
        let (mut res, mut into) = match (current, left) {
            (None, _) => (stock.clone(), base.clone()),
            (Some(r), Some(Some(a))) if a.fits(&base) => (r, a),
            (Some(_), Some(Some(a))) => {
                merge.refused = true;
                merge.notes.push(format!(
                    "{resource} in {erp} has another format or slice size, as {by} left it; these slices are not merged, so its {} slices stay",
                    a.count
                ));
                return Ok(merge);
            }
            (Some(_), _) => {
                merge.refused = true;
                merge.notes.push(format!(
                    "{resource} in {erp} is not a texture array this build reads, as {by} left it; these slices are not merged"
                ));
                return Ok(merge);
            }
        };
        if theirs.count > base.count {
            merge.notes.push(format!(
                "{resource} has {} slices, the game's {}; the slices past {} are left out",
                theirs.count, base.count, base.count
            ));
        }
        let past = (into.count..theirs.count.min(base.count))
            .filter(|&i| theirs.slice(i) != base.slice(i))
            .count();
        merge.changed = into.merge(&base, &theirs);
        if merge.changed.is_empty() {
            merge.notes.clear();
        }
        if past > 0 {
            merge.notes.push(format!(
                "{resource} in {erp} holds {} slices, as {by} left it; {past} changed slice(s) past {} are left out",
                into.count, into.count
            ));
        }
        if merge.changed.is_empty() {
            return Ok(merge);
        }
        pack::set_fragment(&packer, &mut res, 1, &into.data).map_err(|e| fail(format!("{erp}: {e}")))?;
        self.place(erp, res, true)?;
        Ok(merge)
    }

    pub fn cohere_arrays(&mut self) -> Result<Vec<String>> {
        let packer = self.packer.clone();
        let mut lines = Vec::new();
        let rels: Vec<String> = self.erps.keys().cloned().collect();
        for rel in rels {
            let mut images: Vec<String> = self.erps[&rel]
                .slots
                .iter()
                .map(|s| match s {
                    Slot::Owned(r) => r.name.as_str(),
                    Slot::Stock(j) => self.erps[&rel].file.erp.resources[*j].name.as_str(),
                })
                .filter_map(|n| match n.strip_suffix(".image") {
                    Some(v) if v.ends_with(".texarray") => Some(n.to_owned()),
                    _ if n.ends_with(".texarray") => Some(format!("{n}.image")),
                    _ => None,
                })
                .collect();
            images.sort();
            images.dedup();
            for image in images {
                let view = image
                    .strip_suffix(".image")
                    .expect("built with the suffix")
                    .to_owned();
                let target = self.erp(&rel)?;
                let (Some(at_image), Some(at_view)) = (
                    target.position(&image).map_err(|e| fail(format!("{rel}: {e}")))?,
                    target.position(&view).map_err(|e| fail(format!("{rel}: {e}")))?,
                ) else {
                    continue;
                };
                let (viewed, imaged) = (target.loaded(at_view)?, target.loaded(at_image)?);
                let Some((said, count)) = view_mismatch(&viewed, &imaged)? else {
                    continue;
                };
                let Some(mut v) = self.current(&rel, &view)? else {
                    continue;
                };
                let Some(mut data) = v.fragments.first().map(|f| f.data()).transpose()? else {
                    continue;
                };
                let Some(at) = data.get_mut(VIEW_COUNT..VIEW_COUNT + 4) else {
                    continue;
                };
                at.copy_from_slice(&(count as u32).to_le_bytes());
                pack::set_fragment(&packer, &mut v, 0, &data).map_err(|e| fail(format!("{rel}: {e}")))?;
                self.place(&rel, v, true)?;
                if let Some(origin) = self.writers.get(&(rel.clone(), image.clone())).cloned() {
                    self.writers.insert((rel.clone(), view.clone()), origin);
                }
                lines.push(format!(
                    "texture array {view}: view said {said} slices, image has {count}; fixed"
                ));
            }
        }
        Ok(lines)
    }
}
