#include <windows.h>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/ui.hpp>

#include <string>
#include <vector>

#include "core/frontend/addon_page.hpp"
#include "core/frontend/name_hash.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/frontend/vm_string.hpp"

namespace swofty::ui {

struct Host {
    static ViewModel *&slot(void *native) { return *reinterpret_cast<ViewModel **>(static_cast<uint8_t *>(native) + 0x2f8); }
    static void bind(ViewModel &vm, const std::string &screen, const std::string &path, void *native)
    {
        vm.screen_ = screen;
        vm.path_ = path;
        vm.native_ = native;
    }
    static void *native(const ViewModel &vm) { return vm.native_; }
};

}

namespace frontend {

namespace {

using namespace swofty::game;
using swofty::ui::Host;
using swofty::ui::ViewModel;

struct Target {
    const char *what, *pattern;
    uintptr_t rva;
};

const Target T_REGVM{"Vm_Register_ViewModel", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 88 02 00 00 4C 89 C3", 0x42bf450};
const Target T_FIXSTR{"Ne_Set_FixedString", "56 57 48 83 EC 28 48 89 D7 48 89 CE 0F B6 02 34 01", 0x1600d80};
const Target T_PATH_CALL{"Vm_Resolve_NodePath_Thunk call",
                         "E8 ?? ?? ?? ?? 4C 8D 2D ?? ?? ?? ?? 4C 89 6C 24 60 4C 8D 64 24 68 C5 F8", 0x42c07bb};
const Target T_SETINT_CALL{"Vm_Set_Int call in FeTile_ItemBase_Populate",
                           "E8 ?? ?? ?? ?? 4C 89 6C 24 30 C5 F8 57 C0 C5 F8 11 44 24 38 48 C7 44 24 48 00 00 00 00 48 8B 00 "
                           "48 89 44 24 38 48 85 C0 74 24 48 8B 48 28 48 89 4C 24 40 48 C7 44 24 48 00 00 00 00 48 85 C9 "
                           "74 04 48 89 59 10 48 89 58 28 66 F0 FF 40 24 48 8D 05 ?? ?? ?? ?? 48 89 44 24 30 48 8D 8F 00 01 "
                           "00 00",
                           0x1698223};
const Target T_REGNODE{"Vm_Register_BindingNode", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 88 01 00 00 49 89 D6", 0x4284af0};
const Target T_VTABLES{"FrontendTransitionViewModel vtable stores",
                       "48 8D 05 ?? ?? ?? ?? 48 89 03 C5 C8 57 F6 C5 F8 11 73 10 48 C7 43 20 00 00 00 00 48 8D 05 ?? ?? ?? ?? "
                       "48 89 43 08 48 89 73 28 48 C7 43 30 00 00 00 00 48 8D 4B 38 48 8D 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? "
                       "C7 83 40 01 00 00 2F ED 5D 10 48 8D 8B 48 01 00 00 48 8D 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? C7 83 50 02 "
                       "00 00 05 15 00 00 C7 83 58 02 00 00 02 00 00 00 80 A3 5C 02 00 00 E0 48 8B 4B 28 48 8D 94 24 90 "
                       "00 00 00 49 89 D8 E8 ?? ?? ?? ?? 80 8B 5C 02 00 00 02 C5 F8 11 B3 80 02 00 00 C5 F8 11 B3 70 02 "
                       "00 00 C5 F8 11 B3 90 02 00 00 48 8D 05 ?? ?? ?? ?? 48 89 03",
                       0x149b53e};
constexpr size_t HELPER_LEA = 0x00, HANDLE_LEA = 0x1b, TRANSITION_LEA = 0xad;
constexpr size_t VM_SIZE = 0x300, VT_SLOTS = 18;

using RegVm_t = uint64_t(void *mgr, uint32_t *mode, void *vm);
using FixStr_t = void(void *dst, const char *src);
using Path_t = void **(void *registry, void *root, const char *path);
using SetInt_t = void *(void *registry, void *parent, const char *name, int64_t *value, int overwrite);
using Getter_t = void *(void *self);

struct Factory {
    std::string prefix;
    swofty::ui::Factory make;
};

struct Instance {
    std::string screen;
    void *native = nullptr;
    std::unique_ptr<ViewModel> vm;
};

class VmHost {
public:
    bool init(uintptr_t module_base, const std::wstring &swofty_dir);
    bool ready() const { return is_ready; }
    bool attach(const std::string &screen);
    void add_factory(const char *screen_prefix, swofty::ui::Factory factory) { factories.push_back({screen_prefix, factory}); }
    bool set(const ViewModel &model, const char *prop, int type, const void *value) const;
    bool set_path(const std::string &path, int type, const void *value) const;
    uintptr_t node_token(const std::string &path) const;
    void *message_system() const;
    void **ui_system_global() const { return ui_system; }

private:
    bool build_vtables();
    void *binding_manager() const;
    void **resolve(void *reg, uint8_t *root, const char *path) const;
    void **data_node(const ViewModel &model, const char *child) const;
    void *parent_of(const ViewModel &model, const char *prop, std::string &leaf) const;
    bool write(void *reg, void *parent, const char *leaf, int type, const void *value) const;

    uintptr_t base = 0, a_regvm = 0, a_fixstr = 0, a_path = 0, a_setint = 0, transition_vt = 0, helper_vt = 0, handle_vt = 0;
    void **ui_system = nullptr;
    bool is_ready = false;
    std::vector<Factory> factories;
    std::vector<Instance *> instances;
    void *vtable[VT_SLOTS + 1] = {}, *vt260[6] = {}, *vt268[3] = {};
};

constinit VmHost host;

uintptr_t scan(const Target &t)
{
    uintptr_t at = 0;
    int hits = swofty::api->scan(".xtls", t.pattern, &at);
    if (hits != 1 || !at) {
        swofty::log("[ui] view models: %s signature matched %d time(s); view models stay off", t.what, hits);
        return 0;
    }
    return at;
}

uintptr_t rel32(uintptr_t insn)
{
    return *reinterpret_cast<const uint8_t *>(insn) == 0xe8 ? insn + 5 + *reinterpret_cast<const int32_t *>(insn + 1) : 0;
}

uintptr_t lea_target(uintptr_t insn)
{
    auto *b = reinterpret_cast<const uint8_t *>(insn);
    if (b[0] != 0x48 || b[1] != 0x8d || (b[2] & 0xc7) != 0x05)
        return 0;
    return insn + 7 + *reinterpret_cast<const int32_t *>(insn + 3);
}

using frontend::name_hash::djb2;

ViewModel *owner(void *native) { return native ? Host::slot(native) : nullptr; }

void *destroy(void *self, int) { return self; }

uint64_t ret0(void *, ...) { return 0; }

void on_message(void *, void *) {}

void on_bound(void *self)
{
    if (ViewModel *vm = owner(self))
        vm->on_bound();
}

void on_activated(void *self)
{
    if (ViewModel *vm = owner(self)) {
        swofty::log("[ui] view model %s: screen activated", vm->screen().c_str());
        vm->on_activated();
    }
}

void on_deactivated(void *self)
{
    if (ViewModel *vm = owner(self)) {
        swofty::log("[ui] view model %s: screen deactivated", vm->screen().c_str());
        vm->on_deactivated();
    }
}

void on_update(void *self, float dt)
{
    if (ViewModel *vm = owner(self))
        vm->update(dt);
}

void on_command(void *self, NeStr **name, const void *data)
{
    ViewModel *vm = owner(self);
    if (!vm || !name || !*name)
        return;
    uint32_t hash = *reinterpret_cast<uint32_t *>(*name);
    const char *text = reinterpret_cast<const char *>(*name) + 9;
    bool handled = vm->on_command({hash, text, data});
    swofty::log("[ui] view model %s: command '%s' (0x%08x) %s", vm->screen().c_str(), text, hash,
                handled ? "handled" : "not handled");
}

bool VmHost::build_vtables()
{
    if (!transition_vt || !helper_vt || !handle_vt) {
        swofty::log("[ui] view models: a vtable address did not resolve (transition base+0x%llx, helper base+0x%llx, "
                    "handle base+0x%llx); view models stay off",
                    (unsigned long long)(transition_vt ? transition_vt - base : 0),
                    (unsigned long long)(helper_vt ? helper_vt - base : 0),
                    (unsigned long long)(handle_vt ? handle_vt - base : 0));
        return false;
    }
    auto *src = reinterpret_cast<void **>(transition_vt), *helper = reinterpret_cast<void **>(helper_vt);
    for (size_t i = 0; i < VT_SLOTS; i++) {
        auto f = reinterpret_cast<uintptr_t>(src[i]);
        if (f < base || f > base + 0x6000000) {
            swofty::log("[ui] view models: template vtable slot %zu is not code (0x%llx); view models stay off", i,
                        (unsigned long long)f);
            return false;
        }
        vtable[1 + i] = src[i];
    }
    void **vt = vtable + 1;
    vt[0] = helper[0];
    vt[1] = helper[1];
    vt[2] = reinterpret_cast<void *>(&destroy);
    vt[5] = reinterpret_cast<void *>(&on_message);
    vt[6] = reinterpret_cast<void *>(&on_bound);
    vt[10] = reinterpret_cast<void *>(&on_command);
    vt[12] = reinterpret_cast<void *>(&on_update);
    vt[13] = reinterpret_cast<void *>(&on_activated);
    vt[15] = reinterpret_cast<void *>(&on_deactivated);
    for (auto &s : vt260)
        s = reinterpret_cast<void *>(&ret0);
    for (auto &s : vt268)
        s = reinterpret_cast<void *>(&ret0);
    return true;
}

void *VmHost::binding_manager() const
{
    if (!ui_system || !*ui_system)
        return nullptr;
    void *sys = *ui_system;
    auto fn = reinterpret_cast<Getter_t *>((*static_cast<void ***>(sys))[0x68 / 8]);
    return fn(sys);
}

void **VmHost::resolve(void *reg, uint8_t *root, const char *path) const
{
    return reinterpret_cast<Path_t *>(a_path)(reg, root + 8, path);
}

void **VmHost::data_node(const ViewModel &model, const char *child) const
{
    void *reg = *g_Vm_Registry;
    if (!reg || !a_path)
        return nullptr;
    auto *root = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(reg) + 8);
    if (!root)
        return nullptr;
    std::string path = model.path();
    if (child && *child)
        path += std::string(".") + child;
    void **node = resolve(reg, root, path.c_str());
    return node && *node ? node : nullptr;
}

void *VmHost::parent_of(const ViewModel &model, const char *prop, std::string &leaf) const
{
    std::string p(prop);
    size_t dot = p.find_last_of('.');
    std::string child = dot == std::string::npos ? "" : p.substr(0, dot);
    leaf = dot == std::string::npos ? p : p.substr(dot + 1);
    void **node = data_node(model, child.c_str());
    return node ? static_cast<uint8_t *>(*node) + 8 : nullptr;
}

bool VmHost::write(void *reg, void *parent, const char *leaf, int type, const void *value) const
{
    switch (type) {
    case 0:
        frontend::vm::set_string(reg, parent, leaf, static_cast<const char *>(value));
        return true;
    case 1: {
        uint8_t v = *static_cast<const int32_t *>(value) != 0;
        Vm_Set_Bool(reg, parent, leaf, &v, 1);
        return true;
    }
    case 2: {
        if (!a_setint)
            return false;
        int64_t v = *static_cast<const int32_t *>(value);
        reinterpret_cast<SetInt_t *>(a_setint)(reg, parent, leaf, &v, 1);
        return true;
    }
    }
    return false;
}

bool VmHost::set(const ViewModel &model, const char *prop, int type, const void *value) const
{
    std::string leaf;
    void *parent = parent_of(model, prop, leaf);
    if (!parent)
        return false;
    return write(*g_Vm_Registry, parent, leaf.c_str(), type, value);
}

bool VmHost::set_path(const std::string &path, int type, const void *value) const
{
    if (!g_Vm_Registry.address() || !a_path)
        return false;
    void *reg = *g_Vm_Registry;
    if (!reg)
        return false;
    auto *root = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(reg) + 8);
    if (!root)
        return false;
    size_t dot = path.find_last_of('.');
    if (dot == std::string::npos || dot == 0 || dot + 1 >= path.size())
        return false;
    std::string leaf = path.substr(dot + 1);
    void **node = resolve(reg, root, path.substr(0, dot).c_str());
    if (!node || !*node)
        return false;
    return write(reg, static_cast<uint8_t *>(*node) + 8, leaf.c_str(), type, value);
}

uintptr_t VmHost::node_token(const std::string &path) const
{
    if (!g_Vm_Registry.address() || !a_path)
        return 0;
    void *reg = *g_Vm_Registry;
    if (!reg)
        return 0;
    auto *root = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(reg) + 8);
    if (!root)
        return 0;
    void **node = resolve(reg, root, path.c_str());
    return node && *node ? reinterpret_cast<uintptr_t>(*node) : 0;
}

void *VmHost::message_system() const
{
    if (!ui_system || !swofty::mem::readable(ui_system, sizeof(void *)) || !*ui_system)
        return nullptr;
    void *sys = *ui_system;
    void **vt = *static_cast<void ***>(sys);
    if (!swofty::mem::readable(vt + 0x58 / 8, sizeof(void *)) || !swofty::mem::in_image(uintptr_t(vt[0x58 / 8])))
        return nullptr;
    return reinterpret_cast<Getter_t *>(vt[0x58 / 8])(sys);
}

bool VmHost::init(uintptr_t module_base, const std::wstring &swofty_dir)
{
    base = module_base;
    a_regvm = scan(T_REGVM);
    a_fixstr = scan(T_FIXSTR);
    if (uintptr_t path_call = scan(T_PATH_CALL))
        a_path = rel32(path_call);
    uintptr_t call = scan(T_SETINT_CALL);
    if (call)
        a_setint = rel32(call);
    if (uintptr_t stores = scan(T_VTABLES)) {
        helper_vt = lea_target(stores + HELPER_LEA);
        handle_vt = lea_target(stores + HANDLE_LEA);
        transition_vt = lea_target(stores + TRANSITION_LEA);
    }
    uintptr_t regnode = scan(T_REGNODE);
    if (regnode) {
        auto *insn = reinterpret_cast<const uint8_t *>(regnode + 0x16);
        if (insn[0] == 0x48 && insn[1] == 0x8b && insn[2] == 0x0d)
            ui_system = reinterpret_cast<void **>(regnode + 0x16 + 7 + *reinterpret_cast<const int32_t *>(insn + 3));
    }
    is_ready = a_regvm && a_fixstr && a_path && ui_system && Vm_Set_String.address() && Vm_Set_Bool.address() &&
               g_Vm_Registry.address() && build_vtables();
    if (call && a_setint != base + 0xd38fd0)
        swofty::log("[ui] view models: Vm_Set_Int resolved to base+0x%llx (KB build has base+0xd38fd0)",
                    (unsigned long long)(a_setint - base));
    frontend::addon_page::load(swofty_dir + L"settings.json");
    swofty::log("[ui] view models: %s (register base+0x%llx, path base+0x%llx, set_int base+0x%llx, vtables "
                "base+0x%llx/0x%llx/0x%llx, ui system %p, %zu factory(ies), %zu addon(s) from settings.json)",
                is_ready ? "ready" : "off", (unsigned long long)(a_regvm ? a_regvm - base : 0),
                (unsigned long long)(a_path ? a_path - base : 0), (unsigned long long)(a_setint ? a_setint - base : 0),
                (unsigned long long)(transition_vt ? transition_vt - base : 0),
                (unsigned long long)(helper_vt ? helper_vt - base : 0), (unsigned long long)(handle_vt ? handle_vt - base : 0),
                static_cast<void *>(ui_system), factories.size(), frontend::addon_page::count());
    return is_ready;
}

bool VmHost::attach(const std::string &screen)
{
    if (!is_ready)
        return false;
    for (auto *i : instances)
        if (i->screen == screen)
            return true;
    const Factory *f = nullptr;
    for (auto &c : factories)
        if (screen.compare(0, c.prefix.size(), c.prefix) == 0)
            f = &c;
    if (!f) {
        swofty::log("[ui] view model for %s: no factory matches the screen name; not registered", screen.c_str());
        return false;
    }
    void *mgr = binding_manager();
    if (!mgr) {
        swofty::log("[ui] view model for %s: binding manager not available yet", screen.c_str());
        return false;
    }
    auto *native = static_cast<uint8_t *>(VirtualAlloc(nullptr, VM_SIZE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
    if (!native)
        return false;
    auto *inst = new Instance{screen, native, f->make(screen)};
    *reinterpret_cast<void ***>(native) = vtable + 1;
    *reinterpret_cast<uintptr_t *>(native + 0x08) = handle_vt;
    *reinterpret_cast<void **>(native + 0x28) = mgr;
    *reinterpret_cast<void **>(native + 0x30) = nullptr;
    reinterpret_cast<FixStr_t *>(a_fixstr)(native + 0x38, screen.c_str());
    *reinterpret_cast<uint32_t *>(native + 0x140) = djb2(screen.c_str());
    reinterpret_cast<FixStr_t *>(a_fixstr)(native + 0x148, "");
    *reinterpret_cast<uint32_t *>(native + 0x250) = 0x1505;
    *reinterpret_cast<uint32_t *>(native + 0x258) = 1;
    *(native + 0x25c) = 0;
    *reinterpret_cast<void ***>(native + 0x260) = vt260;
    *reinterpret_cast<void ***>(native + 0x268) = vt268;
    Host::slot(native) = inst->vm.get();
    Host::bind(*inst->vm, screen, "view_model_data." + screen, native);
    uint32_t mode = 2;
    reinterpret_cast<RegVm_t *>(a_regvm)(mgr, &mode, native);
    *(native + 0x25c) |= 2;
    instances.push_back(inst);
    uint32_t key = *reinterpret_cast<uint32_t *>(native + 0x250);
    swofty::log("[ui] view model for %s registered (native %p, binding key 0x%08x%s, path %s)", screen.c_str(),
                static_cast<void *>(native), key, key == djb2(screen.c_str()) ? " = djb2(screen)" : " != djb2(screen)",
                inst->vm->path().c_str());
    return true;
}

}

}

namespace swofty::ui {

bool ViewModel::set_text(const char *prop, const char *utf8) { return frontend::host.set(*this, prop, 0, utf8); }

bool ViewModel::set_bool(const char *prop, bool value)
{
    int32_t v = value;
    return frontend::host.set(*this, prop, 1, &v);
}

bool ViewModel::set_int(const char *prop, int32_t value) { return frontend::host.set(*this, prop, 2, &value); }

void register_view_model(const char *screen_prefix, Factory factory) { frontend::host.add_factory(screen_prefix, factory); }

}

namespace frontend {

bool ui_vm_init(uintptr_t module_base, const std::wstring &swofty_dir) { return host.init(module_base, swofty_dir); }

bool ui_vm_ready() { return host.ready(); }

bool ui_vm_attach(const std::string &screen) { return host.attach(screen); }

bool ui_vm_set_path(const std::string &path, int type, const void *value) { return host.set_path(path, type, value); }

uintptr_t ui_vm_node_token(const std::string &path) { return host.node_token(path); }

uintptr_t ui_vm_epoch()
{
    if (!g_Vm_Registry.address())
        return 0;
    void *reg = *g_Vm_Registry;
    if (!reg)
        return 0;
    auto *root = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(reg) + 8);
    return reinterpret_cast<uintptr_t>(reg) * 31u + reinterpret_cast<uintptr_t>(root);
}

void *ui_vm_message_system() { return host.message_system(); }

void **ui_vm_ui_system() { return host.ui_system_global(); }

}
