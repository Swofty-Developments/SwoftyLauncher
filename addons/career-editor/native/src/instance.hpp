#pragma once
#include <map>
#include <string>
#include <string_view>

namespace ce {

inline bool provisional_instance(std::string_view id)
{
    size_t cut = id.rfind(':');
    std::string_view tail = cut == std::string_view::npos ? id : id.substr(cut + 1);
    return tail.compare(0, 3, "new") == 0;
}

enum class Adoption { none, provisional, no_legacy, adopted, failed };

template <class Files>
Adoption adopt_legacy(Files &files, const std::string &id, const std::string &instance_path,
                      const std::string &legacy_path, std::string &text)
{
    if (id.empty() || provisional_instance(id))
        return Adoption::provisional;
    std::string existing;
    if (files.read(instance_path, existing))
        return Adoption::none;
    if (legacy_path.empty() || !files.read(legacy_path, text))
        return Adoption::no_legacy;
    if (!files.write(instance_path, text) || !files.write(legacy_path + ".adopted", text))
        return Adoption::failed;
    files.remove(legacy_path);
    return Adoption::adopted;
}

class Instance {
public:
    std::string resolve(bool loaded, const std::string &key, const std::string &saves_id)
    {
        if (!loaded || key.empty() || saves_id.empty()) {
            if (!loaded)
                reset();
            return {};
        }
        if (key != key_) {
            reset();
            key_ = key;
        }
        if (!id_.empty() && id_ != saves_id) {
            if (provisional_instance(id_) && !provisional_instance(saves_id))
                renames_[id_] = saves_id;
            else
                renames_.clear();
        }
        id_ = saves_id;
        return id_;
    }

    std::string renamed_to(const std::string &from) const
    {
        auto it = renames_.find(from);
        return it == renames_.end() ? std::string() : it->second;
    }

private:
    void reset()
    {
        key_.clear();
        id_.clear();
        renames_.clear();
    }

    std::string key_, id_;
    std::map<std::string, std::string> renames_;
};

}
