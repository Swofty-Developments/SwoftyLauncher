#pragma once

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <string_view>

// D3D12 Device Removed Extended Data (DRED) for crash reports. The pure parts: when DRED is switched on, which
// register values mean the GPU was removed, and how breadcrumb and page fault records are worded. dred.cpp does
// the D3D12 calls.

namespace runtime::dred {

constexpr int kMaxOps = 64;
constexpr int kMaxNodes = 32;
constexpr int kMaxAllocations = 32;
constexpr int kMaxChain = 4096;

struct Decision {
    bool on = false;
    const char *why = "";
};

inline Decision decide(const char *env, bool prior_removed)
{
    if (env && env[0] == '1' && !env[1])
        return {true, "SWOFTY_DRED=1"};
    if (env && env[0] == '0' && !env[1])
        return {false, "SWOFTY_DRED=0"};
    if (prior_removed)
        return {true, "the previous session ended with a removed GPU device"};
    return {false, "the previous session did not end with a removed GPU device"};
}

inline bool is_removal_hresult(uint64_t v)
{
    uint32_t h = uint32_t(v);
    if ((v >> 32) != 0 && (v >> 32) != 0xffffffffull)
        return false;
    return h == 0x887a0001 || h == 0x887a0005 || h == 0x887a0006 || h == 0x887a0007 || h == 0x887a0020;
}

inline const char *removal_name(uint32_t h)
{
    switch (h) {
    case 0x887a0001: return "DXGI_ERROR_INVALID_CALL";
    case 0x887a0005: return "DXGI_ERROR_DEVICE_REMOVED";
    case 0x887a0006: return "DXGI_ERROR_DEVICE_HUNG";
    case 0x887a0007: return "DXGI_ERROR_DEVICE_RESET";
    case 0x887a0020: return "DXGI_ERROR_DRIVER_INTERNAL_ERROR";
    default: return "unknown";
    }
}

inline const char *op_name(uint32_t op)
{
    static const char *const names[] = {
        "SETMARKER", "BEGINEVENT", "ENDEVENT", "DRAWINSTANCED", "DRAWINDEXEDINSTANCED", "EXECUTEINDIRECT", "DISPATCH",
        "COPYBUFFERREGION", "COPYTEXTUREREGION", "COPYRESOURCE", "COPYTILES", "RESOLVESUBRESOURCE", "CLEARRENDERTARGETVIEW",
        "CLEARUNORDEREDACCESSVIEW", "CLEARDEPTHSTENCILVIEW", "RESOURCEBARRIER", "EXECUTEBUNDLE", "PRESENT",
        "RESOLVEQUERYDATA", "BEGINSUBMISSION", "ENDSUBMISSION", "DECODEFRAME", "PROCESSFRAMES", "ATOMICCOPYBUFFERUINT",
        "ATOMICCOPYBUFFERUINT64", "RESOLVESUBRESOURCEREGION", "WRITEBUFFERIMMEDIATE", "DECODEFRAME1", "SETPROTECTEDRESOURCESESSION",
        "DECODEFRAME2", "PROCESSFRAMES1", "BUILDRAYTRACINGACCELERATIONSTRUCTURE", "EMITRAYTRACINGACCELERATIONSTRUCTUREPOSTBUILDINFO",
        "COPYRAYTRACINGACCELERATIONSTRUCTURE", "DISPATCHRAYS", "INITIALIZEMETACOMMAND", "EXECUTEMETACOMMAND", "ESTIMATEMOTION",
        "RESOLVEMOTIONVECTORHEAP", "SETPIPELINESTATE1", "INITIALIZEEXTENSIONCOMMAND", "EXECUTEEXTENSIONCOMMAND", "DISPATCHMESH",
        "ENCODEFRAME", "RESOLVEENCODEROUTPUTMETADATA"};
    return op < sizeof names / sizeof *names ? names[op] : "UNKNOWN_OP";
}

inline const char *allocation_type_name(uint32_t t)
{
    switch (t) {
    case 0x13: return "COMMAND_QUEUE";
    case 0x14: return "COMMAND_ALLOCATOR";
    case 0x15: return "PIPELINE_STATE";
    case 0x16: return "COMMAND_LIST";
    case 0x17: return "FENCE";
    case 0x18: return "DESCRIPTOR_HEAP";
    case 0x19: return "HEAP";
    case 0x1b: return "QUERY_HEAP";
    case 0x1c: return "COMMAND_SIGNATURE";
    case 0x1d: return "PIPELINE_LIBRARY";
    case 0x22: return "RESOURCE";
    case 0x23: return "PASS";
    case 0x2a: return "STATE_OBJECT";
    case 0x2b: return "METACOMMAND";
    default: return "OTHER";
    }
}

inline bool node_incomplete(uint32_t count, uint32_t completed)
{
    return completed < count;
}

struct Window {
    uint32_t from = 0, to = 0;
};

inline Window op_window(uint32_t count, uint32_t completed)
{
    Window w;
    uint32_t keep = kMaxOps;
    uint32_t anchor = completed > count ? count : completed;
    w.from = anchor > keep * 3 / 4 ? anchor - keep * 3 / 4 : 0;
    w.to = w.from + keep < count ? w.from + keep : count;
    return w;
}

inline std::string op_line(uint32_t index, uint32_t completed, uint32_t op)
{
    char b[160];
    const char *mark = index + 1 == completed ? "  <- last completed" : index == completed ? "  <- first not completed" : "";
    std::snprintf(b, sizeof b, "      %5u %-4s %s%s\n", index, index < completed ? "done" : "todo", op_name(op), mark);
    return b;
}

inline std::string node_header(unsigned n, const char *list, const char *queue, uint32_t count, uint32_t completed)
{
    char b[400];
    std::snprintf(b, sizeof b, "    node %u: command list %s on queue %s, %u of %u op(s) completed%s\n", n,
                  list && *list ? list : "(unnamed)", queue && *queue ? queue : "(unnamed)", completed > count ? count : completed,
                  count, node_incomplete(count, completed) ? ", NOT FINISHED" : "");
    return b;
}

inline std::string allocation_line(const char *name, uint32_t type, uint64_t object)
{
    char b[400], at[40] = "";
    if (object)
        std::snprintf(at, sizeof at, " at 0x%llx", (unsigned long long)object);
    std::snprintf(b, sizeof b, "    %-18s %s%s\n", allocation_type_name(type), name && *name ? name : "(unnamed)", at);
    return b;
}

inline std::string page_fault_line(uint64_t va)
{
    char b[120];
    std::snprintf(b, sizeof b, "  page fault at GPU virtual address 0x%llx\n", (unsigned long long)va);
    return b;
}

}
