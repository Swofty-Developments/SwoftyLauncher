#pragma once
#include <memory>
#include <set>
#include <string>
#include <vector>

#include "core/apply/content/slices.hpp"
#include "core/frontend/tile_records.hpp"
#include "core/hooks/hooks.hpp"
#include "core/registry/older.hpp"
#include "core/registry/store.hpp"
#include "core/runtime/runtime.hpp"

// Reads the effective ui.tile records of one place from the registry store under the current facts. Holds no state: the
// caller keeps its own log-once flags and logs Result::lines.

namespace frontend::tile_records {

inline RouteExists store_route_exists()
{
    return [](const std::string &id) {
        return runtime::older_store().get("ui.route", id).status == swofty_core::registry::Status::ok;
    };
}

inline Result read(Place place, Options opt)
{
    if (!opt.route_exists)
        opt.route_exists = store_route_exists();
    auto &st = runtime::older_store();
    std::vector<Value> keep;
    std::vector<std::string> ids = swofty_core::registry::older::in_load_order(st, REGISTRY);
    keep.reserve(ids.size());
    std::vector<Input> in;
    for (auto &id : ids) {
        auto rec = st.get(REGISTRY, id);
        if (rec.status != swofty_core::registry::Status::ok)
            continue;
        keep.push_back(std::move(rec.value));
        in.push_back({id, &keep.back()});
    }
    auto facts = swofty_core::hooks::core().current();
    return effective(in, place, facts.get(), opt);
}

inline bool has_place(Place place)
{
    auto &st = runtime::older_store();
    for (auto &id : swofty_core::registry::older::in_load_order(st, REGISTRY)) {
        auto rec = st.get(REGISTRY, id);
        if (rec.status == swofty_core::registry::Status::ok && place_of(text_of(rec.value["place"])) == place)
            return true;
    }
    return false;
}

inline SliceOf store_slices()
{
    return [](const std::string &id) -> std::optional<swofty_core::apply::content::slices::Ref> {
        auto rec = runtime::older_store().get(swofty_core::apply::content::slices::REGISTRY, id);
        if (rec.status != swofty_core::registry::Status::ok)
            return std::nullopt;
        auto array = swofty_core::apply::content::slices::canonical_array(text_of(rec.value["array"]));
        std::string name = text_of(rec.value["name"]);
        if (array.empty() || name.empty())
            return std::nullopt;
        return swofty_core::apply::content::slices::Ref{array, name};
    };
}

// "<array>|<name>" of every enabled ui.texture_slice record in the store: the slices an image string may name.
inline std::set<std::string> store_provided()
{
    namespace slices = swofty_core::apply::content::slices;
    std::set<std::string> out;
    auto &st = runtime::older_store();
    for (auto &id : swofty_core::registry::older::in_load_order(st, slices::REGISTRY)) {
        auto rec = st.get(slices::REGISTRY, id);
        if (rec.status != swofty_core::registry::Status::ok)
            continue;
        auto array = slices::canonical_array(text_of(rec.value["array"]));
        std::string name = text_of(rec.value["name"]);
        if (!array.empty() && !name.empty())
            out.insert(array + "|" + name);
    }
    return out;
}

}
