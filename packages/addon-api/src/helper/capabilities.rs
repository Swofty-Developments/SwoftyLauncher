pub const API_VERSION: u32 = 1;
/// Optional support for the data-only native Chart view block.
pub const VIEW_CHART: &str = "view.chart";
/// Seven optional add-on-controlled Native Chart appearance and image extensions.
pub const VIEW_CHART_OPTIONS: &str = "view.chart.options";
/// Optional caller-supplied in-memory PNG pictures in generic table cells.
pub const VIEW_CELL_IMAGE_DATA: &str = "view.cell.image_data";
/// Listed from launcher 1.10: the launcher may answer a `build` with an earlier reply that lists its `reads`.
pub const BUILD_READS: &str = "build.reads";
pub const RESTART: &str = "swofty.restart";
// The host calls a launcher lists in hello's host_calls when it answers them.
pub const ADDON_CALLS: [&str; 4] = ["addons", "install_addon", "set_addon_enabled", "remove_addon"];
// Listed from launcher 1.3.0, beside ADDON_CALLS.
pub const MOVE_ADDON: &str = "move_addon";
// Listed from launcher 1.3.0; a helper sends tasks only when hello lists it.
pub const TASKS: &str = "tasks";
// Listed from launcher 1.5.12 when the host knows the game.
pub const PROFILE_HEADS: &str = "profile_heads";
// Listed from launcher 1.9.29, after the other host calls.
pub const SAVE_DIR: &str = "save_dir";
