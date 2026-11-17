use anyhow::{Result, bail};
use swofty_core::dbedit::Catalog;
use swofty_core::dbedit::package::{self, Package};
use swofty_core::store::Store;

pub(super) fn open(store: &Store, id: &str) -> Result<Package> {
    if !package::is_package(store, id) {
        bail!("no DB Editor addon named {id}; swofty db addons lists them");
    }
    Ok(Package::open(store, id)?)
}

/// Opens the addon, applies `change` and saves it; when another writer saved
/// the addon in between, starts over from its file.
pub(super) fn edit<T>(
    store: &Store,
    cat: &Catalog,
    id: &str,
    mut change: impl FnMut(&mut Package) -> Result<T>,
) -> Result<T> {
    for _ in 0..5 {
        let stamp = package::modified(store, id);
        let mut p = open(store, id)?;
        let out = change(&mut p)?;
        if package::modified(store, id) == stamp {
            p.save(store, cat)?;
            return Ok(out);
        }
    }
    bail!("{id} kept changing while saving; try again")
}
