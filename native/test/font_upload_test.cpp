#include "core/hud/font_upload.hpp"
#include "core/hud/hud_font_atlas.hpp"

#include <cstdio>

int fails;

void expect(bool ok, const char *what)
{
    std::printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

int main()
{
    {
        hud::FontUpload font;
        int decodes = 0, uploads = 0;
        auto decode = [&](hud::img::Pixels &px) {
            decodes++;
            return hud::img::grey(hud::font::ATLAS_PNG, hud::font::ATLAS_PNG_SIZE, px);
        };
        for (int frame = 0; frame < 5; frame++)
            if (const hud::img::Pixels *px = font.pending(decode)) {
                uploads++;
                expect(px->width == 512 && px->height == 1024 && px->rgba.size() == size_t(512) * 1024,
                       "the first sync gets the decoded 512x1024 atlas");
                font.done();
            }
        expect(decodes == 1, "five syncs decode the atlas once");
        expect(uploads == 1, "and hand it to the upload once");
    }

    {
        hud::FontUpload font;
        int decodes = 0;
        auto decode = [&](hud::img::Pixels &px) {
            decodes++;
            px.width = px.height = 4;
            px.rgba.assign(16, 255);
            return true;
        };
        const hud::img::Pixels *first = font.pending(decode);
        const hud::img::Pixels *again = font.pending(decode);
        expect(first && first == again && decodes == 1, "a sync that did not upload gets the same pixels again");
        font.done();
        expect(!font.pending(decode) && decodes == 1, "once uploaded the pixels are dropped and not decoded again");
    }

    {
        hud::FontUpload font;
        int decodes = 0;
        auto decode = [&](hud::img::Pixels &) {
            decodes++;
            return false;
        };
        expect(!font.pending(decode) && !font.pending(decode) && decodes == 1, "a failed decode is not tried again");
    }

    std::printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails ? 1 : 0;
}
