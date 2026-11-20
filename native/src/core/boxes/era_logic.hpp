#pragma once

// Pure logic of the era picker addon row (era_picker.cpp): the box check, the box order, the image
// default, the Time Trial list and its car limit, the press guard, the links of the row's two node handles
// and the DRS group swap of the host setup mode. No game calls, so the unit test runs on the host.

#include "core/boxes/era/box.hpp"
#include "core/boxes/era/time_trial.hpp"
#include "core/boxes/era/session_trace.hpp"
#include "core/boxes/era/continue_pick.hpp"
#include "core/boxes/era/drs.hpp"
#include "core/boxes/era/warmers.hpp"
#include "core/boxes/era/time_limit.hpp"
#include "core/boxes/era/weekend.hpp"
#include "core/boxes/era/points.hpp"
#include "core/boxes/era/tyre_change.hpp"
#include "core/boxes/era/setup_pointers.hpp"
