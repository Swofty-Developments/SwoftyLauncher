//! The Addons page: install cards, filters, the list in load order and the
//! selected addon's details and conflicts.

mod conflicts;
mod detail;
mod drag;
mod filters;
mod folders;
mod list;
mod model;
mod needs;
mod options;
mod page;
mod problems;
mod row;
mod sync;

use std::time::Duration;

pub(crate) use crate::format::ago_since as ago;
pub(crate) use filters::{checkbox, facet_row};
pub(crate) use model::{AddonsPage, Type};

const FADE: Duration = Duration::from_secs(4);
pub(crate) const FILTERS_W: f32 = 226.;
pub(crate) const DETAIL_W: f32 = 380.;
pub(crate) const SPLIT_AT: f32 = 1000.;
const ITEMS_SHOWN: usize = 10;
