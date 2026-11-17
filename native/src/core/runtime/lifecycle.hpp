#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <atomic>
#include <string>
#include <vector>

#include "core/runtime/subsystems.hpp"
#include "swofty_api.h"

namespace runtime {

// The core's lifecycle, driven by the loader's exports: attach (SwoftyCoreAttach), one init per addon
// (SwoftyCoreInit), the commit that installs every mixin (SwoftyCoreCommit), the window stage (SwoftyCoreStage), and
// on a packed build the retry passes that wait for the game code to decrypt. It starts the subsystems of the ordered
// table in subsystems.cpp at their points.
class Runtime {
public:
    static Runtime &instance();

    int attach(SwoftyApi *a);
    int init_addon(const wchar_t *name, SwoftyAddonInitFn init);
    int commit();

    void when_code_ready(const char *what, void (*fn)());
    bool code_ready();
    // The runtime setup the first commit queues: closes the asset registrations, then starts the ready subsystems.
    void ready();

    SwoftyApi *api() const { return api_; }
    const std::wstring &dir() const { return dir_; }

private:
    struct CodeTask {
        std::string what;
        void (*fn)();
    };

    void start(Start point);
    int commit_pass();
    void late_pass(const char *pass, bool final);
    bool pending();
    void settle_std16(bool force);
    void start_retries();
    size_t run_code_tasks(const char *pass, bool final);
    bool code_decrypted(double &e);
    static DWORD WINAPI retry_worker(LPVOID);
    static void bind_stage(int stage);

    SwoftyApi *api_ = nullptr;
    std::wstring dir_;
    std::vector<CodeTask> code_tasks_;
    volatile LONG retries_started_ = 0;
    /* Standard library 1.6: once every addon has loaded (the window-stage commit, and on a packed build the
       last deferred addon's init too, or the retries giving up), the core arms the game hook points, reports
       undefined points and missing services, and fires game.boot, outside commit_lock. */
    std::atomic<bool> std16_window_{false}, std16_settled_{false};
};

}
