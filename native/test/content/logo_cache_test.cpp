#include "../../src/core/apply/content/logo_cache.hpp"

#include <algorithm>
#include <cstdio>
#include <map>
#include <mutex>
#include <set>
#include <string>
#include <vector>

namespace lc = swofty_core::apply::content::logo_cache;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

static uint64_t hash_of(const std::string &bytes, size_t chunk)
{
    lc::Bytes b;
    for (size_t at = 0; at < bytes.size(); at += chunk)
        b.add(bytes.data() + at, std::min(chunk, bytes.size() - at));
    return b.done();
}

struct Disk {
    struct File {
        std::string bytes;
        uint64_t time = 0;
    };
    std::map<std::string, File> files;
    int hashes = 0;

    lc::Env env()
    {
        return {
            [this](const std::string &rel, uint64_t &size, uint64_t &time) {
                auto f = files.find(rel);
                if (f == files.end())
                    return false;
                size = f->second.bytes.size();
                time = f->second.time;
                return true;
            },
            [this](const std::string &rel, uint64_t &hash) {
                auto f = files.find(rel);
                if (f == files.end())
                    return false;
                hashes++;
                hash = hash_of(f->second.bytes, 4096);
                return true;
            },
        };
    }

    std::vector<std::string> list() const
    {
        std::vector<std::string> out;
        for (auto &[rel, f] : files)
            out.push_back(rel);
        return out;
    }
};

struct Boot {
    uint64_t key;
    size_t hashed;
    std::string stamps;
};

static const std::string DATABASES = "ui/databases.erp";

static Boot boot(Disk &disk, const std::string &stamps)
{
    auto before = lc::parse(stamps);
    auto inputs = lc::inputs_of(before);
    inputs.insert(DATABASES);
    auto k = lc::key_archives(7, disk.list(), inputs, disk.env(), before);
    return {k.key, k.hashed, lc::text(k.stamps)};
}

static Boot built(Disk &disk, const std::string &stamps, std::set<std::string> read)
{
    read.insert(DATABASES);
    auto k = lc::key_archives(7, disk.list(), read, disk.env(), lc::parse(stamps));
    return {k.key, k.hashed, lc::text(k.stamps)};
}

static lc::Keyed reference_key(uint64_t key, const std::vector<std::string> &files, const std::set<std::string> &inputs, const lc::Env &env,
                               const lc::Stamps &before)
{
    lc::Keyed out;
    for (auto &rel : files) {
        lc::Seen now;
        if (!env.stat(rel, now.size, now.time))
            continue;
        if (!inputs.count(rel)) {
            key = lc::stamp(key, rel + "|" + std::to_string(now.size) + "|s");
            continue;
        }
        auto was = before.find(rel);
        if (was != before.end() && was->second.hashed && was->second.size == now.size && was->second.time == now.time) {
            now = was->second;
        } else if (env.hash(rel, now.hash)) {
            now.hashed = true;
            out.hashed++;
            out.hashed_bytes += now.size;
        }
        char hex[17];
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(now.hash));
        key = lc::stamp(key, rel + "|" + std::to_string(now.size) + "|" + (now.hashed ? std::string("h") + hex : "t" + std::to_string(now.time)));
        out.stamps[rel] = now;
    }
    out.key = key;
    return out;
}

static void many_files()
{
    std::mutex lock;
    std::map<std::string, std::pair<std::string, uint64_t>> files;
    std::vector<std::string> names;
    std::set<std::string> inputs;
    for (int i = 0; i < 600; i++) {
        std::string rel = "ui/a" + std::to_string(i) + ".erp";
        files[rel] = {std::string(size_t(100 + i * 37), char('a' + i % 26)), uint64_t(1000 + i)};
        names.push_back(rel);
        if (i % 3 == 0)
            inputs.insert(rel);
    }
    names.push_back("ui/missing.erp");
    inputs.insert("ui/missing.erp");
    inputs.insert(names[1]);
    lc::Env env{
        [&](const std::string &rel, uint64_t &size, uint64_t &time) {
            auto f = files.find(rel);
            if (f == files.end())
                return false;
            size = f->second.first.size();
            time = f->second.second;
            return true;
        },
        [&](const std::string &rel, uint64_t &hash) {
            if (rel == names[3])
                return false;
            std::lock_guard<std::mutex> g(lock);
            hash = hash_of(files.at(rel).first, 4096);
            return true;
        },
    };
    lc::Stamps none;
    lc::Keyed want = reference_key(7, names, inputs, env, none), got = lc::key_archives(7, names, inputs, env, none);
    check(got.key == want.key && got.hashed == want.hashed && got.hashed_bytes == want.hashed_bytes && lc::text(got.stamps) == lc::text(want.stamps),
          "hashing the inputs on several threads gives the sequential key and stamps");
    lc::Keyed again = lc::key_archives(7, names, inputs, env, got.stamps), again_want = reference_key(7, names, inputs, env, want.stamps);
    check(again.key == again_want.key && again.hashed == again_want.hashed && lc::text(again.stamps) == lc::text(again_want.stamps),
          "stamps taken on several threads are reused like the sequential ones");
}

int main()
{
    many_files();
    std::string data(100000, '\0');
    for (size_t i = 0; i < data.size(); i++)
        data[i] = char((i * 2654435761u) >> 13);
    uint64_t whole = hash_of(data, data.size());
    check(hash_of(data, 1) == whole && hash_of(data, 7) == whole && hash_of(data, 33) == whole && hash_of(data, 1 << 16) == whole,
          "the content hash is the same however the bytes arrive");
    std::string flipped = data;
    flipped[54321] ^= 1;
    check(hash_of(flipped, 4096) != whole, "one changed bit changes the hash");
    check(hash_of(data + std::string(1, '\0'), 4096) != whole, "a trailing zero byte changes the hash");
    check(hash_of("", 1) != hash_of(std::string(1, '\0'), 1), "an empty file and a one-byte file differ");

    lc::Stamps st;
    st["a/b.erp"] = {10, 20, 0xabcdef, true};
    st["c.erp"] = {1, 2, 0, false};
    auto back = lc::parse(lc::text(st) + "broken line\nx\t1\t2\n\t1\t2\t-\ny\tz\t2\t-\n");
    check(back.size() == 2 && back["a/b.erp"].hash == 0xabcdef && back["a/b.erp"].hashed && !back["c.erp"].hashed &&
              back["c.erp"].size == 1 && back["c.erp"].time == 2,
          "stamps read back what they wrote and skip broken lines");

    Disk disk;
    disk.files[DATABASES] = {"database bytes", 100};
    disk.files["ui/common.erp"] = {data, 100};
    disk.files["ui/flow.erp"] = {"flow bytes", 100};
    disk.files["ui/stock.erp"] = {"stock bytes", 50};

    Boot first = boot(disk, "");
    check(first.hashed == 1 && disk.hashes == 1, "a first boot hashes only databases.erp");
    Boot keyed = built(disk, first.stamps, {"ui/common.erp"});
    check(keyed.hashed == 1 && disk.hashes == 2, "after a build only the archive it read is hashed besides databases.erp");
    check(keyed.stamps.find("ui/common.erp") != std::string::npos && keyed.stamps.find("ui/flow.erp") == std::string::npos &&
              keyed.stamps.find("ui/stock.erp") == std::string::npos,
          "the stamps name the archives the build read and no other");
    Boot same = boot(disk, keyed.stamps);
    check(same.key == keyed.key && same.hashed == 0 && disk.hashes == 2, "the next boot finds the built key without reading an archive");

    disk.files["ui/common.erp"].time = 200;
    disk.files[DATABASES].time = 200;
    Boot installed = boot(disk, same.stamps);
    check(installed.key == keyed.key && installed.hashed == 2,
          "inputs copied in again with the same bytes and a new write time keep the key (the cache is reused)");
    Boot quiet = boot(disk, installed.stamps);
    check(quiet.key == keyed.key && quiet.hashed == 0, "an input hashed under its new write time is not read again");

    disk.files["ui/flow.erp"].time = 300;
    disk.files["ui/flow.erp"].bytes = "FLOW BYTES";
    Boot recopied = boot(disk, quiet.stamps);
    check(recopied.key == keyed.key && recopied.hashed == 0,
          "an archive the build does not read keeps the key at the same size and is never read");

    disk.files["ui/stock.erp"] = {"stock bytes, patched", 400};
    Boot updated = boot(disk, recopied.stamps);
    check(updated.key != keyed.key && updated.hashed == 0, "a game update that resizes another archive changes the key without reading it");
    disk.files["ui/stock.erp"] = {"stock bytes", 50};

    disk.files["ui/common.erp"] = {flipped, 300};
    Boot edited = boot(disk, quiet.stamps);
    check(edited.key != keyed.key && edited.hashed == 1, "changed bytes of the same size in an input change the key");

    disk.files["ui/common.erp"] = {data, 400};
    Boot restored = boot(disk, edited.stamps);
    check(restored.key == keyed.key, "the earlier bytes copied back give the earlier key");

    disk.files[DATABASES] = {"database bytes!", 500};
    Boot patched = boot(disk, restored.stamps);
    check(patched.key != keyed.key && patched.hashed == 1, "a game update to databases.erp changes the key");
    disk.files[DATABASES] = {"database bytes", 200};

    disk.files["ui/common.erp"].time = 100;
    disk.files["ui/common.erp"].bytes = flipped;
    Boot sneaky = boot(disk, keyed.stamps);
    check(sneaky.key == keyed.key && sneaky.hashed == 1, "stamps are trusted while size and write time hold");

    Boot fresh = boot(disk, "");
    Boot fresh_built = built(disk, fresh.stamps, {"ui/common.erp"});
    check(fresh_built.key == edited.key && fresh.hashed + fresh_built.hashed == 2, "without stamps the inputs are read again after the build");

    Boot moved = built(disk, fresh_built.stamps, {"ui/flow.erp"});
    check(moved.key != fresh_built.key && moved.stamps.find("ui/common.erp") == std::string::npos,
          "an archive the build no longer reads leaves the stamps");
    Boot after_move = boot(disk, moved.stamps);
    check(after_move.key == moved.key && after_move.hashed == 0, "the next boot keys the archives the last build read");

    disk.files.erase("ui/stock.erp");
    Boot removed = boot(disk, fresh_built.stamps);
    check(removed.key != fresh_built.key && removed.hashed == 0, "a removed archive changes the key");

    disk.files["ui/new.erp"] = {"new bytes", 10};
    Boot added = boot(disk, removed.stamps);
    check(added.key != removed.key && added.hashed == 0, "an added archive changes the key without being read");

    std::string unhashed = removed.stamps;
    auto at = unhashed.find("ui/common.erp");
    unhashed = unhashed.substr(0, at) + "ui/common.erp\t" + std::to_string(flipped.size()) + "\t100\t-\n" +
               unhashed.substr(unhashed.find('\n', at) + 1);
    Boot retry = boot(disk, unhashed);
    check(retry.key == added.key && retry.hashed == 1, "a stamp without a hash is hashed again");

    std::printf("%s\n", failures ? "FAILED" : "all logo cache checks passed");
    return failures ? 1 : 0;
}
