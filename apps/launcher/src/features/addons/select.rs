#[derive(Default)]
pub(super) struct Selection {
    ids: Vec<String>,
    anchor: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ticks {
    None,
    Some,
    All,
}

impl Selection {
    pub(super) fn toggle(&mut self, id: &str) {
        match self.ids.iter().position(|i| i == id) {
            Some(at) => {
                self.ids.remove(at);
            }
            None => self.ids.push(id.to_owned()),
        }
        self.set_anchor(id);
    }

    pub(super) fn set_anchor(&mut self, id: &str) {
        self.anchor = Some(id.to_owned());
    }

    pub(super) fn extend(&mut self, shown: &[&str], id: &str) {
        let at = |x: &str| shown.iter().position(|s| *s == x);
        match (self.anchor.as_deref().and_then(at), at(id)) {
            (Some(a), Some(b)) => self.add(&shown[a.min(b)..=a.max(b)]),
            _ => self.add(&[id]),
        }
        self.set_anchor(id);
    }

    pub(super) fn ticks(&self, ids: &[String]) -> Ticks {
        match ids.iter().filter(|id| self.contains(id)).count() {
            0 => Ticks::None,
            n if n == ids.len() => Ticks::All,
            _ => Ticks::Some,
        }
    }

    pub(super) fn toggle_all(&mut self, ids: &[String]) {
        match self.ticks(ids) {
            Ticks::All => self.ids.retain(|id| !ids.contains(id)),
            Ticks::None | Ticks::Some => {
                let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                self.add(&ids);
            }
        }
    }

    fn add(&mut self, ids: &[&str]) {
        for id in ids {
            if !self.contains(id) {
                self.ids.push((*id).to_owned());
            }
        }
    }

    pub(super) fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|i| i == id)
    }

    pub(super) fn clear(&mut self) {
        self.ids.clear();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.ids.len()
    }

    pub(super) fn ids(&self) -> &[String] {
        &self.ids
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.ids.retain(|id| keep(id));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Bulk {
    On,
    Off,
    Uninstall,
}

pub(super) struct Pick {
    pub(super) id: String,
    pub(super) locked: bool,
    pub(super) builtin: bool,
    pub(super) shader_only: bool,
    pub(super) warns_on_off: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Plan {
    pub(super) apply: Vec<String>,
    pub(super) skipped: usize,
    pub(super) warned: Vec<String>,
}

pub(super) fn plan(action: Bulk, picks: &[Pick]) -> Plan {
    let mut out = Plan::default();
    for pick in picks {
        let fixed = match action {
            Bulk::On | Bulk::Off => pick.locked || pick.shader_only,
            Bulk::Uninstall => pick.locked || pick.builtin,
        };
        if fixed {
            out.skipped += 1;
        } else if action == Bulk::Off && pick.warns_on_off {
            out.warned.push(pick.id.clone());
        } else {
            out.apply.push(pick.id.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(id: &str) -> Pick {
        Pick {
            id: id.to_owned(),
            locked: false,
            builtin: false,
            shader_only: false,
            warns_on_off: false,
        }
    }

    #[test]
    fn toggle_selects_then_deselects() {
        let mut s = Selection::default();
        s.toggle("a");
        s.toggle("b");
        assert_eq!(s.ids(), ["a", "b"]);
        assert!(s.contains("a"));
        s.toggle("a");
        assert_eq!(s.ids(), ["b"]);
        assert_eq!(s.len(), 1);
        assert!(!s.is_empty());
    }

    #[test]
    fn clear_empties_the_selection() {
        let mut s = Selection::default();
        s.toggle("a");
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn retain_drops_ids_that_are_gone() {
        let mut s = Selection::default();
        s.toggle("a");
        s.toggle("b");
        s.retain(|id| id == "b");
        assert_eq!(s.ids(), ["b"]);
    }

    const SHOWN: [&str; 5] = ["a", "b", "c", "d", "e"];

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn shift_click_ticks_every_shown_row_from_the_anchor() {
        let mut s = Selection::default();
        s.toggle("b");
        s.extend(&SHOWN, "d");
        assert_eq!(s.ids(), ["b", "c", "d"]);
    }

    #[test]
    fn shift_click_above_the_anchor_ticks_the_rows_between_in_list_order() {
        let mut s = Selection::default();
        s.toggle("d");
        s.extend(&SHOWN, "a");
        assert_eq!(s.ids(), ["d", "a", "b", "c"]);
    }

    #[test]
    fn a_plain_click_sets_the_anchor_without_ticking() {
        let mut s = Selection::default();
        s.set_anchor("b");
        assert!(s.is_empty());
        s.extend(&SHOWN, "c");
        assert_eq!(s.ids(), ["b", "c"]);
    }

    #[test]
    fn the_last_clicked_row_is_the_next_anchor() {
        let mut s = Selection::default();
        s.set_anchor("a");
        s.extend(&SHOWN, "b");
        s.extend(&SHOWN, "d");
        assert_eq!(s.ids(), ["a", "b", "c", "d"]);
        s.toggle("e");
        s.toggle("e");
        s.extend(&SHOWN, "e");
        assert_eq!(s.ids(), ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn shift_click_without_an_anchor_ticks_only_that_row() {
        let mut s = Selection::default();
        s.extend(&SHOWN, "c");
        assert_eq!(s.ids(), ["c"]);
        s.extend(&SHOWN, "e");
        assert_eq!(s.ids(), ["c", "d", "e"]);
    }

    #[test]
    fn a_range_covers_only_shown_rows() {
        let shown = ["a", "d", "e"];
        let mut s = Selection::default();
        s.toggle("a");
        s.extend(&shown, "e");
        assert_eq!(s.ids(), ["a", "d", "e"]);
    }

    #[test]
    fn a_hidden_anchor_ticks_only_the_clicked_row() {
        let shown = ["a", "d", "e"];
        let mut s = Selection::default();
        s.toggle("b");
        s.extend(&shown, "e");
        assert_eq!(s.ids(), ["b", "e"]);
    }

    #[test]
    fn a_range_keeps_rows_already_ticked_once() {
        let mut s = Selection::default();
        s.toggle("c");
        s.toggle("a");
        s.extend(&SHOWN, "d");
        assert_eq!(s.ids(), ["c", "a", "b", "d"]);
    }

    #[test]
    fn clearing_keeps_the_anchor() {
        let mut s = Selection::default();
        s.toggle("b");
        s.clear();
        s.extend(&SHOWN, "c");
        assert_eq!(s.ids(), ["b", "c"]);
    }

    #[test]
    fn a_folder_shows_none_some_or_all_ticked() {
        let folder = ids(&["b", "c"]);
        let mut s = Selection::default();
        assert_eq!(s.ticks(&folder), Ticks::None);
        s.toggle("a");
        assert_eq!(s.ticks(&folder), Ticks::None);
        s.toggle("c");
        assert_eq!(s.ticks(&folder), Ticks::Some);
        s.toggle("b");
        assert_eq!(s.ticks(&folder), Ticks::All);
        assert_eq!(s.ticks(&[]), Ticks::None);
    }

    #[test]
    fn a_folder_tick_ticks_all_its_addons() {
        let folder = ids(&["b", "c", "d"]);
        let mut s = Selection::default();
        s.toggle("a");
        s.toggle_all(&folder);
        assert_eq!(s.ids(), ["a", "b", "c", "d"]);
        assert_eq!(s.ticks(&folder), Ticks::All);
    }

    #[test]
    fn a_partly_ticked_folder_ticks_the_rest() {
        let folder = ids(&["b", "c", "d"]);
        let mut s = Selection::default();
        s.toggle("c");
        s.toggle_all(&folder);
        assert_eq!(s.ids(), ["c", "b", "d"]);
    }

    #[test]
    fn a_fully_ticked_folder_unticks_only_its_addons() {
        let folder = ids(&["b", "c"]);
        let mut s = Selection::default();
        s.toggle("a");
        s.toggle_all(&folder);
        s.toggle("e");
        s.toggle_all(&folder);
        assert_eq!(s.ids(), ["a", "e"]);
        assert_eq!(s.ticks(&folder), Ticks::None);
    }

    #[test]
    fn a_folder_tick_leaves_the_anchor() {
        let mut s = Selection::default();
        s.set_anchor("a");
        s.toggle_all(&ids(&["d", "e"]));
        s.extend(&SHOWN, "b");
        assert_eq!(s.ids(), ["d", "e", "a", "b"]);
    }

    #[test]
    fn turning_on_skips_always_on_and_shader_only() {
        let locked = Pick {
            locked: true,
            ..pick("core")
        };
        let shader = Pick {
            shader_only: true,
            ..pick("look")
        };
        let p = plan(Bulk::On, &[pick("a"), locked, shader, pick("b")]);
        assert_eq!(p.apply, ["a", "b"]);
        assert_eq!(p.skipped, 2);
        assert!(p.warned.is_empty());
    }

    #[test]
    fn turning_off_leaves_addons_that_warn_about_saves() {
        let warns = Pick {
            warns_on_off: true,
            ..pick("drivers")
        };
        let p = plan(Bulk::Off, &[pick("a"), warns]);
        assert_eq!(p.apply, ["a"]);
        assert_eq!(p.warned, ["drivers"]);
        assert_eq!(p.skipped, 0);
    }

    #[test]
    fn turning_on_ignores_the_saves_warning() {
        let warns = Pick {
            warns_on_off: true,
            ..pick("drivers")
        };
        assert_eq!(plan(Bulk::On, &[warns]).apply, ["drivers"]);
    }

    #[test]
    fn uninstall_skips_built_in_and_always_on() {
        let builtin = Pick {
            builtin: true,
            ..pick("simplified")
        };
        let locked = Pick {
            locked: true,
            ..pick("core")
        };
        let warns = Pick {
            warns_on_off: true,
            ..pick("drivers")
        };
        let p = plan(Bulk::Uninstall, &[builtin, pick("a"), locked, warns]);
        assert_eq!(p.apply, ["a", "drivers"]);
        assert_eq!(p.skipped, 2);
    }

    #[test]
    fn uninstall_allows_a_shader_only_addon() {
        let shader = Pick {
            shader_only: true,
            ..pick("look")
        };
        assert_eq!(plan(Bulk::Uninstall, &[shader]).apply, ["look"]);
    }
}
