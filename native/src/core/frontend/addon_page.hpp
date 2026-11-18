#pragma once
#include <cstddef>
#include <string>

namespace addon_page {

void load(const std::wstring &settings_path);
size_t count();

}
