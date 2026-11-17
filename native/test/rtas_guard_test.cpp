#include "rtas_guard_logic.hpp"

#include <cstdio>
#include <string>

namespace rg = runtime::rtas_guard;

int fails;

void expect(bool ok, const char *what)
{
    std::printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

rg::Geo tri(uint64_t vb, uint32_t vertices, uint64_t ib, uint32_t indices)
{
    rg::Geo g;
    g.vertices = vb;
    g.vertex_count = vertices;
    g.indices = ib;
    g.index_count = indices;
    g.indexed = indices || ib;
    return g;
}

int main()
{
    expect(rg::fault(tri(0x1000, 300, 0x2000, 900)) == rg::Fault::none, "an indexed geometry with both buffers is fine");
    expect(rg::fault(tri(0, 300, 0x2000, 900)) == rg::Fault::vertices, "a null vertex buffer with indices is a fault");
    expect(rg::fault(tri(0x1000, 300, 0, 900)) == rg::Fault::indices, "a null index buffer with indices is a fault");
    rg::Geo plain = tri(0, 300, 0, 0);
    plain.indexed = false;
    expect(rg::fault(plain) == rg::Fault::vertices, "a null vertex buffer without indices is a fault");
    rg::Geo none = tri(0, 0, 0, 0);
    none.indexed = false;
    expect(rg::fault(none) == rg::Fault::none, "a geometry with no primitives may have null buffers");
    expect(rg::fault(tri(0, 0, 0, 0)) == rg::Fault::none, "an indexed geometry with no indices may have null buffers");
    rg::Geo box;
    box.type = rg::kAabbs;
    box.aabb_count = 4;
    expect(rg::fault(box) == rg::Fault::aabbs, "a null AABB buffer with boxes is a fault");
    rg::Geo bad = tri(0, 300, 0x2000, 900);
    rg::empty(bad);
    expect(rg::fault(bad) == rg::Fault::none, "an emptied geometry has no fault");

    rg::Book book;
    std::vector<rg::Geo> tyre = {tri(0x1000, 300, 0x2000, 900), tri(0, 300, 0x3000, 900), tri(0x4000, 30, 0x5000, 90)};
    rg::Plan p = book.plan({0xa000, 0, false}, tyre);
    expect(p.faulty == 1 && p.emptied.size() == 1 && p.emptied[0] == 1, "a build empties only its faulty geometry");
    expect(book.tracked() == 1, "the structure with an emptied geometry is remembered");

    std::vector<rg::Geo> later = {tri(0x1000, 300, 0x2000, 900), tri(0x6000, 300, 0x3000, 900), tri(0x4000, 30, 0x5000, 90)};
    p = book.plan({0xa000, 0xa000, true}, later);
    expect(p.faulty == 0 && p.kept_empty == 1 && p.emptied == std::vector<uint32_t>{1},
           "an in-place update keeps the geometry its source build emptied empty");
    p = book.plan({0xb000, 0xa000, true}, later);
    expect(p.kept_empty == 1 && book.tracked() == 2, "an update into another structure carries the emptied geometry over");

    p = book.plan({0xa000, 0, false}, later);
    expect(p.emptied.empty() && book.tracked() == 1, "a full rebuild with good buffers forgets the structure");
    p = book.plan({0xc000, 0xc000, true}, later);
    expect(p.emptied.empty(), "an update of an unknown structure with good buffers changes nothing");

    rg::Call blas{rg::kBottom, 0xa000, 0xd000, 0, 0, 3, false};
    expect(rg::hole(blas) == rg::Hole::none && rg::act(blas) == rg::Act::send, "a build with every address is sent");
    rg::Call c = blas;
    c.dest = 0;
    expect(rg::hole(c) == rg::Hole::dest && rg::act(c) == rg::Act::skip, "a build with no destination is not sent");
    c = blas;
    c.scratch = 0;
    expect(rg::hole(c) == rg::Hole::scratch && rg::act(c) == rg::Act::skip, "a build with no scratch memory is not sent");
    c = blas;
    c.update = true;
    expect(rg::hole(c) == rg::Hole::source && rg::act(c) == rg::Act::skip, "an update with no source is not sent");
    c.source = 0xa000;
    expect(rg::act(c) == rg::Act::send, "an in-place update is sent");

    rg::Call tlas{rg::kTop, 0xe000, 0xd000, 0, 0x7000, 40, false};
    expect(rg::act(tlas) == rg::Act::send, "a top-level build with its instances is sent");
    tlas.instances = 0;
    expect(rg::hole(tlas) == rg::Hole::instances && rg::act(tlas) == rg::Act::no_instances,
           "a top-level build with a null instance list builds no instances");
    tlas.update = true;
    tlas.source = 0xe000;
    expect(rg::act(tlas) == rg::Act::skip, "a top-level update with a null instance list is not sent");
    tlas.count = 0;
    tlas.update = false;
    expect(rg::act(tlas) == rg::Act::send, "a top-level build of no instances needs no list");
    rg::Call bottom_zero{rg::kBottom, 0xa000, 0xd000, 0, 0, 2, false};
    expect(rg::act(bottom_zero) == rg::Act::send, "a bottom-level build never looks at the instance list");

    expect(rg::kept_posts({0x100, 0, 0x300}) == std::vector<uint32_t>({0, 2}), "postbuild info with no destination is dropped");
    expect(rg::kept_posts({}).empty(), "no postbuild info keeps nothing");
    expect(rg::detailed(8, 8) && !rg::detailed(9, 8), "the first builds are logged in detail");
    expect(rg::milestone(16) && !rg::milestone(12) && !rg::milestone(0), "later builds are counted at powers of two");

    rg::Shape one, same_counts, other;
    one.add(tri(0x1000, 300, 0x2000, 900));
    same_counts.add(tri(0x5000, 300, 0x6000, 900));
    other.add(tri(0x1000, 300, 0x2000, 903));
    expect(one.hash == same_counts.hash, "the shape is the primitive counts, not the buffer addresses");
    expect(one.hash != other.hash, "another index count is another shape");
    rg::Trace trace(2);
    expect(trace.want(one.hash), "a shape is named the first time it is built");
    expect(!trace.want(one.hash), "the same shape is not named again, in any structure");
    expect(trace.want(other.hash) && trace.full(), "the shape that reaches the cap is named");
    rg::Shape third;
    third.add(tri(0x1000, 4, 0x2000, 6));
    expect(!trace.want(third.hash) && trace.named() == 2, "nothing is named past the cap");

    rg::Entry car;
    car.list = 0x78ee;
    car.dest = 0x92c4;
    car.flags = 0x14;
    car.count = 3;
    car.geo[0] = {rg::kTriangles, 926, 5040, 0, 12, 1, 6, 57, false};
    car.geo[1] = {rg::kTriangles, 66, 192, 0, 12, 1, 6, 57, false};
    car.geo[2] = {rg::kTriangles, 40, 60, 0, 16, 1, 6, 42, false};
    expect(rg::geometry_text(car) == " v926/i5040(vf6 s12 if57 fl1) v66/i192 v40/i60(vf6 s16 if42 fl1)",
           "each geometry shows its counts, and its formats where they differ from the first");
    rg::Entry many = car;
    many.count = rg::kBriefGeos + 3;
    for (int i = 0; i < rg::kBriefGeos; i++)
        many.geo[i] = car.geo[0];
    expect(rg::geometry_text(many).find(" +3 more") != std::string::npos, "geometries past the brief are counted");

    rg::Recent<4> ring;
    expect(ring.oldest_first().empty() && rg::recent_section({}, 0).empty(), "no builds give no section");
    for (uint64_t i = 0; i < 6; i++) {
        rg::Entry e = car;
        e.seq = i;
        e.frame = i < 4 ? 10 : 11;
        e.list = i == 4 ? 0x1111 : 0x78ee;
        ring.push(e);
    }
    std::vector<rg::Entry> kept = ring.oldest_first();
    expect(kept.size() == 4 && kept.front().seq == 2 && kept.back().seq == 5 && ring.pushed() == 6,
           "the ring keeps the newest builds, oldest first");
    expect(rg::list_ordinals(kept) == std::vector<uint32_t>({1, 2, 1, 1}),
           "a build's place counts the builds on its list in its frame");
    std::string section = rg::recent_section(kept, ring.pushed());
    expect(section.find("the newest 4 of 6") != std::string::npos, "the section says how many builds it shows");
    expect(section.find("build 2 frame 10 list 0x78ee #1+ bottom-level into 0x92c4") != std::string::npos,
           "a place in the oldest frame of a full ring may miss earlier builds");
    expect(section.find("build 5 frame 11 list 0x78ee #1 bottom-level") != std::string::npos,
           "a new frame starts the count again");
    rg::Entry top;
    top.top = true;
    top.count = 40;
    top.flags = rg::kPerformUpdate;
    std::string top_line = rg::recent_section({top}, 1);
    expect(top_line.find("top-level update into 0x0 (flags 0x20), 40 instances:\n") != std::string::npos,
           "a top-level build shows its instance count and no geometries");

    expect(rg::mode(nullptr) == rg::Mode::on && rg::mode("1") == rg::Mode::on && rg::mode("ON") == rg::Mode::on,
           "the guard is on by default");
    expect(rg::mode("0") == rg::Mode::off && rg::mode("Off") == rg::Mode::off, "0 or off turns it off");
    expect(rg::mode("log") == rg::Mode::log, "log only logs");

    std::printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails ? 1 : 0;
}
