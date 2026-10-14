# Tervia's GPUI patches

This directory contains the crates.io source of `gpui-pre` **0.3.8**, published from Zed revision `279fe070bb389b79652e52065b2f001edcc0b11b`, under its original Apache-2.0 license. The original crate archive SHA-256 is `d7f0bc3ebba070f8981b3f8e905a12f47d27794a30d029e59cdb9edf7a4b3360`.

The manifest patch adds one Cargo feature, `windows-manifest-provided-by-host`, and makes it suppress GPUI's resource compilation in `build.rs`. Trailing whitespace in two upstream documentation-comment lines is removed to satisfy repository whitespace checks. Without this override, GPUI and Tervia both contribute `RT_MANIFEST` resource 1, producing Windows linker error CVT1100. Disabling default features on only the direct dependency does not solve this because other GPUI Kit dependencies enable them transitively.

Tervia enables the override through its direct GPUI dependency whenever `native` is enabled. Its own `assets/windows.manifest` is the single owner of application settings, including GPUI's Common Controls v6, DPI awareness and segment heap settings, plus Tervia's long-path and UTF-8 settings. UI test support remains enabled only by the `ui-tests` feature.

The list patch in `src/elements/list.rs` retains the caller's uniform row-height estimate separately from measured item sizes. First layout and width changes still invalidate actual measurements, but off-screen rows fall back to the estimate instead of contributing zero height. This lets the Diff scroll immediately without folding and unfolding its file header. Regression tests in `tests/list.rs` at the application root cover first-frame wheel scrolling, resizing, remeasurement and virtualized rendering; run them with `cargo test --locked --features ui-tests --test list`.

Remove the `[patch.crates-io]` entry and this directory when the pinned upstream backend supports host-owned manifests and retains uniform row-height estimates across width invalidation. At an upgrade, check that there is still one embedded manifest with all of these settings, that the application icon remains present and that the list regression tests pass. Do not edit the source in the shared Cargo registry cache.
