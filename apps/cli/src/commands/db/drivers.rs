use swofty_core::dbedit::{Catalog, driver_manager};
use swofty_core::store::Store;

/// Why `path` isn't in the game database when it names a Driver Manager driver's row.
pub(super) fn driver_row(store: &Store, cat: &Catalog, path: &str) -> Option<String> {
    let label = path
        .split(['[', ']', '=', '.'])
        .find(|p| driver_manager::is_label(p))?;
    Some(
        match driver_manager::drivers(store, cat)
            .iter()
            .find(|d| d.label == label)
        {
            Some(d) => format!(
                "{path}: Driver Manager adds {} ({label}) when the profile builds; edit the driver on the Driver Manager page",
                d.name
            ),
            None => format!(
                "{path}: {label} is a Driver Manager label, and Driver Manager doesn't add that driver"
            ),
        },
    )
}

/// Notes on the Driver Manager drivers `value` picks: one Driver Manager doesn't add,
/// or any while it's off in `profile`.
pub(super) fn driver_notes(store: &Store, cat: &Catalog, profile: &str, value: &str) -> Vec<String> {
    let labels: Vec<&str> = value.split(',').filter(|v| driver_manager::is_label(v)).collect();
    if labels.is_empty() {
        return Vec::new();
    }
    let drivers = driver_manager::drivers(store, cat);
    let on = store
        .profile(profile)
        .is_ok_and(|p| p.enabled().any(|id| id == driver_manager::ID));
    labels
        .into_iter()
        .filter_map(|l| match drivers.iter().find(|d| d.label == l) {
            Some(_) if on => None,
            Some(d) => Some(format!(
                "{l} is Driver Manager's {}, and Driver Manager is off in {profile}; {}",
                d.name,
                driver_manager::MISSING
            )),
            None => Some(format!(
                "Driver Manager doesn't add {l}; {}",
                driver_manager::MISSING
            )),
        })
        .collect()
}
