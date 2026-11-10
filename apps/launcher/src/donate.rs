use crate::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::*;
use swofty_i18n::tr;

pub const URL: &str = "https://donate.swofty.net/";

const PINK: u32 = 0xe64980;
const PINK_HOVER: u32 = 0xff6b9d;

pub(crate) fn link() -> Stateful<Div> {
    h_flex()
        .id("donate-link")
        .gap(px(6.))
        .items_center()
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(rgb(PINK))
        .hover(|s| s.text_color(rgb(PINK_HOVER)))
        .tooltip(|window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tr("Donate to Developers")).build(window, cx)
        })
        .child(Icon::new(IconName::Heart).size(px(13.)))
        .child(tr("Donate"))
        .on_click(|_, _, cx| cx.open_url(URL))
}
