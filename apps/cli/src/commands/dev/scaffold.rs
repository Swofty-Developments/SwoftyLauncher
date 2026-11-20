use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use swofty_core::manifest::MANIFEST_FILE;

use crate::commands::dev::toolchain::python;

pub fn pack(sdk: &Path, folder: &Path, out: &Path, zip: bool, with: &[String]) -> Result<String> {
    let mut cmd = Command::new(python()?);
    cmd.arg(sdk.join("tools/pack_addon.py"));
    if zip {
        cmd.arg("--zip");
    }
    for f in with {
        cmd.arg("--with").arg(f);
    }
    let o = cmd
        .arg(folder)
        .arg(out)
        .output()
        .context("python3 is needed to pack an addon")?;
    if !o.status.success() {
        bail!(
            "pack_addon.py failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("{}", src.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == "build" || n.ends_with(".dll") || n.starts_with('.') {
            continue;
        }
        let (from, to) = (entry.path(), dest.join(&name));
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub fn short_name(id: &str) -> String {
    let last = id.rsplit('.').next().unwrap_or(id);
    let s: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "addon".into() } else { s }
}

pub(super) fn title(id: &str) -> String {
    id.rsplit('.')
        .next()
        .unwrap_or(id)
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const REGISTRY_SOURCE: &str = r#"#include <swofty/registry.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/sounds.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    auto sound = sounds::engine_sounds.add("screamer", {
        .bundle = file("sounds/screamer.bdl"),
        .settings_from = sounds::engine_sounds["game/ferrari_2025"],
    });
    cars::vehicles.layer(cars::vehicles["game/ferrari_2025"], [=](cars::Vehicle &v) { v.engine_sound = sound; });
}

SWOFTY_ADDON()
"#;

const REGISTRY_TEST: &str = r#"#include <cstdio>
#include <string>
#include <vector>

#include <swofty/fake_core.hpp>

#include "../native/src/@NAME@.cpp"

static int failures = 0;

static void check(bool ok, const char *what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    failures += !ok;
}

static void run(int round)
{
    swofty::fake::Core core{"@ID@"};
    core.stock(swofty::cars::vehicles, "ferrari_2025", swofty::cars::Vehicle{.label = "ferrari_2025"});
    core.run_register();
    std::printf("round %d\n", round);
    check(core.recording()["registries"].size() == 2, "both registries are declared");
    check(core.ids("engine_sound") == std::vector<std::string>{"@ID@/screamer"}, "adds the screamer sound");
    auto layers = core.layers("vehicle", "game/ferrari_2025");
    check(layers.size() == 1, "one layer on the 2025 Ferrari");
    if (layers.size() == 1) {
        auto v = core.apply(*layers[0], swofty::json::Value::object());
        check(v && (*v)["engine_sound"].as_string() == "@ID@/screamer", "the layer puts the screamer on it");
    }
    if (core.rules()) {
        auto car = core.effective("vehicle", "game/ferrari_2025");
        check(car && (*car)["engine_sound"].as_string() == "@ID@/screamer", "the effective Ferrari has it");
        check(core.origin("vehicle", "game/ferrari_2025", "/engine_sound") == "@ID@", "and the origin says so");
    }
}

int main()
{
    run(1);
    run(2);
    return failures ? 1 : 0;
}
"#;

pub fn new_addon(sdk: &Path, id: &str, dest: &Path, registry: bool) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        bail!("{id:?} isn't an addon id: use letters, digits, dots, dashes and underscores (author.name)");
    }
    if dest.exists() && std::fs::read_dir(dest)?.next().is_some() {
        bail!("{} already exists and isn't empty", dest.display());
    }
    copy_dir(&sdk.join("templates/addon"), dest)?;
    let path = dest.join(MANIFEST_FILE);
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    m["id"] = id.into();
    m["name"] = title(id).into();
    if registry {
        let name = short_name(id);
        copy_dir(&sdk.join("packages/sdk/include"), &dest.join("native/include"))?;
        let src = dest.join("native/src");
        std::fs::remove_file(src.join("sample_plugin.cpp")).ok();
        std::fs::write(src.join(format!("{name}.cpp")), REGISTRY_SOURCE)?;
        let build = dest.join("native/build.sh");
        let script = std::fs::read_to_string(&build)?
            .replace("sample_plugin", &name)
            .replace(
                "-O2 -Wall -Wextra",
                "-O2 -Wall -Wextra -Wno-nullability-completeness",
            );
        std::fs::write(&build, script)?;
        std::fs::create_dir_all(dest.join("test"))?;
        std::fs::write(
            dest.join("test/register_test.cpp"),
            REGISTRY_TEST.replace("@NAME@", &name).replace("@ID@", id),
        )?;
        for d in ["db", "ui"] {
            std::fs::remove_dir_all(dest.join(d)).ok();
        }
        m["description"] =
            "An engine sound under the addon's own key, put on the 2025 Ferrari by a layer.".into();
        m["kind"] = serde_json::json!(["native"]);
        m["compat"]["launcher"] = ">=1.6.0".into();
        m["options"] = serde_json::json!([]);
        m["operations"] = serde_json::json!({ "native": [ { "dll": format!("native/{name}.dll") } ] });
    }
    std::fs::write(&path, serde_json::to_string_pretty(&m)? + "\n")?;
    Ok(())
}
