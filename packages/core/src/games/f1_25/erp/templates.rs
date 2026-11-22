use swofty_formats::erp::Fragment;

use super::ErpBuild;
use crate::Result;

/// The fragment of a `.ui` resource that holds its templates, which the game registers by name.
const TEMPLATES: [u8; 4] = *b"syst";

impl ErpBuild<'_> {
    /// The templates fragment of the `.ui` resource in `erp` that a put-resource is about to replace, when it
    /// has one.
    pub(super) fn templates_before(&mut self, erp: &str, resource: &str) -> Result<Option<Fragment>> {
        let Ok(target) = self.erp(erp) else {
            return Ok(None);
        };
        let Some(i) = target.position(resource).ok().flatten() else {
            return Ok(None);
        };
        if !target
            .entry(&target.slots[i])
            .fragments
            .iter()
            .any(|f| f.name == TEMPLATES)
        {
            return Ok(None);
        }
        Ok(target
            .loaded(i)?
            .fragments
            .iter()
            .find(|f| f.name == TEMPLATES)
            .cloned())
    }

    /// Gives the `.ui` resource `origin` put in `erp` the templates of the copy it replaced, when it brought
    /// none of its own. Screens such as Choose Driver build their tiles from templates only that fragment
    /// registers, so without it they stay empty.
    pub(super) fn keep_templates(
        &mut self,
        erp: &str,
        resource: &str,
        templates: Fragment,
        origin: &str,
    ) -> Result<()> {
        let target = self.erp(erp)?;
        let i = target.find(erp, resource)?;
        let res = target.owned(i)?;
        if res.fragments.iter().any(|f| f.name == TEMPLATES) {
            return Ok(());
        }
        res.fragments.push(templates);
        res.hash = res.content_hash()?;
        self.notes.push(format!(
            "{erp}: {resource} from {origin} has no templates fragment; the templates of the copy it replaces are kept"
        ));
        Ok(())
    }
}
