---
title: Storage
description: Read and write an addon's own files under swofty/addons/<id>/ with swofty/storage.hpp; the launcher keeps them between sessions.
---

# Storage

Use storage to read and write your addon's own files under `swofty/addons/<id>/`. Give it paths relative to that folder: the core rejects absolute paths, drive letters and any path that climbs above it. Your addon can only reach its own folder because the core identifies the calling DLL the same way as the [asset API](/reference/addons/asset-api).

| | |
|---|---|
| Header | `swofty/storage.hpp` |
| Namespace | `swofty::storage` |
| Table | `SwoftyApi::storage` (`SwoftyStorage`) |
| Feature | `storage` — stable |
| API version | 5 |
| Thread | Caller thread (any thread) |

Check the `swofty::storage::Result` each call returns: `ok`, `late`, `unavailable` or `invalid`. The [result codes](./#result-codes) explain what each means. To keep writes atomic, the core writes a temp file, then renames it.

## Calls

| Call | Does |
|---|---|
| `available()` | `true` when the feature is on and implemented |
| `read(path, buf, cap, &len)` | reads the whole file into `buf` and sets `len` to the file size; returns `invalid` if the size exceeds `cap` |
| `read(path, out)` | reads the whole file into a `std::string` |
| `write(path, data)` | replaces the file atomically (`std::string_view` or `std::span<const std::byte>`) |
| `append(path, data)` | appends to the file, creating it if it doesn't exist |
| `remove(path)` | deletes the file |
| `size(path, &out)` | writes the file size in bytes |
| `list(dir, buf, cap, &len)` | writes the directory's names into `buf`, separated by newlines; returns `invalid` if the listing exceeds `cap` |

## Example

Here's how you append a line to a log file, then read it back:

```cpp [native/src/my_plugin.cpp]
#include <swofty/storage.hpp>

namespace st = swofty::storage;

SWOFTY_ON_LOAD
{
    if (!st::available())
        return;

    st::append("laps.csv", "lap,time\n");

    std::string body;
    if (st::read("laps.csv", body) == st::Result::ok)
        swofty::log("[my-addon] laps.csv is %zu bytes", body.size());
}

SWOFTY_ADDON()
```

While the game runs, the file is at `swofty/addons/<id>/laps.csv` in the game folder. Between sessions it is at `$DATA/addon-storage/<id>/laps.csv`. Name your plugin in `operations.native`:

```json [swofty.json]
"operations": {
  "native": [{ "dll": "native/my_plugin.dll" }]
}
```

## Between sessions

The launcher keeps what your addon writes. `$DATA` is the launcher's data folder (see [Where your files are stored](/guide/installation#where-your-files-are-stored)).

- When a session's files are rolled back, every file in `swofty/addons/<id>/` that the install didn't place moves to `$DATA/addon-storage/<id>/`, keeping its path. The game folder is then back to stock.
- The next install of a profile with your addon moves them back into `swofty/addons/<id>/` before the game starts. The install log has a `storage: put back <n> file(s) <id> wrote` line.
- Switching to a profile without your addon, updating or reinstalling the addon, **Restore game folder** (`swofty restore`) and the one-time setup keep the files. A restore first moves the files in every addon folder that the launcher didn't place to `$DATA/addon-storage/<id>/`, then removes `swofty/`.
- A file your addon removed stays removed.
- A file at a path your addon ships (a file in `files` on `operations.native`) is replaced by the shipped one in the game folder. The written copy stays in `$DATA/addon-storage/<id>/`.
- A file already in `swofty/addons/<id>/` when the install runs, which a session that ended without rolling back left there, is kept over the stored copy.
- Files are kept per data folder. A launcher with another data folder (`SWOFTY_DATA_DIR`) doesn't see them.

## Availability

`available()` returns `true` while the `storage` feature is on. See [the native-libraries design](/design/native-libraries#_5-storage) for the design.
