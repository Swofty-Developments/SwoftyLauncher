"""Fit original Mercedes suit artwork to the F1 25 body without changing either game."""
from pathlib import Path

import bcn
import build_suits as suits
import suit_transfer as transfer
from season_assets import OriginalErp, original_chain


def original_kit(game, year, label):
    if year == 2020:
        return transfer.old_kit(game, label, archive_type=OriginalErp)
    if year != 2021:
        raise ValueError(f"original suit transfer does not support {year}")
    archive = OriginalErp(str(game / f"{year}_asset_groups/character_package/team_livery/{label}/{label}.erp"))
    meshes = suits.team_meshes(label)
    prefix = f"eaid://character_package/team_livery/{label}/textures/{label}_"
    textures = {}
    for part in ("livery_v2_d", "logo_01_d", "glove_d", "glove_logo_d", "boots_d"):
        fmt, width, height, levels = original_chain(game, archive, archive.find(prefix + part + ".tif.image"))
        textures[part] = bcn.decode(fmt, levels[0], width, height)
    return {
        "body": transfer.merge(transfer.new_submeshes(archive, suits.BODY, "livery_v2")),
        "boots_mesh": transfer.merge(transfer.new_submeshes(archive, meshes["feet"], "boots")),
        "livery": transfer.bake_decals(textures["livery_v2_d"],
                                      transfer.new_submeshes(archive, meshes["logos"], "livery_v2_logo"), textures["logo_01_d"]),
        "glove": transfer.bake_decals(textures["glove_d"],
                                     transfer.new_submeshes(archive, meshes["glove_logos"], "gloves_logo"), textures["glove_logo_d"]),
        "boots": textures["boots_d"],
    }


def build(game25, original_game, year, team, cache):
    base = team.get("suit", {}).get("base", "2025-mercedes")
    label = suits.suit_sources.generated_label(year, team, base)
    source_label = f"{year}-mercedes"
    old = original_kit(Path(original_game), year, source_label)
    base_path = Path(game25) / f"2025_asset_groups/{suits.LIVERY}/{base}/{base}.erp"
    stock = suits.erpw.Erp(str(base_path))
    archive = suits.suit_archive(base_path, base, label, {})
    meshes = suits.team_meshes(label)
    uv, cover, distance = transfer.cached_map(cache, transfer.merge(transfer.new_submeshes(stock, suits.BODY, "livery_v2")),
                                             old["body"], *suits.SIZES["livery_v2_d"])
    buv, bcover, bdistance = transfer.cached_map(cache, transfer.merge(transfer.new_submeshes(archive, meshes["feet"], "boots")),
                                               old["boots_mesh"], *suits.SIZES["boots_d"])
    textures = {
        "livery_v2_d": transfer.apply_map(old["livery"], transfer.continue_map(uv, cover, distance), cover),
        "boots_d": transfer.apply_map(old["boots"], transfer.continue_map(buv, bcover, bdistance), bcover),
        "glove_d": transfer.resize(old["glove"], suits.SIZES["glove_d"]),
        "logo_01_d": transfer.transparent(suits.SIZES["logo_01_d"]),
        "glove_logo_d": transfer.transparent(suits.SIZES["glove_logo_d"]),
    }
    prefix = f"eaid://{suits.LIVERY}/{label}/textures/{label}_"
    for part in (*textures, "logo_01_n", "glove_logo_n"):
        transfer.put(archive, prefix + part + ".tif.image", textures.get(part), suits.SIZES[part])
    meta = {"label": label, "base": base, "design": f"original F1 {year}", "original_kit": source_label,
            "colours": [], "logo_slots": [], "glove_slots": [], "sponsors": 0}
    return label, archive, meta, suits.render(stock, archive, meshes, textures), textures["livery_v2_d"], textures["logo_01_d"]
