//! The games the launcher knows: each one's id, name, exe and the stores that sell it.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameId {
    F1_25,
    Overcooked2,
}

impl GameId {
    pub const ALL: [GameId; 2] = [GameId::F1_25, GameId::Overcooked2];

    pub const fn info(self) -> &'static GameInfo {
        match self {
            GameId::F1_25 => &F1_25,
            GameId::Overcooked2 => &OVERCOOKED_2,
        }
    }

    pub const fn id(self) -> &'static str {
        self.info().id
    }

    pub fn parse(id: &str) -> Option<GameId> {
        Self::ALL.into_iter().find(|g| g.id() == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreId {
    Steam { app_id: u32 },
    EaApp { install_keys: &'static [&'static str] },
    Epic { exe: &'static str },
}

impl StoreId {
    pub const fn id(&self) -> &'static str {
        match self {
            StoreId::Steam { .. } => "steam",
            StoreId::EaApp { .. } => "ea",
            StoreId::Epic { .. } => "epic",
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            StoreId::Steam { .. } => "Steam",
            StoreId::EaApp { .. } => "EA app",
            StoreId::Epic { .. } => "Epic Games",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Full,
    NotYet,
}

impl Support {
    pub const fn id(self) -> &'static str {
        match self {
            Support::Full => "full",
            Support::NotYet => "not_yet",
        }
    }
}

#[derive(Debug)]
pub struct GameInfo {
    pub game: GameId,
    pub id: &'static str,
    pub name: &'static str,
    pub exes: &'static [&'static str],
    pub stores: &'static [StoreId],
    pub support: Support,
}

impl GameInfo {
    pub const fn exe(&self) -> &'static str {
        self.exes[0]
    }

    pub fn steam_app_id(&self) -> Option<u32> {
        self.stores.iter().find_map(|s| match s {
            StoreId::Steam { app_id } => Some(*app_id),
            _ => None,
        })
    }
}

pub const F1_25_EA_KEYS: &[&str] = &[
    r"HKLM\SOFTWARE\Codemasters\F1_25",
    r"HKLM\SOFTWARE\WOW6432Node\Codemasters\F1_25",
    r"HKLM\SOFTWARE\EA Games\F1 25",
    r"HKLM\SOFTWARE\WOW6432Node\EA Games\F1 25",
    r"HKLM\SOFTWARE\EA Sports\F1 25",
    r"HKLM\SOFTWARE\WOW6432Node\EA Sports\F1 25",
];

pub const F1_25: GameInfo = GameInfo {
    game: GameId::F1_25,
    id: "f1_25",
    name: "F1 25",
    exes: &["F1_25.exe"],
    stores: &[
        StoreId::Steam { app_id: 3_059_520 },
        StoreId::EaApp {
            install_keys: F1_25_EA_KEYS,
        },
    ],
    support: Support::Full,
};

pub const OVERCOOKED_2: GameInfo = GameInfo {
    game: GameId::Overcooked2,
    id: "overcooked_2",
    name: "Overcooked 2",
    exes: &["Overcooked2.exe"],
    stores: &[
        StoreId::Steam { app_id: 728_880 },
        StoreId::Epic {
            exe: "Overcooked2.exe",
        },
    ],
    support: Support::NotYet,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_round_trips_and_names_its_own_info() {
        for game in GameId::ALL {
            assert_eq!(GameId::parse(game.id()), Some(game));
            assert_eq!(game.info().game, game);
        }
        assert_eq!(GameId::parse("F1 25"), None);
        assert_eq!(GameId::parse(""), None);
    }

    #[test]
    fn f1_25_is_the_game_the_released_constants_name() {
        assert_eq!(crate::GAME_ID, "f1_25");
        assert_eq!(F1_25.id, crate::GAME_ID);
        assert_eq!(F1_25.exe(), "F1_25.exe");
        assert_eq!(F1_25.steam_app_id(), Some(3_059_520));
        assert_eq!(F1_25.support, Support::Full);
    }

    #[test]
    fn overcooked_2_is_a_steam_and_epic_game_not_supported_yet() {
        assert_eq!(OVERCOOKED_2.id, "overcooked_2");
        assert_eq!(OVERCOOKED_2.exe(), "Overcooked2.exe");
        assert_eq!(OVERCOOKED_2.steam_app_id(), Some(728_880));
        assert_eq!(OVERCOOKED_2.support, Support::NotYet);
        assert_eq!(OVERCOOKED_2.support.id(), "not_yet");
    }
}
