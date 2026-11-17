#pragma once

#include "core/runtime/loader/image.hpp"

#include <atomic>
#include <string>
#include <unordered_map>

namespace runtime::loader {

// A packed build's code reads encrypted until the game decrypts it. This samples the entropy of sections and pages
// so bindings, scans and hooks wait for decrypted code instead of acting on garbage.
class CodeEncryption {
public:
    static constexpr double ENCRYPTED = 7.5, PLAIN = 6.5;

    bool packed() const { return packed_; }
    void set_packed(bool packed) { packed_ = packed; }

    double page_entropy(uintptr_t p) const;
    double section_entropy(const IMAGE_SECTION_HEADER *s) const;
    double section_now(const std::string &name);
    bool page_encrypted(uintptr_t a) const;
    void forget_samples() { section_reads_.clear(); }

    bool plain_seen() const { return code_plain_.load(std::memory_order_acquire); }
    void mark_plain() { code_plain_.store(true, std::memory_order_release); }

    static const char *reads(double e);

private:
    struct SectionRead {
        double entropy;
        DWORD at;
    };

    bool packed_ = false;
    std::unordered_map<std::string, SectionRead> section_reads_;
    std::atomic<bool> code_plain_{false};
};

CodeEncryption &code();

}
