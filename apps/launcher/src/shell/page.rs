use crate::game::Owner;
use swofty_core::session::SessionRecord;
use swofty_i18n::tr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    News,
    Addons,
    Browser,
    Downloads,
    Shaders,
    Importer,
    Tools,
    Profiles,
    Careers,
    Database,
    Multiplayer,
    Online,
    Settings,
    Section(usize),
}

impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Self::News => tr("News"),
            Self::Addons => tr("Addons"),
            Self::Browser => tr("Addon Browser"),
            Self::Downloads => tr("Downloads"),
            Self::Shaders => tr("Shaders"),
            Self::Importer => tr("Simplified Importer"),
            Self::Tools => tr("Tools"),
            Self::Profiles => tr("Profiles"),
            Self::Careers => tr("Saves"),
            Self::Database => tr("DB Editor"),
            Self::Multiplayer => tr("Multiplayer"),
            Self::Online => tr("Online"),
            Self::Settings => tr("Settings"),
            Self::Section(_) => "",
        }
    }

    /// The top bar's name for the page: the importer is just Importer there.
    pub fn nav_label(self) -> &'static str {
        match self {
            Self::Importer => tr("Importer"),
            _ => self.label(),
        }
    }
}

pub enum PlayState {
    Detecting,
    NotInstalled,
    Ready,
    Launching,
    Restoring,
    Running,
    Elsewhere(Owner),
    Checking,
    Preparing,
    Crashed(SessionRecord),
    Building,
}
