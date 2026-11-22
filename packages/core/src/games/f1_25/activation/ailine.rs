use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::bxml::{Document, Element, Kind};

// The stock values of every track's ai_track_config.space_lines generation settings.
const ABSOLUTE_SPACE: f32 = 2.2;
const CENTRE_SHARE: f32 = 0.35;
const SIDE_SHARE: f32 = 0.15;
const MIXED: u8 = 4;
const WAYPOINT_TYPES: [&str; 9] = [
    "left_racing_limit",
    "right_racing_limit",
    "left_track_limit",
    "right_track_limit",
    "racing_line",
    "left_white_line",
    "right_white_line",
    "left_run_off",
    "right_run_off",
];
const ROUTE_KINDS: [&str; 2] = ["track", "pit"];
// The lines the game's AISplineData loader reads after its routes, in its order: the section, the element, how a
// message names one, and the lowest gate it may stand at. Every gate reference has to be below the gate count.
const LINES: [(&str, &str, &str, i64); 4] = [
    ("brake_lines", "brake_line", "brake line", 0),
    ("corner_start_lines", "corner_start_line", "corner start", 1),
    ("corner_end_lines", "corner_end_line", "corner end", 0),
    ("retire_lines", "retire_line", "retire line", 0),
];
const RETIRE_KEYS: [&str; 4] = ["x", "y", "z", "side"];

#[derive(Debug, Clone, PartialEq)]
pub enum Spacing {
    Fine,
    Fixed { doc: Document, routes: Vec<String> },
    Unfixable(Vec<String>),
}

struct Route {
    kind: String,
    gates: Vec<u32>,
}

struct Line {
    widths: BTreeMap<u32, (f32, f32)>,
    routes: Vec<Route>,
}

fn below(a: f32, b: f32) -> bool {
    !matches!(a.partial_cmp(&b), Some(Ordering::Greater | Ordering::Equal))
}

fn space(left: f32, right: f32) -> u8 {
    let (l, r) = (left.abs(), right.abs());
    if !below(ABSOLUTE_SPACE, l) && r > ABSOLUTE_SPACE {
        return 0;
    }
    let sum = l + r;
    let fl = l / sum;
    if SIDE_SHARE > fl {
        return 0;
    }
    if !below(ABSOLUTE_SPACE, r) && l > ABSOLUTE_SPACE {
        return 2;
    }
    let fr = r / sum;
    if SIDE_SHARE > fr {
        return 2;
    }
    if below(fr, CENTRE_SHARE) || below(fl, CENTRE_SHARE) {
        MIXED
    } else {
        1
    }
}

fn num<T: std::str::FromStr>(e: &Element, key: &[u8]) -> Option<T> {
    std::str::from_utf8(e.attr(key)?).ok()?.trim().parse().ok()
}

fn lengths(gate: &Element) -> BTreeMap<u32, f32> {
    gate.child(b"waypoints")
        .into_iter()
        .flat_map(|w| w.children_named(b"waypoint"))
        .filter_map(|w| Some((num(w, b"id")?, num(w, b"length")?)))
        .collect()
}

fn widths(gate: &Element) -> Option<(f32, f32)> {
    let l = lengths(gate);
    let left = l.get(&5).or_else(|| l.get(&2))?;
    let right = l.get(&6).or_else(|| l.get(&3))?;
    Some((*left, *right))
}

fn read(doc: &Document) -> Option<Line> {
    let root = &doc.root;
    let mut widths_of = BTreeMap::new();
    for g in root.child(b"gates")?.children_named(b"gate") {
        widths_of.insert(num(g, b"id")?, widths(g)?);
    }
    let mut links = BTreeMap::new();
    for l in root.child(b"links")?.children_named(b"link") {
        links.insert(
            num::<u32>(l, b"id")?,
            (num::<u32>(l, b"from_gate")?, num::<u32>(l, b"to_gate")?),
        );
    }
    let mut routes = Vec::new();
    for r in root.child(b"routes")?.children_named(b"route") {
        let (from, to) = (num::<u32>(r, b"from_link")?, num::<u32>(r, b"to_link")?);
        let mut gates = vec![links.get(&from)?.0];
        for id in from..=to {
            gates.push(links.get(&id)?.1);
        }
        if gates.iter().any(|g| !widths_of.contains_key(g)) {
            return None;
        }
        let kind = String::from_utf8_lossy(r.attr(b"type").unwrap_or(b"route")).into_owned();
        routes.push(Route { kind, gates });
    }
    Some(Line {
        widths: widths_of,
        routes,
    })
}

fn space_gates(line: &Line) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for route in &line.routes {
        let types: Vec<u8> = route
            .gates
            .iter()
            .map(|g| {
                let (l, r) = line.widths[g];
                space(l, r)
            })
            .collect();
        let Some(&first) = types.first() else {
            continue;
        };
        let (mut cur, mut last) = (first, 0);
        for (u, &t) in types.iter().enumerate().skip(1) {
            if t == cur {
                last = u;
            } else if t != MIXED {
                if cur != MIXED {
                    out.insert(route.gates[last]);
                }
                out.insert(route.gates[u]);
                cur = t;
                last = u;
            }
        }
    }
    out
}

fn unspaced(line: &Line) -> Vec<usize> {
    let with = space_gates(line);
    (0..line.routes.len())
        .filter(|&i| line.routes[i].gates.iter().all(|g| !with.contains(g)))
        .collect()
}

fn changed(left: f32, right: f32, kind: u8) -> (f32, f32) {
    let (l, r) = (left.abs(), right.abs());
    let sign = |old: f32, fallback: f32| if old == 0.0 { fallback } else { old.signum() };
    let (sl, sr) = (sign(left, -1.0), sign(right, 1.0));
    let (wide, narrow) = (l.max(r), l.min(r));
    let new = match kind {
        MIXED | 1 if wide > ABSOLUTE_SPACE => narrow.min(ABSOLUTE_SPACE) * 0.9,
        MIXED | 1 => wide * SIDE_SHARE * 0.5,
        _ => wide,
    };
    match r <= l {
        true => (left, sr * new),
        false => (sl * new, right),
    }
}

fn gate_mut(doc: &mut Document, id: u32) -> Option<&mut Element> {
    doc.root
        .child_mut(b"gates")?
        .children
        .iter_mut()
        .find(|g| g.kind == Kind::Element && g.name == b"gate" && num::<u32>(g, b"id") == Some(id))
}

fn set_widths(gate: &mut Element, left: f32, right: f32) -> Option<()> {
    let waypoints = gate.child_mut(b"waypoints")?;
    let have = waypoints.children_named(b"waypoint").count();
    let template = waypoints
        .children
        .iter()
        .find(|w| w.kind == Kind::Element && w.name == b"waypoint")?
        .clone();
    if have < 5 {
        return None;
    }
    if have < 7 {
        for (id, kind) in WAYPOINT_TYPES.iter().enumerate().take(7).skip(have) {
            let mut w = template.clone();
            w.children.clear();
            w.set_attr(b"id", id.to_string().as_bytes());
            w.set_attr(b"type", kind.as_bytes());
            w.set_attr(b"length", b"0");
            waypoints.children.push(w);
        }
        waypoints.set_attr(b"num_waypoints", b"7");
    }
    for (id, value) in [(5u32, left), (6, right)] {
        let w = waypoints
            .children
            .iter_mut()
            .find(|w| w.kind == Kind::Element && num::<u32>(w, b"id") == Some(id))?;
        w.set_attr(b"length", value.to_string().as_bytes());
    }
    Some(())
}

fn respace(doc: &mut Document, line: &mut Line, route: usize) -> Option<()> {
    let mut uses = BTreeMap::<u32, usize>::new();
    for r in &line.routes {
        for g in r.gates.iter().collect::<BTreeSet<_>>() {
            *uses.entry(*g).or_default() += 1;
        }
    }
    let gates = line.routes[route].gates.clone();
    let types: Vec<u8> = gates
        .iter()
        .map(|g| {
            let (l, r) = line.widths[g];
            space(l, r)
        })
        .collect();
    let run = types.iter().copied().find(|&t| t != MIXED).unwrap_or(MIXED);
    let own = |k: usize| uses[&gates[k]] == 1;
    let k = (1..gates.len().saturating_sub(1))
        .find(|&k| own(k - 1) && own(k) && own(k + 1) && types[k - 1] != MIXED && types[k + 1] != MIXED)
        .or_else(|| (1..gates.len().saturating_sub(1)).find(|&k| own(k - 1) && own(k) && own(k + 1)))?;
    let (l, r) = line.widths[&gates[k]];
    let (nl, nr) = changed(l, r, run);
    let kind = space(nl, nr);
    if kind == MIXED || kind == run {
        return None;
    }
    set_widths(gate_mut(doc, gates[k])?, nl, nr)?;
    line.widths.insert(gates[k], (nl, nr));
    Some(())
}

pub fn give_routes_space(data: &[u8]) -> Spacing {
    let Ok(mut doc) = Document::parse(data) else {
        return Spacing::Fine;
    };
    let Some(mut line) = read(&doc) else {
        return Spacing::Fine;
    };
    let broken = unspaced(&line);
    if broken.is_empty() {
        return Spacing::Fine;
    }
    let names: Vec<String> = broken.iter().map(|&i| line.routes[i].kind.clone()).collect();
    for &i in &broken {
        if respace(&mut doc, &mut line, i).is_none() {
            return Spacing::Unfixable(names);
        }
    }
    let Some(again) = read(&doc) else {
        return Spacing::Unfixable(names);
    };
    match unspaced(&again).is_empty() {
        true => Spacing::Fixed { doc, routes: names },
        false => Spacing::Unfixable(names),
    }
}

fn route_words(routes: &[String]) -> String {
    let mut words: Vec<&str> = routes
        .iter()
        .map(|r| match r.as_str() {
            "pit" => "pit lane",
            _ => "racing line",
        })
        .collect();
    words.dedup();
    words.join(" and ")
}

pub fn fixed_note(who: &str, routes: &[String]) -> String {
    format!(
        "fixed the AI {} of {who} so F1 25 doesn't crash when an AI car drives it",
        route_words(routes)
    )
}

pub fn unfixable_message(who: &str, routes: &[String], fix: &str) -> String {
    format!(
        "{who} would crash F1 25 when an AI car drives its {}: the AI racing line has no change of passing \
         space there, and the launcher can't find a gate to change. {fix}",
        route_words(routes)
    )
}

fn label(e: &Element, kind: &str) -> String {
    match e.attr(b"id") {
        Some(id) => format!("{kind} {}", String::from_utf8_lossy(id)),
        None => format!("a {kind} without an id"),
    }
}

fn needs_id(e: &Element, kind: &str, bad: &mut Vec<String>) {
    if e.attr(b"id").is_none() {
        bad.push(format!("a {kind} has no id"));
    }
}

fn gate_in(e: &Element, what: &str, key: &str, low: i64, gates: usize, bad: &mut Vec<String>) {
    if e.attr(key.as_bytes()).is_none() {
        bad.push(format!("{what} has no {key}"));
        return;
    }
    let Some(gate) = num::<i64>(e, key.as_bytes()) else {
        return;
    };
    let high = gates as i64 - 1;
    if (low..=high).contains(&gate) {
        return;
    }
    bad.push(match (key, low, gate) {
        ("gate_id", 1, 0) => {
            format!("{what} starts at gate 0, the first gate of the lap, where no corner can start")
        }
        _ if high < low => format!("{what} has {key} {gate}, and the track has {gates} gates"),
        _ => format!("{what} has {key} {gate}, outside gates {low} to {high}"),
    });
}

fn section<'a>(root: &'a Element, name: &str, bad: &mut Vec<String>) -> Option<&'a Element> {
    let found = root.child(name.as_bytes());
    if found.is_none() {
        bad.push(format!("it has no <{name}> list"));
    }
    found
}

pub fn loadable(doc: &Document) -> Result<(), Vec<String>> {
    let root = &doc.root;
    let mut bad = Vec::new();
    if root.name != b"track" {
        bad.push(format!(
            "its top element is <{}>, not <track>",
            String::from_utf8_lossy(&root.name)
        ));
    }
    let gates = section(root, "gates", &mut bad).map_or(0, |g| g.children_named(b"gate").count());
    let mut links = Vec::new();
    for link in section(root, "links", &mut bad)
        .into_iter()
        .flat_map(|l| l.children_named(b"link"))
    {
        needs_id(link, "link", &mut bad);
        links.extend(num::<i64>(link, b"id"));
        let what = label(link, "link");
        for key in ["from_gate", "to_gate"] {
            gate_in(link, &what, key, 0, gates, &mut bad);
        }
    }
    for route in section(root, "routes", &mut bad)
        .into_iter()
        .flat_map(|r| r.children_named(b"route"))
    {
        let what = label(route, "route");
        for key in ["type", "from_route", "to_route"] {
            match route.attr(key.as_bytes()) {
                None => bad.push(format!("{what} has no {key}")),
                Some(v) if !ROUTE_KINDS.iter().any(|k| k.as_bytes() == v) => bad.push(format!(
                    "{what} has {key} \"{}\", where the game reads only track or pit",
                    String::from_utf8_lossy(v)
                )),
                Some(_) => {}
            }
        }
        for key in ["from_link", "to_link"] {
            if route.attr(key.as_bytes()).is_none() {
                bad.push(format!("{what} has no {key}"));
            }
        }
        if let (Some(from), Some(to)) = (num::<i64>(route, b"from_link"), num::<i64>(route, b"to_link"))
            && !links.iter().any(|id| (from..=to).contains(id))
        {
            bad.push(format!(
                "{what} runs over links {from} to {to}, and no link has an id in that range"
            ));
        }
    }
    for (name, element, kind, low) in LINES {
        for line in section(root, name, &mut bad)
            .into_iter()
            .flat_map(|s| s.children_named(element.as_bytes()))
        {
            needs_id(line, kind, &mut bad);
            let what = label(line, kind);
            gate_in(line, &what, "gate_id", low, gates, &mut bad);
            if element == "corner_start_line" && line.attr(b"pair_id").is_some() {
                gate_in(line, &what, "pair_id", low, gates, &mut bad);
            }
            if element == "retire_line" {
                for key in RETIRE_KEYS {
                    if line.attr(key.as_bytes()).is_none() {
                        bad.push(format!("{what} has no {key}"));
                    }
                }
            }
        }
    }
    match bad.is_empty() {
        true => Ok(()),
        false => Err(bad),
    }
}

pub fn unloadable_message(who: &str, defects: &[String], fix: &str) -> String {
    let more = match defects.len().saturating_sub(1) {
        0 => String::new(),
        1 => " (and 1 more problem)".to_owned(),
        n => format!(" (and {n} more problems)"),
    };
    format!(
        "{who} would crash F1 25 when a session on it starts: the game can't load its AI data, because {}{more}. \
         Ask the track's author to fix it. {fix}",
        defects.first().map_or("", String::as_str)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn el(name: &str, attrs: &[(&str, String)], children: Vec<Element>) -> Element {
        Element {
            kind: Kind::Element,
            pad: 0,
            name: name.as_bytes().to_vec(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
                .collect(),
            children,
            end_pad: 0,
        }
    }

    fn gate(id: u32, left: f32, right: f32, waypoints: usize) -> Element {
        let lengths = [
            left + 0.5,
            right - 0.5,
            left - 0.2,
            right + 0.2,
            0.0,
            left,
            right,
            left - 1.0,
            right + 1.0,
        ];
        let wps = (0..waypoints)
            .map(|i| {
                el(
                    "waypoint",
                    &[
                        ("id", i.to_string()),
                        ("type", WAYPOINT_TYPES[i].to_string()),
                        ("length", lengths[i].to_string()),
                    ],
                    Vec::new(),
                )
            })
            .collect();
        el(
            "gate",
            &[("id", id.to_string()), ("name", format!("ai_gate_track_{id:03}"))],
            vec![
                el(
                    "position",
                    &[("x", "0".into()), ("y", "0".into()), ("z", "0".into())],
                    Vec::new(),
                ),
                el("waypoints", &[("num_waypoints", waypoints.to_string())], wps),
            ],
        )
    }

    // A 40-gate lap and a 12-gate pit lane from gate 32 back onto gate 7; gates 5-9 and 30-34 are as
    // wide as the first pit gate.
    pub(crate) fn aispline(pit: impl Fn(u32) -> (f32, f32), waypoints: usize) -> Document {
        let mut gates = Vec::new();
        for id in 0..40u32 {
            let side = match id / 5 {
                1 | 6 => pit(0),
                b if b % 2 == 0 => (-6.0, 1.5),
                _ => (-1.5, 6.0),
            };
            gates.push(gate(id, side.0, side.1, 9));
        }
        for n in 0..12u32 {
            let (l, r) = pit(n);
            gates.push(gate(40 + n, l, r, waypoints));
        }
        let mut links = Vec::new();
        let mut link = |from: u32, to: u32| {
            let id = links.len();
            links.push(el(
                "link",
                &[
                    ("id", id.to_string()),
                    ("from_gate", from.to_string()),
                    ("to_gate", to.to_string()),
                ],
                Vec::new(),
            ));
        };
        link(39, 0);
        for g in 1..40 {
            link(g - 1, g);
        }
        link(32, 40);
        for g in 41..52 {
            link(g - 1, g);
        }
        link(51, 7);
        let routes = vec![
            el(
                "route",
                &[
                    ("id", "0".into()),
                    ("type", "track".into()),
                    ("from_route", "track".into()),
                    ("to_route", "track".into()),
                    ("from_link", "0".into()),
                    ("to_link", "39".into()),
                ],
                Vec::new(),
            ),
            el(
                "route",
                &[
                    ("id", "1".into()),
                    ("type", "pit".into()),
                    ("from_route", "track".into()),
                    ("to_route", "track".into()),
                    ("from_link", "40".into()),
                    ("to_link", "52".into()),
                ],
                Vec::new(),
            ),
        ];
        let lines = |section: &str, element: &str, at: &[&[(&str, &str)]]| {
            let items = at
                .iter()
                .enumerate()
                .map(|(id, attrs)| {
                    let mut attrs: Vec<(&str, String)> =
                        attrs.iter().map(|(k, v)| (*k, v.to_string())).collect();
                    attrs.insert(0, ("id", id.to_string()));
                    el(element, &attrs, Vec::new())
                })
                .collect();
            el(section, &[], items)
        };
        Document {
            root: el(
                "track",
                &[("name", "default".into())],
                vec![
                    el("gates", &[("num_gates", "52".into())], gates),
                    el("links", &[("num_links", "53".into())], links),
                    lines(
                        "corner_start_lines",
                        "corner_start_line",
                        &[&[("gate_id", "10")], &[("gate_id", "30"), ("pair_id", "44")]],
                    ),
                    lines(
                        "corner_end_lines",
                        "corner_end_line",
                        &[&[("gate_id", "14")], &[("gate_id", "34")]],
                    ),
                    lines(
                        "brake_lines",
                        "brake_line",
                        &[&[("gate_id", "8")], &[("gate_id", "28")]],
                    ),
                    lines(
                        "retire_lines",
                        "retire_line",
                        &[&[
                            ("gate_id", "20"),
                            ("x", "12.5"),
                            ("y", "0.4"),
                            ("z", "-80.25"),
                            ("side", "left"),
                        ]],
                    ),
                    el("routes", &[("num_routes", "2".into())], routes),
                ],
            ),
            tail: Vec::new(),
        }
    }

    fn line_mut<'a>(doc: &'a mut Document, section: &str, n: usize) -> &'a mut Element {
        let s = doc.root.child_mut(section.as_bytes()).unwrap();
        s.children
            .iter_mut()
            .filter(|c| c.kind == Kind::Element)
            .nth(n)
            .unwrap()
    }

    fn without(doc: &mut Document, section: &str, n: usize, key: &str) {
        line_mut(doc, section, n)
            .attrs
            .retain(|(k, _)| k != key.as_bytes());
    }

    #[test]
    fn a_line_like_the_stock_ones_loads() {
        let doc = aispline(|_| (-4.1, 3.0), 9);
        assert_eq!(loadable(&doc), Ok(()));
        assert_eq!(loadable(&Document::parse(&doc.to_bytes()).unwrap()), Ok(()));
    }

    #[test]
    fn a_corner_starting_at_gate_0_is_named() {
        let mut doc = aispline(|_| (-4.1, 3.0), 9);
        let start = line_mut(&mut doc, "corner_start_lines", 0);
        start.set_attr(b"id", b"12");
        start.set_attr(b"gate_id", b"0");
        let defects = loadable(&doc).unwrap_err();
        assert_eq!(
            defects,
            ["corner start 12 starts at gate 0, the first gate of the lap, where no corner can start"]
        );
        assert_eq!(
            unloadable_message(
                "Circuit de Ølstykke",
                &defects,
                "Turn Circuit de Ølstykke off in this profile."
            ),
            "Circuit de Ølstykke would crash F1 25 when a session on it starts: the game can't load its AI data, \
             because corner start 12 starts at gate 0, the first gate of the lap, where no corner can start. Ask the \
             track's author to fix it. Turn Circuit de Ølstykke off in this profile."
        );
        line_mut(&mut doc, "corner_start_lines", 0).set_attr(b"gate_id", b"52");
        assert_eq!(
            loadable(&doc).unwrap_err(),
            ["corner start 12 has gate_id 52, outside gates 1 to 51"]
        );
    }

    #[test]
    fn a_corner_pair_outside_the_gates_is_named() {
        let mut doc = aispline(|_| (-4.1, 3.0), 9);
        for (pair, want) in [
            ("0", "corner start 1 has pair_id 0, outside gates 1 to 51"),
            ("52", "corner start 1 has pair_id 52, outside gates 1 to 51"),
            ("-3", "corner start 1 has pair_id -3, outside gates 1 to 51"),
        ] {
            line_mut(&mut doc, "corner_start_lines", 1).set_attr(b"pair_id", pair.as_bytes());
            assert_eq!(loadable(&doc).unwrap_err(), [want], "{pair}");
        }
        line_mut(&mut doc, "corner_start_lines", 1).set_attr(b"pair_id", b"51");
        assert_eq!(loadable(&doc), Ok(()));
    }

    #[test]
    fn a_missing_list_is_named() {
        for section in [
            "gates",
            "links",
            "routes",
            "brake_lines",
            "corner_start_lines",
            "corner_end_lines",
            "retire_lines",
        ] {
            let mut doc = aispline(|_| (-4.1, 3.0), 9);
            doc.root.children.retain(|c| c.name != section.as_bytes());
            let defects = loadable(&doc).unwrap_err();
            assert_eq!(defects[0], format!("it has no <{section}> list"));
            if section != "gates" && section != "links" {
                assert_eq!(defects.len(), 1, "{section}: {defects:?}");
            }
        }
        let mut doc = aispline(|_| (-4.1, 3.0), 9);
        doc.root.name = b"spline".to_vec();
        assert_eq!(
            loadable(&doc).unwrap_err(),
            ["its top element is <spline>, not <track>"]
        );
    }

    #[test]
    fn every_loader_rule_names_the_line_that_breaks_it() {
        type Edit = fn(&mut Document);
        let cases: [(Edit, &str); 10] = [
            (
                |d| line_mut(d, "links", 5).set_attr(b"to_gate", b"60"),
                "link 5 has to_gate 60, outside gates 0 to 51",
            ),
            (
                |d| line_mut(d, "brake_lines", 1).set_attr(b"gate_id", b"-1"),
                "brake line 1 has gate_id -1, outside gates 0 to 51",
            ),
            (
                |d| without(d, "corner_end_lines", 0, "gate_id"),
                "corner end 0 has no gate_id",
            ),
            (
                |d| without(d, "retire_lines", 0, "side"),
                "retire line 0 has no side",
            ),
            (|d| without(d, "retire_lines", 0, "y"), "retire line 0 has no y"),
            (
                |d| line_mut(d, "routes", 1).set_attr(b"type", b"pitlane"),
                "route 1 has type \"pitlane\", where the game reads only track or pit",
            ),
            (|d| without(d, "routes", 0, "to_route"), "route 0 has no to_route"),
            (
                |d| without(d, "routes", 0, "from_link"),
                "route 0 has no from_link",
            ),
            (|d| without(d, "brake_lines", 0, "id"), "a brake line has no id"),
            (
                |d| {
                    let r = line_mut(d, "routes", 1);
                    r.set_attr(b"from_link", b"60");
                    r.set_attr(b"to_link", b"70");
                },
                "route 1 runs over links 60 to 70, and no link has an id in that range",
            ),
        ];
        for (edit, want) in cases {
            let mut doc = aispline(|_| (-4.1, 3.0), 9);
            edit(&mut doc);
            assert_eq!(loadable(&doc).unwrap_err(), [want]);
        }
        let mut doc = aispline(|_| (-4.1, 3.0), 9);
        line_mut(&mut doc, "links", 0).set_attr(b"from_gate", b"x");
        line_mut(&mut doc, "brake_lines", 0).set_attr(b"gate_id", b"60");
        line_mut(&mut doc, "corner_start_lines", 0).set_attr(b"gate_id", b"0");
        let defects = loadable(&doc).unwrap_err();
        assert_eq!(
            defects,
            [
                "brake line 0 has gate_id 60, outside gates 0 to 51",
                "corner start 0 starts at gate 0, the first gate of the lap, where no corner can start",
            ]
        );
        assert!(
            unloadable_message("Test Ring", &defects, "")
                .contains("because brake line 0 has gate_id 60, outside gates 0 to 51 (and 1 more problem).")
        );
    }

    #[test]
    fn space_types_follow_the_game() {
        assert_eq!(space(-4.1, 3.0), 1);
        assert_eq!(space(-1.5, 6.0), 0);
        assert_eq!(space(-6.0, 1.5), 2);
        assert_eq!(space(-6.0, 2.5), MIXED);
        assert_eq!(space(-2.0, 2.0), 1);
        assert_eq!(space(-2.0, 0.1), 2);
        assert_eq!(space(0.0, 0.0), MIXED);
    }

    #[test]
    fn a_line_with_space_changes_on_every_route_is_left_alone() {
        let doc = aispline(|n| if n < 6 { (-1.2, 6.0) } else { (-6.0, 1.2) }, 9);
        assert_eq!(give_routes_space(&doc.to_bytes()), Spacing::Fine);
        assert_eq!(give_routes_space(b"!pkg"), Spacing::Fine);
    }

    #[test]
    fn a_pit_lane_without_space_changes_gets_one_on_its_own_gates() {
        for (widths, waypoints) in [
            ((-4.1, 3.0), 7),
            ((-2.0, 1.8), 9),
            ((-1.5, 6.0), 7),
            ((-3.0, 3.0), 5),
        ] {
            let doc = aispline(|_| widths, waypoints);
            let before = read(&doc).unwrap();
            assert_eq!(unspaced(&before), [1], "{widths:?}");
            let Spacing::Fixed { doc: fixed, routes } = give_routes_space(&doc.to_bytes()) else {
                panic!("{widths:?} should be fixed");
            };
            assert_eq!(routes, ["pit"]);
            let bytes = fixed.to_bytes();
            assert_eq!(Document::parse(&bytes).unwrap(), fixed);
            let after = read(&fixed).unwrap();
            assert!(unspaced(&after).is_empty());
            let moved: Vec<u32> = after
                .widths
                .iter()
                .filter(|(g, w)| before.widths[g] != **w)
                .map(|(g, _)| *g)
                .collect();
            assert_eq!(moved, [41], "{widths:?}");
            let track: BTreeSet<u32> = after.routes[0].gates.iter().copied().collect();
            let spaced = space_gates(&after);
            assert_eq!(
                spaced.intersection(&track).collect::<Vec<_>>(),
                space_gates(&before).intersection(&track).collect::<Vec<_>>()
            );
            let gate = fixed
                .root
                .child(b"gates")
                .unwrap()
                .children_named(b"gate")
                .nth(41)
                .unwrap();
            assert_eq!(lengths(gate).len(), waypoints.max(7));
            assert_eq!(give_routes_space(&bytes), Spacing::Fine);
        }
    }

    #[test]
    fn a_pit_lane_without_gates_of_its_own_is_reported() {
        let mut doc = aispline(|_| (-4.1, 3.0), 9);
        doc.root.child_mut(b"links").unwrap().children[40].set_attr(b"to_gate", b"33");
        doc.root.child_mut(b"routes").unwrap().children[1].set_attr(b"to_link", b"40");
        assert_eq!(
            give_routes_space(&doc.to_bytes()),
            Spacing::Unfixable(vec!["pit".into()])
        );
    }
}
