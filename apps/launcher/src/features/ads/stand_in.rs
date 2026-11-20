//! A box the size of a banner where it goes, to look at a page's layout with ads while they are off. Debug builds
//! only: a release build never reads the variable.

use std::ffi::OsStr;
use std::sync::LazyLock;

use super::Unit;
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use swofty_i18n::tr;

/// Set, banners that have no ad to show draw the stand-in.
const ENV: &str = "SWOFTY_ROOM_PREVIEW_AD";

static ON: LazyLock<bool> =
    LazyLock::new(|| enabled(cfg!(debug_assertions), std::env::var_os(ENV).as_deref()));

/// Whether banners draw the stand-in: in a `debug` build with [`ENV`] `set`.
fn enabled(debug: bool, set: Option<&OsStr>) -> bool {
    debug && set.is_some_and(|v| !v.is_empty())
}

/// The stand-in for `unit`, while it is on.
pub(super) fn stand_in(unit: Unit, p: Palette) -> Option<Div> {
    if !*ON {
        return None;
    }
    let size = unit.size();
    Some(
        v_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .child(tr("Advertisement")),
            )
            .child(
                div()
                    .w(size.width)
                    .h(size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(format!(
                        "{} \u{00d7} {}",
                        f32::from(size.width),
                        f32::from(size.height)
                    )),
            ),
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::enabled;

    #[test]
    fn only_a_debug_build_draws_the_ad_stand_in() {
        assert!(enabled(true, Some(OsStr::new("1"))));
        assert!(!enabled(false, Some(OsStr::new("1"))), "a release build");
        assert!(!enabled(true, None));
        assert!(!enabled(true, Some(OsStr::new(""))));
    }
}
