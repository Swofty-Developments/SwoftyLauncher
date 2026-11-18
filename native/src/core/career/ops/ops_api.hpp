#pragma once

#include <cstddef>
#include <cstdint>

#include "swofty_api.h"

// The SwoftyCareerOps entries career_ops.cpp's table takes from other files.
namespace career::ops::live {

int api_apply_teams(SwoftyText request, char *out, size_t cap, size_t *len);
int api_set_car_number(uint32_t driver, int32_t want_number, uint32_t flags);

}
