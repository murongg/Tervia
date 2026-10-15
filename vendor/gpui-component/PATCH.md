# Tervia's component patches

This directory contains the crates.io source of `gpui-component` **0.7.2**, published from GPUI Kit revision `c61937007769b0d09158b4c175368a392489e637`, under its original Apache-2.0 license. The original crate archive SHA-256 is `fb68a4725f3ef44c772e1490f775cf8950a17e8e5836572f080dc5bbcaede5db`.

The patch in `src/diff/mod.rs` adds `Diff::render_hunk_suffix`, an application-owned slot on the right of each `@@` metadata header. It receives the file and its original hunk index so Tervia can attach the correct stage or unstage action. Headers with an action grow to 32 logical pixels and truncate long scope labels to keep the action visible. Headers without the callback and the other separator modes retain their existing behavior. The header debug selector supports position checks in native UI tests.

Git operations, disabled states, patch validation, scrolling, folding, selection and code rendering remain owned by their existing implementations. This avoids attaching a hunk action to an unrelated source line or moving it with layout offsets.

Run `cargo test --locked --features ui-tests --bin tervia review_hunk_actions` to check icon size and placement inside the right side of each header in unified and split modes, including staged and deletion-only sections.

The syntax patch exposes `highlighter::language_for_path` so file editors and diffs share filename detection, including common aliases and extensionless build/shell files. It wires the existing Swift, C#, and CMake highlight queries, supplies GraphQL and Protobuf queries, combines C++ with its C rules and JavaScript/TSX with JSX rules, and selects Ruby injections for ERB. C++ and TSX features enable the dependencies that supply their inherited queries. The application enables the full `tree-sitter-languages` feature; SQL's grammar currently requires `cc ~1.2.1`, so the lockfile uses the compatible 1.2 series.

Run `cargo test --locked --features ui-tests --test highlight` for parsing, light/dark token styles, aliases, JSX and language injections. Run `cargo test --locked --features ui-tests --bin tervia opening_files_uses_their_grammar_and_matches_git_diff` to check editor/Diff agreement through real file opens.

The Diff selection patch adds `DiffState::selected_excerpts` and `DiffExcerpt`, exposing exact source fragments with their side, original/modified path and inclusive source line numbers. It covers gutter selections and native partial text selections, separates sparse patch context and mixed versions, and never reads working files. Source-row debug selectors support native drag-selection checks. Clipboard formatting and copy actions remain application-owned.

Run `cargo test --locked --features ui-tests --bin tervia code_context` to check versions, renamed paths, sparse context, draft copying, native text selection in unified/split views, keyboard copying, and cancellation of a toolbar copy gesture.

Remove this directory and its `[patch.crates-io]` entry when the pinned upstream component includes the hunk-header slot, equivalent syntax support and the source-fragment selection API. Do not edit the shared Cargo registry source.
