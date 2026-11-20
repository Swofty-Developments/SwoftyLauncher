//! Exporting DB Editor addons: as the game files their stacked edits make, with a readme, or as a zip of one
//! addon.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use swofty_formats::gvdb::Gvdb;

use super::{GameExport, Package, README};
use crate::dbedit::Catalog;
use crate::install::Game;
use crate::store::Store;
use crate::{Error, Result, io_err};

/// Applies the addons `ids`, in order, to the stock game database and
/// writes it to a zip at its game-folder path, with a `README.txt`.
pub fn export_game_files(store: &Store, game: &Game, ids: &[String], out: &Path) -> Result<GameExport> {
    let packages = ids
        .iter()
        .map(|id| Package::open(store, id))
        .collect::<Result<Vec<_>>>()?;
    export_packages(store, game, &packages, out)
}

/// [`export_game_files`] of addons in memory, such as [`Package::unsaved`].
pub fn export_packages(store: &Store, game: &Game, packages: &[Package], out: &Path) -> Result<GameExport> {
    if packages.is_empty() {
        return Err(Error::Other("pick at least one addon to export".into()));
    }
    let src = crate::dbedit::stock_database(store, game)?;
    let stock = std::fs::read(&src).map_err(io_err(&src))?;
    let mut db = Gvdb::parse(&stock)?;
    let keys: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let fields = std::collections::BTreeMap::new();
    let names = crate::game_db::row_keys::Names {
        fields: &fields,
        keys: crate::game_db::row_keys::RowKeys::builtin(),
        stock: Some(&keys),
        tracks: None,
    };
    let mut export = GameExport::default();
    let mut left_out = 0;
    for p in packages {
        let ops = p.edits.to_ops();
        let mut report = crate::ops::DbReport::default();
        let (_, failed) = crate::ops::apply_db_ops_lenient(&mut db, &ops, &names, &mut report);
        left_out += failed.len() + report.stock_removals;
        export.changes += ops.ops.len() - failed.len() - report.stock_removals;
        let name = &p.manifest.name;
        export.skipped.extend(
            failed
                .into_iter()
                .map(|(_, e)| format!("{name}: change skipped: {e}")),
        );
        if report.stock_removals > 0 {
            export
                .skipped
                .push(crate::planner::stock_removals_line(name, report.stock_removals));
        }
    }
    if let Ok(cat) = Catalog::open(store, game) {
        for t in crate::dbedit::rows::trim_row_counts(&cat, &mut db) {
            let by = packages.iter().rev().find(|p| p.edits.get(t.key).is_some());
            export
                .skipped
                .push(t.line(by.map_or("database", |p| p.manifest.name.as_str())));
            left_out += 1;
        }
    }
    for c in crate::dbedit::calendar::cut_rounds(&db) {
        let by = packages.iter().rev().find(|p| p.edits.get(c.key).is_some());
        export
            .warnings
            .push(c.line(by.map_or("database", |p| p.manifest.name.as_str())));
    }
    let picks: Vec<(String, Vec<String>)> = packages
        .iter()
        .map(|p| {
            (
                p.manifest.name.clone(),
                crate::dbedit::driver_manager::picks(&p.edits),
            )
        })
        .collect();
    if picks.iter().any(|(_, p)| !p.is_empty()) {
        let names = crate::dbedit::driver_manager::names(store);
        export.warnings.extend(crate::dbedit::driver_manager::warnings(
            &db,
            &picks,
            &names,
            crate::dbedit::driver_manager::Why::GameFiles,
        ));
    }
    let bytes = db.to_bytes();
    if bytes == stock {
        return Err(Error::Other(
            "these addons change nothing in this game's database".into(),
        ));
    }
    let build = crate::game_folder::baseline::sha256_bytes(&stock)[..16].to_owned();
    let readme = readme(packages, game, &build, left_out);
    let tmp = out.with_extension(format!("zip.{}.tmp", std::process::id()));
    let written = write_game_zip(&tmp, &bytes, &readme).and_then(|()| {
        std::fs::rename(&tmp, out).map_err(io_err(out))?;
        std::fs::metadata(out).map_err(io_err(out))
    });
    if written.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    export.bytes = written?.len();
    Ok(export)
}

pub(super) fn write_game_zip(path: &Path, db: &[u8], readme: &str) -> Result<()> {
    let file = std::fs::File::create(path).map_err(io_err(path))?;
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [(README, readme.as_bytes()), (crate::build::GVDB, db)] {
        z.start_file(name, opts)?;
        z.write_all(data).map_err(io_err(path))?;
    }
    z.finish()?.sync_all().map_err(io_err(path))
}

pub(super) fn readme(packages: &[Package], game: &Game, build: &str, skipped: usize) -> String {
    let mut out = String::from("F1 25 game database with DB Editor addons\n\n");
    out += &format!(
        "This zip holds F1 25's game database, {}, with these Swofty Launcher DB Editor addons applied in this order. Where two change the same value, the later one wins.\n\n",
        crate::build::GVDB
    );
    for p in packages {
        let m = &p.manifest;
        out += &format!(
            "- {} {} by {} ({} change(s))\n",
            m.name,
            m.version,
            m.author,
            p.edits.len()
        );
    }
    out += &format!(
        "\nMade for F1 25 game build {}, stock database {build}.\n",
        game.identity.key()
    );
    if skipped > 0 {
        out += &format!("{skipped} change(s) did not apply to this database and are left out.\n");
    }
    out += "\nInstall\n\
        1. Close F1 25.\n\
        2. In Steam, right-click F1 25 > Manage > Browse local files.\n\
        3. Extract this zip into that folder and replace the file when asked.\n\n\
        Undo\n\
        In Steam, right-click F1 25 > Properties > Installed Files > Verify integrity of game files.\n\n\
        Swofty Launcher\n\
        Swofty Launcher refuses modded launches while this file is replaced.\n\
        With Swofty Launcher, install the addons' .zip files on its Addons page instead.\n";
    out
}
