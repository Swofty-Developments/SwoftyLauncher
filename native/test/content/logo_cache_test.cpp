#include "core/apply/content/logo_cache.hpp"

#include <algorithm>
#include <cstdlib>
#include <cstdio>
#include <cstring>
#include <functional>
#include <map>
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

static const uint64_t SECOND = 10000000;

struct Disk {
    struct File {
        std::string bytes;
        uint64_t time = 0, changed = 0, id = 0;
        uint32_t links = 1;
    };
    std::map<std::string, File> files;
    uint64_t next_id = 1;
    uint64_t now = 1000 * SECOND;
    int hashes = 0;
    std::function<void(const std::string &rel)> while_hashing;

    void put(const std::string &rel, const std::string &bytes, uint64_t time) { files[rel] = {bytes, time, time, next_id++, 1}; }

    void write(const std::string &rel, const std::string &bytes, uint64_t time)
    {
        files[rel].bytes = bytes;
        files[rel].time = time;
        files[rel].changed = time;
    }

    lc::Env env()
    {
        return {
            [this](const std::string &rel, uint64_t &size) {
                auto f = files.find(rel);
                if (f == files.end())
                    return false;
                size = f->second.bytes.size();
                return true;
            },
            [this](const std::string &rel, lc::Seen &seen) {
                auto f = files.find(rel);
                if (f == files.end())
                    return false;
                seen.size = f->second.bytes.size();
                seen.time = f->second.time;
                seen.changed = f->second.changed;
                seen.id = f->second.id;
                seen.links = f->second.links;
                return true;
            },
            [this](const std::string &rel, uint64_t &hash) {
                auto f = files.find(rel);
                if (f == files.end())
                    return false;
                hashes++;
                hash = hash_of(f->second.bytes, 4096);
                if (while_hashing)
                    while_hashing(rel);
                return true;
            },
            [this] { return now; },
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

static std::string old_format(const std::string &stamps)
{
    std::string out;
    for (auto &[rel, s] : lc::parse(stamps)) {
        char hex[17];
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(s.hash));
        out += rel + "\t" + std::to_string(s.size) + "\t" + std::to_string(s.time) + "\t" + (s.hashed ? hex : "-") + "\n";
    }
    return "#format\t2\n" + out;
}

int main()
{
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
    st["a/b.erp"].size = 10;
    st["a/b.erp"].time = 20;
    st["a/b.erp"].changed = 21;
    st["a/b.erp"].id = 0x123456789ull;
    st["a/b.erp"].links = 2;
    st["a/b.erp"].hash = 0xabcdef;
    st["a/b.erp"].hashed = true;
    st["c.erp"].size = 1;
    st["c.erp"].time = 2;
    auto back =
        lc::parse(lc::text(st) +
                  "broken line\nx\t1\t2\n\t1\t2\t-\ny\tz\t2\t-\nw\t1\t2\t3\t4\t-\t-\nv\t1\t2\t3\t4\t5\t6\t-\nu\t-1\t2\t-\nt\t "
                  "1\t2\t-\ns\t1\t2\t99999999999999999999\t4\t1\t-\n");
    check(back.size() == 2 && back["a/b.erp"].hash == 0xabcdef && back["a/b.erp"].hashed && back["a/b.erp"].changed == 21 &&
              back["a/b.erp"].id == 0x123456789ull && back["a/b.erp"].links == 2 && !back["c.erp"].hashed &&
              back["c.erp"].size == 1 && back["c.erp"].time == 2,
          "stamps read back what they wrote and skip broken lines");
    check(lc::text(st).compare(0, std::strlen(lc::STAMPS_FORMAT) + 1, std::string(lc::STAMPS_FORMAT) + "\n") == 0,
          "stamps name their format on the first line");

    Disk disk;
    disk.put(DATABASES, "database bytes", 100 * SECOND);
    disk.put("ui/common.erp", data, 100 * SECOND);
    disk.put("ui/flow.erp", "flow bytes", 100 * SECOND);
    disk.put("ui/stock.erp", "stock bytes", 50 * SECOND);

    Boot first = boot(disk, "");
    check(first.hashed == 1 && disk.hashes == 1, "a first boot hashes only databases.erp");
    Boot keyed = built(disk, first.stamps, {"ui/common.erp"});
    check(keyed.hashed == 1 && disk.hashes == 2, "after a build only the archive it read is hashed besides databases.erp");
    check(keyed.stamps.find("ui/common.erp") != std::string::npos && keyed.stamps.find("ui/flow.erp") == std::string::npos &&
              keyed.stamps.find("ui/stock.erp") == std::string::npos,
          "the stamps name the archives the build read and no other");
    Boot same = boot(disk, keyed.stamps);
    check(same.key == keyed.key && same.hashed == 0 && disk.hashes == 2,
          "the next boot finds the built key without reading an archive");

    disk.write("ui/common.erp", data, 200 * SECOND);
    disk.write(DATABASES, "database bytes", 200 * SECOND);
    Boot installed = boot(disk, same.stamps);
    check(installed.key == keyed.key && installed.hashed == 2,
          "inputs written again with the same bytes and a new write time keep the key (the cache is reused)");
    Boot quiet = boot(disk, installed.stamps);
    check(quiet.key == keyed.key && quiet.hashed == 0, "an input hashed under its new write time is not read again");

    disk.files["ui/common.erp"].links = 2;
    disk.files["ui/common.erp"].changed = 300 * SECOND;
    Boot linked = boot(disk, quiet.stamps);
    check(linked.key == keyed.key && linked.hashed == 0,
          "linking an input into the game folder again moves only its change time and it is not read");
    disk.files["ui/common.erp"].links = 1;
    disk.files["ui/common.erp"].changed = 310 * SECOND;
    Boot unlinked = boot(disk, linked.stamps);
    check(unlinked.key == keyed.key && unlinked.hashed == 0, "taking that link away again is not a reason to read it");

    disk.files["ui/common.erp"].bytes = flipped;
    disk.files["ui/common.erp"].changed = 320 * SECOND;
    Boot in_place = boot(disk, unlinked.stamps);
    check(in_place.key != keyed.key && in_place.hashed == 1,
          "bytes changed in place with the write time put back are read again, by their change time");

    uint64_t time = disk.files["ui/common.erp"].time;
    disk.put("ui/common.erp", data, time);
    Boot copied_back = boot(disk, in_place.stamps);
    check(copied_back.key == keyed.key && copied_back.hashed == 1,
          "a copy put back with the old size and write time is read again");
    disk.put("ui/common.erp", flipped, time);
    disk.files["ui/common.erp"].changed = disk.files["ui/common.erp"].time;
    Boot restored = boot(disk, copied_back.stamps);
    check(restored.key == in_place.key && restored.hashed == 1,
          "a restored copy with every time of the hashed one but another file id is read again");
    disk.put("ui/common.erp", data, 200 * SECOND);
    Boot back_again = boot(disk, restored.stamps);
    check(back_again.key == keyed.key && back_again.hashed == 1, "the earlier bytes copied back give the earlier key");

    disk.files["ui/common.erp"].links = 2;
    disk.files["ui/common.erp"].bytes = flipped;
    disk.files["ui/common.erp"].changed = 330 * SECOND;
    Boot linked_sneaky = boot(disk, back_again.stamps);
    check(linked_sneaky.key == keyed.key && linked_sneaky.hashed == 0,
          "a linked input is trusted while its id, size and write time hold, as the launcher's cache does");
    disk.files["ui/common.erp"].bytes = data;
    disk.files["ui/common.erp"].links = 1;
    disk.files["ui/common.erp"].changed = 200 * SECOND;

    disk.files["ui/flow.erp"].time = 300 * SECOND;
    disk.files["ui/flow.erp"].bytes = "FLOW BYTES";
    Boot recopied = boot(disk, back_again.stamps);
    check(recopied.key == keyed.key && recopied.hashed == 0,
          "an archive the build does not read keeps the key at the same size and is never read");

    disk.put("ui/stock.erp", "stock bytes, patched", 400 * SECOND);
    Boot updated = boot(disk, recopied.stamps);
    check(updated.key != keyed.key && updated.hashed == 0,
          "a game update that resizes another archive changes the key without reading it");
    disk.put("ui/stock.erp", "stock bytes", 50 * SECOND);

    disk.write(DATABASES, "database bytes!", 500 * SECOND);
    Boot patched = boot(disk, recopied.stamps);
    check(patched.key != keyed.key && patched.hashed == 1, "a game update to databases.erp changes the key");
    disk.write(DATABASES, "database bytes", 510 * SECOND);
    Boot unpatched = boot(disk, patched.stamps);
    check(unpatched.key == keyed.key && unpatched.hashed == 1, "its old bytes give the old key");

    disk.write("ui/common.erp", data, disk.now - SECOND);
    Boot fresh_write = boot(disk, unpatched.stamps);
    check(fresh_write.key == keyed.key && fresh_write.hashed == 1, "an input written a second ago is hashed");
    check(fresh_write.stamps.find("ui/common.erp\t" + std::to_string(data.size())) != std::string::npos &&
              lc::parse(fresh_write.stamps)["ui/common.erp"].hashed == false,
          "its hash is not kept while the write is under two seconds old");
    Boot unsettled = boot(disk, fresh_write.stamps);
    check(unsettled.key == keyed.key && unsettled.hashed == 1, "so the next boot reads it again");
    disk.now += 2 * SECOND;
    Boot settled_now = boot(disk, unsettled.stamps);
    Boot settled_quiet = boot(disk, settled_now.stamps);
    check(settled_now.hashed == 1 && settled_quiet.key == keyed.key && settled_quiet.hashed == 0,
          "once the write is two seconds old its hash is kept");
    disk.files["ui/common.erp"].links = 2;
    disk.files["ui/common.erp"].changed = disk.now;
    Boot just_linked = boot(disk, settled_quiet.stamps);
    check(just_linked.hashed == 0, "a link made a moment ago does not unsettle a hash already kept");
    disk.files["ui/common.erp"].links = 1;

    disk.write("ui/common.erp", data, 600 * SECOND);
    disk.while_hashing = [&](const std::string &rel) {
        if (rel == "ui/common.erp")
            disk.files[rel].changed += 1;
    };
    Boot moving = boot(disk, settled_quiet.stamps);
    disk.while_hashing = nullptr;
    check(moving.hashed == 1 && !lc::parse(moving.stamps)["ui/common.erp"].hashed,
          "a hash taken while the file changed is not kept");
    Boot after_moving = boot(disk, moving.stamps);
    check(after_moving.key == keyed.key && after_moving.hashed == 1, "and the file is read again the next boot");

    disk.files["ui/common.erp"].changed = 0;
    Boot no_change_time = boot(disk, after_moving.stamps);
    Boot no_change_again = boot(disk, no_change_time.stamps);
    check(no_change_time.key == keyed.key && no_change_again.hashed == 1, "a file system with no change time keeps no hash");
    disk.write("ui/common.erp", data, 700 * SECOND);

    Boot settle = boot(disk, no_change_again.stamps);
    Boot fresh = boot(disk, "");
    Boot fresh_built = built(disk, fresh.stamps, {"ui/common.erp"});
    check(fresh_built.key == keyed.key && fresh.hashed + fresh_built.hashed == 2 && settle.key == keyed.key,
          "without stamps the inputs are read again after the build");

    Boot old = boot(disk, old_format(fresh_built.stamps));
    check(old.key == fresh_built.key && old.hashed == 2,
          "stamps from before the change time was kept keep their inputs and are hashed again");
    Boot old_after = boot(disk, old.stamps);
    check(old_after.key == fresh_built.key && old_after.hashed == 0, "and are kept from then on");

    Boot moved = built(disk, fresh_built.stamps, {"ui/flow.erp"});
    check(moved.key != fresh_built.key && moved.stamps.find("ui/common.erp") == std::string::npos,
          "an archive the build no longer reads leaves the stamps");
    Boot after_move = boot(disk, moved.stamps);
    check(after_move.key == moved.key && after_move.hashed == 0, "the next boot keys the archives the last build read");

    disk.files.erase("ui/stock.erp");
    Boot removed = boot(disk, fresh_built.stamps);
    check(removed.key != fresh_built.key && removed.hashed == 0, "a removed archive changes the key");

    disk.put("ui/new.erp", "new bytes", 10 * SECOND);
    Boot added = boot(disk, removed.stamps);
    check(added.key != removed.key && added.hashed == 0, "an added archive changes the key without being read");

    auto stamps = lc::parse(removed.stamps);
    stamps["ui/common.erp"].hashed = false;
    Boot retry = boot(disk, lc::text(stamps));
    check(retry.key == added.key && retry.hashed == 1, "a stamp without a hash is hashed again");

    Disk again;
    again.put(DATABASES, "database bytes", 100 * SECOND);
    again.put("ui/common.erp", data, 100 * SECOND);
    again.put("ui/flow.erp", "flow bytes", 100 * SECOND);
    again.put("ui/stock.erp", "stock bytes", 50 * SECOND);
    Boot s1 = boot(again, "");
    Boot s1b = built(again, s1.stamps, {"ui/common.erp", "ui/flow.erp"});
    for (auto &[rel, f] : again.files) {
        f.time += 100 * SECOND;
        f.changed = f.time;
    }
    std::vector<std::string> shuffled = again.list();
    std::reverse(shuffled.begin(), shuffled.end());
    auto before = lc::parse(s1b.stamps);
    auto inputs = lc::inputs_of(before);
    inputs.insert(DATABASES);
    auto s2 = lc::key_archives(7, shuffled, inputs, again.env(), before);
    check(s2.key == s1b.key,
          "a second session with every write time changed and the list in another order gets the first session's key");
    check(s2.hashed == 3, "the second session hashes only the three inputs");
    auto s3 = lc::key_archives(7, shuffled, inputs, again.env(), s2.stamps);
    check(s3.key == s1b.key && s3.hashed == 0, "a third session reads nothing");
    auto dup = shuffled;
    dup.insert(dup.end(), shuffled.begin(), shuffled.end());
    check(lc::key_archives(7, dup, inputs, again.env(), s2.stamps).key == s1b.key, "a listed archive counts once");

    {
        lc::Routes routes = {{"ui/a.erp", "logos_x_a.erp"}, {"ui/b.erp", "logos_x_b.erp"}};
        std::set<std::string> present = {"logos_x_a.erp", "logos_x_b.erp"};
        auto exists = [&](const std::string &f) { return present.count(f) > 0; };
        int built = 0;
        lc::Routes back;
        check(lc::manifest_parse(lc::manifest_text(84, routes), exists, built, back) && built == 84 && back == routes,
              "a manifest of a clean build comes back whole with its built count");
        present.erase("logos_x_b.erp");
        check(!lc::manifest_parse(lc::manifest_text(84, routes), exists, built, back) && back.empty(),
              "a manifest naming a missing archive is not reused");
        present.insert("logos_x_b.erp");
        check(!lc::manifest_parse("ui/a.erp\tlogos_x_a.erp\nui/b.erp\tlogos_x_b.erp\n", exists, built, back) && back.empty(),
              "a manifest written before the built count was kept is not reused");
        check(!lc::manifest_parse(lc::manifest_text(0, {}), exists, built, back), "a manifest with no archives is not reused");
        lc::Sources sources = {{"ui/a.erp", 0x0123456789abcdefull}}, sources_back;
        check(lc::manifest_parse(lc::manifest_text(84, routes, sources), exists, built, back, &sources_back) && built == 84 &&
                  back == routes && sources_back == sources,
              "a manifest keeps the hash each copy was made from, apart from its routes");
        check(lc::manifest_parse(lc::manifest_text(84, routes, sources), exists, built, back) && back == routes,
              "a reader that does not ask for the hashes still gets the routes alone");
        check(lc::manifest_parse(lc::manifest_text(84, routes), exists, built, back, &sources_back) && sources_back.empty(),
              "a manifest with no hashes reads back none");
        check(lc::cacheable(2, 2, 0), "a build that wrote every archive and refused nothing is cached");
        check(!lc::cacheable(2, 3, 0), "a build that lost an archive is not cached");
        check(!lc::cacheable(2, 2, 1), "a build with a refused slot is not cached");
        check(!lc::cacheable(0, 0, 0), "a build with no archive is not cached");
    }

    std::printf("%s\n", failures ? "FAILED" : "all logo cache checks passed");
    return failures ? 1 : 0;
}
