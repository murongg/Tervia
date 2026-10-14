# Tervia

<!-- impeccable:product-schema 1 -->

## Platform
Native desktop, with macOS as the primary review platform and Windows / Linux platform adapters and build scripts. No WebView or browser runtime.

## Stack
Rust + GPUI. GPUI Kit supplies native text editing and controls.

## Product Purpose
Bring terminal use, Git review, file navigation, and small code edits into one workspace.

## Capabilities and Constraints
- The left project sidebar collapses to a 56 logical-pixel icon rail and expands to a flat project list. Clicking a project restores its last working directory. Working directories switch from a searchable titlebar popover.
- The right workspace has three fixed tabs: Terminal, Git, and Files.
- Each project retains its current tab, terminal sessions, file selection, and in-memory changes.
- Terminal sessions now run the user's real login/interactive shell over a PTY, with native GPUI rendering backed by alacritty_terminal. Commands execute normally and the shell loads the user's existing configuration.
- Startup restores the open project list, order, selection and sidebar expansion. Missing directories are skipped; an empty list presents the open/clone welcome screen. Projects are actual directories; terminal sessions start in their directory. Terminal processes and unsaved editing drafts remain limited to the current run.
- Opening a folder creates one workspace root and initially selects that folder. The sidebar lists only project roots. The titlebar shows the project and current working directory, and opens a searchable flat list containing the root, automatically discovered Git child repositories, and explicitly added directories. Add Working Directory uses a native folder chooser scoped to that project; Files folder menus offer Set as Working Directory. Ordinary subfolders appear only after explicit selection. Each working directory retains independent terminals, files, drafts, Git inputs and module selection. Workspace history remembers added directories and the last selection, skipping missing directories on restart. Git discovery remains bounded and populates the working-directory picker without adding sidebar projects; automatic results refresh independently of saved manual directories.
- Long Git lists are virtualized; folder and section folding is retained per project.
- Selecting a Git branch or tag opens its own paginated history and commit graph without changing HEAD or working files. Checkout remains explicit; branch/tag operations use contextual menus and compact creation/rename forms. Scope changes reject stale history results, while refreshes preserve the selected branch and history queries use fully qualified refs.
- Git review defaults to a collapsed commit form, with an expandable Diff, file/change navigation and a return path from full-file inspection. Recent comparisons retain their native reading state only when a freshly loaded patch is unchanged.
- Git uses the system CLI on background workers: changes, file/hunk staging, commits/amend, history, branches/tags, remotes and synchronization, stash, merge/rebase and conflict recovery.
- Linked worktrees belong to one logical project, identified by the shared Git directory. Its project entry restores the last working directory while each directory retains independent terminals, file tabs, drafts and Git state. A collapsible Worktrees section and management view support opening, creation from new/existing branches or detached HEAD, lock/unlock, safe removal and pruning missing registrations. Removal retains branches, rejects unsaved drafts and Git changes, and stops only affected terminals after success; force removal is not exposed.
- The file editor opens real UTF-8 files and saves to disk. External disk changes, newer reads and newly typed drafts cannot silently be replaced by older results.
- File browsing supports one replaceable preview tab, pinning by double-click/editing, background fuzzy filename search, literal project text search with source jumps, native Markdown reading/source views, and read-only image previews. Search honors project ignores and explicitly reports limits.
- Files shows the selected project’s directory tree, with lazy directory loading, virtualized rows, folding and automatic filesystem monitoring. Expansion and opened document drafts are kept independently per project. The tree hides Git metadata and does not recurse through directory symlinks. Native monitoring pauses when the workspace loses foreground activation and performs an immediate refresh before resuming when activated. It covers visited directories and opened files without recursively subscribing to dependency trees. Clean buffers synchronize automatically; dirty drafts are preserved with a disk-change notice. Manual file refresh is a compact icon beside Find and Save.
- Preferences persist in a user-editable TOML file, organized by feature domain (`ui` and `ui.fonts` today). Settings changes save immediately; manual edits apply on restart. Comments, unknown settings and dotfile symlinks are preserved, and invalid files or external edits are never silently overwritten. Project history saves independently in `workspace.json` beside the configuration file, remembering the last worktree directory with a main-directory fallback.
- Native macOS verification covers both themes, continuous window gradient, full-row header hover, count insets, multiple nested repositories, virtual scrolling/folding, staging/commit and conflict editing/resolution/continuation with synthetic repositories.
- Interface text supports ten languages and defaults to the system's preferred supported language.
- Agent tabs identify supported CLI entrypoints using background process inspection; Hermes uses the official monochrome mark with theme-aware contrast.

## Brand Commitments
Tervia. Quiet, compact, readable developer tooling.

## Open Decisions
Terminal session persistence, advanced history editing, hosting-provider APIs, and extended terminal protocols are future work. Windows GUI behavior and Linux desktop interactions still require hands-on platform validation.
