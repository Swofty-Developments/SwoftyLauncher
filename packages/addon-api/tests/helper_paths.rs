use std::sync::Arc;

use swofty_addon::helper::{
    ADDON_CALLS, API_VERSION, BUILD_READS, Built, Call, Hello, HostCall, Incoming, Invalidate, MOVE_ADDON,
    PROFILE_HEADS, RESTART, Reply, Request, SAVE_DIR, TASKS, VIEW_CHART, Welcome, claim_word, run_helper,
};
use swofty_addon::launcher::{Addon, Claim};

#[test]
fn the_helper_protocol_keeps_its_public_paths_and_names() {
    assert_eq!(API_VERSION, 1);
    assert_eq!(VIEW_CHART, "view.chart");
    assert_eq!(BUILD_READS, "build.reads");
    assert_eq!(RESTART, "swofty.restart");
    assert_eq!(
        ADDON_CALLS,
        ["addons", "install_addon", "set_addon_enabled", "remove_addon"]
    );
    assert_eq!(MOVE_ADDON, "move_addon");
    assert_eq!(TASKS, "tasks");
    assert_eq!(PROFILE_HEADS, "profile_heads");
    assert_eq!(SAVE_DIR, "save_dir");
    let _: fn(Arc<dyn Addon>) = run_helper;
    let _: fn(Claim) -> &'static str = claim_word;
    let _ = [
        size_of::<Request<Call>>(),
        size_of::<Request<HostCall>>(),
        size_of::<Hello>(),
        size_of::<Welcome>(),
        size_of::<Built>(),
        size_of::<Invalidate>(),
        size_of::<Reply>(),
        size_of::<Incoming>(),
    ];
}
