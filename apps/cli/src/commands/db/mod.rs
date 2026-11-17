mod addon;
mod drivers;
mod print;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use clap::{Args, Subcommand};
use swofty_core::dbedit::package::{self, Package};
use swofty_core::dbedit::{Catalog, Edits, NodeKind, driver_manager, modules};
use swofty_core::manifest::Category;
use swofty_core::store::Store;

use self::addon::{edit, open};
use self::drivers::{driver_notes, driver_row};
use self::print::{print_deps, row_line, show};
use crate::commands::{Execute, Invocation};

#[derive(Args)]
pub(crate) struct DbArgs {
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) cmd: DbCommand,
}

impl Execute for DbArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, &invocation.profile, self.game, self.cmd)
    }
}

#[derive(Subcommand)]
pub enum DbCommand {
    #[command(
        about = "List DB Editor addons and whether each is on in the profile",
        alias = "packages"
    )]
    Addons,
    #[command(about = "Create an empty DB Editor addon")]
    New { name: String },
    #[command(about = "Delete a DB Editor addon from every profile")]
    Delete { id: String },
    #[command(about = "Write a DB Editor addon to a .zip to share it")]
    Export { id: String, file: PathBuf },
    #[command(
        about = "Add a DB Editor addon from a .zip, a DB Editor export, an EGO GameDatabases Editor export, or a modded gamedatabases.2025.bin or mod .zip holding one"
    )]
    Import { file: PathBuf },
    #[command(
        about = "Write the game database with DB Editor addons applied to a .zip players without the launcher extract into the game folder"
    )]
    ExportGame {
        file: PathBuf,
        #[arg(
            long = "addon",
            alias = "package",
            value_name = "ID",
            help = "An addon to apply, in order; repeat it (default: the DB Editor addons on in the profile, in load order)"
        )]
        addons: Vec<String>,
    },
    #[command(about = "Turn off every DB Editor addon that is on in the profile")]
    OffAll,
    #[command(about = "Show a DB Editor addon's details, or change them with the flags")]
    Details {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_parser = category, help = "One of seasons, tracks, cars, helmets, career, ai, performance, utility, uncategorised")]
        category: Option<CategoryArg>,
    },
    #[command(about = "Search names, tables, keys, values and English text")]
    Search {
        query: String,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    #[command(
        about = "List what a copy of a row does not bring along: rows of other tables labelled with the row's label or a name derived from it, and values that pick the row by label or stat_id"
    )]
    Deps {
        #[arg(help = "The row, as a dotted or selector path")]
        row: String,
    },
    #[command(about = "Show a key's value, or what a table, row or container holds")]
    Get {
        #[arg(help = "A dotted or selector path, or 0x and 16 hex digits")]
        path: String,
        #[arg(
            long,
            alias = "package",
            help = "Show values with this addon's changes (default: stock)"
        )]
        addon: Option<String>,
    },
    #[command(about = "Change a key's value in a DB Editor addon")]
    Set {
        path: String,
        value: String,
        #[arg(long, alias = "package")]
        addon: String,
    },
    #[command(
        about = "Add a key the game database lacks; TYPE is the record type number (0 int, 1 float, 2 string)"
    )]
    Add {
        path: String,
        #[arg(value_name = "TYPE")]
        tag: u8,
        value: String,
        #[arg(long, alias = "package")]
        addon: String,
    },
    #[command(
        about = "Add a row to a table for an entity that has none: a copy of another entity's row, or of the game default row with FROM default"
    )]
    CopyRow {
        #[arg(help = "The table's rows, such as driver_statistics.driver_statistics.DriverStatisticsHolder")]
        table: String,
        #[arg(help = "The entity whose row to copy, or default")]
        from: String,
        #[arg(help = "The entity the new row belongs to")]
        to: String,
        #[arg(
            long,
            help = "The field naming a row's entity (default: the one the DB Editor pages use, else _metadata_)"
        )]
        field: Option<String>,
        #[arg(long, alias = "package")]
        addon: String,
    },
    #[command(about = "Remove a row copy-row added, with its changes")]
    RemoveRow {
        table: String,
        to: String,
        #[arg(long)]
        field: Option<String>,
        #[arg(long, alias = "package")]
        addon: String,
    },
    #[command(about = "Put a key back to its stock value in a DB Editor addon, or every key with --all")]
    Revert {
        path: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long, alias = "package")]
        addon: String,
    },
    #[command(about = "List a DB Editor addon's database changes")]
    List {
        #[arg(long, alias = "package")]
        addon: String,
    },
}

/// The field naming a row's entity in `table`: `--field`, else the one a DB Editor page selects it by.
fn row_field(table: &str, field: Option<String>) -> String {
    field.unwrap_or_else(|| {
        modules()
            .iter()
            .flat_map(|m| m.row_specs())
            .find(|s| s.table == table)
            .map_or_else(|| "_metadata_".to_owned(), |s| s.field)
    })
}

#[derive(Debug, Clone, Copy)]
pub struct CategoryArg(Option<Category>);

fn category(s: &str) -> std::result::Result<CategoryArg, String> {
    if s == "uncategorised" {
        return Ok(CategoryArg(None));
    }
    match serde_json::from_value(serde_json::Value::String(s.to_owned())) {
        Ok(Category::Shaders) => Err("shaders is for shader packs, not DB Editor addons; see --help".into()),
        Ok(c) => Ok(CategoryArg(Some(c))),
        Err(_) => Err(format!("{s} is not a category; see --help")),
    }
}

pub fn command(store: &Store, profile: &str, game: Option<PathBuf>, cmd: DbCommand) -> Result<ExitCode> {
    let catalog = || -> Result<Arc<Catalog>> {
        let g = crate::commands::build::game(store, game.clone())?;
        Ok(Catalog::open(store, &g)?)
    };
    match cmd {
        DbCommand::Addons => {
            let p = store.profile(profile)?;
            let on: Vec<&str> = p.enabled().collect();
            let package::Scan {
                packages: all,
                damaged,
            } = package::scan(store)?;
            for (id, e) in &damaged {
                println!("bad  {id}  {e}");
            }
            for i in &all {
                let state = if on.contains(&i.id.as_str()) { "on " } else { "off" };
                println!(
                    "{state}  {}  {} {} by {}, {} change(s)",
                    i.id, i.name, i.version, i.author, i.changes
                );
            }
            println!(
                "{} DB Editor addon(s); on/off is for {profile}",
                all.len() + damaged.len()
            );
        }
        DbCommand::New { name } => {
            let p = Package::create(store, &name)?;
            println!(
                "created {} ({}); turn it on with swofty addons enable {}",
                p.id, p.manifest.name, p.id
            );
        }
        DbCommand::Delete { id } => {
            open(store, &id)?;
            Package::delete(store, &id)?;
            println!("deleted {id}");
        }
        DbCommand::Export { id, file } => {
            let p = open(store, &id)?;
            Package::export_zip(store, &id, &file)?;
            println!("wrote {} ({} change(s)) to {}", id, p.edits.len(), file.display());
        }
        DbCommand::Import { file } => {
            let got = package::import(store, &*catalog()?, &file)?;
            let (p, r) = (&got.package, &got.report);
            let counts = match r.format {
                "bin" => format!("{} change(s)", r.imported),
                _ => format!(
                    "{} imported, {} unchanged, {} skipped",
                    r.imported,
                    r.unchanged,
                    r.skipped.len()
                ),
            };
            println!(
                "{} ({}): {counts} into {} ({})",
                file.display(),
                r.format,
                p.id,
                p.name
            );
            if let Some(was) = &got.renamed_from {
                println!("  {was} is already installed; installed as {}", p.id);
            }
            let prefix = if r.format == "bin" { "" } else { "skipped " };
            for s in &r.skipped {
                println!("  {prefix}{s}");
            }
        }
        DbCommand::ExportGame { file, addons } => {
            let ids = match addons.is_empty() {
                true => package::enabled_packages(store, profile)?,
                false => addons,
            };
            if ids.is_empty() {
                bail!("no DB Editor addons are on in {profile}; pass --addon <id>");
            }
            for id in &ids {
                open(store, id)?;
            }
            let g = crate::commands::build::game(store, game.clone())?;
            let got = package::export_game_files(store, &g, &ids, &file)?;
            println!(
                "wrote {} ({} change(s) from {}, {} bytes)",
                file.display(),
                got.changes,
                ids.join(", "),
                got.bytes
            );
            for s in got.skipped.iter().chain(&got.warnings) {
                println!("  {s}");
            }
        }
        DbCommand::OffAll => {
            let n = package::turn_off_all(store, profile)?;
            println!("turned off {n} DB Editor addon(s) in {profile}");
        }
        DbCommand::Details {
            id,
            name,
            version,
            author,
            description,
            category,
        } => {
            let mut p = open(store, &id)?;
            if let Some(CategoryArg(c)) = category {
                p.set_category(c);
            }
            if name.is_some()
                || version.is_some()
                || author.is_some()
                || description.is_some()
                || category.is_some()
            {
                let i = p.info();
                p.set_details(
                    name.as_deref().unwrap_or(&i.name),
                    version.as_deref().unwrap_or(&i.version),
                    author.as_deref().unwrap_or(&i.author),
                    description.as_deref().unwrap_or(&i.description),
                )?;
                p.write(store)?;
            }
            let i = p.info();
            println!(
                "{}\n  name: {}\n  version: {}\n  author: {}",
                i.id, i.name, i.version, i.author
            );
            println!("  description: {}\n  changes: {}", i.description, i.changes);
            if let Some(c) = p.manifest.category {
                println!("  category: {}", c.label());
            }
            if !i.game_build.is_empty() {
                println!("  database: {}", i.game_build);
            }
        }
        DbCommand::Search { query, limit } => {
            let cat = catalog()?;
            for h in cat.search_with(&query, limit, &driver_manager::drivers(store, &cat)) {
                println!("{:<10} {}  {}", format!("{:?}", h.kind), h.title, h.detail);
            }
        }
        DbCommand::Deps { row } => {
            let cat = catalog()?;
            let node = cat
                .find(&row)
                .ok_or_else(|| anyhow!("{row}: not in the game database"))?;
            if !matches!(cat.node(node).kind, NodeKind::Row { .. }) {
                bail!("{row}: not a row");
            }
            print_deps(&cat, node);
        }
        DbCommand::Get { path, addon } => {
            let edits = match &addon {
                Some(id) => open(store, id)?.edits,
                None => Edits::default(),
            };
            let cat = catalog()?;
            match (cat.find(&path), edits.copied_path(&cat, &path)) {
                (Some(node), _) => show(&cat, &edits, node),
                (None, Some(c)) => {
                    let value = edits.value(&cat, c.key).unwrap_or_default();
                    println!("{} (0x{:016x})", c.row.path(&c.rel), c.key);
                    println!("  value: {value}");
                    println!("  {}", row_line(c.row));
                    if edits.get(c.key).is_some() {
                        println!(
                            "  copied: {}",
                            cat.record_by_key(c.source).map_or("", |r| r.value)
                        );
                    }
                }
                (None, None) => match driver_row(store, &cat, &path) {
                    Some(why) => bail!("{why}"),
                    None => bail!("{path}: not in the game database"),
                },
            }
        }
        DbCommand::Set { path, value, addon } => {
            let cat = catalog()?;
            edit(store, &cat, &addon, |p| {
                let key = p
                    .edits
                    .resolve(&cat, &path)
                    .map_err(|e| anyhow!(driver_row(store, &cat, &path).unwrap_or(e)))?;
                p.edits.set(&cat, key, &value).map_err(|e| anyhow!("{path}: {e}"))
            })?;
            println!("{path} = {value} in {addon}");
            for note in driver_notes(store, &cat, profile, &value) {
                println!("  note: {note}");
            }
        }
        DbCommand::CopyRow {
            table,
            from,
            to,
            field,
            addon,
        } => {
            let cat = catalog()?;
            let field = row_field(&table, field);
            let source = (from != "default").then_some(from.as_str());
            let line = edit(store, &cat, &addon, |p| {
                p.edits
                    .copy_row(&cat, &table, &field, source, &to)
                    .map_err(|e| anyhow!("{e}"))?;
                Ok(row_line(p.edits.row(&table, &field, &to).expect("just added")))
            })?;
            println!("added {line} in {addon}");
        }
        DbCommand::RemoveRow {
            table,
            to,
            field,
            addon,
        } => {
            let cat = catalog()?;
            let field = row_field(&table, field);
            edit(store, &cat, &addon, |p| {
                match p.edits.remove_row(&table, &field, &to) {
                    true => Ok(()),
                    false => bail!("{addon} adds no row for {to} to {table}"),
                }
            })?;
            println!("removed the row for {to} from {table} in {addon}");
        }
        DbCommand::Add {
            path,
            tag,
            value,
            addon,
        } => {
            let cat = catalog()?;
            let key = edit(store, &cat, &addon, |p| {
                p.edits
                    .add(&cat, &path, tag, &value)
                    .map_err(|e| anyhow!("{path}: {e}"))
            })?;
            println!("added {path} (0x{key:016x}) = {value} in {addon}");
        }
        DbCommand::Revert { path, all, addon } => {
            let cat = catalog()?;
            match (path, all) {
                (_, true) => {
                    let n = edit(store, &cat, &addon, |p| Ok(p.clear_edits()))?;
                    println!("reverted {n} change(s) in {addon}");
                }
                (Some(path), false) => {
                    edit(store, &cat, &addon, |p| {
                        let key = p.edits.resolve(&cat, &path).map_err(|e| anyhow!("{e}"))?;
                        match p.edits.revert(key) {
                            true => Ok(()),
                            false => bail!("{path} has no change in {addon}"),
                        }
                    })?;
                    println!("reverted {path} in {addon}");
                }
                (None, false) => bail!("name a key to revert, or pass --all"),
            }
        }
        DbCommand::List { addon } => {
            let p = open(store, &addon)?;
            for r in &p.edits.rows {
                println!("{}", row_line(r));
            }
            for e in &p.edits.edits {
                let name = e.path.clone().unwrap_or_else(|| format!("0x{:016x}", e.key));
                let stock = match (&e.stock, e.is_copied()) {
                    (Some(s), _) => s.as_str(),
                    (None, true) => "(copied row)",
                    (None, false) => "(added)",
                };
                let value = e.value.as_deref().unwrap_or("(removed)");
                println!("{name}: {stock} -> {value}");
            }
            println!("{} change(s) in {addon}", p.edits.len());
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
