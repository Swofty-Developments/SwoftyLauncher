#pragma once

#include <span>

namespace runtime {

// Where in the core's lifecycle a subsystem starts. attach: in SwoftyCoreAttach, before the binding set loads.
// attach_code: at the end of SwoftyCoreAttach, once the game code reads decrypted. commit_code: at the first
// SwoftyCoreCommit, after the first install pass, once the code reads decrypted. ready: in the runtime setup the
// first commit queues last (runtime::ready), which also closes the addons' asset registrations.
enum class Start { attach, attach_code, commit_code, ready };

struct Subsystem {
    const char *name;
    Start start;
    void (*install)();
};

// Every subsystem the core starts, in start order. The core never unloads (the loader keeps it until the game
// exits), so there is no shutdown step.
std::span<const Subsystem> subsystems();

}
