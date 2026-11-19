use super::*;

pub(crate) type FixtureContainer<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>);

pub(crate) struct Gen<'a> {
    pub n: u64,
    pub event: &'a str,
    pub session: Option<&'a str>,
    pub containers: Vec<FixtureContainer<'a>>,
}

pub(crate) fn fnv(name: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub(crate) fn write(p: &Plain, g: &Gen) {
    let gd = p.generation_dir(g.n);
    std::fs::create_dir_all(gd.join("containers")).unwrap();
    let containers: Vec<serde_json::Value> = g
        .containers
        .iter()
        .map(|(name, source, slots)| {
            let bytes = format!("bag {name} {}", g.n);
            std::fs::write(gd.join("containers").join(format!("{}.bag", fnv(name))), &bytes).unwrap();
            serde_json::json!({
                "name": name, "fnv64": fnv(name), "type_id": 12, "bytes": bytes.len(), "sha256": "x",
                "registered": false, "source": source,
                "slots": slots.iter().enumerate().map(|(i, (guid, who))| serde_json::json!({
                    "slot": i, "guid": guid, "name": who, "season": 2, "round": 5, "team": 7, "driver": 61, "last_played": 100 + i
                })).collect::<Vec<_>>()
            })
        })
        .collect();
    let manifest = serde_json::json!({
        "schema": 1, "generation": g.n, "event": g.event, "written_at": 1000 + g.n, "core_version": "1.10.0",
        "exe_sha256": "e", "profile": "default", "session": g.session, "loaded_addons": [],
        "containers": containers
    });
    std::fs::write(gd.join("manifest.json"), manifest.to_string()).unwrap();
}

pub(crate) fn current(p: &Plain, n: u64) {
    std::fs::create_dir_all(&p.root).unwrap();
    std::fs::write(p.root.join(CURRENT), n.to_string()).unwrap();
}

pub(crate) fn pair<'a>(
    mode: &str,
    source: &'a str,
    slots: Vec<(&'a str, &'a str)>,
) -> Vec<FixtureContainer<'a>> {
    let leak = |s: String| -> &'a str { Box::leak(s.into_boxed_str()) };
    vec![
        (leak(format!("gm_{mode}")), source, slots),
        (
            leak(format!("{mode}_SaveSlotGameModeComponent")),
            source,
            Vec::new(),
        ),
    ]
}

pub(crate) fn career_gen<'a>(
    n: u64,
    event: &'a str,
    session: Option<&'a str>,
    careers: &[(&'a str, &'a str)],
) -> Gen<'a> {
    let mut containers = pair("driver_career", "game", vec![("0x01", "Stock")]);
    containers.extend(pair("driver_career_progressive", "kept", careers.to_vec()));
    Gen {
        n,
        event,
        session,
        containers,
    }
}
