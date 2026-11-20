#include "core/runtime/loader/code_encryption.hpp"

#include <cmath>

namespace runtime::loader {

CodeEncryption &code()
{
    static CodeEncryption instance;
    return instance;
}

double CodeEncryption::page_entropy(uintptr_t p) const
{
    if (!page_readable(p, 0x1000))
        return -1;
    unsigned long cnt[256] = {0};
    auto b = reinterpret_cast<const uint8_t *>(p);
    for (int i = 0; i < 0x1000; i++)
        cnt[b[i]]++;
    double h = 0;
    for (int i = 0; i < 256; i++)
        if (cnt[i]) {
            double pr = cnt[i] / 4096.0;
            h -= pr * std::log2(pr);
        }
    return h;
}

double CodeEncryption::section_entropy(const IMAGE_SECTION_HEADER *s) const
{
    uintptr_t start = image().base() + s->VirtualAddress;
    size_t size = s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData;
    double sum = 0;
    int ok = 0;
    for (int k = 0; k < 16; k++) {
        size_t off =
            size > 16 * 0x1000 ? (size_t)((double)k / 16 * (double)(size - 0x1000)) & ~(size_t)0xfff : (size_t)k * 0x1000;
        if (k && off + 0x1000 > size)
            break;
        double e = page_entropy(start + off);
        if (e >= 0)
            ok++, sum += e;
    }
    return ok ? sum / ok : -1;
}

const char *CodeEncryption::reads(double e)
{
    return e < 0 ? "unreadable" : e > ENCRYPTED ? "encrypted" : e > PLAIN ? "partly encrypted" : "decrypted";
}

/* Bulk decryption only goes one way, so a section seen decrypted is not sampled again; an encrypted one is
   resampled at most every 50 ms because addon inits ask for many bindings in a row. */
double CodeEncryption::section_now(const std::string &name)
{
    auto s = image().section_by_name(name);
    if (!s)
        return -1;
    DWORD now = GetTickCount();
    auto it = section_reads_.find(name);
    if (it != section_reads_.end() && ((it->second.entropy >= 0 && it->second.entropy <= PLAIN) || now - it->second.at < 50))
        return it->second.entropy;
    double e = section_entropy(s);
    section_reads_[name] = {e, now};
    return e;
}

bool CodeEncryption::page_encrypted(uintptr_t a) const
{
    double e = page_entropy(a & ~uintptr_t(0xfff));
    return e < 0 || e > ENCRYPTED;
}

}
