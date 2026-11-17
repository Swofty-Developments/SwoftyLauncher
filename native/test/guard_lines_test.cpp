#include "core/guards/lines.hpp"

#include <cctype>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

// Renders every SiteGuard's off, needs and settle lines from its spec in the guard sources, through the same
// functions the guards log with (core/guards/lines.hpp), and compares them with guard_lines/expected.txt: the lines
// the guards logged before the registry (guard_lines/extract.py writes it from the starting commit's sources).

namespace lines = swofty_core::guards::lines;

struct Expected {
    std::string off, unresolved, armed, not_installed;
};

struct Spec {
    std::string file, name, env, armed;
    std::vector<std::string> needs;
};

int failures = 0;

void expect(bool ok, const std::string &what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what.c_str());
        failures++;
    }
}

std::string read(const std::filesystem::path &p)
{
    std::ifstream in(p, std::ios::binary);
    std::stringstream s;
    s << in.rdbuf();
    return s.str();
}

size_t skip_space(const std::string &t, size_t at)
{
    while (at < t.size()) {
        if (std::isspace((unsigned char)t[at]))
            at++;
        else if (t.compare(at, 2, "//") == 0)
            at = t.find('\n', at);
        else
            break;
    }
    return at;
}

// Adjacent string literals from `at`, concatenated; `at` moves past them. Empty `ok` when `at` holds none.
std::string literals(const std::string &t, size_t &at, bool &ok)
{
    std::string out;
    ok = false;
    for (at = skip_space(t, at); at < t.size() && t[at] == '"'; at = skip_space(t, at)) {
        ok = true;
        for (at++; at < t.size() && t[at] != '"'; at++) {
            if (t[at] == '\\' && at + 1 < t.size()) {
                char c = t[++at];
                out += c == 'n' ? '\n' : c == 't' ? '\t' : c;
            } else {
                out += t[at];
            }
        }
        at++;
    }
    return out;
}

std::map<std::string, std::string> constants(const std::string &t)
{
    std::map<std::string, std::string> out;
    const std::string mark = "constexpr const char *";
    for (size_t at = t.find(mark); at != std::string::npos; at = t.find(mark, at + 1)) {
        size_t name = at + mark.size(), eq = t.find(" = ", name);
        if (eq == std::string::npos)
            continue;
        size_t v = eq + 3;
        bool ok;
        std::string value = literals(t, v, ok);
        if (ok)
            out[t.substr(name, eq - name)] = value;
    }
    return out;
}

std::string field(const std::string &block, const char *name, bool &ok)
{
    size_t at = block.find(std::string(".") + name + " = ");
    ok = false;
    if (at == std::string::npos)
        return {};
    at += std::string(name).size() + 4;
    return literals(block, at, ok);
}

std::vector<std::string> needs(const std::string &block, const std::map<std::string, std::string> &consts, bool &ok)
{
    std::vector<std::string> out;
    size_t at = block.find(".needs = {");
    ok = at != std::string::npos;
    if (!ok)
        return out;
    at += 10;
    size_t end = block.find('}', at);
    while (at < end) {
        at = skip_space(block, at);
        if (block[at] == '"') {
            bool lit;
            out.push_back(literals(block, at, lit));
        } else {
            size_t from = at;
            while (at < end && (std::isalnum((unsigned char)block[at]) || block[at] == '_'))
                at++;
            auto c = consts.find(block.substr(from, at - from));
            if (c == consts.end()) {
                ok = false;
                return out;
            }
            out.push_back(c->second);
        }
        at = skip_space(block, at);
        if (at < end && block[at] == ',')
            at++;
    }
    return out;
}

// The SiteGuard specs in one source: each `CoreSetId::<stem>` followed by a spec holding `.set = `.
void specs_in(const std::filesystem::path &path, std::map<std::string, Spec> &out)
{
    std::string t = read(path);
    auto consts = constants(t);
    const std::string mark = "CoreSetId::";
    for (size_t at = t.find(mark); at != std::string::npos; at = t.find(mark, at + 1)) {
        size_t stem_end = at + mark.size();
        while (stem_end < t.size() && (std::isalnum((unsigned char)t[stem_end]) || t[stem_end] == '_'))
            stem_end++;
        std::string stem = t.substr(at + mark.size(), stem_end - at - mark.size());
        size_t set = t.find(".set = ", stem_end), close = t.find("}};", stem_end), paren = t.find("})\n", stem_end);
        size_t end = std::min(close, paren);
        if (set == std::string::npos || set > end)
            continue;
        std::string block = t.substr(stem_end, end - stem_end);
        Spec s;
        s.file = path.string();
        bool a, b, c, d;
        s.name = field(block, "name", a);
        s.env = field(block, "env", b);
        s.armed = field(block, "armed", c);
        s.needs = needs(block, consts, d);
        expect(a && b && c && d, stem + ": its spec in " + s.file + " parses");
        out[stem] = s;
    }
}

std::map<std::string, Expected> expected_lines(const std::filesystem::path &path)
{
    std::map<std::string, Expected> out;
    std::ifstream in(path);
    std::string line, stem;
    while (std::getline(in, line)) {
        if (line.size() > 2 && line.front() == '[' && line.back() == ']') {
            stem = line.substr(1, line.size() - 2);
            continue;
        }
        size_t space = line.find(' ');
        if (stem.empty() || stem.rfind("hook ", 0) == 0 || space == std::string::npos)
            continue;
        std::string kind = line.substr(0, space), text = line.substr(space + 1);
        Expected &e = out[stem];
        (kind == "off" ? e.off : kind == "unresolved" ? e.unresolved : kind == "armed" ? e.armed : e.not_installed) = text;
    }
    return out;
}

int main(int argc, char **argv)
{
    std::filesystem::path guards = argc > 1 ? argv[1] : "../src/core/guards";
    std::filesystem::path expected_path = argc > 2 ? argv[2] : "guard_lines/expected.txt";
    auto expected = expected_lines(expected_path);
    std::map<std::string, Spec> specs;
    for (auto &entry : std::filesystem::recursive_directory_iterator(guards))
        if (entry.path().extension() == ".cpp")
            specs_in(entry.path(), specs);

    expect(!expected.empty(), "expected.txt lists guards");
    for (auto &[stem, e] : expected) {
        auto it = specs.find(stem);
        expect(it != specs.end(), stem + " is a SiteGuard in the guard sources");
        if (it == specs.end())
            continue;
        const Spec &s = it->second;
        std::vector<const char *> need;
        for (auto &n : s.needs)
            need.push_back(n.c_str());
        auto check = [&](const std::string &got, const std::string &want, const char *which) {
            expect(got == want, stem + " " + which + " line\n  got:  " + got + "\n  want: " + want);
        };
        check(lines::off(s.name, s.env, "%s"), e.off, "off");
        check(lines::unresolved(s.name, need), e.unresolved, "unresolved");
        check(lines::armed(s.name, s.armed), e.armed, "armed");
        check(lines::not_installed(s.name), e.not_installed, "not installed");
    }
    for (auto &[stem, s] : specs)
        expect(expected.count(stem), stem + " (" + s.file + ") has its starting lines in expected.txt");

    if (failures)
        return 1;
    std::printf("guard lines: %zu SiteGuards log the lines they logged before the registry\n", expected.size());
    return 0;
}
