//! The flows, regions and ui data the addon screens are listed in.

use swofty_formats::pkg::Node;

use crate::Result;
use crate::ui::node::*;

pub fn flow_entries(data: &[u8], flow: &str, screens: &[String]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let flows = root_obj(p)?.at_mut("Flows")?.a_mut()?;
        let f = flows
            .iter_mut()
            .find(|f| f.name() == flow)
            .ok_or_else(|| err(format!("flows.ui has no {flow}")))?;
        let ents = f
            .slot_mut(CHILDREN)?
            .first_mut()
            .ok_or_else(|| err(format!("{flow} has no scenes")))?
            .slot_mut(CHILDREN)?;
        let tmpl = ents
            .first()
            .cloned()
            .ok_or_else(|| err(format!("{flow} has no entries")))?;
        for screen in screens {
            let entry = format!("enter_{screen}");
            if ents.iter().any(|e| e.name() == entry) {
                return Err(err(format!("{flow} has {entry} already")));
            }
            let mut e = tmpl.clone();
            let p0 = e.props_mut()?;
            set_str(p0, "name", &entry)?;
            set_str(p0.at_mut("screen")?.o_mut()?, "value", screen)?;
            p0.drop_key("command");
            ents.push(e);
        }
        fix_ids(f, None);
        Ok(())
    })
}

pub fn regions(data: &[u8], screens: &[String]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let regs = root_obj(p)?
            .at_mut("regions")?
            .o_mut()?
            .at_mut("Contents")?
            .a_mut()?;
        let reg = regs
            .iter_mut()
            .find(|r| r.o().ok().and_then(|o| o.str_at("Name").ok()) == Some("region_undefined"))
            .ok_or_else(|| err("region_loading has no region_undefined"))?;
        let c = reg.o_mut()?.at_mut("Contents")?.o_mut()?;
        for screen in screens {
            if c.get(screen).is_some() {
                return Err(err(format!("region_undefined has {screen} already")));
            }
            let mut e = c.at("fe_home")?.clone();
            let eo = e.o_mut()?;
            set_str(eo, "Name", screen)?;
            set_str(
                eo,
                "Value",
                &format!("asset_groups/ui_package/screens/{screen}.assetgroup"),
            )?;
            c.set(screen, e);
        }
        Ok(())
    })
}

pub fn ui_data(data: &[u8], entries: Vec<(String, Node)>) -> Result<Vec<u8>> {
    edit(data, |p| {
        let c = root_obj(p)?
            .at_mut("ui_menu_data")?
            .o_mut()?
            .at_mut("Contents")?
            .o_mut()?;
        c.set("swofty", uidb_dict("swofty", entries));
        Ok(())
    })
}
