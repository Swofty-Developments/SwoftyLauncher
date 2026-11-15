# Native chart development checks

Chart data and validation live in `packages/addon-api/src/view/chart.rs`; the launcher renderer is `apps/launcher/src/blocks/chart.rs`. The gallery and JSON-lines example use the same API and native renderer.

Helpers must check `hello.params.capabilities` for `view.chart`. Treat a missing list as empty. Do not infer chart support from version strings; the PR does not change the release version or API version.

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
cargo build -p swofty-addon --example native_charts
python tools/native-charts/protocol-smoke.py target/debug/examples/native_charts
cargo run -p swofty-launcher -- --chart-demo
```

The smoke test verifies capability detection (including older/future version strings), a normal reload Effect after the example button, persisted chart data on subsequent views, null tick replies, explicit gaps and absence of generated chart files. It needs only Python's standard library, not a runtime chart service.

On Windows, also run the packing/layout tests for the pinned graphics patch:

```text
cargo test -p gpui-pre-windows --lib compact_path_tests
rustfmt --edition 2024 --check vendor/gpui-pre-windows/src/directx_renderer.rs
```

The vendored Windows backend and its narrow renderer/shader change are documented in `vendor/gpui-pre-windows/SWOFTY-PATCH.md`. Its Apache-2.0 license is included. Native vertex coordinates, backgrounds/gradients, opacity, clipping and draw order are preserved; style metadata is stored once per path instead of per vertex. Other platform backends are unchanged. CI runs normal workspace checks on Linux and Windows and the backend packing tests on Windows.

## Optional memory probes

`profile-memory.py` measures the actual native window using Windows `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX2`). Logs distinguish private working set, total working set and private committed memory. Compare the same counter between builds. Private working set is closest to Task Manager's private resident-memory column; total working set includes shared resident pages.

```text
python tools/native-charts/profile-memory.py target/release/swofty-launcher.exe charts.json --seconds 120 --output memory.json
```

The input is a JSON array of Chart objects, or an array of chart arrays to cycle pages. The gallery mode opens no launcher store, game or addon helper. It forces redraws every 100 ms. No chart image files are generated.

For a full-launcher measurement, first install and enable the generic example addon in a separate prepared profile with the CLI and `SWOFTY_DATA_DIR`. Package the compiled example as `bin/native_charts.exe` (Windows) or `bin/native_charts` (Linux), beside `docs/native-charts-demo/swofty.json`. Supply a complete View JSON:

```text
python tools/native-charts/profile-memory.py target/release/swofty-launcher.exe view.json --section example.native-charts/charts --profile path/to/separate-profile --seconds 120 --output full-memory.json
```

The existing `SWOFTY_LAUNCHER_OPEN_PAGE` developer hook accepts `section:addon-id/section-id`. `SWOFTY_CHART_DEMO_VIEW` makes the example helper read a supplied fixture on view requests; with tick=true it sends normal invalidate notifications and null tick results. Replacing the fixture between ticks exercises normal view reloads. `SWOFTY_CHART_METRICS` enables bounded native-paint diagnostics. These probes are opt-in and do not change ordinary launches. No user recordings or game files are included in this repository.

On Windows, upstream tests may need Git for Windows' usr/bin utilities (`true`, `sleep`) and a working Python on the test process PATH. A Windows Store python3 alias can falsely appear installed; remove that alias directory from the test process PATH when needed. These are development-test requirements, not launcher runtime dependencies.
