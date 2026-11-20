#include "core/diagnostics/perf/pdh.hpp"

#include "core/hooks/guard.hpp"

namespace perf::pdh {

namespace {

template <class F> F resolve(HMODULE m, const char *name)
{
    return reinterpret_cast<F>(reinterpret_cast<void *>(GetProcAddress(m, name)));
}

int caught(EXCEPTION_POINTERS *e, Fault &fault, const char *call)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (!swofty_core::hooks::guard::recoverable(code))
        return EXCEPTION_CONTINUE_SEARCH;
    fault.call = call;
    fault.code = code;
    fault.at = reinterpret_cast<uintptr_t>(e->ExceptionRecord->ExceptionAddress);
    return EXCEPTION_EXECUTE_HANDLER;
}

}

HMODULE pinned(const wchar_t *dll)
{
    HMODULE m = nullptr;
    if (GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN, dll, &m) && m)
        return m;
    HMODULE loaded = LoadLibraryW(dll);
    if (!loaded)
        return nullptr;
    m = nullptr;
    return GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                              reinterpret_cast<LPCWSTR>(loaded), &m) &&
                   m
               ? m
               : loaded;
}

bool load(Api &api)
{
    HMODULE m = pinned(L"pdh.dll");
    if (!m)
        return false;
    api.open = resolve<OpenFn>(m, "PdhOpenQueryW");
    api.add = resolve<AddFn>(m, "PdhAddEnglishCounterW");
    api.collect = resolve<CollectFn>(m, "PdhCollectQueryData");
    api.array = resolve<ArrayFn>(m, "PdhGetFormattedCounterArrayW");
    return api.open && api.add && api.collect && api.array;
}

PDH_STATUS open(const Api &api, PDH_HQUERY *query, Fault &fault)
{
    __try {
        return api.open(nullptr, 0, query);
    } __except (caught(GetExceptionInformation(), fault, "PdhOpenQueryW")) {
        return PDH_STATUS(fault.code);
    }
}

PDH_STATUS add(const Api &api, PDH_HQUERY query, const wchar_t *path, PDH_HCOUNTER *counter, Fault &fault)
{
    __try {
        return api.add(query, path, 0, counter);
    } __except (caught(GetExceptionInformation(), fault, "PdhAddEnglishCounterW")) {
        return PDH_STATUS(fault.code);
    }
}

PDH_STATUS collect(const Api &api, PDH_HQUERY query, Fault &fault)
{
    __try {
        return api.collect(query);
    } __except (caught(GetExceptionInformation(), fault, "PdhCollectQueryData")) {
        return PDH_STATUS(fault.code);
    }
}

PDH_STATUS array(const Api &api, PDH_HCOUNTER counter, DWORD *size, DWORD *count, PPDH_FMT_COUNTERVALUE_ITEM_W items,
                 Fault &fault)
{
    __try {
        return api.array(counter, PDH_FMT_DOUBLE, size, count, items);
    } __except (caught(GetExceptionInformation(), fault, "PdhGetFormattedCounterArrayW")) {
        return PDH_STATUS(fault.code);
    }
}

}
