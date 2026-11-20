#include "core/diagnostics/crash/dred_state.hpp"

#include <string>
#include <vector>

// A removed device's DRED data: the unfinished command lists' breadcrumbs (and the ray tracing builds they stopped
// on), their contexts and the allocations around a page fault.
namespace runtime::dred {

namespace {

std::string name_a(const char *p)
{
    std::string s;
    for (int i = 0; i < 128; i++) {
        if ((i == 0 || ((reinterpret_cast<uintptr_t>(p) + i) & 0xfff) == 0) && !readable(p + i, 1))
            break;
        if (!p[i])
            break;
        s += (unsigned char)p[i] >= 0x20 && (unsigned char)p[i] < 0x7f ? p[i] : '?';
    }
    return s;
}

std::string name_w(const wchar_t *p)
{
    std::string s;
    for (int i = 0; i < 128; i++) {
        if ((i == 0 || ((reinterpret_cast<uintptr_t>(p + i)) & 0xfff) == 0) && !readable(p + i, 2))
            break;
        if (!p[i])
            break;
        s += p[i] >= 0x20 && p[i] < 0x7f ? char(p[i]) : '?';
    }
    return s;
}

std::string pick(const char *a, const wchar_t *w)
{
    std::string s = a ? name_a(a) : std::string();
    return s.empty() && w ? name_w(w) : s;
}

template <typename Node> std::string breadcrumbs(const Node *head, BuildStops &stops)
{
    std::vector<const Node *> unfinished, finished;
    unsigned total = 0;
    for (const Node *n = head; n && total < kMaxChain && readable(n, sizeof *n); n = n->pNext, total++) {
        uint32_t done = n->pLastBreadcrumbValue && readable(n->pLastBreadcrumbValue, 4) ? *n->pLastBreadcrumbValue : 0;
        (node_incomplete(n->BreadcrumbCount, done) ? unfinished : finished).push_back(n);
    }
    std::string s;
    char b[160];
    std::snprintf(b, sizeof b, "\n  auto-breadcrumbs: %u command list node(s), %zu not finished; showing %s\n", total,
                  unfinished.size(), "unfinished first");
    s += b;
    if (!total)
        return s + "    none recorded\n";
    unsigned shown = 0;
    for (const auto *list : {&unfinished, &finished})
        for (const Node *n : *list) {
            if (shown == kMaxNodes) {
                s += "    (more nodes not shown)\n";
                return s;
            }
            uint32_t count = n->BreadcrumbCount;
            uint32_t done = n->pLastBreadcrumbValue && readable(n->pLastBreadcrumbValue, 4) ? *n->pLastBreadcrumbValue : 0;
            s += node_header(shown++, pick(n->pCommandListDebugNameA, n->pCommandListDebugNameW).c_str(),
                             pick(n->pCommandQueueDebugNameA, n->pCommandQueueDebugNameW).c_str(), count, done,
                             reinterpret_cast<uint64_t>(n->pCommandList));
            if (list == &unfinished && n->pCommandHistory && readable(n->pCommandHistory, count * sizeof(uint32_t)))
                stops.add(build_stop(n->pCommandHistory, count, done, reinterpret_cast<uint64_t>(n->pCommandList)));
            Window w = op_window(count, done);
            if (w.from)
                s += "      ... " + std::to_string(w.from) + " earlier op(s) not shown\n";
            if (!n->pCommandHistory || !readable(n->pCommandHistory + w.from, (w.to - w.from) * sizeof(uint32_t))) {
                s += "      command history not readable\n";
                continue;
            }
            for (uint32_t i = w.from; i < w.to; i++)
                s += op_line(i, done, n->pCommandHistory[i]);
            if (w.to < count)
                s += "      ... " + std::to_string(count - w.to) + " later op(s) not shown\n";
        }
    return s;
}

template <typename Node> std::string contexts(const Node *head)
{
    std::string s;
    unsigned seen = 0, shown = 0;
    for (const Node *n = head; n && seen < kMaxChain && readable(n, sizeof *n); n = n->pNext, seen++) {
        if (!n->BreadcrumbContextsCount ||
            !readable(n->pBreadcrumbContexts, n->BreadcrumbContextsCount * sizeof *n->pBreadcrumbContexts))
            continue;
        for (UINT i = 0; i < n->BreadcrumbContextsCount && shown < kMaxOps; i++, shown++) {
            char b[300];
            std::snprintf(b, sizeof b, "    op %u of %s: %s\n", n->pBreadcrumbContexts[i].BreadcrumbIndex,
                          pick(n->pCommandListDebugNameA, n->pCommandListDebugNameW).c_str(),
                          name_w(n->pBreadcrumbContexts[i].pContextString).c_str());
            s += b;
        }
    }
    return shown ? "\n  breadcrumb contexts:\n" + s : s;
}

template <typename Alloc> std::string allocations(const char *title, const Alloc *head, uint64_t object_of(const Alloc *))
{
    std::string s;
    unsigned n = 0, seen = 0;
    for (const Alloc *a = head; a && seen < kMaxChain && readable(a, sizeof *a); a = a->pNext, seen++)
        if (n++ < kMaxAllocations)
            s += allocation_line(pick(a->ObjectNameA, a->ObjectNameW).c_str(), uint32_t(a->AllocationType), object_of(a));
    char b[120];
    std::snprintf(b, sizeof b, "  %s: %u%s\n", title, n, n > kMaxAllocations ? " (first 32 shown)" : "");
    return b + s;
}

uint64_t no_object(const D3D12_DRED_ALLOCATION_NODE *) { return 0; }

uint64_t object_of(const D3D12_DRED_ALLOCATION_NODE1 *a) { return reinterpret_cast<uint64_t>(a->pObject); }

}

std::string removed_device(ID3D12Device *dev, HRESULT reason, BuildStops &stops)
{
    char b[200];
    std::snprintf(b, sizeof b, "  device %p: GetDeviceRemovedReason 0x%08lx (%s)\n", static_cast<void *>(dev),
                  (unsigned long)reason, removal_name(uint32_t(reason)));
    std::string s = b;
    ID3D12DeviceRemovedExtendedData1 *d1 = nullptr;
    if (SUCCEEDED(dev->QueryInterface(__uuidof(ID3D12DeviceRemovedExtendedData1), reinterpret_cast<void **>(&d1)))) {
        D3D12_DRED_AUTO_BREADCRUMBS_OUTPUT1 ab{};
        D3D12_DRED_PAGE_FAULT_OUTPUT1 pf{};
        HRESULT ha = d1->GetAutoBreadcrumbsOutput1(&ab), hp = d1->GetPageFaultAllocationOutput1(&pf);
        if (SUCCEEDED(ha)) {
            s += breadcrumbs(ab.pHeadAutoBreadcrumbNode, stops);
            s += contexts(ab.pHeadAutoBreadcrumbNode);
        } else {
            std::snprintf(b, sizeof b, "\n  auto-breadcrumbs: unavailable (0x%08lx; DRED was not on for this device)\n",
                          (unsigned long)ha);
            s += b;
        }
        if (SUCCEEDED(hp)) {
            s += "\n" + page_fault_line(pf.PageFaultVA);
            s += allocations("existing allocations", pf.pHeadExistingAllocationNode, object_of);
            s += allocations("recently freed allocations", pf.pHeadRecentFreedAllocationNode, object_of);
        } else {
            std::snprintf(b, sizeof b, "\n  page fault output: unavailable (0x%08lx)\n", (unsigned long)hp);
            s += b;
        }
        d1->Release();
        return s;
    }
    ID3D12DeviceRemovedExtendedData *d0 = nullptr;
    if (SUCCEEDED(dev->QueryInterface(__uuidof(ID3D12DeviceRemovedExtendedData), reinterpret_cast<void **>(&d0)))) {
        D3D12_DRED_AUTO_BREADCRUMBS_OUTPUT ab{};
        D3D12_DRED_PAGE_FAULT_OUTPUT pf{};
        if (SUCCEEDED(d0->GetAutoBreadcrumbsOutput(&ab)))
            s += breadcrumbs(ab.pHeadAutoBreadcrumbNode, stops);
        if (SUCCEEDED(d0->GetPageFaultAllocationOutput(&pf))) {
            s += "\n" + page_fault_line(pf.PageFaultVA);
            s += allocations("existing allocations", pf.pHeadExistingAllocationNode, no_object);
            s += allocations("recently freed allocations", pf.pHeadRecentFreedAllocationNode, no_object);
        }
        d0->Release();
        return s;
    }
    return s + "  the device has no ID3D12DeviceRemovedExtendedData (an older Windows, Wine or vkd3d)\n";
}

}
