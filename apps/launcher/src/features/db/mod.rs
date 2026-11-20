mod draft;
mod edit;
mod fields;
mod import;
mod link;
mod page;
mod raw;
mod save;
mod state;
#[cfg(test)]
mod tests;
mod util;
mod view;

pub(crate) use fields::Slots;
pub(crate) use link::LinkState;
pub(crate) use state::{CellEdit, Db, DbChange, DbPrompt, Debounced, Load, Nav, Reveal, Then, ViewSlot};
pub(crate) use util::{
    ADDABLE, bool_text, clean, debug_path, guarded, hex, home, icon_named, kind_of, lock, short, swap_field,
    text, type_name,
};
