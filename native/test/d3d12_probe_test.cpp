#include "core/render/probe_logic.hpp"

#include <cstdio>

int fails;

void expect(bool ok, const char *what)
{
    std::printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

struct FakeDevice {
    int refs = 1;
    bool *freed = nullptr;

    void AddRef() { ++refs; }
    void Release()
    {
        if (--refs == 0 && freed)
            *freed = true;
    }
};

struct Factory {
    FakeDevice devices[8];
    bool freed[8] = {};
    int made = 0;
    bool fail = false;

    FakeDevice *make()
    {
        if (fail || made == 8)
            return nullptr;
        FakeDevice *d = &devices[made];
        d->freed = &freed[made];
        made++;
        return d;
    }
};

void release(FakeDevice *&d)
{
    if (d)
        d->Release();
    d = nullptr;
}

int main()
{
    using Hold = render::d3d12::ProbeHold<FakeDevice>;

    {
        Factory f;
        Hold hold;
        auto make = [&] { return f.make(); };
        hold.open();
        hold.open();
        FakeDevice *overlay = hold.get(make);
        release(overlay);
        hold.close();
        expect(f.made == 1 && hold.holding() && !f.freed[0], "an inner scope closing keeps the outer scope's device");
        hold.open();
        FakeDevice *postfx = hold.get(make);
        release(postfx);
        hold.close();
        FakeDevice *grade = hold.get(make);
        release(grade);
        FakeDevice *srv = hold.get(make);
        FakeDevice *rtas = hold.get(make);
        expect(srv == rtas && srv == &f.devices[0], "every probe in the window gets the same device");
        release(srv);
        release(rtas);
        expect(f.made == 1, "five probes in one window make one device");
        expect(!f.freed[0] && f.devices[0].refs == 1, "between probes only the holder's reference is left");
        bool released = hold.close();
        expect(released && f.freed[0] && !hold.holding() && hold.scopes() == 0,
               "the last scope closing releases the device and frees it");
        FakeDevice *late = hold.get(make);
        expect(f.made == 2 && late == &f.devices[1] && !hold.holding(), "a probe after the window makes its own device");
        release(late);
        expect(f.freed[1], "the late probe's own release frees its device");
    }

    {
        Factory f;
        Hold hold;
        hold.open();
        expect(!hold.close() && f.made == 0, "a window with no probe makes and releases nothing");
    }

    {
        Factory f;
        Hold hold;
        auto make = [&] { return f.make(); };
        f.fail = true;
        hold.open();
        expect(hold.get(make) == nullptr && !hold.holding(), "a failed creation holds nothing");
        f.fail = false;
        FakeDevice *next = hold.get(make);
        expect(next == &f.devices[0] && hold.holding(), "the next probe in the window tries again and is held");
        release(next);
        expect(hold.close() && f.freed[0], "and is released when the window closes");
    }

    {
        Factory f;
        Hold hold;
        auto make = [&] { return f.make(); };
        FakeDevice *a = hold.get(make);
        FakeDevice *b = hold.get(make);
        expect(f.made == 2 && a != b && !hold.holding(), "without a scope every probe makes its own device");
        release(a);
        release(b);
        expect(f.freed[0] && f.freed[1], "and each caller's release frees its own");
    }

    std::printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails ? 1 : 0;
}
