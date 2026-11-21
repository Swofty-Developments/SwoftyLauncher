#pragma once

#include "core/apply/content/erp_file.hpp"
#include "core/apply/content/uidb.hpp"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <string_view>
#include <vector>

namespace frontend::view {

using CameraUid = std::array<uint64_t, 2>;

inline std::vector<CameraUid> circuit_camera_uids(const std::filesystem::path &root, std::string_view track)
{
    namespace erp = swofty_core::apply::content::erpfile;
    using swofty_core::apply::content::uidb::Node;
    using swofty_core::apply::content::uidb::Pkg;

    if (!track.starts_with("game/"))
        return {};
    const std::string code(track.substr(5));
    if (code.empty() || code.find_first_not_of("abcdefghijklmnopqrstuvwxyz0123456789_") != std::string::npos)
        return {};

    const auto dir = std::filesystem::path("2025_asset_groups/environment_package/tracks") / code / "wep";
    std::ifstream file(root / dir / (code + "_common.erp"), std::ios::binary | std::ios::ate);
    const auto end = file.tellg();
    if (end < 0)
        return {};

    erp::Source source{uint64_t(end), [&](uint64_t offset, size_t count, char *out) {
                           return bool(file.seekg(std::streamoff(offset)).read(out, std::streamsize(count)));
                       }};
    erp::Archive archive;
    std::string why;
    if (!erp::open(source, archive, why))
        return {};

    // Use the semantic layers of this circuit's WEP, rather than camera labels
    // or a catalog compiled from a particular circuit or game installation.
    const std::string resource = "eaid://environment_package/tracks/" + code + "/wep/" + code;
    std::vector<CameraUid> uids;
    for (const char *layer : {"cameras_replay_main", "cameras_replay_pitbox"}) {
        const auto *entry = archive.find(resource + "-" + layer + ".layer");
        if (!entry)
            continue;
        if (entry->frags.size() != 1)
            return {};
        const auto &fragment = entry->frags.front();
        std::string bytes;
        Pkg pkg;
        if (fragment.size > (1u << 20) || fragment.packed > (1u << 20) || !erp::fragment(source, archive, fragment, bytes, why) ||
            bytes.size() != fragment.size || !pkg.read(bytes) || pkg.name != "INST" || Pkg::text_of(pkg.root, "layer") != layer)
            return {};
        const Node *list = pkg.find("instances");
        if (!list || list->kind != Node::array)
            return {};
        for (const auto &instance : list->list) {
            const auto type = Pkg::text_of(instance, "template");
            if (type != "ReplayCam" && type != "StaticCamera" && type != "ProgressBasedSplineCamera" && type != "SplineCamera")
                continue;
            for (const auto &[key, value] : instance.items) {
                if (key.value != "uniqueid" || value.kind != Node::value || value.val.type != "woid" ||
                    value.val.value.size() != sizeof(CameraUid))
                    continue;
                CameraUid uid{};
                std::memcpy(uid.data(), value.val.value.data(), sizeof(uid));
                if (uid != CameraUid{})
                    uids.push_back(uid);
                break;
            }
        }
    }
    std::sort(uids.begin(), uids.end());
    uids.erase(std::unique(uids.begin(), uids.end()), uids.end());
    return uids;
}

}
