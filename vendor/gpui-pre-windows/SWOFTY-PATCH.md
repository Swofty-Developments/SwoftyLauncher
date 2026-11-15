# Compact native path buffers

Vendored from crates.io `gpui-pre-windows` 0.3.11, an Apache-2.0 snapshot of
Zed commit `f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`. Original license included.

Only `src/directx_renderer.rs` and `src/shaders.hlsl` differ from the published
crate. Native path vertices now carry position, curve coordinates and a path
index (20 bytes). Background/gradient and clipping metadata are stored once per
path (88 bytes), instead of repeated in every vertex (104 bytes per vertex).
The matching vertex/fragment shaders read the same metadata through the index.
Positions, gradients, transparency, clipping, multisampling and draw order are
unchanged. There is no telemetry- or addon-specific rendering code here.

The extra buffer is owned by the existing renderer pipelines and recreated on
device recovery. It uses the existing dynamic-buffer helpers and size limit.
Tests check the Rust/HLSL buffer layout and exact preservation of each packed
vertex's position, curve coordinates, background and clipping metadata.

The launcher pins this local crate through `[patch.crates-io]` so source builds
reproduce the Windows graphics fix without modifying the user's Cargo cache.

`compact-paths.patch` contains only the two upstream-source differences, including
the packing/layout tests, for review without reading the complete vendored snapshot.
