use crate::game::Owner;
use gpui_kit::assets::IconName;
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
            Self::Settings => tr("Settings"),
            Self::Section(_) => "",
        }
    }

    /// The sidebar's name for the page: the importer is just Importer there.
    pub fn nav_label(self) -> &'static str {
        match self {
            Self::Importer => tr("Importer"),
            _ => self.label(),
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::News => IconName::Newspaper,
            Self::Addons => IconName::Puzzle,
            Self::Browser => IconName::PackageSearch,
            Self::Downloads => IconName::Download,
            Self::Shaders => IconName::Palette,
            Self::Importer => IconName::FolderInput,
            Self::Tools => IconName::Wrench,
            Self::Profiles => IconName::Layers,
            Self::Careers => IconName::Save,
            Self::Database => IconName::Database,
            Self::Multiplayer => IconName::Users,
            Self::Settings => IconName::Settings,
            Self::Section(_) => IconName::Package,
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
