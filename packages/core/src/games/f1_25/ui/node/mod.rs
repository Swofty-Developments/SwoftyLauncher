mod build;
mod ext;
mod file;
mod frame;
mod ids;
mod paths;
mod signals;
mod typed;
mod values;

pub use build::{
    behaviours, hidden, instance, layout_path, literal_label, literal_text, node, screen, set_bool,
    strip_data,
};
pub use ext::{NodeExt, ObjExt, set_prop, set_prop_value, set_str, set_text_value, set_value};
pub use file::{canonical, edit, first_screen, first_screen_of, root_obj, ui_bytes, uidb_dict, uidb_var};
pub use frame::{Frame, FrameSources, chain_focus, finish, frame};
pub use ids::{clone_node, fix_ids, rename, rename_refs, replace_strings};
pub use paths::{Binding, bindings, find_path, node_at, node_at_mut, node_paths_of};
pub use signals::{Attr, add_events, command, drop_events_to, event, raise_cmd, sig, sig_with};
pub use typed::{
    FRACTION_BEHAVIOURS, LABEL_BEHAVIOURS, bool_prop, colour_node, decision_commands, fraction_node,
    group_node, label_node, mvcd_node, static_colour_node, svcd_node, texture_node, typed_prop, visible_node,
    wire_command,
};
pub use values::{arr, boolv, component_id, err, f32v, fourcc, fourcc_str, i32v, obj, prop, s, u32v};

pub const PROPS: usize = 0;
pub const CHILDREN: usize = 1;
pub const COMMANDS: usize = 2;
pub const EVENTS: usize = 3;
pub const LAYOUT: usize = 4;
/// A node's own component scene, as a block's o_block.
pub const COMPONENT: usize = 5;
pub const BEHAVIOURS: usize = 6;

pub const SCREEN_BEHAVIOURS: &[&str] = &[
    "NBEL", "NBDC", "NBMG", "NBVM", "NBAC", "NBOS", "NBXA", "NBMO", "NBTR",
];
pub const GROUP_BEHAVIOURS: &[&str] = &[
    "NBEL", "NBDC", "NBMG", "NBVM", "NBFC", "NBAN", "NBAC", "NBOS", "NBXA", "NBMO",
];
const DECISIONS: &[&str] = &["SVCD", "MVCD", "DBVV", "DBRV", "DBCD", "DBAM", "WUDN", "UIVF"];
