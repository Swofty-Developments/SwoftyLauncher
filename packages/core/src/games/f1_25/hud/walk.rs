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
//! three towers on one board, when every record's root sat in one shared host scene instanced once
//! for the stock board and once per record. Each screen record now has a host scene of its own (the
//! cap with only that record's root and editor copy), so a board holds the bare cap once and each
//! record's own host once ([`Board`]): its cost grows with the records' sum, not with the records'
//! count times their sum. Stacks do not add up: a record's root is walked with only its later
//! siblings waiting, so each record keeps its own.

use swofty_formats::pkg::Node;

/// Slots of one flattened tree (`u16` child indices).
pub const SLOT_LIMIT: usize = 0x1_0000;
/// Entries of the walk's stack.
pub const STACK_LIMIT: usize = 0x100;
/// Slots of the race HUD's tree beside the standings board, rounded up: the race-start abort's tree
/// held 73,903 slots around a board of four instances of one 14,101-slot shared host scene.
pub const RACE_REST: usize = 17_500;
/// Boards in the HUD customisation tree, and its slots beside them: its abort's tree held 66,306
/// slots, three boards of two 10,915-slot host scene instances and 816 more.
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

/// One screen's board: the stock instance of the bare cap (`cap` slots) and one instance of each of
/// the `records` screen records' own host scenes, `hosts` slots between them (each host holds the cap
/// and its record's root and editor copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    pub cap: usize,
    pub hosts: usize,
    pub records: usize,
}

impl Board {
    pub fn instances(&self) -> usize {
        1 + self.records
    }

    pub fn slots(&self) -> usize {
        self.cap + self.hosts
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

    /// The board without one addon's records: `slots` of their host scenes and `here` of the records
    /// on the screen.
    pub fn without(&self, slots: usize, here: usize) -> Board {
        Board {
            cap: self.cap,
            hosts: self.hosts.saturating_sub(slots),
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
        "{pfx}: {addon} adds {} nodes to its own host scene ({} walk slots, {} in the editor copy; walk stack {stack} of {STACK_LIMIT})",
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

/// The build line for the board of `screen`; the standings line also counts HUD customisation's tree.
pub fn board_line(screen: &str, b: Board) -> String {
    let mut line = format!(
        "screen {screen}: the board holds the bare cap ({} walk slots) and {} record host scene{} ({} walk slots), {} in all; the screen's UI tree about {} of the {SLOT_LIMIT} slots it can index",
        b.cap,
        b.records,
        if b.records == 1 { "" } else { "s" },
        b.hosts,
        b.slots(),
        b.tree()
    );
    if screen == "standings" {
        line.push_str(&format!(
            "; HUD customisation's tree about {} ({EDITOR_BOARDS} boards)",
            b.editor()
        ));
    }
    line
}

/// HUD customisation's tree past the slot limit: the addons with a record on the standings, and
/// the ones whose removal alone brings the tree back inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorOver {
    pub addons: Vec<String>,
    pub off: Vec<String>,
    pub board: Board,
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

    const CAP: usize = 8;
    const S10: usize = 2291;
    const S11: usize = 2292;
    const TVTT: usize = 9510;

    /// The board of records whose roots and editor copies take `own` walk slots each, every one in a
    /// host scene of its own beside a copy of the cap.
    fn board(own: &[usize]) -> Board {
        Board {
            cap: CAP,
            hosts: own.iter().map(|o| CAP + o).sum(),
            records: own.len(),
        }
    }

    /// The same board when every record sat in one shared host scene, instanced once for the stock
    /// board and once per record.
    fn shared(own: &[usize]) -> usize {
        (1 + own.len()) * (CAP + own.iter().sum::<usize>())
    }

    /// TV Timing Tower 0.7.2 as composed: 24 rows of 403 nodes (443 slots), 10,907 slots in its host
    /// scene beside the cap's 8. Instanced twice per board, the shared host took HUD customisation's
    /// three boards to 66,306 slots, where its abort's tree was; its own host keeps it inside.
    #[test]
    fn the_reported_tower_fits_the_race_and_hud_customisation() {
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
            out[0].contains("own host scene (10907 walk slots, 101 in the editor copy"),
            "{out:?}"
        );
        assert_eq!(EDITOR_BOARDS * shared(&[10907]) + EDITOR_REST, 66306);
        let b = board(&[10907]);
        assert_eq!((b.slots(), b.editor()), (10923, 33585));
        assert!(!b.over() && !b.editor_over());
        let line = board_line("standings", b);
        assert!(
            line.contains(
                "the bare cap (8 walk slots) and 1 record host scene (10915 walk slots), 10923 in all"
            ),
            "{line}"
        );
        assert!(
            line.ends_with("; HUD customisation's tree about 33585 (3 boards)"),
            "{line}"
        );
        assert!(!board_line("replay", b).contains("HUD customisation"));
    }

    /// SWOFTY 2010 1.0.1, SWOFTY 2011 1.0.4 and TV Timing Tower 0.26.1 on the standings, with the
    /// walk slots their build lines list. In one shared host scene they took the race-start tree to
    /// 73,904 slots (the field's abort held 73,903); each in its own host, the board costs their sum
    /// and the race and HUD customisation both fit.
    #[test]
    fn three_towers_on_one_board_fit_with_a_host_scene_each() {
        let own = [S10, S11, TVTT];
        assert_eq!(shared(&own) + RACE_REST, 73904);
        let b = board(&own);
        assert_eq!(
            (b.instances(), b.hosts, b.slots(), b.tree(), b.editor()),
            (4, 14117, 14125, 31625, 43191)
        );
        assert!(!b.over() && !b.editor_over());
        assert!(
            board_line("standings", b).contains(
                "the bare cap (8 walk slots) and 3 record host scenes (14117 walk slots), 14125 in all"
            ),
            "{}",
            board_line("standings", b)
        );
    }

    #[test]
    fn the_season_towers_cost_their_sum() {
        let seasons = board(&[S10, S11]);
        assert_eq!(shared(&[S10, S11]), 13773);
        assert_eq!((seasons.slots(), seasons.tree()), (4607, 22107));
        assert!(!seasons.over() && !seasons.editor_over());
        let third = board(&[S10, S11, 2708]);
        assert_eq!(EDITOR_BOARDS * shared(&[S10, S11, 2708]) + EDITOR_REST, 88404);
        assert_eq!(third.editor(), 22785);
        assert!(!third.editor_over());
    }

    /// Six towers the size of TV Timing Tower's: their own hosts alone take the race tree past the
    /// limit, and turning any one of them off brings it back inside.
    #[test]
    fn a_board_whose_own_hosts_are_too_big_is_still_over() {
        let own = [TVTT; 6];
        let b = board(&own);
        assert_eq!((b.slots(), b.tree()), (57116, 74616));
        assert!(b.over() && b.editor_over());
        let five = b.without(CAP + TVTT, 1);
        assert_eq!(five, board(&[TVTT; 5]));
        assert!(!five.over() && five.editor_over());
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
        assert!(board(&[big.slots]).over());
    }
}
