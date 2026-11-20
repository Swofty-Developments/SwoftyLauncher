#pragma once
#include <cstddef>
#include <string>

namespace frontend::addon_page {

void load(const std::wstring &settings_path);
size_t count();

}
