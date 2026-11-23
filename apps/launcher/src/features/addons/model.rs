use super::drag::DropAt;
use crate::services::library::{Changes, Source, Type};
use crate::shell::Shell;
use crate::shell::notices::Notification;
use crate::ui::scale::px;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;
use swofty_core::manifest::Category;
use swofty_core::profile::conflicts::Report;
use swofty_core::profile::folders::Key;
use swofty_core::profile::order;
use swofty_i18n::{tr, trn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum State {
    On,
    Off,
    Conflicts,
    Update,
}

impl State {
    pub(super) const ALL: [State; 4] = [Self::On, Self::Off, Self::Conflicts, Self::Update];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Off => tr("Off"),
            Self::Conflicts => tr("Has conflicts"),
            Self::Update => tr("Update available"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    Category,
    Type,
    Source,
    State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Facet {
    Category(Option<Category>),
    Type(Type),
    Source(Source),
    State(State),
}

impl Facet {
    pub(super) fn group(self) -> Group {
        match self {
            Self::Category(_) => Group::Category,
            Self::Type(_) => Group::Type,
            Self::Source(_) => Group::Source,
            Self::State(_) => Group::State,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Category(Some(c)) => c.label(),
            Self::Category(None) => tr("Uncategorised"),
            Self::Type(t) => t.label(),
            Self::Source(s) => s.label(),
            Self::State(s) => s.label(),
        }
    }
}

/// Addons that load first, in this order: SwoftyMod, then Simplified for Swofty.
/// They make up Built In, and nothing is placed above them.
pub(super) const PINNED: [&str; 2] = [
    swofty_core::addons::builtin::CORE_ID,
    swofty_core::updates::managed::ID,
];

#[derive(Clone, PartialEq)]
pub(super) enum AddonListItem {
    Heading,
    Folder {
        key: Key,
        at: usize,
        count: usize,
        on: usize,
        open: bool,
        members: Vec<String>,
    },
    Addon {
        index: usize,
        id: String,
        position: Option<usize>,
        at: usize,
        nested: Option<(Key, usize)>,
    },
    Detail(String),
    End,
    BrokenHeading,
    Broken(usize),
}

impl AddonListItem {
    fn key(&self) -> String {
        match self {
            Self::Heading => "heading".to_owned(),
            Self::Folder { key, .. } => format!("folder:{key:?}"),
            Self::Addon { id, .. } => format!("addon:{id}"),
            Self::Detail(id) => format!("detail:{id}"),
            Self::End => "end".to_owned(),
            Self::BrokenHeading => "broken-heading".to_owned(),
            Self::Broken(i) => format!("broken:{i}"),
        }
    }
}

pub(super) fn anchor(old: &[AddonListItem], new: &[AddonListItem], top: usize) -> Option<(usize, bool)> {
    let at: HashMap<String, usize> = new.iter().enumerate().map(|(i, item)| (item.key(), i)).collect();
    old.get(top..)?
        .iter()
        .enumerate()
        .find_map(|(n, item)| at.get(&item.key()).map(|&ix| (ix, n == 0)))
}

/// The addon rows the list shows, top to bottom.
pub(super) fn shown_addons(items: &[AddonListItem]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| match item {
            AddonListItem::Addon { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

/// The list index of `id`'s row.
pub(super) fn row_of(items: &[AddonListItem], id: &str) -> Option<usize> {
    items
        .iter()
        .position(|item| matches!(item, AddonListItem::Addon { id: at, .. } if at == id))
}

/// Where Back from the panel open inline under row `row` scrolls the list: to where it was
/// when that panel opened, or else to the row itself when it's above the top of the list.
pub(super) fn back_offset(
    opened_from: Option<&(String, ListOffset)>,
    id: &str,
    row: usize,
    top: ListOffset,
) -> Option<ListOffset> {
    match opened_from {
        Some((at, offset)) if at == id && offset.item_ix <= row => Some(*offset),
        _ => (top.item_ix > row).then_some(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        }),
    }
}

pub(crate) struct AddonsPage {
    pub(super) search: Entity<InputState>,
    pub(super) window: AnyWindowHandle,
    pub(super) facets: BTreeSet<Facet>,
    pub(super) scroll: ListState,
    pub(super) smooth: crate::ui::scroll::SmoothScroll,
    pub(super) list_items: Vec<AddonListItem>,
    pub(super) seen: Option<Instant>,
    pub(super) scroll_pending: bool,
    /// The addon a row click opened, and where the list was scrolled then.
    pub(super) opened_from: Option<(String, ListOffset)>,
    pub(super) animating: bool,
    pub(super) read_at: Option<Instant>,
    pub(super) open_pairs: HashSet<(String, String)>,
    pub(super) details: HashSet<(String, String)>,
    pub(super) all_items: HashSet<(String, String)>,
    pub(super) advanced: bool,
    pub(super) changes: HashMap<String, Changes>,
    pub(super) drop: Option<DropAt>,
    pub(super) selection: super::select::Selection,
    pub(super) ctrl: bool,
    /// From the core's reports of the last session: each addon's status lines, and the registry
    /// conflicts with the addons each is about.
    pub(super) status: std::collections::BTreeMap<String, Vec<String>>,
    pub(super) registry_conflicts: Vec<(Vec<String>, swofty_core::stdlib::reports::Conflict)>,
    pub(super) diagnosis: order::Diagnosis,
    pub(super) problems_key: Option<(Instant, u64, u64)>,
    pub(super) careers: Vec<swofty_core::stdlib::careers::Career>,
    pub(super) _search: Subscription,
    pub(crate) conflicts: Conflicts,
    pub(crate) toasts: Vec<Notification>,
}

impl AddonsPage {
    pub(crate) fn cancel_scroll(&self) {
        self.smooth.cancel();
    }

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Search addons")));
        let changed = cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            search,
            window: window.window_handle(),
            facets: BTreeSet::new(),
            scroll: ListState::new(0, ListAlignment::Top, px(200.)),
            smooth: Default::default(),
            list_items: Vec::new(),
            seen: None,
            scroll_pending: false,
            opened_from: None,
            animating: false,
            read_at: None,
            open_pairs: HashSet::new(),
            details: HashSet::new(),
            all_items: HashSet::new(),
            advanced: false,
            changes: HashMap::new(),
            drop: None,
            selection: Default::default(),
            ctrl: false,
            status: Default::default(),
            registry_conflicts: Vec::new(),
            diagnosis: order::Diagnosis::empty(),
            problems_key: None,
            careers: Vec::new(),
            _search: changed,
            conflicts: Conflicts::default(),
            toasts: Vec::new(),
        }
    }

    pub(crate) fn clear_selection(&mut self) -> bool {
        let had = !self.selection.is_empty();
        self.selection.clear();
        had
    }

    pub(super) fn extend_selection(&mut self, id: &str) {
        self.selection.extend(&shown_addons(&self.list_items), id);
    }

    pub(super) fn ticking(&self) -> bool {
        self.ctrl || !self.selection.is_empty()
    }

    pub(super) fn clear_search(&self, cx: &mut App) {
        let (input, window) = (self.search.clone(), self.window);
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    input.update(cx, |s, cx| s.set_value("", window, cx))
                })
                .ok();
        });
    }
}

/// The last conflict check of the active profile, and whether one is running now.
#[derive(Default)]
pub(crate) struct Conflicts {
    pub(super) report: Option<Report>,
    pub(crate) checking: bool,
}

impl Conflicts {
    pub(super) fn begin(&mut self, blocked: bool) -> bool {
        if self.checking || blocked {
            return false;
        }
        self.checking = true;
        true
    }

    /// Takes a finished check of `profile`'s build, keeping its report while that profile is still
    /// active, and returns the notice that says how it went.
    pub(super) fn take(&mut self, active: &str, result: Result<Report, String>) -> Notification {
        self.checking = false;
        match result {
            Ok(report) => {
                let text = match report.pairs.len() {
                    0 => tr("No conflicts between addons").to_owned(),
                    n => trn(n, "Found {count} conflict", "Found {count} conflicts"),
                };
                if report.profile == active {
                    self.report = Some(report);
                }
                Notification::success(text)
            }
            Err(e) => Notification::error(e).title(tr("Could not check conflicts")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AddonListItem, Conflicts, anchor, back_offset, row_of, shown_addons};
    use gpui_kit::{ListOffset, px};
    use swofty_core::profile::conflicts::Report;
    use swofty_core::profile::folders::Key;

    fn addon(id: &str, index: usize) -> AddonListItem {
        AddonListItem::Addon {
            index,
            id: id.to_owned(),
            position: None,
            at: index,
            nested: None,
        }
    }

    #[test]
    fn anchor_follows_an_addon_whose_index_changed() {
        let old = [
            AddonListItem::Heading,
            addon("a", 0),
            addon("b", 1),
            addon("c", 2),
        ];
        let new = [AddonListItem::Heading, addon("a", 0), addon("c", 1)];
        assert_eq!(anchor(&old, &new, 3), Some((2, true)));
    }

    #[test]
    fn anchor_moves_to_the_next_row_when_the_top_row_is_gone() {
        let old = [
            AddonListItem::Heading,
            addon("a", 0),
            addon("b", 1),
            addon("c", 2),
        ];
        let new = [AddonListItem::Heading, addon("a", 0), addon("c", 1)];
        assert_eq!(anchor(&old, &new, 2), Some((2, false)));
    }

    #[test]
    fn shown_addons_are_the_addon_rows_in_list_order() {
        let folder = |open: bool, members: &[&str]| AddonListItem::Folder {
            key: Key::Own("f1".to_owned()),
            at: 1,
            count: members.len(),
            on: 0,
            open,
            members: members.iter().map(|m| (*m).to_owned()).collect(),
        };
        let items = [
            AddonListItem::Heading,
            addon("a", 0),
            AddonListItem::Detail("a".to_owned()),
            folder(false, &["b", "c"]),
            folder(true, &["d"]),
            addon("d", 3),
            addon("e", 4),
            AddonListItem::End,
            AddonListItem::BrokenHeading,
            AddonListItem::Broken(0),
        ];
        assert_eq!(shown_addons(&items), ["a", "d", "e"]);
    }

    #[test]
    fn anchor_is_none_without_old_rows() {
        assert_eq!(anchor(&[], &[AddonListItem::Heading], 0), None);
    }

    fn at(item_ix: usize, offset: f32) -> ListOffset {
        ListOffset {
            item_ix,
            offset_in_item: px(offset),
        }
    }

    #[test]
    fn back_from_an_inline_panel_returns_the_list_to_where_it_was_when_the_panel_opened() {
        let items = [
            AddonListItem::Heading,
            addon("a", 0),
            addon("b", 1),
            AddonListItem::Detail("b".to_owned()),
            addon("c", 2),
        ];
        let row = row_of(&items, "b").unwrap();
        assert_eq!(row, 2);
        let opened = ("b".to_owned(), at(1, 12.));
        assert_eq!(
            back_offset(Some(&opened), "b", row, at(3, 900.)),
            Some(at(1, 12.))
        );
        assert_eq!(back_offset(Some(&opened), "b", row, at(1, 12.)), Some(at(1, 12.)));
    }

    #[test]
    fn back_without_a_saved_scroll_shows_the_row_once_it_is_above_the_list() {
        let opened = ("a".to_owned(), at(0, 0.));
        assert_eq!(back_offset(Some(&opened), "b", 2, at(3, 400.)), Some(at(2, 0.)));
        assert_eq!(back_offset(None, "b", 2, at(3, 0.)), Some(at(2, 0.)));
        assert_eq!(back_offset(None, "b", 2, at(2, 30.)), None);
        assert_eq!(back_offset(None, "b", 2, at(0, 0.)), None);
    }

    #[test]
    fn a_saved_scroll_below_the_row_is_not_used() {
        let opened = ("b".to_owned(), at(4, 0.));
        assert_eq!(back_offset(Some(&opened), "b", 2, at(1, 0.)), None);
        assert_eq!(row_of(&[AddonListItem::Heading], "b"), None);
    }

    fn report(profile: &str) -> Report {
        Report {
            profile: profile.to_owned(),
            ..Report::default()
        }
    }

    #[test]
    fn a_conflict_check_runs_once_at_a_time_and_never_while_blocked() {
        let mut c = Conflicts::default();
        assert!(!c.begin(true));
        assert!(!c.checking);
        assert!(c.begin(false));
        assert!(!c.begin(false));
        c.take("Main", Ok(report("Main")));
        assert!(!c.checking);
        assert!(c.begin(false));
    }

    #[test]
    fn a_check_of_the_active_profile_keeps_its_report() {
        let mut c = Conflicts::default();
        c.begin(false);
        c.take("Main", Ok(report("Main")));
        assert_eq!(c.report.as_ref().map(|r| r.profile.as_str()), Some("Main"));
    }

    #[test]
    fn a_check_finishing_after_a_profile_switch_leaves_the_report_alone() {
        let mut c = Conflicts {
            report: Some(report("Other")),
            checking: true,
        };
        c.take("Other", Ok(report("Main")));
        assert_eq!(c.report.as_ref().map(|r| r.profile.as_str()), Some("Other"));
        assert!(!c.checking);
    }

    #[test]
    fn a_failed_check_ends_and_keeps_the_last_report() {
        let mut c = Conflicts {
            report: Some(report("Main")),
            checking: true,
        };
        c.take("Main", Err("Locate F1 25 first.".into()));
        assert!(!c.checking);
        assert!(c.report.is_some());
    }
}
