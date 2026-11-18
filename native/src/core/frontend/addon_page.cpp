#include "core/frontend/addon_page.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>
#include <swofty/ui.hpp>

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "core/frontend/name_hash.hpp"
#include "core/runtime/json.hpp"
#include "core/runtime/settings.hpp"

namespace frontend::addon_page {

namespace {

using frontend::name_hash::djb2;
using swofty::ui::ViewModel;

struct Option {
    std::string id, label, type;
    std::vector<std::string> commands, texts;
    int current = 0;
};

struct AddonInfo {
    std::string id, name;
    std::vector<Option> options;
};

constinit std::vector<AddonInfo> addons;

class AddonPage : public ViewModel {
public:
    void on_activated() override
    {
        const AddonInfo *m = find_addon();
        if (!m) {
            set_text("title", screen().c_str());
            swofty::log("[ui] view model %s: no addon for this page in settings.json", screen().c_str());
            return;
        }
        set_text("title", m->name.c_str());
        for (size_t i = 0; i < m->options.size(); i++)
            publish(*m, i);
        swofty::log("[ui] view model %s: published title '%s' and %zu option row(s)", screen().c_str(), m->name.c_str(),
                    m->options.size());
    }

    bool on_command(const swofty::ui::Command &c) override
    {
        AddonInfo *m = find_addon();
        if (!m)
            return false;
        std::string name = c.name ? c.name : "";
        for (size_t i = 0; i < m->options.size(); i++) {
            Option &o = m->options[i];
            int count = int(o.commands.size());
            if (!count)
                continue;
            int next = -1;
            for (int k = 0; k < count && next < 0; k++)
                if (djb2(o.commands[k].c_str()) == c.hash)
                    next = k;
            std::string tail = ":" + m->id + ":" + o.id;
            if (next < 0 && name == "swofty_next" + tail)
                next = o.type == "slider" ? (o.current + 1 < count ? o.current + 1 : o.current) : (o.current + 1) % count;
            if (next < 0 && name == "swofty_prev" + tail)
                next = o.type == "slider" ? (o.current > 0 ? o.current - 1 : 0) : (o.current + count - 1) % count;
            if (next < 0)
                continue;
            o.current = next;
            settings_command(djb2(o.commands[next].c_str()));
            publish(*m, i);
            return true;
        }
        return false;
    }

private:
    AddonInfo *find_addon()
    {
        auto &list = addons;
        size_t at = screen().find_last_of('_');
        if (at == std::string::npos)
            return nullptr;
        int index = atoi(screen().c_str() + at + 1);
        return index >= 0 && size_t(index) < list.size() ? &list[size_t(index)] : nullptr;
    }

    void publish(const AddonInfo &, size_t i)
    {
        const Option &o = find_addon()->options[i];
        char key[64];
        auto prop = [&](const char *leaf) {
            snprintf(key, sizeof key, "o%zu.%s", i, leaf);
            return key;
        };
        int count = int(o.commands.size());
        set_text(prop("label"), o.label.c_str());
        set_text(prop("text"), o.current >= 0 && o.current < int(o.texts.size()) ? o.texts[size_t(o.current)].c_str() : "");
        set_int(prop("index"), o.current);
        set_int(prop("value"), o.current);
        set_int(prop("min"), 0);
        set_int(prop("max"), count > 0 ? count - 1 : 0);
        set_int(prop("step"), 1);
        set_bool(prop("is_slider"), o.type == "slider");
    }
};

SWOFTY_VIEW_MODEL("swofty_addon_", AddonPage)

}

void load(const std::wstring &settings_path)
{
    HANDLE f = CreateFileW(settings_path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return;
    std::string text;
    LARGE_INTEGER size{};
    if (GetFileSizeEx(f, &size) && size.QuadPart > 0 && size.QuadPart < (16 << 20)) {
        text.resize(size_t(size.QuadPart));
        DWORD got = 0;
        if (!ReadFile(f, text.data(), DWORD(text.size()), &got, nullptr))
            text.clear();
    }
    CloseHandle(f);
    json::Value doc;
    if (text.empty() || !json::parse(text, doc))
        return;
    for (auto &jm : doc.field("addons", "mods").a) {
        AddonInfo m;
        m.id = jm["id"].str();
        m.name = jm["name"].str(m.id.c_str());
        for (auto &jo : jm["options"].a) {
            Option o;
            o.id = jo["id"].str();
            o.label = jo["label"].str(o.id.c_str());
            o.type = jo["type"].str();
            o.current = int(jo["current"].num(0));
            for (auto &st : jo["states"].a) {
                o.commands.push_back(st["command"].str());
                o.texts.push_back(st["text"].str());
            }
            m.options.push_back(std::move(o));
        }
        addons.push_back(std::move(m));
    }
}

size_t count() { return addons.size(); }

}
