#pragma once
#include "core/render/badges/badge_archive.hpp"

// End-to-end archive-worker fixtures. No game process or game writes are needed.
namespace tyre_image_service_tests {
namespace fs = std::filesystem;
namespace uidb = swofty_core::apply::content::uidb;
inline uidb::Node object()
{
    uidb::Node n;
    n.kind = uidb::Node::object;
    return n;
}
inline uidb::Node text(const std::string &s)
{
    uidb::Node n;
    n.val.type = "stri";
    n.val.value = s;
    return n;
}
inline void put(uidb::Node &n, const std::string &key, uidb::Node value)
{
    uidb::Val k;
    k.type = "stri";
    k.value = key;
    n.items.emplace_back(std::move(k), std::move(value));
}
inline std::string database(bool reverse, const std::string &uri)
{
    uidb::Node rows;
    rows.kind = uidb::Node::array;
    rows.tag = "!ili";
    for (const char *name : reverse ? std::vector<const char *>{"Wet", "Soft"} : std::vector<const char *>{"Soft", "Wet"}) {
        auto row = object();
        put(row, "Name", text(name));
        rows.list.push_back(std::move(row));
    }
    auto array = object();
    put(array, "TextureArrayName", text(uri));
    put(array, "Contents", std::move(rows));
    auto group = object();
    put(group, "key_icons", std::move(array));
    auto allocation = object();
    put(allocation, "Contents", std::move(group));
    auto contents = object();
    put(contents, "tyre_allocation", std::move(allocation));
    auto overrides = object();
    put(overrides, "Contents", std::move(contents));
    uidb::Pkg pkg;
    uidb::Data strings;
    strings.type = "stri";
    pkg.tables.push_back(std::move(strings));
    pkg.root = object();
    put(pkg.root, "overrides", std::move(overrides));
    return pkg.write();
}
inline std::vector<std::string> surface(bool blue)
{
    std::vector<std::string> parts(2);
    parts[0].resize(40);
    parts[1].resize(32);
    for (auto [at, v] : {std::pair{8, 55u}, {12, 4u}, {16, 4u}, {24, 1u}, {28, 2u}})
        std::memcpy(parts[0].data() + at, &v, 4);
    // Opaque red (or blue) then opaque green.
    parts[1][0] = parts[1][1] = char(-1);
    if (blue)
        parts[1][8] = parts[1][10] = 31;
    else
        parts[1][9] = parts[1][11] = char(-8);
    parts[1][16] = parts[1][17] = char(-1);
    parts[1][24] = parts[1][26] = char(-32);
    parts[1][25] = parts[1][27] = 7;
    return parts;
}
inline void erp(const fs::path &path, const std::map<std::string, std::vector<std::string>> &resources)
{
    std::string entries, payload;
    uint32_t fragments = 0;
    auto put = [](std::string &s, auto v) { s.append(reinterpret_cast<const char *>(&v), sizeof v); };
    for (const auto &[name, parts] : resources) {
        std::string entry(4, '\0');
        put(entry, uint16_t(name.size() + 1));
        entry += name;
        entry += '\0';
        entry.append(20, '\0');
        entry += char(parts.size());
        for (const auto &part : parts) {
            entry += "temp";
            put(entry, uint64_t(payload.size()));
            put(entry, uint64_t(part.size()));
            put(entry, uint32_t(16));
            entry += '\x10';
            put(entry, uint64_t(part.size()));
            payload += part;
            ++fragments;
        }
        entry.append(16, '\0');
        uint32_t length = uint32_t(entry.size() - 4);
        std::memcpy(entry.data(), &length, 4);
        entries += entry;
    }
    std::string index;
    put(index, uint32_t(resources.size()));
    put(index, fragments);
    index += entries;
    std::string bytes = "ERPK";
    put(bytes, uint32_t(3));
    bytes.append(8, '\0');
    put(bytes, uint64_t(48));
    put(bytes, uint64_t(index.size()));
    put(bytes, uint64_t(48 + index.size()));
    bytes.append(8, '\0');
    bytes += index;
    bytes += payload;
    std::ofstream file(path, std::ios::binary);
    file.write(bytes.data(), std::streamsize(bytes.size()));
    assert(file.good());
}
inline int run(const SwoftyLiveImages *api)
{
    using namespace badge_images;
    const auto temp = fs::canonical(fs::temp_directory_path());
    const auto root =
        temp / ("swofty-tyre-service-test-" + std::to_string(GetCurrentProcessId()) + "-" + std::to_string(GetTickCount64()));
    fs::create_directories(root / "2025_asset_groups/ui_package");
    fs::create_directories(root / "swofty");
    const auto assets = root / "swofty/assets.json";
    {
        std::ofstream file(assets);
        file
            << R"({"resource_erps":[{"file":"overlay.erp","erps":["2025_asset_groups/ui_package/base.erp"]},{"file":"last.erp","erps":["2025_asset_groups/ui_package/base.erp"]}]})";
    }
    assert(SetEnvironmentVariableW(L"SWOFTY_BADGE_FIXTURE_ASSETS", assets.c_str()));
    const std::string uri = "ui_package/art/texture_arrays/test/tyres.texarray";
    const std::string custom = "ui_package/art/texture_arrays/test/custom.texarray";
    const std::string db = archive::DATABASE, image = "eaid://" + uri + ".image";
    const std::string overlay = "swofty/overlay.erp", last = "swofty/last.erp";
    const std::string reference = "#overrides.tyre_allocation.key_icons.Soft";
    erp(root / "2025_asset_groups/ui_package/base.erp", {{db, {database(false, uri)}}, {image, surface(false)}});
    erp(root / "swofty/redirect.erp", {{db, {database(true, uri)}}, {image, surface(false)}});
    erp(root / "swofty/overlay.erp", {{db, {database(true, uri)}}, {image, surface(false)}});
    erp(root / "swofty/last.erp", {{image, surface(true)}});
    erp(root / "swofty/copy.erp", {{image, surface(false)}});
    erp(root / "swofty/custom.erp", {{db, {database(false, custom)}}, {"eaid://" + custom + ".image", surface(true)}});
    // Hide configured overlays for the initial stock/redirect checks using accepted route receipts.
    const auto empty = root / "swofty/empty.erp";
    erp(empty, {});
    const std::map<std::string, std::string> hidden = {{overlay, "swofty/empty.erp"}, {last, "swofty/empty.erp"}};
    file_routes(hidden);
    {
        archive::Env probe;
        probe.root = root;
        probe.routes = hidden;
        probe.overlays = {overlay, last};
        archive::Resources resources(probe);
        assert(resources.available());
        std::vector<std::string> parts;
        assert(resources.read(db, parts));
        uidb::Pkg parsed;
        assert(parsed.read(parts[0]));
        archive::Catalogue c(live_image_reference::TYRES[0]);
        assert(c.load(resources));
        archive::Pixels pixels;
        assert(c.decode(reference, pixels));
        assert(pixels.rgba[0] == 255);
    }
    uint8_t rgba[data::PIXELS]{};
    uint32_t w{}, h{};
    uint64_t revision{};
    auto wait = [&](const char *key = nullptr) {
        int status;
        const auto until = GetTickCount64() + 5000;
        do {
            status = api->image(key ? key : reference.c_str(), rgba, sizeof rgba, &w, &h, &revision);
            if (status == SWOFTY_ASSET_LATE)
                Sleep(1);
        } while (status == SWOFTY_ASSET_LATE && GetTickCount64() < until);
        return status;
    };
    assert(wait() == SWOFTY_ASSET_OK && w == 4 && h == 4 && rgba[0] == 255);
    auto before = revision;
    auto routed = hidden;
    routed["2025_asset_groups/ui_package/base.erp"] = "swofty/redirect.erp";
    file_routes(routed);
    assert(wait() == SWOFTY_ASSET_OK && rgba[1] == 255 && revision > before);
    // Removing a route restores stock pixels; no stale green image survives.
    file_routes(hidden);
    assert(wait() == SWOFTY_ASSET_OK && rgba[0] == 255);
    // Reverse resource overlay order: last supplies blue pixels while first supplies the reversed UIDB.
    file_routes({});
    assert(wait() == SWOFTY_ASSET_OK && rgba[1] == 255);
    // Accepted copy replaces the last overlay's pixels. Use stock UIDB to select slice 0.
    routed = {{overlay, "swofty/empty.erp"}};
    file_routes(routed);
    assert(wait() == SWOFTY_ASSET_OK && rgba[2] == 255);
    overlay_routes({{last, "swofty/copy.erp"}});
    assert(wait() == SWOFTY_ASSET_OK && rgba[0] == 255);
    overlay_routes({});
    // A mod UIDB can redirect the catalogue to another allowed UI array.
    routed = hidden;
    routed["2025_asset_groups/ui_package/base.erp"] = "swofty/custom.erp";
    file_routes(routed);
    assert(wait() == SWOFTY_ASSET_OK && rgba[2] == 255);
    assert(wait("#overrides.tyre_allocation.key_icons.Missing") == SWOFTY_ASSET_UNAVAILABLE);
    api->clear();
    assert(wait() == SWOFTY_ASSET_OK && rgba[2] == 255);
    // Retire queued work before changing its sources; only new pixels may publish.
    api->clear();
    assert(api->image(reference.c_str(), rgba, sizeof rgba, &w, &h, &revision) == SWOFTY_ASSET_LATE);
    file_routes(hidden);
    session_unloaded();
    assert(wait() == SWOFTY_ASSET_OK && rgba[0] == 255);
    // Ambiguous copies fail closed rather than choosing the last scanned ERP.
    erp(root / "2025_asset_groups/ui_package/duplicate.erp", {{image, surface(true)}});
    api->clear();
    assert(wait() == SWOFTY_ASSET_UNAVAILABLE);
    api->clear();
    file_routes({});
    const auto resolved = fs::canonical(root);
    assert(resolved.parent_path() == temp && resolved.filename().wstring().starts_with(L"swofty-tyre-service-test-"));
    fs::remove_all(resolved);
    std::puts("PASS: tyre archive worker, file routes, overlay order/copies, mod UIDB/array, route removal, cache revisions, "
              "clear, session retirement and conflicting duplicates");
    return 0;
}
} // namespace tyre_image_service_tests
