//! The budget of the game's UI tree walk (`0x144167620`, called by the job `0x144344970`).
//!
//! The game flattens a screen's node objects into 16-byte slots (`0x14416ac60`): one slot per node,
//! plus `(n + 4) / 8` more for the child list of a node with four or more children. Children are
//! named by a `u16` slot index, so a flattened tree holds at most 65,536 slots; past that the
//! indices wrap to the start of the tree and the walk never ends. The walk keeps a fixed stack of
//! 256 entries: a node with `n` children pushes itself and its other `n - 1` children, walks the
//! first child at once, and the game aborts (`Ne_FatalAbort`) when a push finds the stack full.
//!
//! Every abort of this walk seen in the field had a tree past 65,536 slots: 66,208 and 66,306 in HUD
//! customisation with one tower on, 73,903 (an `EGO_UI` block of 0x120af0 bytes) at race start with
//! three towers on one board. A board instances the host scene once for the stock board and once for
//! each screen record on it, and every instance holds every screen record's root and editor copy, so
//! the records' slots add up and the sum is taken `1 + records` times ([`Board`]). Stacks do not add
//! up: a record's root is walked with only its later siblings waiting, so each record keeps its own.

use swofty_formats::pkg::Node;

/// Slots of one flattened tree (`u16` child indices).
pub const SLOT_LIMIT: usize = 0x1_0000;
/// Entries of the walk's stack.
pub const STACK_LIMIT: usize = 0x100;
/// Slots of the race HUD's tree beside the standings board, rounded up: the race-start abort's tree
/// held 73,903 slots around a board of four 14,101-slot host scenes.
pub const RACE_REST: usize = 17_500;
/// Boards in the HUD customisation tree, and its slots beside them: its abort's tree held 66,306
/// slots, six 10,915-slot host scenes and 816 more.
pub const EDITOR_BOARDS: usize = 3;
pub const EDITOR_REST: usize = 816;

/// What a subtree costs the walk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cost {
    pub nodes: usize,
    pub slots: usize,
    /// The most stack entries the subtree holds at once, walked from an empty stack.
    pub stack: usize,
    /// The largest child count of one node.
    pub widest: usize,
}

/// Slots a node with `children` children takes by itself.
pub fn own_slots(children: usize) -> usize {
    if children < 4 { 1 } else { 1 + (children + 4) / 8 }
}

/// Whether a node with `children` children can be walked with `held` entries on the stack.
pub fn fits(held: usize, children: usize) -> bool {
    held + children <= STACK_LIMIT
}

fn kids(e: &Node) -> impl Iterator<Item = &Node> {
    e.as_obj()
        .and_then(|o| o.get("children"))
        .and_then(Node::as_arr)
        .into_iter()
        .flat_map(|a| a.items.iter().filter(|c| c.as_obj().is_some()))
}

impl Cost {
    /// A node whose children cost `children`, in the scene's order.
    pub fn of(children: &[Cost]) -> Cost {
        let n = children.len();
        let mut out = Cost {
            nodes: 1,
            slots: own_slots(n),
            stack: n,
            widest: n,
        };
        for (i, c) in children.iter().enumerate() {
            out.nodes += c.nodes;
            out.slots += c.slots;
            out.widest = out.widest.max(c.widest);
            if c.stack > 0 {
                out.stack = out.stack.max(n - i + c.stack);
            }
        }
        out
    }
}

/// One screen's board: the stock instance of the host scene and one instance for each of the
/// `records` screen records on the screen, each holding the whole `host` scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    pub host: usize,
    pub records: usize,
}

impl Board {
    pub fn instances(&self) -> usize {
        1 + self.records
    }

    pub fn slots(&self) -> usize {
        self.instances() * self.host
    }

    /// The slots of the tree of the screen that shows the board.
    pub fn tree(&self) -> usize {
        self.slots() + RACE_REST
    }

    /// The slots of the HUD customisation tree.
    pub fn editor(&self) -> usize {
        EDITOR_BOARDS * self.slots() + EDITOR_REST
    }

    /// Whether the screen that shows the board is past the slot limit, where the game aborts.
    pub fn over(&self) -> bool {
        self.tree() > SLOT_LIMIT
    }

    pub fn editor_over(&self) -> bool {
        self.editor() > SLOT_LIMIT
    }

    /// The board without one addon's records: `slots` of the host scene and `here` of the records on
    /// the screen.
    pub fn without(&self, slots: usize, here: usize) -> Board {
        Board {
            host: self.host.saturating_sub(slots),
            records: self.records.saturating_sub(here),
        }
    }
}

/// The cost of the scene element `e` and everything under it.
pub fn cost(e: &Node) -> Cost {
    let children: Vec<Cost> = kids(e).map(cost).collect();
    Cost::of(&children)
}

/// The build lines for the record `pfx` of `addon`, whose live root costs `live` and whose editor
/// copy costs `preview` in the host scene.
pub fn lines(addon: &str, pfx: &str, live: Cost, preview: Cost) -> Vec<String> {
    let stack = live.stack.max(preview.stack);
    let mut out = vec![format!(
        "{pfx}: {addon} adds {} nodes to the host scene ({} walk slots, {} in the editor copy; walk stack {stack} of {STACK_LIMIT})",
        live.nodes + preview.nodes,
        live.slots + preview.slots,
        preview.slots,
    )];
    if stack > STACK_LIMIT {
        out.push(format!(
            "warning: {pfx}: {addon} is past the game's UI walk stack ({stack} of {STACK_LIMIT}); the game aborts on a screen that shows it"
        ));
    }
    out
}

/// The build lines for the board of `screen`; the HUD customisation warning for the standings only.
pub fn board_lines(screen: &str, b: Board) -> Vec<String> {
    let mut out = vec![format!(
        "screen {screen}: the board holds the host scene {} times ({} walk slots each, {} in all); the screen's UI tree about {} of the {SLOT_LIMIT} slots it can index",
        b.instances(),
        b.host,
        b.slots(),
        b.tree()
    )];
    if screen == "standings" && b.editor_over() {
        out.push(format!(
            "warning: screen {screen}: HUD customisation would hold about {} of the {SLOT_LIMIT} slots of one UI tree; the game can close when it opens",
            b.editor()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAF: Cost = Cost {
        nodes: 1,
        slots: 1,
        stack: 0,
        widest: 0,
    };

    fn wide(n: usize, child: Cost) -> Cost {
        Cost::of(&vec![child; n])
    }

    #[test]
    fn a_child_list_spills_into_more_slots_from_four_children() {
        assert_eq!([0, 3, 4, 11, 12, 26, 59].map(own_slots), [1, 1, 2, 2, 3, 4, 8]);
    }

    #[test]
    fn the_stack_holds_the_node_and_its_waiting_siblings_on_the_way_down() {
        assert_eq!(wide(26, LEAF).stack, 26);
        let chain = Cost::of(&[Cost::of(&[LEAF])]);
        assert_eq!(chain.stack, 2);
        // the last child is walked with only its parent's entry left
        assert_eq!(Cost::of(&[LEAF, LEAF, wide(5, LEAF)]).stack, 6);
        assert_eq!(Cost::of(&[wide(5, LEAF), LEAF, LEAF]).stack, 8);
    }

    /// Both field reports: a node with 26 children, each a node with one child (two slots apart),
    /// met the stack with 236 entries held, and the push of the 20th entry found it full.
    #[test]
    fn the_reported_abort_is_a_26_child_node_on_a_stack_of_236() {
        let node = wide(26, Cost::of(&[LEAF]));
        assert_eq!(node.slots, 4 + 26 * 2);
        assert_eq!(node.stack, 27);
        assert!(!fits(236, node.widest));
        assert!(fits(236, 20) && !fits(236, 21));
        assert!(fits(229, node.stack));
    }

    /// TV Timing Tower 0.7.2 as composed: 24 rows of 403 nodes (443 slots), 10,907 slots and the
    /// cap's 8 a host scene, twice per board. The race HUD holds one board; HUD customisation, whose
    /// abort's tree held 66,306 slots, holds three.
    #[test]
    fn the_reported_tower_fits_the_race_and_not_hud_customisation() {
        let row = Cost {
            nodes: 403,
            slots: 443,
            stack: 61,
            widest: 59,
        };
        let rows = wide(24, row);
        assert_eq!((rows.nodes, rows.slots, rows.stack), (9673, 10636, 85));
        let live = Cost {
            nodes: 9827,
            slots: 10806,
            stack: 88,
            widest: 59,
        };
        let preview = Cost {
            nodes: 97,
            slots: 101,
            stack: 14,
            widest: 5,
        };
        let out = lines("pablo.tv-timing-tower", "tvtt", live, preview);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(
            out[0].contains("10907 walk slots, 101 in the editor copy"),
            "{out:?}"
        );
        let board = Board {
            host: 10915,
            records: 1,
        };
        assert_eq!(board.editor(), 66306);
        assert!(!board.over() && board.editor_over());
        let out = board_lines("standings", board);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(
            out[0].contains("2 times (10915 walk slots each, 21830 in all)"),
            "{out:?}"
        );
        assert!(
            out[1].starts_with("warning: screen standings: HUD customisation"),
            "{out:?}"
        );
        assert_eq!(board_lines("replay", board).len(), 1);
    }

    const CAP: usize = 8;
    const S10: usize = 2291;
    const S11: usize = 2292;
    const TVTT: usize = 9510;

    /// SWOFTY 2010 1.0.1, SWOFTY 2011 1.0.4 and TV Timing Tower 0.26.1 on the standings, with the
    /// walk slots their build lines list: each fits on its own, the board alone fits a tree, and the
    /// race-start abort's tree held 73,903 slots. Turning any one of them off brings it back inside.
    #[test]
    fn three_towers_on_one_board_take_the_race_tree_past_the_limit() {
        for own in [S10, S11, TVTT] {
            let alone = Board {
                host: CAP + own,
                records: 1,
            };
            assert!(!alone.over(), "{alone:?}");
        }
        let board = Board {
            host: CAP + S10 + S11 + TVTT,
            records: 3,
        };
        assert_eq!((board.instances(), board.slots()), (4, 56404));
        assert!(board.slots() < SLOT_LIMIT);
        assert_eq!(board.tree(), 73904);
        assert!(board.over());
        for own in [S10, S11, TVTT] {
            assert!(!board.without(own, 1).over(), "{own}");
        }
        assert!(board_lines("standings", board)[0].contains("4 times (14101 walk slots each, 56404 in all)"));
    }

    #[test]
    fn the_boards_that_ran_in_the_field_fit() {
        let tower = Board {
            host: CAP + TVTT,
            records: 1,
        };
        assert_eq!((tower.slots(), tower.editor()), (19036, 57924));
        assert!(!tower.over() && !tower.editor_over());
        let seasons = Board {
            host: CAP + S10 + S11,
            records: 2,
        };
        assert_eq!(seasons.slots(), 13773);
        assert!(!seasons.over() && !seasons.editor_over());
    }

    #[test]
    fn a_season_tower_beside_the_tv_tower_fits_the_race_and_warns_for_hud_customisation() {
        let one = Board {
            host: CAP + S10 + TVTT,
            records: 2,
        };
        assert_eq!(one.slots(), 35427);
        assert!(!one.over() && one.editor_over());
    }

    /// The three roots of the race-start abort as siblings: their stacks of 62, 62 and 129 do not add
    /// up to 253, the last is walked with one entry waiting.
    #[test]
    fn sibling_records_keep_their_own_walk_stack() {
        let root = |stack| Cost {
            nodes: 1,
            slots: 1,
            stack,
            widest: 0,
        };
        assert_eq!(Cost::of(&[root(62), root(62), root(129)]).stack, 130);
        assert_eq!(Cost::of(&[root(129), root(62), root(62)]).stack, 132);
    }

    #[test]
    fn a_record_past_the_walk_stack_gets_a_warning_that_names_it() {
        let deep = wide(200, wide(60, LEAF));
        assert!(deep.stack > STACK_LIMIT);
        let out = lines("a.b", "ab", deep, LEAF);
        assert_eq!(out.len(), 2);
        assert!(out[1].starts_with("warning: ab: a.b is past"), "{out:?}");
        let big = wide(100, wide(100, wide(4, LEAF)));
        assert!(big.stack <= STACK_LIMIT);
        assert_eq!(lines("a.b", "ab", big, LEAF).len(), 1);
        assert!(
            Board {
                host: big.slots,
                records: 1
            }
            .over()
        );
    }
}
