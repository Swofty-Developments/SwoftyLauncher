// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <string>

#include "router.hpp"

namespace steward {

bool server_start(Backend &backend, std::span<const Asset> assets);
uint16_t server_port();
const std::string &server_token();
bool server_running();
bool open_browser(const std::string &url);

}
