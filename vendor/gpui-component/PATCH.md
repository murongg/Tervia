# Tervia's Diff header patch

This directory contains the crates.io source of `gpui-component` **0.7.2**, published from GPUI Kit revision `c61937007769b0d09158b4c175368a392489e637`, under its original Apache-2.0 license. The original crate archive SHA-256 is `fb68a4725f3ef44c772e1490f775cf8950a17e8e5836572f080dc5bbcaede5db`.

The patch in `src/diff/mod.rs` adds `Diff::render_hunk_suffix`, an application-owned slot on the right of each `@@` metadata header. It receives the file and its original hunk index so Tervia can attach the correct stage or unstage action. Headers with an action grow to 32 logical pixels and truncate long scope labels to keep the action visible. Headers without the callback and the other separator modes retain their existing behavior. The header debug selector supports position checks in native UI tests.

Git operations, disabled states, patch validation, scrolling, folding, selection and code rendering remain owned by their existing implementations. This avoids attaching a hunk action to an unrelated source line or moving it with layout offsets.

Run `cargo test --locked --features ui-tests --bin tervia review_hunk_actions` to check icon size and placement inside the right side of each header in unified and split modes, including staged and deletion-only sections.

Remove this directory and its `[patch.crates-io]` entry when the pinned upstream component exposes a hunk-header action slot. Do not edit the shared Cargo registry source.
