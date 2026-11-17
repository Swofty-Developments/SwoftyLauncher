use crate::dialogs::data_move::Moving;
use crate::dialogs::found_data::FoundPrompt;
use crate::dialogs::tasks::TaskDialog;
use crate::features::onboarding::Onboarding;

/// The dialogs and full-window overlays that are open over the page.
#[derive(Default)]
pub(crate) struct Overlays {
    pub(crate) progress: Option<(f32, String)>,
    pub(crate) tasks: Option<TaskDialog>,
    pub(crate) moving: Option<Moving>,
    pub(crate) found_data: Option<FoundPrompt>,
    pub(crate) dropping: bool,
    pub(crate) onboarding: Option<Onboarding>,
}
