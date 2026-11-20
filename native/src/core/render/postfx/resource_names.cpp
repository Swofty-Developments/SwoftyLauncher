#include "core/render/postfx/list_placement.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/render/postfx/pipeline.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cwchar>
#include <cwctype>

namespace render::postfx {

namespace {

constexpr wchar_t TONEMAP_NAME[] = L"eaid://ego_package/tonemap_output.image";
constexpr wchar_t BACKBUFFER_NAME[] = L"BackBuffer";
constexpr wchar_t HUDLESS_NAME[] = L"eaid://ego_package/hudless_backbuffer.image";
constexpr wchar_t DEPTH_NAME[] = L"eaid://ego_package/gbuffer_depth_stencil.image";
constexpr wchar_t UI_PREFIX[] = L"eaid://ui_package/";
constexpr unsigned NAMED_LINES = 80;
constexpr const wchar_t *INTERESTING[] = {L"tonemap", L"output", L"final",  L"hudless", L"backbuffer", L"pathtrac",
                                          L"_pt",     L"pt_",    L"denois", L"upscal",  L"composite",  L"present",
                                          L"display", L"post",   L"lut",    L"bloom",   L"exposure"};

bool interesting(LPCWSTR name)
{
    if (!wcsstr(name, L"eaid://") || wcsncmp(name, UI_PREFIX, wcslen(UI_PREFIX)) == 0)
        return false;
    std::wstring lower(name);
    for (auto &c : lower)
        c = wchar_t(towlower(c));
    for (auto *k : INTERESTING)
        if (lower.find(k) != std::wstring::npos)
            return true;
    return false;
}

}

// Lets go of the held depth buffers other than keep: all of them when the picture changes, the old ones
// when the game names a new depth buffer, and only renamed when the game gives a held one another name.
void ListPlacement::drop_depths(ID3D12Resource *keep, ID3D12Resource *renamed)
{
    auto drop = [&](ID3D12Resource *&d) {
        if (d && (keep ? d != keep : !renamed || d == renamed))
            postfx::hold(d, nullptr);
    };
    for (auto &u : back_uses)
        drop(u.depth);
    drop(depth_res);
}

void ListPlacement::log_named(ID3D12Object *o, LPCWSTR name)
{
    if (named_seen.size() >= NAMED_LINES || std::find(named_seen.begin(), named_seen.end(), name) != named_seen.end())
        return;
    ID3D12Resource *res = nullptr;
    if (FAILED(o->QueryInterface(__uuidof(ID3D12Resource), reinterpret_cast<void **>(&res))) || !res)
        return;
    D3D12_RESOURCE_DESC d = res->GetDesc();
    res->Release();
    if (d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D)
        return;
    named_seen.emplace_back(name);
    swofty::log("[postfx] diag: named texture %s %llux%u fmt %d flags 0x%x", runtime::narrow(name).c_str(),
                (unsigned long long)d.Width, d.Height, int(d.Format), unsigned(d.Flags));
}

void ListPlacement::on_setname(ID3D12Object *o, LPCWSTR name)
{
    static const uint32_t probe = perf::probe("core", "postfx SetName");
    if (!name)
        return;
    perf::Scope p(probe);
    auto *res = reinterpret_cast<ID3D12Resource *>(o);
    std::lock_guard<std::recursive_mutex> g(pipeline.lock);
    if (pipeline.watched.bits(res)) {
        names.erase(res);
        pipeline.watched.watch(res, 0);
    }
    drop_depths(wcscmp(name, DEPTH_NAME) == 0 ? res : nullptr, res);
    bool tonemap = wcscmp(name, TONEMAP_NAME) == 0, back = wcscmp(name, BACKBUFFER_NAME) == 0;
    if (back)
        back_views.named(res);
    uint8_t bits = (tonemap ? W_TONEMAP : 0) | (back ? W_BACK : 0) | (wcscmp(name, HUDLESS_NAME) == 0 ? W_HUDLESS : 0) |
                   (wcscmp(name, DEPTH_NAME) == 0 ? W_DEPTH : 0) | (wcsncmp(name, UI_PREFIX, wcslen(UI_PREFIX)) == 0 ? W_UI : 0);
    if (tonemap || back || (!diag_done && wcsstr(name, L"eaid://")))
        names[res] = name;
    if (back && !saw_back) {
        saw_back = true;
        swofty::log("[postfx] the game names its back buffers \"BackBuffer\"");
    }
    if (tonemap && !tonemap_named) {
        tonemap_named = true;
        swofty::log("[postfx] the game named its tonemap output %p", static_cast<void *>(res));
    }
    if (interesting(name))
        log_named(o, name);
    if (bits)
        pipeline.watched.watch(res, bits);
}

}
