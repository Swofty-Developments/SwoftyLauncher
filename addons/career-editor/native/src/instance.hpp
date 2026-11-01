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
