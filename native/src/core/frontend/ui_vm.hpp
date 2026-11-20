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
uintptr_t ui_vm_node_token(const std::string &path);
uintptr_t ui_vm_epoch();
void *ui_vm_message_system();

}
