#pragma once

#include <string>

// Republishes the hub tab of `addon` when `option` is one of its rows, after the option's state changed.
void hub_option_changed(const std::string &addon, const std::string &option);
