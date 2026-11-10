use crate::theme::Palette;
use gpui_kit::*;

use super::Shell;

impl Shell {
    pub(super) fn title_bar(&self, p: Palette, _: &mut Context<Self>) -> impl IntoElement {
        crate::ui::title_bar(p)
    }
}
