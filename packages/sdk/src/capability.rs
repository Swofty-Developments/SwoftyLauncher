#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Log,
    Scan,
    Hook,
    ReadDb,
    Options,
    OnFrame,
    Assets,
    Events,
    Overlay,
    Telemetry,
    Storage,
    Career,
    Popup,
    Lang,
}

impl Capability {
    pub const ALL: [Capability; 14] = [
        Self::Log,
        Self::Scan,
        Self::Hook,
        Self::ReadDb,
        Self::Options,
        Self::OnFrame,
        Self::Assets,
        Self::Events,
        Self::Overlay,
        Self::Telemetry,
        Self::Storage,
        Self::Career,
        Self::Popup,
        Self::Lang,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Scan => "scan",
            Self::Hook => "hook",
            Self::ReadDb => "read-db",
            Self::Options => "options",
            Self::OnFrame => "on-frame",
            Self::Assets => "assets",
            Self::Events => "events",
            Self::Overlay => "overlay",
            Self::Telemetry => "telemetry",
            Self::Storage => "storage",
            Self::Career => "career",
            Self::Popup => "popup",
            Self::Lang => "lang",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub fn available_in(self, api_version: u32) -> bool {
        match self {
            Self::Log | Self::Scan => api_version >= 1,
            Self::Hook => api_version >= 2,
            Self::Options => api_version >= 3,
            Self::Assets => api_version >= 4,
            Self::ReadDb | Self::OnFrame | Self::Events | Self::Overlay | Self::Telemetry | Self::Storage => {
                api_version >= 5
            }
            Self::Career => api_version >= 6,
            Self::Popup => api_version >= 7,
            Self::Lang => api_version >= 8,
        }
    }
}
