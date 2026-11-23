#pragma once
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>
#include <string_view>
#include <system_error>
#include <utility>
#include <vector>

#include "core/runtime/durable_file.hpp"
#include "core/runtime/json.hpp"

// Pure logic of the test save sandbox (save_sandbox.cpp, re/db/subsystems/core.md#save-sandbox): the
// run token `swofty test` hands the game, the checks on the save device before its Steam remote storage
// interface is replaced, and the folder that stands in for Steam remote storage. No Windows calls, so
// the unit test runs on the host.

namespace runtime::save_sandbox {

constexpr const char *OFF_ENV = "SWOFTY_LAUNCHER_SAVES";
constexpr unsigned EXIT_TEST = 91;
constexpr unsigned EXIT_LAUNCHER = 93;
constexpr const char *STATUS_FILE = "launcher-saves.json";
constexpr const char *RUN_ENV = "SWOFTY_TEST_RUN";
constexpr const char *TOKEN_ENV = "SWOFTY_TEST_TOKEN";
constexpr const char *RUN_FILE = "run.json";
constexpr const char *REMOTE = "remote";
constexpr const char *TMP_SUFFIX = ".swofty-tmp";
constexpr const char *PREV_SUFFIX = ".swofty-prev";
constexpr const char *PENDING_NAME = "batch.swofty-pending";
constexpr const char *PENDING_SUFFIX = ".swofty-pending";
constexpr const char *INDEX_TAIL = "#QKRHMYXE";
constexpr size_t USER_DATA_CAP = 0x104;
constexpr size_t NAME_CAP = 260;
constexpr int SLOTS = 64;
constexpr int64_t QUOTA = int64_t(1) << 30;

// ISteamRemoteStorage (STEAMREMOTESTORAGE_INTERFACE_VERSION016) vtable slots, in declaration order.
enum Slot : int {
    FileWrite = 0,
    FileRead = 1,
    FileWriteAsync = 2,
    FileReadAsync = 3,
    FileReadAsyncComplete = 4,
    FileForget = 5,
    FileDelete = 6,
    FileShare = 7,
    SetSyncPlatforms = 8,
    FileWriteStreamOpen = 9,
    FileWriteStreamWriteChunk = 10,
    FileWriteStreamClose = 11,
    FileWriteStreamCancel = 12,
    FileExists = 13,
    FilePersisted = 14,
    GetFileSize = 15,
    GetFileTimestamp = 16,
    GetSyncPlatforms = 17,
    GetFileCount = 18,
    GetFileNameAndSize = 19,
    GetQuota = 20,
    IsCloudEnabledForAccount = 21,
    IsCloudEnabledForApp = 22,
    SetCloudEnabledForApp = 23,
    GetLocalFileChangeCount = 55,
    GetLocalFileChange = 56,
    BeginFileWriteBatch = 57,
    EndFileWriteBatch = 58,
};

inline const char *slot_name(int i)
{
    static const char *const first[] = {"FileWrite",
                                        "FileRead",
                                        "FileWriteAsync",
                                        "FileReadAsync",
                                        "FileReadAsyncComplete",
                                        "FileForget",
                                        "FileDelete",
                                        "FileShare",
                                        "SetSyncPlatforms",
                                        "FileWriteStreamOpen",
                                        "FileWriteStreamWriteChunk",
                                        "FileWriteStreamClose",
                                        "FileWriteStreamCancel",
                                        "FileExists",
                                        "FilePersisted",
                                        "GetFileSize",
                                        "GetFileTimestamp",
                                        "GetSyncPlatforms",
                                        "GetFileCount",
                                        "GetFileNameAndSize",
                                        "GetQuota",
                                        "IsCloudEnabledForAccount",
                                        "IsCloudEnabledForApp",
                                        "SetCloudEnabledForApp"};
    if (i >= 0 && i < int(sizeof first / sizeof *first))
        return first[i];
    switch (i) {
    case GetLocalFileChangeCount:
        return "GetLocalFileChangeCount";
    case GetLocalFileChange:
        return "GetLocalFileChange";
    case BeginFileWriteBatch:
        return "BeginFileWriteBatch";
    case EndFileWriteBatch:
        return "EndFileWriteBatch";
    default:
        return i >= 24 && i <= 54 ? "UGC or workshop call" : "unknown";
    }
}

// The slots the sandbox serves from its folder. Every other slot answers 0 (false, no call, no file)
// and is logged; none is ever forwarded to Steam.
inline bool served(int i)
{
    switch (i) {
    case FileWrite:
    case FileRead:
    case FileForget:
    case FileDelete:
    case SetSyncPlatforms:
    case FileExists:
    case FilePersisted:
    case GetFileSize:
    case GetFileTimestamp:
    case GetSyncPlatforms:
    case GetFileCount:
    case GetFileNameAndSize:
    case GetQuota:
    case IsCloudEnabledForAccount:
    case IsCloudEnabledForApp:
    case SetCloudEnabledForApp:
    case GetLocalFileChangeCount:
    case BeginFileWriteBatch:
    case EndFileWriteBatch:
        return true;
    default:
        return false;
    }
}

struct Run {
    bool ok = false;
    std::string why, id, token, saves, exe_sha256;
    // The save sandbox is required: run.json names a saves folder.
    bool sandbox = false;
    // The test agent's protocol and its test DLLs, relative to the run's agent/ folder.
    int agent_v = 0;
    std::vector<std::string> dlls;
};

// Where a test run's game saves go: the run's sandbox when run.json names saves, else the session's saves as at
// Play (the profile's launcher saves when the session has them on), never Steam's saves behind the session's back.
enum class RunSaves { sandbox, launcher };

inline RunSaves run_saves(const Run &r) { return r.sandbox ? RunSaves::sandbox : RunSaves::launcher; }

// A test DLL path in run.json: relative, inside the run's agent folder.
inline bool valid_dll(std::string_view d)
{
    return !d.empty() && d.size() <= 200 && d.find("..") == std::string_view::npos && d[0] != '/' && d[0] != '\\' &&
           d.find(':') == std::string_view::npos;
}

inline bool simple_name(std::string_view n)
{
    if (n.empty() || n.size() > 64 || n == "." || n == "..")
        return false;
    return std::all_of(n.begin(), n.end(), [](char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '.';
    });
}

// run.json in the run folder (docs/design/std-2/test-suite.md "The test run"):
// {"schema":1,"run":"<id>","token":"<hex>","saves":"saves","exe_sha256":"...","agent":{"v":1,"dlls":[...]}}.
// The token in the environment must equal the file's, so a stale variable alone never switches test
// mode on and a run folder alone never does either. "saves" asks for the save sandbox; without it the
// run is the test agent's only.
inline Run parse_run(std::string_view text, std::string_view token)
{
    Run r;
    json::Value doc;
    if (text.empty())
        return r.why = "the run folder has no readable run.json", r;
    if (!json::parse(std::string(text), doc) || doc.type != json::Value::Obj)
        return r.why = "run.json does not parse as an object", r;
    const json::Value &schema = doc["schema"];
    if (schema.type != json::Value::Num || schema.n != 1)
        return r.why = "run.json has no schema 1", r;
    const json::Value &t = doc["token"];
    if (t.type != json::Value::Str || t.s.size() < 16)
        return r.why = "run.json has no token", r;
    if (token.empty())
        return r.why = std::string("the environment has no ") + TOKEN_ENV, r;
    if (t.s != token)
        return r.why = std::string("the token in ") + TOKEN_ENV + " is not this run's", r;
    const json::Value &saves = doc["saves"];
    if (saves.type != json::Value::Null && (saves.type != json::Value::Str || !simple_name(saves.s)))
        return r.why = "run.json's saves is not a plain folder name", r;
    const json::Value &agent = doc["agent"];
    if (agent.type == json::Value::Obj) {
        r.agent_v = int(agent["v"].num(0));
        for (auto &d : agent["dlls"].a) {
            if (d.type != json::Value::Str || !valid_dll(d.s))
                return r.why = "run.json names a test DLL outside the run's agent folder: " + d.str(), r;
            r.dlls.push_back(d.s);
        }
    }
    r.token = t.s;
    r.sandbox = saves.type == json::Value::Str;
    r.saves = r.sandbox ? saves.s : std::string();
    if (doc["run"].type == json::Value::Str)
        r.id = doc["run"].s;
    if (!simple_name(r.id))
        return r.why = "run.json's run is not a plain name", r;
    if (doc["exe_sha256"].type == json::Value::Str)
        r.exe_sha256 = doc["exe_sha256"].s;
    r.ok = true;
    return r;
}

// Whether run.json still holds the run's token: `swofty test` removes it when the run ends, which
// stops the agent.
inline bool still_armed(std::string_view text, std::string_view token)
{
    json::Value doc;
    return !text.empty() && json::parse(std::string(text), doc) && doc.type == json::Value::Obj &&
           doc["token"].type == json::Value::Str && !token.empty() && doc["token"].s == token;
}

// The launcher's saves (swofty/settings.json): {"launcher_saves":{"profile":"default","remote":"Z:\\...\\saves\\default\\remote"}}.
// The block is there when the profile uses launcher saves; the core then serves the profile save from
// `remote` with the same proxy the test sandbox uses.
struct Launcher {
    bool present = false, ok = false;
    std::string why, profile, remote;
};

inline std::vector<std::string> split_path(std::string_view p)
{
    std::vector<std::string> out;
    std::string cur;
    for (char c : p) {
        if (c == '/' || c == '\\') {
            if (!cur.empty())
                out.push_back(cur);
            cur.clear();
        } else
            cur += c;
    }
    if (!cur.empty())
        out.push_back(cur);
    return out;
}

inline bool ansi_path(std::string_view p);
inline bool utf8_path(std::string_view p);

// The store of a launcher profile ends in saves/<profile>/remote, so a setting that names any other folder
// is refused.
inline bool store_path_ok(std::string_view remote, std::string_view profile)
{
    if (remote.empty() || remote.size() > 400 || !utf8_path(remote))
        return false;
    auto parts = split_path(remote);
    if (parts.size() < 3)
        return false;
    for (size_t i = 0; i < parts.size(); i++)
        if (parts[i] == "." || parts[i] == ".." || (i > 0 && parts[i].find(':') != std::string::npos))
            return false;
    size_t n = parts.size();
    return parts[n - 1] == REMOTE && parts[n - 2] == profile && parts[n - 3] == "saves";
}

inline Launcher parse_launcher(std::string_view text)
{
    Launcher l;
    json::Value doc;
    if (text.empty() || !json::parse(std::string(text), doc) || doc.type != json::Value::Obj)
        return l;
    const json::Value &b = doc["launcher_saves"];
    if (b.type == json::Value::Null)
        return l;
    l.present = true;
    if (b.type != json::Value::Obj)
        return l.why = "settings.json's launcher_saves is not an object", l;
    if (b["profile"].type != json::Value::Str || !simple_name(b["profile"].s))
        return l.why = "settings.json's launcher_saves has no plain profile name", l;
    l.profile = b["profile"].s;
    if (b["remote"].type != json::Value::Str)
        return l.why = "settings.json's launcher_saves names no remote folder", l;
    l.remote = b["remote"].s;
    if (!store_path_ok(l.remote, l.profile))
        return l.why = "settings.json's launcher_saves names a folder that is not saves/" + l.profile + "/remote: " + l.remote, l;
    l.ok = true;
    return l;
}

enum class Mode { off, test, launcher, fail };

struct Selection {
    Mode mode = Mode::off;
    std::string why;
};

// Which save mode a session runs in. A test run keeps its own sandbox and never mixes with the launcher's.
// `ea_backend` says the EA copy's storage can be served; until it can, an EA copy keeps its own saves.
inline Selection select_mode(bool test_env, const Launcher &l, bool opted_out, std::string_view distribution, bool ea_backend)
{
    if (test_env)
        return {Mode::test, ""};
    if (!l.present)
        return {Mode::off, ""};
    if (opted_out)
        return {Mode::off, "this session runs on Steam's saves (the launcher asked for it once)"};
    if (distribution == "ea" && !ea_backend)
        return {Mode::off, "launcher saves are not available on the EA copy yet; this session uses the EA copy's own saves"};
    if (!l.ok)
        return {Mode::fail, l.why};
    return {Mode::launcher, ""};
}

// A path the game can open: a Unix path (the CLI under Proton) maps to Wine's Z: drive.
inline std::string windows_path(std::string p)
{
    if (!p.empty() && p[0] == '/')
        p = "Z:" + p;
    std::replace(p.begin(), p.end(), '/', '\\');
    while (p.size() > 3 && p.back() == '\\')
        p.pop_back();
    return p;
}

inline bool ansi_path(std::string_view p)
{
    return !p.empty() && std::all_of(p.begin(), p.end(), [](char c) { return c >= 0x20 && c < 0x7f; });
}

// A store path from settings.json is UTF-8 (a Windows user folder can be "C:\\Users\\Hüseyin"); the sandbox opens it
// through wide paths, so any well-formed UTF-8 without control characters is accepted.
inline bool utf8_path(std::string_view p)
{
    if (p.empty())
        return false;
    for (size_t i = 0; i < p.size();) {
        auto c = static_cast<unsigned char>(p[i]);
        if (c < 0x80) {
            if (c < 0x20 || c == 0x7f)
                return false;
            i++;
            continue;
        }
        size_t n = c >= 0xc2 && c <= 0xdf ? 2 : c >= 0xe0 && c <= 0xef ? 3 : c >= 0xf0 && c <= 0xf4 ? 4 : 0;
        if (!n || i + n > p.size())
            return false;
        for (size_t k = 1; k < n; k++)
            if ((static_cast<unsigned char>(p[i + k]) & 0xc0) != 0x80)
                return false;
        i += n;
    }
    return true;
}

// A Steam remote storage file name: one flat name, as the save device builds them
// (<title dir>@<save>#<file>). Anything that could leave the folder is refused.
inline bool valid_name(std::string_view n)
{
    if (n.empty() || n.size() > NAME_CAP || n == "." || n == "..")
        return false;
    for (const char *suffix : {TMP_SUFFIX, PREV_SUFFIX, PENDING_SUFFIX})
        if (n.size() >= strlen(suffix) && n.substr(n.size() - strlen(suffix)) == suffix)
            return false;
    for (char c : n)
        if (static_cast<unsigned char>(c) < 0x20 || c == '/' || c == '\\' || c == ':' || c == '*' || c == '?' || c == '"' ||
            c == '<' || c == '>' || c == '|')
            return false;
    return n.find("..") == std::string_view::npos;
}

// The EA copy's save device (vtable bound as vt_Ne_SaveSystem2Win) has no Steam helper: it opens <title dir>\\<save>\\<file> in place.
// Game_Read_GameConfig builds one title config (the title's name, a string whose pointer is at +0x10; a u64 at +0x58;
// the title dir, an inline wide buffer of +0x68 characters, pointer +0x70, storage +0x78) and copies it twice: into
// the save system (+0x50, the device's +0x18) and into the save system's user interface (GameSaveSystemUserInterface,
// the device's +0x10, at +0xa8). Before a load the interface's prompt (its vt+0x10) compares the load's config with
// its own copy: a different title dir asks load_prev_save_confirmation_msg ("import profile data?"), and a load that
// then fails asks corrupt_save_retry_confirmation. So the title dir is replaced in both copies.
constexpr size_t EA_DEVICE_UI = 0x10;
constexpr size_t EA_DEVICE_CONFIG = 0x18;
constexpr size_t EA_UI_CONFIG = 0xa8;
constexpr size_t EA_CONFIG_NAME_PTR = 0x10;
constexpr size_t EA_NAME_CAP = 0x40;
constexpr size_t EA_CONFIG_CAP = 0x68;
constexpr size_t EA_CONFIG_PTR = 0x70;
constexpr size_t EA_CONFIG_INLINE = 0x78;
constexpr size_t EA_TITLE_CAP = 0x104;
constexpr const char *EA_TITLE_TAIL = "savegame\\ea";
constexpr const char *EA_DIR_NAME = "ea";

struct EaConfig {
    bool readable = false;
    uintptr_t at = 0;
    uint64_t cap = 0;
    uintptr_t ptr = 0, inline_at = 0;
    std::wstring title;
    std::string name;
};

struct EaDevice {
    bool readable = false;
    uintptr_t vtable = 0, start_slot = 0, start = 0, ui = 0;
    EaConfig config, ui_config;
};

// `read(at, out, n)` copies n bytes from address `at` and answers whether they were readable.
template <class Read> EaConfig read_ea_config(uintptr_t at, Read &&read)
{
    EaConfig c;
    c.at = at;
    uintptr_t name = 0;
    c.readable = at && read(at + EA_CONFIG_CAP, &c.cap, sizeof c.cap) && read(at + EA_CONFIG_PTR, &c.ptr, sizeof c.ptr) &&
                 read(at + EA_CONFIG_NAME_PTR, &name, sizeof name);
    if (!c.readable)
        return c;
    c.inline_at = at + EA_CONFIG_INLINE;
    uint16_t w[EA_TITLE_CAP] = {};
    if (c.ptr == c.inline_at && read(c.inline_at, w, sizeof w)) {
        size_t n = 0;
        while (n < EA_TITLE_CAP && w[n])
            n++;
        if (n < EA_TITLE_CAP)
            for (size_t i = 0; i < n; i++)
                c.title += wchar_t(w[i]);
    }
    for (char ch = 0; name && c.name.size() < EA_NAME_CAP && read(name + c.name.size(), &ch, 1) && ch;)
        c.name += ch;
    return c;
}

template <class Read> EaDevice read_ea_device(uintptr_t device, uintptr_t start, Read &&read)
{
    EaDevice d;
    d.start = start;
    uintptr_t config = 0;
    d.readable = device && read(device, &d.vtable, sizeof d.vtable) && d.vtable &&
                 read(d.vtable + 8, &d.start_slot, sizeof d.start_slot) &&
                 read(device + EA_DEVICE_CONFIG, &config, sizeof config) && read(device + EA_DEVICE_UI, &d.ui, sizeof d.ui);
    if (!d.readable)
        return d;
    d.config = read_ea_config(config, read);
    d.ui_config = read_ea_config(d.ui ? d.ui + EA_UI_CONFIG : 0, read);
    d.readable = d.config.readable;
    return d;
}

inline bool ends_with_nocase(const std::wstring &s, std::wstring_view tail)
{
    if (s.size() < tail.size())
        return false;
    for (size_t i = 0; i < tail.size(); i++) {
        wchar_t a = s[s.size() - tail.size() + i], b = tail[i];
        if (a >= L'A' && a <= L'Z')
            a += 32;
        if (b >= L'A' && b <= L'Z')
            b += 32;
        if (a != b)
            return false;
    }
    return true;
}

inline std::string check_ea_device(const EaDevice &d, uintptr_t expected_vtable)
{
    if (!d.readable)
        return "the EA save device is not readable";
    if (!d.start || d.start_slot != d.start)
        return "the save device's vtable slot +0x08 is not Ne_SaveSystem2Win_Start: not the NeSaveSystem2Win device";
    if (expected_vtable && d.vtable != expected_vtable)
        return "the save device's vtable is not the EA build's NeSaveSystem2Win";
    if (d.config.cap != EA_TITLE_CAP)
        return "the save device's title folder buffer is not 260 characters: not the layout the EA backend was written for";
    if (!d.config.ptr || d.config.ptr != d.config.inline_at)
        return "the save device's title folder is not held inline in its save system";
    if (!ends_with_nocase(d.config.title, L"\\savegame\\ea"))
        return "the save device's title folder does not end in savegame\\ea: it is not the EA copy's save folder";
    if (!d.ui || !d.ui_config.readable)
        return "the save device's user interface (+0x10) is not readable";
    if (d.ui_config.cap != EA_TITLE_CAP || d.ui_config.ptr != d.ui_config.inline_at)
        return "the save system's user interface does not hold its copy of the title folder inline (+0x110, +0x118): not "
               "the layout the EA backend was written for";
    if (d.config.name.empty() || d.ui_config.name != d.config.name || d.ui_config.title != d.config.title)
        return "the save system's user interface (+0xa8) does not hold a copy of the save device's title config";
    return {};
}

// `write(at, in, n)` copies n bytes to address `at` and answers whether it could.
template <class Write> bool write_ea_title(const EaConfig &c, const std::wstring &title, Write &&write)
{
    if (!c.inline_at || title.empty() || title.size() >= EA_TITLE_CAP)
        return false;
    std::vector<uint16_t> w(title.begin(), title.end());
    w.push_back(0);
    return write(c.inline_at, w.data(), w.size() * sizeof(uint16_t));
}

// Points both copies of the title dir at `title`, the interface's first, and reads them back.
template <class Read, class Write>
std::string point_ea_device(const EaDevice &d, const std::wstring &title, Read &&read, Write &&write)
{
    if (title.empty() || title.size() >= EA_TITLE_CAP)
        return "the title folder does not fit the save device's 260 character buffer";
    if (!write_ea_title(d.ui_config, title, write) || read_ea_config(d.ui_config.at, read).title != title)
        return "the save system's user interface did not take the launcher's folder";
    if (!write_ea_title(d.config, title, write) || read_ea_config(d.config.at, read).title != title)
        return "the save device's title folder did not take the launcher's folder";
    return {};
}

// The title folder the EA device is pointed at for a store `.../saves/<profile>/remote`: its sibling `ea`, whose
// `profile` folder is the profile save. Anything else is refused.
inline std::string ea_title_dir(std::string_view remote, std::string_view profile)
{
    if (!store_path_ok(remote, profile))
        return {};
    std::string p(remote);
    while (!p.empty() && (p.back() == '\\' || p.back() == '/'))
        p.pop_back();
    size_t cut = p.find_last_of("\\/");
    if (cut == std::string::npos)
        return {};
    std::string out = p.substr(0, cut + 1) + EA_DIR_NAME;
    return out.size() < EA_TITLE_CAP ? out : std::string();
}

// The title folder the EA device is pointed at in a test run: `<run>/saves/ea`, beside the run's `remote`, whose
// `profile` folder is the profile save. The folder keeps the separator the run folder uses.
inline std::string run_ea_title_dir(std::string_view saves)
{
    if (saves.empty() || saves.size() > 400 || !utf8_path(saves))
        return {};
    std::string p(saves);
    while (!p.empty() && (p.back() == '\\' || p.back() == '/'))
        p.pop_back();
    auto parts = split_path(p);
    if (parts.size() < 2)
        return {};
    for (size_t i = 0; i < parts.size(); i++)
        if (parts[i] == "." || parts[i] == ".." || (i > 0 && parts[i].find(':') != std::string::npos))
            return {};
    size_t cut = p.find_last_of("\\/");
    std::string out = p + p[cut] + EA_DIR_NAME;
    return out.size() < EA_TITLE_CAP ? out : std::string();
}

// The EA device writes the profile save in place, so a save shows as a change of the profile folder's files.
struct FolderStamp {
    uint32_t files = 0;
    uint64_t bytes = 0;
    int64_t newest = 0;
    bool operator==(const FolderStamp &) const = default;
};

inline FolderStamp stamp_of(const std::filesystem::path &dir)
{
    FolderStamp s;
    std::error_code ec;
    std::filesystem::directory_iterator it(dir, ec), end;
    for (; !ec && it != end; it.increment(ec)) {
        std::error_code fe;
        if (!it->is_regular_file(fe) || fe)
            continue;
        uint64_t n = it->file_size(fe);
        auto t = it->last_write_time(fe);
        if (fe)
            continue;
        s.files++;
        s.bytes += n;
        s.newest = std::max<int64_t>(s.newest, t.time_since_epoch().count());
    }
    return s;
}

// A save ends once the profile folder changed and then stayed the same for SETTLE_MS; each such save counts once.
constexpr uint64_t SETTLE_MS = 1000;

struct SaveSettle {
    FolderStamp seen;
    uint64_t since = 0;
    bool open = false;
    uint32_t saves = 0;

    void start(const FolderStamp &s, uint64_t now)
    {
        seen = s;
        since = now;
        open = false;
    }

    uint32_t observe(const FolderStamp &s, uint64_t now)
    {
        if (!(s == seen)) {
            seen = s;
            since = now;
            open = true;
        } else if (open && now - since >= SETTLE_MS) {
            open = false;
            saves++;
        }
        return saves;
    }
};

// What the core reads from the save device at Ne_SaveSystem2Win_Start, before it changes anything.
struct Device {
    bool readable = false;
    uintptr_t vtable = 0, start_slot = 0, start = 0;
    uintptr_t helper = 0, iface = 0, iface_vtable = 0;
    bool iface_vtable_in_game = false, iface_slots_ok = false;
    uint8_t store_in_cloud = 0;
    size_t user_data_len = 0;
};

inline std::string check_device(const Device &d)
{
    if (!d.readable)
        return "the save device is not readable";
    if (!d.start || d.start_slot != d.start)
        return "the save device's vtable slot +0x08 is not Ne_SaveSystem2Win_Start: not the NeSaveSystem2Win device";
    if (!d.helper)
        return "the save device has no Steam helper (+0x30): this build saves locally, which the sandbox does not cover";
    if (d.store_in_cloud != 1)
        return "the save device's m_storeInCloud (+0x3e8) is off: it would read and write the player's local save "
               "folder (Steam.cloudEnabled is false), which the sandbox does not cover";
    if (!d.iface)
        return "the Steam helper holds no remote storage interface (+0x108)";
    if (d.iface_vtable_in_game)
        return "the remote storage interface's vtable lies in the game image: not Steam's interface";
    if (!d.iface_slots_ok)
        return "the remote storage interface's first slots are not code";
    if (d.user_data_len >= USER_DATA_CAP)
        return "the user data folder (+0x00) is not a terminated string";
    return {};
}

// The folder that stands in for Steam remote storage: one file per remote name, written whole through a
// temporary file and a rename, read back byte for byte. The save device's files (the encrypted data file
// and its security index) are opaque bytes here.
class RemoteDir {
public:
    RemoteDir() = default;
    explicit RemoteDir(std::filesystem::path root) : root_(std::move(root)) {}

    const std::filesystem::path &root() const { return root_; }

    // A save is a data file and its index, written as one pair: each write is flushed to disk with its directory, the
    // files a batch replaces are kept as <name>.swofty-prev until the pair is whole, and a pending record names them. A
    // crash between the two writes leaves the record without both files, and recover() puts the old pair back.
    bool write(std::string_view name, const void *data, int32_t size)
    {
        if (!valid_name(name) || size < 0 || (size && !data))
            return false;
        std::error_code ec;
        std::filesystem::create_directories(root_, ec);
        if (!note(name))
            return false;
        if (!runtime::durable::write_file(path(name), data, size_t(size)))
            return false;
        if (is_index(name))
            pending_.index = true;
        else
            pending_.data = true;
        pending_.writes++;
        return save_pending();
    }

    int32_t read(std::string_view name, void *out, int32_t cap) const
    {
        if (!valid_name(name) || cap <= 0 || !out)
            return 0;
        std::ifstream f(path(name), std::ios::binary);
        if (!f)
            return 0;
        f.read(static_cast<char *>(out), cap);
        return int32_t(f.gcount());
    }

    bool remove(std::string_view name)
    {
        std::error_code ec;
        if (!valid_name(name) || !exists(name) || !note(name))
            return false;
        bool ok = std::filesystem::remove(path(name), ec) && !ec;
        runtime::durable::sync_dir(root_);
        return ok;
    }

    static bool is_index(std::string_view name)
    {
        return name.size() >= strlen(INDEX_TAIL) && name.substr(name.size() - strlen(INDEX_TAIL)) == INDEX_TAIL;
    }

    struct Recovery {
        enum Kind { none, rolled_forward, rolled_back, unreadable } kind = none;
        std::vector<std::string> files;
    };

    // Called when the game opens a save for loading or begins a write batch: a record left by an earlier run (or an
    // abandoned batch) is settled. A pair whose data file and index were both written is complete, so the new files stay;
    // anything less goes back to the files it replaced, and files the batch created are removed.
    Recovery recover(bool force = false) { return settle(force, false); }

    Recovery settle(bool force, bool forward)
    {
        Recovery r;
        std::error_code ec;
        if (!std::filesystem::is_regular_file(path(PENDING_NAME), ec))
            return r;
        if (!force && pending_open_ && std::chrono::steady_clock::now() - touched_ < std::chrono::seconds(10))
            return r;
        Pending p;
        if (!read_pending(p)) {
            r.kind = Recovery::unreadable;
            return r;
        }
        bool whole = forward || (p.index && p.data);
        for (auto &[name, had_prev] : p.files) {
            r.files.push_back(name);
            auto prev = prev_path(name);
            if (whole) {
                std::filesystem::remove(prev, ec);
            } else if (had_prev) {
                std::filesystem::rename(prev, path(name), ec);
            } else {
                std::filesystem::remove(path(name), ec);
            }
        }
        std::filesystem::remove(path(PENDING_NAME), ec);
        runtime::durable::sync_dir(root_);
        pending_ = Pending();
        pending_open_ = false;
        r.kind = whole ? Recovery::rolled_forward : Recovery::rolled_back;
        return r;
    }

    // EndFileWriteBatch: a whole pair (or a batch of only deletions) is final, so the kept old files go.
    bool commit()
    {
        if (!pending_open_)
            return true;
        if (!(pending_.index && pending_.data) && pending_.writes)
            return false;
        Recovery r = settle(true, true);
        return r.kind != Recovery::unreadable;
    }

    // A write or delete outside a write batch is final at once, so its kept copy and record go.
    bool settle_lone()
    {
        if (!pending_open_)
            return true;
        return settle(true, true).kind != Recovery::unreadable;
    }

    bool exists(std::string_view name) const
    {
        std::error_code ec;
        return valid_name(name) && std::filesystem::is_regular_file(path(name), ec);
    }

    int32_t size(std::string_view name) const
    {
        std::error_code ec;
        if (!exists(name))
            return 0;
        auto n = std::filesystem::file_size(path(name), ec);
        return ec || n > uint64_t(INT32_MAX) ? 0 : int32_t(n);
    }

    int64_t timestamp(std::string_view name) const
    {
        std::error_code ec;
        if (!exists(name))
            return 0;
        auto t = std::filesystem::last_write_time(path(name), ec);
        if (ec)
            return 0;
        auto sys = std::chrono::time_point_cast<std::chrono::seconds>(t - std::filesystem::file_time_type::clock::now() +
                                                                      std::chrono::system_clock::now());
        return int64_t(sys.time_since_epoch().count());
    }

    uint64_t used() const
    {
        uint64_t n = 0;
        for (auto &[name, size] : scan())
            n += uint64_t(size);
        return n;
    }

    // GetFileCount takes a snapshot that GetFileNameAndSize indexes, as Steam's own list does.
    int32_t count()
    {
        listing_ = scan();
        return int32_t(listing_.size());
    }

    const char *name_at(int32_t i, int32_t *size)
    {
        if (listing_.empty())
            listing_ = scan();
        if (i < 0 || size_t(i) >= listing_.size()) {
            if (size)
                *size = 0;
            return "";
        }
        if (size)
            *size = listing_[size_t(i)].second;
        return listing_[size_t(i)].first.c_str();
    }

private:
    struct Pending {
        std::vector<std::pair<std::string, bool>> files;
        bool index = false, data = false;
        int writes = 0;
    };

    std::filesystem::path path(std::string_view name) const { return root_ / std::filesystem::path(std::string(name)); }
    std::filesystem::path prev_path(const std::string &name) const { return path(name + PREV_SUFFIX); }

    bool save_pending() const
    {
        std::string t = "{\"schema\":1,\"index\":" + std::string(pending_.index ? "true" : "false") +
                        ",\"data\":" + (pending_.data ? "true" : "false") + ",\"files\":[";
        for (size_t i = 0; i < pending_.files.size(); i++)
            t += (i ? "," : "") + std::string("{\"name\":\"") + pending_.files[i].first +
                 "\",\"had_prev\":" + (pending_.files[i].second ? "true" : "false") + "}";
        t += "]}\n";
        return runtime::durable::write_file(path(PENDING_NAME), t);
    }

    bool read_pending(Pending &p) const
    {
        std::ifstream f(path(PENDING_NAME), std::ios::binary);
        std::string text((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
        json::Value doc;
        if (!json::parse(text, doc) || doc.type != json::Value::Obj || doc["files"].type != json::Value::Arr)
            return false;
        p.index = doc["index"].type == json::Value::Bool && doc["index"].b;
        p.data = doc["data"].type == json::Value::Bool && doc["data"].b;
        for (const json::Value &e : doc["files"].a)
            if (e["name"].type == json::Value::Str && valid_name(e["name"].s))
                p.files.emplace_back(e["name"].s, e["had_prev"].type == json::Value::Bool && e["had_prev"].b);
        return true;
    }

    bool note(std::string_view name)
    {
        std::string n(name);
        pending_open_ = true;
        touched_ = std::chrono::steady_clock::now();
        for (auto &[known, had] : pending_.files)
            if (known == n)
                return true;
        std::error_code ec;
        bool had_prev = std::filesystem::is_regular_file(path(n), ec);
        if (had_prev) {
            std::ifstream f(path(n), std::ios::binary);
            std::string bytes((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
            if (!runtime::durable::write_file(prev_path(n), bytes))
                return false;
        }
        pending_.files.emplace_back(n, had_prev);
        return save_pending();
    }

    std::vector<std::pair<std::string, int32_t>> scan() const
    {
        std::vector<std::pair<std::string, int32_t>> out;
        std::error_code ec;
        for (auto it = std::filesystem::directory_iterator(root_, ec); !ec && it != std::filesystem::directory_iterator();
             it.increment(ec)) {
            std::error_code e2;
            if (!it->is_regular_file(e2))
                continue;
            std::string n = it->path().filename().string();
            if (!valid_name(n))
                continue;
            auto sz = it->file_size(e2);
            out.emplace_back(n, e2 || sz > uint64_t(INT32_MAX) ? 0 : int32_t(sz));
        }
        std::sort(out.begin(), out.end());
        return out;
    }

    std::filesystem::path root_;
    std::vector<std::pair<std::string, int32_t>> listing_;
    Pending pending_;
    bool pending_open_ = false;
    std::chrono::steady_clock::time_point touched_{};
};

}
