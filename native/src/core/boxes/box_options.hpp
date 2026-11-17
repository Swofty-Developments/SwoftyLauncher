#pragma once

#include <cstddef>
#include <string>

// The box rules that are game options (box_options.cpp), set and put back by the era picker's box rules.
namespace runtime {

// Sets the option of rule `rule` (an index into BOX_OPTION_RULES) to the box's setting. Returns the line to log.
std::string box_option_apply(size_t rule, const std::string &setting);
// Puts the player's index back where the option of rule `rule` still holds the box's. Returns 1 when it did.
int box_option_put_back(size_t rule);
// Drops every remembered index without touching the options.
void box_options_forget();

}
