#pragma once
#include <cstdint>
#include <filesystem>
#include <string>
#include <system_error>

#ifdef _WIN32
#include <windows.h>
#else
#include <fcntl.h>
#include <unistd.h>
#endif

namespace runtime::durable {

constexpr const char *TMP_SUFFIX = ".swofty-tmp";

inline bool sync_dir(const std::filesystem::path &dir)
{
#ifdef _WIN32
    HANDLE h = CreateFileW(dir.c_str(), GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr);
    if (h == INVALID_HANDLE_VALUE)
        return false;
    bool ok = FlushFileBuffers(h) != 0;
    CloseHandle(h);
    return ok;
#else
    int fd = ::open(dir.c_str(), O_RDONLY | O_DIRECTORY);
    if (fd < 0)
        return false;
    bool ok = ::fsync(fd) == 0;
    ::close(fd);
    return ok;
#endif
}

inline bool write_flushed(const std::filesystem::path &file, const void *data, size_t size)
{
#ifdef _WIN32
    HANDLE h = CreateFileW(file.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (h == INVALID_HANDLE_VALUE)
        return false;
    bool ok = true;
    size_t done = 0;
    while (ok && done < size) {
        DWORD put = 0, want = DWORD(size - done > (1u << 30) ? (1u << 30) : size - done);
        ok = WriteFile(h, static_cast<const char *>(data) + done, want, &put, nullptr) && put;
        done += put;
    }
    ok = ok && FlushFileBuffers(h);
    CloseHandle(h);
    return ok;
#else
    int fd = ::open(file.c_str(), O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0)
        return false;
    bool ok = true;
    size_t done = 0;
    while (ok && done < size) {
        ssize_t n = ::write(fd, static_cast<const char *>(data) + done, size - done);
        ok = n > 0;
        if (ok)
            done += size_t(n);
    }
    ok = ok && ::fsync(fd) == 0;
    ::close(fd);
    return ok;
#endif
}

// tmp, flush to disk, rename over the final name, then flush the directory so the rename itself survives a crash.
// dir_synced reports the last step on its own: a filesystem that cannot flush a directory still got a good write.
inline bool write_file(const std::filesystem::path &file, const void *data, size_t size, bool *dir_synced = nullptr)
{
    std::filesystem::path tmp = file;
    tmp += TMP_SUFFIX;
    if (!write_flushed(tmp, data, size)) {
        std::error_code ec;
        std::filesystem::remove(tmp, ec);
        return false;
    }
    std::error_code ec;
    std::filesystem::rename(tmp, file, ec);
    if (ec) {
        std::filesystem::remove(tmp, ec);
        return false;
    }
    bool s = sync_dir(file.parent_path().empty() ? std::filesystem::path(".") : file.parent_path());
    if (dir_synced)
        *dir_synced = s;
    return true;
}

inline bool write_file(const std::filesystem::path &file, const std::string &text, bool *dir_synced = nullptr)
{
    return write_file(file, text.data(), text.size(), dir_synced);
}

}
