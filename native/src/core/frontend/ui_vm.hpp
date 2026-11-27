#pragma once
#include <cstdint>
#include <string>

// The view model host (ui_vm.cpp): addon view models registered with the game's binding manager, and writes into the
// game's view model registry by node path (type 0 string, 1 bool, 2 int).
namespace frontend {

bool ui_vm_init(uintptr_t module_base, const std::wstring &swofty_dir);
bool ui_vm_ready();
bool ui_vm_attach(const std::string &screen);
bool ui_vm_set_path(const std::string &path, int type, const void *value);
// Binds the reference leaf at `path` to the node at `target` (a node_ref_logic.hpp Result as int).
int ui_vm_set_ref(const std::string &path, const std::string &target);
uintptr_t ui_vm_node_token(const std::string &path);
uintptr_t ui_vm_epoch();
void *ui_vm_message_system();
// The game's g_Vm_UiSystem global, or null before the view model host resolved it.
void **ui_vm_ui_system();

}
