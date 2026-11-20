#include "core/guards/lines.hpp"

#include <cctype>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

// Renders every SiteGuard's off, needs and settle lines and every MidHookSpec guard's off, hook failure and armed lines
// from its spec in the guard sources, through the same functions the guards log with (core/guards/lines.hpp), and
// compares them with guard_lines/expected.txt: the lines the guards logged before they became specs
// (guard_lines/extract.py writes it from the sources before each change), and guard_lines/added.txt: the lines of the
// guards written as specs from the start, by hand. Every guard source's own swofty::log formats, in source order, are
// pinned in guard_lines/logged.txt.

namespace lines = swofty_core::guards::lines;

struct ExpectedPart {
    std::string unresolved, armed, not_installed;
};

struct Expected {
    std::string off, unresolved, armed, not_installed, failed;
    std::vector<ExpectedPart> parts;
};

struct Part {
    std::string name, armed;
    std::vector<std::string> needs;
};

struct Spec {
    std::string file, name, env, armed, failed_at;
    std::vector<std::string> needs;
    bool names_site = false;
    std::vector<Part> parts;
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
    const std::string sig = "const runtime::Sig ";
    for (size_t at = t.find(sig); at != std::string::npos; at = t.find(sig, at + 1)) {
        size_t name = at + sig.size(), brace = t.find('{', name);
        if (brace == std::string::npos)
            continue;
        size_t v = brace + 1;
        bool ok;
        std::string value = literals(t, v, ok);
        if (ok)
            out[t.substr(name, brace - name) + ".what"] = value;
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

// A field holding adjacent string literals or a named constant (a `constexpr const char *` or a Sig's `.what`).
std::string text_field(const std::string &block, const char *name, const std::map<std::string, std::string> &consts, bool &ok)
{
    std::string out = field(block, name, ok);
    if (ok)
        return out;
    size_t at = block.find(std::string(".") + name + " = ");
    if (at == std::string::npos)
        return {};
    at += std::string(name).size() + 4;
    size_t end = block.find(',', at);
    auto c = consts.find(block.substr(at, end - at));
    ok = c != consts.end();
    return ok ? c->second : std::string();
}

// The MidHookSpec guards in one source: each `SWOFTY_MID_HOOK_GUARD(<stem>, <what>, <spec>)` and the spec it names.
void hook_specs_in(const std::filesystem::path &path, std::map<std::string, Spec> &out)
{
    std::string t = read(path);
    auto consts = constants(t);
    const std::string mark = "SWOFTY_MID_HOOK_GUARD(";
    for (size_t at = t.find(mark); at != std::string::npos; at = t.find(mark, at + 1)) {
        size_t comma = t.find(',', at), close = t.find(')', at), last = t.rfind(',', close);
        std::string stem = t.substr(at + mark.size(), comma - at - mark.size());
        std::string var = t.substr(skip_space(t, last + 1), close - skip_space(t, last + 1));
        size_t from = t.find("MidHookSpec " + var + "{"), to = t.find("\n};", from);
        expect(from != std::string::npos && to != std::string::npos, stem + ": its spec " + var + " is in " + path.string());
        if (from == std::string::npos || to == std::string::npos)
            continue;
        std::string block = t.substr(from, to - from);
        Spec s;
        s.file = path.string();
        bool a, b, c, d = true;
        s.name = field(block, "name", a);
        s.env = field(block, "env", b);
        s.armed = field(block, "armed", c);
        s.names_site = block.find(".failed_at = ") != std::string::npos;
        if (s.names_site)
            s.failed_at = text_field(block, "failed_at", consts, d);
        expect(a && b && c && d, stem + ": its spec in " + s.file + " parses");
        out[stem] = s;
    }
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
        size_t parts_at = block.find(".parts = ");
        std::string own = block.substr(0, parts_at);
        Spec s;
        s.file = path.string();
        bool a, b, c, d;
        s.name = field(own, "name", a);
        s.env = field(own, "env", b);
        s.armed = field(own, "armed", c);
        s.needs = needs(own, consts, d);
        expect(a && b && c && d, stem + ": its spec in " + s.file + " parses");
        for (size_t p = block.find(".set = ", parts_at); parts_at != std::string::npos && p != std::string::npos;) {
            size_t next = block.find(".set = ", p + 1);
            std::string text = block.substr(p, next == std::string::npos ? std::string::npos : next - p);
            Part part;
            bool e, f, g;
            part.name = field(text, "name", e);
            part.armed = field(text, "armed", f);
            part.needs = needs(text, consts, g);
            expect(e && f && g, stem + ": part " + std::to_string(s.parts.size()) + " of its spec parses");
            s.parts.push_back(part);
            p = next;
        }
        out[stem] = s;
    }
}

// The `[<stem>]` sections of expected.txt, or with `hooks` its `[hook <stem>]` sections.
std::map<std::string, Expected> expected_lines(const std::filesystem::path &path, bool hooks)
{
    std::map<std::string, Expected> out;
    std::ifstream in(path);
    std::string line, stem;
    while (std::getline(in, line)) {
        if (line.empty() || line.front() == '#')
            continue;
        if (line.size() > 2 && line.front() == '[' && line.back() == ']') {
            stem = line.substr(1, line.size() - 2);
            continue;
        }
        size_t space = line.find(' ');
        if (stem.empty() || (stem.rfind("hook ", 0) == 0) != hooks || space == std::string::npos)
            continue;
        std::string kind = line.substr(0, space), text = line.substr(space + 1);
        Expected &e = out[hooks ? stem.substr(5) : stem];
        if (kind == "part_unresolved")
            e.parts.push_back({text, {}, {}});
        if (kind.rfind("part_", 0) == 0) {
            if (!e.parts.empty())
                (kind == "part_armed"           ? e.parts.back().armed
                 : kind == "part_not_installed" ? e.parts.back().not_installed
                                                : e.parts.back().unresolved) = text;
            continue;
        }
        (kind == "off"          ? e.off
         : kind == "unresolved" ? e.unresolved
         : kind == "armed"      ? e.armed
         : kind == "failed"     ? e.failed
                                : e.not_installed) = text;
    }
    return out;
}

// Every swofty::log call's format in one source, in source order: its adjacent string literals, or "(expr) <text>" for
// a format that is not a literal. A newline or tab in a literal reads as \n or \t, as logged.txt holds it.
std::vector<std::string> log_formats(const std::string &t)
{
    std::vector<std::string> out;
    const std::string mark = "swofty::log(";
    for (size_t at = t.find(mark); at != std::string::npos; at = t.find(mark, at + 1)) {
        size_t arg = at + mark.size();
        bool ok;
        std::string text = literals(t, arg, ok), shown;
        if (!ok) {
            size_t from = skip_space(t, at + mark.size()), end = t.find_first_of(",)", from);
            text = "(expr) " + t.substr(from, end - from);
        }
        for (char c : text)
            shown += c == '\n' ? std::string("\\n") : c == '\t' ? std::string("\\t") : std::string(1, c);
        out.push_back(shown);
    }
    return out;
}

// The `[log <source>]` sections of logged.txt, each source named relative to the guards folder.
std::map<std::string, std::vector<std::string>> logged_lines(const std::filesystem::path &path)
{
    std::map<std::string, std::vector<std::string>> out;
    std::ifstream in(path);
    std::string line, source;
    while (std::getline(in, line)) {
        if (line.empty() || line.front() == '#')
            continue;
        if (line.rfind("[log ", 0) == 0 && line.back() == ']') {
            source = line.substr(5, line.size() - 6);
            expect(!out.count(source), source + " has one section in logged.txt");
            out[source];
            continue;
        }
        expect(!source.empty(), "logged.txt line outside a section: " + line);
        if (!source.empty())
            out[source].push_back(line);
    }
    return out;
}

int main(int argc, char **argv)
{
    std::filesystem::path guards = argc > 1 ? argv[1] : "../src/core/guards";
    std::filesystem::path expected_path = argc > 2 ? argv[2] : "guard_lines/expected.txt";
    std::filesystem::path added_path = argc > 3 ? argv[3] : "guard_lines/added.txt";
    std::filesystem::path logged_path = argc > 4 ? argv[4] : "guard_lines/logged.txt";
    auto expected = expected_lines(expected_path, false);
    auto expected_hooks = expected_lines(expected_path, true);
    for (bool hooks : {false, true})
        for (auto &[stem, e] : expected_lines(added_path, hooks)) {
            auto &into = hooks ? expected_hooks : expected;
            expect(!into.count(stem), stem + " is pinned in one of expected.txt and added.txt, not both");
            into[stem] = e;
        }
    std::map<std::string, Spec> specs, hook_specs;
    std::map<std::string, std::vector<std::string>> logs;
    for (auto &entry : std::filesystem::recursive_directory_iterator(guards))
        if (entry.path().extension() == ".cpp") {
            specs_in(entry.path(), specs);
            hook_specs_in(entry.path(), hook_specs);
            auto formats = log_formats(read(entry.path()));
            if (!formats.empty())
                logs[entry.path().lexically_relative(guards).generic_string()] = formats;
        }

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
        expect(s.parts.size() == e.parts.size(), stem + " offers " + std::to_string(e.parts.size()) + " further sets");
        for (size_t i = 0; i < s.parts.size() && i < e.parts.size(); i++) {
            const Part &p = s.parts[i];
            std::vector<const char *> part_need;
            for (auto &n : p.needs)
                part_need.push_back(n.c_str());
            check(lines::unresolved(p.name, part_need), e.parts[i].unresolved, "part unresolved");
            check(lines::armed(p.name, p.armed), e.parts[i].armed, "part armed");
            check(lines::not_installed(p.name), e.parts[i].not_installed, "part not installed");
        }
    }
    for (auto &[stem, s] : specs)
        expect(expected.count(stem), stem + " (" + s.file + ") has its lines in expected.txt or added.txt");

    expect(!expected_hooks.empty(), "expected.txt lists hook guards");
    for (auto &[stem, e] : expected_hooks) {
        auto it = hook_specs.find(stem);
        expect(it != hook_specs.end(), stem + " is a MidHookSpec guard in the guard sources");
        if (it == hook_specs.end())
            continue;
        const Spec &s = it->second;
        auto check = [&](const std::string &got, const std::string &want, const char *which) {
            expect(got == want, "hook " + stem + " " + which + " line\n  got:  " + got + "\n  want: " + want);
        };
        check(lines::off(s.name, s.env, "%s"), e.off, "off");
        check(lines::hook_failed(s.name, s.names_site ? s.failed_at.c_str() : nullptr, "%s"), e.failed, "hook failure");
        check(lines::armed(s.name, s.armed), e.armed, "armed");
    }
    for (auto &[stem, s] : hook_specs)
        expect(expected_hooks.count(stem), stem + " (" + s.file + ") has its lines in expected.txt or added.txt");

    auto logged = logged_lines(logged_path);
    for (auto &[source, want] : logged) {
        auto it = logs.find(source);
        std::vector<std::string> got = it == logs.end() ? std::vector<std::string>() : it->second;
        if (got == want)
            continue;
        std::string shown = "[log " + source + "]";
        for (auto &f : got)
            shown += "\n  " + f;
        expect(false, source + " logs other formats than logged.txt pins; it logs:\n  " + shown);
    }
    for (auto &[source, formats] : logs)
        expect(logged.count(source), source + " has its " + std::to_string(formats.size()) + " log formats in logged.txt");

    if (failures)
        return 1;
    std::printf("guard lines: %zu SiteGuards and %zu MidHookSpec guards log the lines pinned for them; %zu guard sources "
                "log the formats pinned for them\n",
                expected.size(), expected_hooks.size(), logged.size());
    return 0;
}
