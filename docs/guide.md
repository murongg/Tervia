# Tervia User and Developer Guide

[English](guide.md) | [简体中文](guide.zh-CN.md) · [Back to Tervia](../README.md)

Detailed usage, configuration, build, and release instructions. Run all commands from the repository root.

## Running

The project pins Rust 1.99.0 in `rust-toolchain.toml`; the first run downloads the toolchain and dependencies. macOS requires macOS 15+ and Xcode:

```sh
./scripts/run.sh
```

The script builds a release executable by default; you can also open `build/Tervia.app` directly after building. This build supports online updates. For a development build, run `cargo run --features native`.

Linux requires Vulkan drivers and development libraries for Fontconfig, ALSA, XKB, XCB, Wayland, PipeWire, and related components. See `.github/workflows/native.yml` for Ubuntu / Debian installation commands. Run `cargo run --features native`, or use `./scripts/linux.sh` to create an archive.

On Windows, use the MSVC Rust toolchain and Visual Studio C++ Build Tools, then run `cargo run --features native`. Run `./scripts/windows.ps1` in PowerShell to create a ZIP. The terminal uses system ConPTY and opens PowerShell by default with its Profile enabled. Linux defaults to Bash and macOS to Zsh; when `SHELL` is set, its value takes precedence.

Tervia owns the Windows application manifest. A [scoped patch to the pinned GPUI dependency](../vendor/gpui-pre/PATCH.md) prevents duplicate manifest resources while preserving DPI awareness, Common Controls v6, long-path and UTF-8 settings. Windows CI checks these settings in the linked executable.

Git integration has been verified on macOS through synthetic repository tests, native interaction tests, and interface checks in both dark and light themes. These checks covered discovery of multiple repositories, collapsing and scrolling, hunk staging, commits, conflict editing, and continuing merges. The Git core has passed cross-compilation checks for Windows GNU and Linux GNU; the graphical interface still needs testing on actual machines running those systems. CI is configured to test, build, and package on all three systems, but remote CI has not yet run.

## Releases

The [release script](../scripts/release.sh) uses a POSIX shell, Git, and Cargo (use Git Bash on Windows). Set `package.version` in `Cargo.toml`, run `cargo check` to update `Cargo.lock` and cache dependencies, and commit your changes before releasing. The script reads that committed version; it does not bump versions automatically.

```sh
git fetch --tags origin
sh scripts/release.sh --dry-run  # Preview the changelog without making changes
sh scripts/release.sh --push     # Generate, commit, tag, and push to origin
```

Running without flags creates a local release and prints the push command. The script generates `CHANGELOG.md` from non-merge commits since the nearest reachable `v*` version tag, or the full history for the first release. Conventional Commits are grouped into features, fixes, performance, documentation, breaking changes, and other changes. Existing changelog entries are retained. A `chore(release): v<version>` commit is created, then an annotated tag containing the current release notes points to that commit.

Releases require a clean working tree, a branch checkout, full history, a matching lockfile, and a new tag. `--push` fetches origin tags first and pushes the current branch and tag atomically. If a hook or push fails, generated files, commits, and tags are retained for inspection; a failed push prints the command to retry. The script respects Git hooks and signing settings.

Pushing the tag triggers the [release workflow](../.github/workflows/release.yml), which reuses native CI checks and release builds on macOS, Linux, and Windows. After all three succeed, it publishes a GitHub Release using the annotated tag's changelog, with versioned platform archives and `SHA256SUMS`. Manually pushed lightweight tags fall back to GitHub-generated release notes. Tags such as `v0.2.0-beta.1` publish a prerelease and require the same version in `Cargo.toml`. GitHub publishing uses the built-in `GITHUB_TOKEN`; macOS releases also require the Apple secrets below. Existing releases are not overwritten.

### macOS signing and notarization

Configure these repository secrets under **Settings → Secrets and variables → Actions** before pushing a release tag:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE` | Base64-encoded `.p12` containing one **Developer ID Application** certificate and its private key. |
| `MACOS_CERTIFICATE_PASSWORD` | The password used when exporting the `.p12`. |
| `APPLE_ID` | Your Apple developer account email. |
| `APPLE_TEAM_ID` | Your 10-character developer team ID. |
| `APPLE_APP_SPECIFIC_PASSWORD` | An app-specific password for notarization, not your Apple account password. |

An Apple Developer Program membership is required. Export the certificate together with its private key from Keychain Access, then copy its Base64 representation with `base64 -i DeveloperID.p12 | pbcopy` and paste it into the secret. The certificate must match `APPLE_TEAM_ID`; Apple Development and Mac App Store distribution certificates are not accepted. See [Apple's Developer ID guidance](https://developer.apple.com/developer-id/) and [notarization documentation](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

[sign.sh](../scripts/sign.sh) imports the certificate into a temporary keychain, signs with Hardened Runtime and a secure timestamp, submits a ZIP to Apple's notary service, waits up to 30 minutes for `Accepted`, staples the ticket, and checks Gatekeeper before the workflow creates the final macOS ZIP. Missing secrets, invalid signatures, rejected or timed-out notarization, and failed ticket checks stop the release. Notarization failures include Apple's diagnostic log when available. Temporary certificates, credentials, and the keychain are removed on success or failure. No extra keychain-password secret is needed.

Pull requests targeting `main` run formatting checks, tests, and Clippy on macOS, Linux, and Windows through the [CI workflow](../.github/workflows/ci.yml). Pushes to `main` run [native builds](../.github/workflows/native.yml); these and local development builds use ad-hoc signing. For local notarization, export the same five environment variables and run `sh scripts/sign.sh build/Tervia.app` after packaging on a Mac with Xcode. `sh scripts/sign.sh --check` only checks that the required values are present and the team ID has the expected format; it does not contact Apple or verify the certificate.

Linux packages require the runtime libraries described above. Each archive targets the architecture of its CI runner; macOS archives include that architecture in the filename.

## Launch at login

Enable **Settings → Interface → Startup → Launch at login** to open Tervia automatically when you sign in. It is off by default. This setting reads and changes the current user's system startup entry directly, is not stored in `config.toml`, and is preserved by **Reset defaults**. Failures show their cause and a retry action in settings.

macOS uses a user LaunchAgent, Windows uses the current user's Run entry, and Linux uses XDG Autostart. Install the app in a stable location first; after moving it, turn the setting off and on again to update its path.

## Online updates

By default, Tervia checks the latest stable GitHub Release in the background at startup and every **60 minutes** while running. **Settings → Updates** provides an automatic-check switch, common check intervals, manual checking, release notes, and **Download and install**. An available update shows a dotted download icon at the top right of the main window; click it to confirm installation. During downloads, the icon becomes a circular progress indicator. It spins while the download size is unknown or verification/installation is in progress, then becomes a restart action after installation. Failures show their cause in the initiating window and keep the update available for retry. Once installed, choose **Restart Tervia**, or restart later yourself.

Intervals are measured from the last completed check, including manual checks and failures. Failed checks retry at the configured interval. Settings changes take effect immediately; editing `[updates]` in `config.toml` takes effect after restarting. Custom `interval_minutes` values from **5 to 10080 minutes (7 days)** are supported and retained in the settings menu. Disabling automatic checks leaves manual checking available. Checks pause during installation and confirmations and stop once an update is installed until restart; downloads and installation always require confirmation.

Updates require an exact platform/architecture match and verify the archive against its SHA-256 digest captured during the check. macOS replaces the complete signed, notarized `.app` after signature and Gatekeeper checks; Windows and Linux replace the executable from the release archive. The installation must be writable by the current user; updates do not elevate privileges. Failed verification or installation preserves the existing app.

Save all edited files across projects and worktrees and wait for pending writes/Git operations before restarting. Restart disconnects the UI while terminal hosts keep their processes alive; the new version reconnects to those same sessions. Development builds support automatic and manual checks but cannot install updates. Older versions need one manual upgrade to a release containing this feature.

## Available interactions

- A compact project icon rail and an expandable project list. Each project keeps its own terminals, tabs, and editing drafts when you switch projects.
- Fixed Terminal / Git / Files tabs, with dark and light themes. Git and Files can retain the same terminal sessions in an adjustable bottom dock; use the terminal icon beside the branch menu, View → Terminal Dock, or Cmd+J / Ctrl+Shift+J. Creating or splitting a dock session keeps the document module.
- Interface support for Simplified Chinese, Traditional Chinese, English, Japanese, Korean, French, German, Spanish, Portuguese, and Russian. The system's preferred language is used by default, and you can switch languages immediately in settings. Preferences are saved in a human-editable TOML configuration file. New Git interface text primarily covers Simplified Chinese and English; some text in other languages falls back to English.
- The terminal recognizes Claude / Codex / Gemini / Grok / Hermes / OpenCode / OpenClaw and displays their brand icons and labels. When an agent exits, the label returns to the shell. macOS / Linux use the foreground process group; Windows uses the current shell's child process tree, so background agents on Windows may also affect the label. Recognition does not scan terminal conversation content.
- A real PTY terminal that loads your local login / interactive shell configuration, with terminal creation, switching, closing, splitting, scrollback, keyboard input, UTF-8 / input methods, ANSI / true color, and a full-screen alternate buffer.
- Drag to select text, then use ⌘C to copy and ⌘V to paste. Bracketed paste, control keys, and application cursor keys are supported. Hold Shift while dragging to select text when a full-screen application enables mouse reporting.
- Cmd-click terminal file references (Ctrl-click on Windows / Linux) to open files at their source line and column, including `src/main.rs:42:7`, quoted paths with spaces, and absolute paths within the current project. Relative references resolve from the selected project directory; ordinary selection and TUI mouse input remain available.
- Terminal Find (Cmd+F, Ctrl+Shift+F on Windows / Linux, or the toolbar Search icon) searches literal rendered text and scrollback. Lowercase queries ignore case; uppercase queries match case. Enter / Shift+Enter moves between matches; Escape returns to the shell. Results are bounded to 5,000 matches. Explicit terminal bells flag unfocused sessions and their project; focusing the pane clears its attention indicator.
- The shell draws its own prompt and input, with support for the basic protocols used by existing prompt themes. There is no separate input field.
- **Local command suggestions**: after typing at least two characters in the shell, a cursor-adjacent list shows up to six prefix matches. For example, `git` offers `git status`, `git stash`, and `git commit`; `git sta` narrows the list. Successful commands from the same directory and shell come first, ranked by frequency and recency and deduplicated, followed by common Git, Cargo, npm, and pnpm commands. No candidate is selected by default: Enter executes the typed command unchanged. Up/Down explicitly selects a candidate; Enter then inserts only its missing suffix, and another Enter executes it. Tab can fill the first candidate directly; clicking also only fills. Escape dismisses the list and returns arrows and Tab to the shell. Editing the prefix clears the selection. Empty prompts and command completion never open suggestions. The list appears below the input, flips above or shows fewer rows when space is limited, and never resizes the terminal. It hides during search, IME composition, scrollback, and fullscreen applications. Shell history navigation, shell Tab completion, or cursor movement suspends prefix completion until the next empty prompt so guessed text cannot be appended to an edited line.
- **Reuse existing shell history**: after startup files load, the shell reports its actual history path. Zsh plain/extended history, Bash plain/timestamped history, Fish history, and PowerShell PSReadLine history are supported, including custom `HISTFILE`, `fish_history` / `XDG_DATA_HOME`, and `HistorySavePath`. Reads are bounded to the last 1 MiB and the latest 2,000 eligible single-line records; leading-space, recognizable multiline, incomplete, and invalid-encoding records are skipped. Bash history without timestamps cannot reliably identify every multiline boundary. Successful current-directory records rank before imported shell history, followed by common commands; each history group is ranked by frequency and recency, with duplicates removed across groups. Shell files stay read-only and imported records are not copied into `command-history.json` or assigned invented directories or exit codes. File changes are picked up on the next completion request. Unsaved history in other terminals becomes available when that shell writes it. Unconfigured or disabled history and Fish private mode do not import history.
- Settings → Interface → Terminal → Local command suggestions, or `[terminal] suggestions = false`, disables suggestions and new history recording. The app retains at most 2,000 command records in `command-history.json` beside `config.toml`; output is never saved and this feature makes no model or network requests. Commands beginning with whitespace or containing newlines are excluded from learning. Quit the app and delete this file to clear history. Damaged history is preserved and reported instead of overwritten.
- Temporary shell startup wrappers collect command boundaries without editing user startup files. Zsh and Bash have real PTY coverage; Fish uses native events, and PowerShell uses PSReadLine. Bash remains a login shell and bootstraps from its first prompt; profiles replacing `PROMPT_COMMAND` entirely, or owning a standalone custom DEBUG trap, disable integration. Other shells and unintegrated SSH/nested shells retain normal terminal behavior.
- Real Git management: Changes / History / Branches / Remotes / Stash, native inline / side-by-side diffs, and hunk staging.
- Files shows the selected project’s directory tree, with lazy loading, virtualized rows and per-project folding. Native file monitoring pauses while the workspace is in the background, refreshes immediately on foreground entry, and updates the tree and clean editor buffers automatically; unsaved drafts are preserved when disk contents change. Open files from the tree, Git or the system file picker, with file tabs, syntax highlighting, search, disk saving and a compact refresh control.
- Files and Git diffs share syntax detection for Rust, JavaScript / JSX, TypeScript / TSX, Python, Go, C / C++, C#, Java, Kotlin, Swift, HTML / CSS, JSON / YAML / TOML, Shell, SQL, Markdown, Vue, Dart, PowerShell, Fish, Dockerfile / Containerfile, INI, XML, JSON5, SCSS / Less, HCL / Terraform, Nix and other bundled languages. Common filename aliases such as Makefile, CMakeLists.txt, Cargo.lock, .env variants, .editorconfig and .npmrc are recognized; Markdown code blocks and embedded web/template code use their own grammars.
- Single-click previews files in one temporary tab; double-click or editing pins it. Cmd+P (Ctrl+P on the Files page) finds filenames; Cmd/Ctrl+Shift+F searches project text. Arrow keys select results, Enter opens them and Escape returns to the tree; text matches jump to their source line/column. Markdown has native reading/source views and common image formats have read-only native previews under the existing 4 MiB read limit. Searches honor ignore rules, skip dependency trees/images/binary files, and report incomplete or capped results.
- File tabs support close buttons, middle-click close, context menus for closing other/right/all tabs, drag reordering, Ctrl+Tab navigation and reopening closed tabs. Unsaved files are confirmed before closing.
- Select source in Files or Git Diff and choose **Copy Context**, or press Cmd/Ctrl+Shift+C, to copy the project-relative path, inclusive line numbers and selected code. Files uses the current draft. Diff uses its displayed source, marks `original` / `modified`, and separates versions or gaps between supplied patch lines; it also supports native text selection and gutter line selection. Markdown must be in Source view; image previews have no code context.
- The welcome page offers Open Project and Use Terminal, recent projects, and a secondary Clone Repository action.
- A separate native settings window with a feature-category sidebar to select installed common monospaced fonts, adjust terminal / code and interface font sizes, preview changes immediately, and restore defaults.

On startup, Tervia restores the last project, local terminal, or welcome screen, together with the project list, its order and the expanded/collapsed sidebar. A project with linked worktrees remembers its last working directory and falls back to its main directory if that worktree is unavailable. First launch opens the welcome page with project and terminal entry points. An unavailable last project returns to the welcome page with an explanation. Terminals start in the project directory, Git operations use your local Git CLI, and saving files writes to disk. Switching projects preserves each project's terminals, commit message, and editing drafts during the current run. Module selection, the bottom dock, shell-tab count/selection, open-file order and source cursor/scroll positions restore after quitting. Unsaved drafts recover with their original saved base, while clean files reload current disk content in the background. Quitting disconnects the UI while terminal hosts keep running. Reopening or updating reconnects to the same shells, running programs and AI conversations, with output, terminal modes and scroll positions restored. New project shells start in their working directory and new local shells start in the home directory.

## Local terminal

The permanent Local Terminal entry at the top of the project sidebar needs no project or directory selection. It starts in your home directory using your existing shell configuration and has independent tabs, split panes, search and output copying. It never scans your home directory or registers it as a project. Switching projects preserves running local sessions.

Cmd+T / Ctrl+Shift+T enters the local terminal from the welcome page and creates a session in the current context elsewhere. Cmd+Shift+N / Ctrl+Shift+N creates a local terminal from any main screen. Closing the last local tab leaves an empty surface with a New Terminal action; each context supports up to 64 tabs. Exit, reopen and application updates preserve live terminal sessions. Explicitly closing a tab ends its session. Running `cd` in a local shell never creates or switches projects; explicitly open a project for Files and Git.

## Terminal session recovery

Independent local hosts own terminal processes. Quitting, closing the main window or updating Tervia disconnects the UI while shells, agents and running programs continue. Reopening reconnects to the same PTY, replays output and resize events in order, then enables live input. Historical bells, clipboard requests and command-history recording are suppressed during replay.

Closing a terminal tab, confirming Close Project or removing a worktree ends the corresponding sessions. Closed tabs and sessions whose terminal process has exited are omitted on restart. If all sessions have ended or been closed, the terminal page stays empty until you create a tab. A running shell is retained even after a foreground command finishes. An unavailable host reports an error instead of silently rerunning commands or substituting a new shell. OS reboot, logout or termination of the host ends live-process continuity.

Metadata, authentication and raw output remain local in `sessions/` beside `config.toml`. Unix directories use `0700` and data files use `0600`; Windows inherits the user configuration directory's access controls. Output streams to disk while in-memory scrollback remains bounded to 10,000 lines. Long-running noisy sessions increase disk use and replay time. Clear Terminal restores the cleared screen; closing the tab or project removes its replay records. Immutable host runtimes in `sessions/runtime/` keep app replacement from disrupting running sessions. On macOS, the complete signed application bundle is retained so its executable, Info.plist, resource seal and notarization metadata remain together.

## Project management

The sidebar Search control, File menu and Cmd+Option+P (Ctrl+Alt+P on Windows/Linux) open global project switching. Search names, paths or abbreviations to enter a project or a known working directory/worktree. Empty searches prioritize favorites, then recent projects; Back to Previous Project restores the prior project's last directory.

Favorite projects with the star or context menu. Drag rows within the favorite or ordinary group to reorder them; Move Up/Down provides the same operation from the context menu. Sidebar order stays stable while switching projects and survives restarting.

Close Project saves drafts, reading positions and layout to workspace recovery before ending that project's terminal sessions with a named confirmation. Failed recovery saves keep the project and terminals open; Git/file operations must finish before closing. Recently Closed Projects is available in the sidebar, File menu and welcome screen. Reopening restores the editing context and starts new shells when needed. Closing leaves project files on disk unchanged and preserves local terminal processes. Closing the final project returns to existing local sessions, or to the welcome page if there are none.

## Configuration

Tervia creates a commented `config.toml` on first launch. Use **config.toml** at the bottom of the Settings sidebar to find it:

| Platform | Default location |
| --- | --- |
| macOS | `~/Library/Application Support/tervia/config.toml` |
| Linux | `$XDG_CONFIG_HOME/tervia/config.toml`, or `~/.config/tervia/config.toml` |
| Windows | `%APPDATA%\tervia\config.toml` |

Set `TERVIA_CONFIG_FILE` to an absolute file path to override this location. The configuration applies to the whole app, regardless of the project directory.

The app stores project history in `workspace.json` alongside `config.toml`, including when `TERVIA_CONFIG_FILE` overrides the configuration location. Opening or switching projects, changing the sidebar and removing worktrees save this state automatically. Writes replace the file atomically. Invalid state or changes made by another running instance display a project-history warning and preserve the existing file; fix the file and restart to resume saving. Preferences remain independent of project history. Recovery uses workspace schema v2, reads legacy v1 history, and coalesces ordinary writes over 300 ms; project navigation and quitting flush immediately. Missing directories with unsaved drafts remain in the state file for later recovery. Each directory supports recovery of up to 64 shell tabs and 512 open files; oversized or invalid state reports an error without replacing the existing file.

Top-level tables represent feature domains: `ui` contains appearance and its `fonts` subtable, `files` controls explorer grouping, and `updates` contains automatic-check settings. Future terminal, editor, Git and workspace settings can use separate top-level domains without flattening unrelated options together.

```toml
[ui]
language = "system" # system, zh-CN, zh-TW, en, ja, ko, fr, de, es, pt, ru
theme = "dark"      # dark or light

[ui.fonts]
code_family = "Menlo" # Installed font shared by terminal and editor
code_size = 14        # 11–24
ui_size = 13          # 11–16

[files]
nesting = false       # Group related files in the explorer

[updates]
auto_check = true
interval_minutes = 60 # 5–10080; default: 1 hour
```

The default code font is Menlo on macOS, Consolas on Windows and DejaVu Sans Mono on Linux. Omitted settings use defaults. See [the commented template](../assets/config.toml) for all supported settings.

Enable **Settings → Interface → File list → Nest related files** to group companion files in the same directory. Built-in rules cover `.env`, Docker, package managers, Rust, Python and common source/test pairs, inspired by [antfu's File Nesting Config](https://github.com/antfu/vscode-file-nesting-config). Groups start collapsed; the arrow expands them while the filename opens the file. The switch applies immediately across projects and is off by default.

Settings-window changes, the theme toggle and Restore defaults apply immediately and save to the same file. Saves preserve comments and unknown settings, replace the file atomically and retain dotfile symlinks. Manual edits take effect after restarting Tervia. If the file changes while the app is running, saving stops and asks you to restart instead of overwriting those edits.

Invalid TOML, unsupported values or read errors fall back to defaults and show a configuration error. The original file remains untouched; open Settings to view details and locate it. Failed saves leave changes active for the current session, with an error shown in Settings. Fix the file or permissions and restart.

## Git management

- Open real working directories, initialize repositories, or clone into new directories. The sidebar stays a flat project list; selecting a project restores its last working directory. The titlebar shows the current project and directory, with a searchable directory picker (⌘⌥O on macOS, Ctrl+Alt+O elsewhere). Add directories through the picker’s native folder chooser or Set as Working Directory in a Files folder menu. Git child repositories appear automatically alongside manually added directories, with the root always available. Each directory keeps its own terminals, drafts and selected module; workspace history remembers the list and last selection. Git discovery refreshes the picker without adding projects to the sidebar. Both `.git` directories and `.git` files are supported.
- History, changed files, Git branches, the Git sidebar, stash entries, and raw text details use virtualized rendering for visible rows. Folders and groups for changes / branches / tags can be collapsed, with state preserved per project.
- A continuous gray-blue gradient spans the entire window. Collapsible rows highlight across their full width on hover, with padding to the right of the counts.

Child repository scanning runs in the background, traversing at most 3 child-directory levels, 8,192 directories, and 128 Git working directories, with a 3-second time budget. Partial scans identify whether time, depth, directory count, working-directory count, or unreadable folders caused incomplete coverage. Worktrees still appear under one logical repository. Scanning skips `.git` metadata, node_modules, target, build, .venv, and .cache, and does not follow child directory symlinks. Missed repositories can be opened directly or found by rescanning.

- Index and worktree status are shown separately. A file can appear in both staged and unstaged groups. Additions, modifications, deletions, renames, and conflicts are supported.
- Discard one file or all files from the unstaged group after confirmation; files restore from the index, preserving staged changes. Untracked files can be moved to the system Trash individually or as a group. Ignored files and unsaved editor drafts are preserved. Conflicts, submodules and nested repositories require separate handling; failed trash operations never fall back to permanent deletion.
- Native virtualized diffs with syntax highlighting, inline / side-by-side views, and staging / unstaging by file or hunk. Snapshots are checked before applying hunks, preserving CRLF and trailing whitespace.
- Git review starts with a collapsed commit form, retains drafts, and offers an expanded Diff, previous/next file and change navigation, and Back to Diff after opening a full file. Recent reading positions are retained when the patch is unchanged.
- Mark Reviewed records the current file/index/HEAD version; refreshed content changes clear its marker, including edits that leave the same Git status letter. Set Checkpoint captures current disk content, including untracked files that Git does not ignore, without changing the real index. Compare Checkpoint shows the next round of changes in a focused Diff, with manual refresh and Back to Working Copy; staging remains in the ordinary working-copy view. Checkpoints and reviewed markers last for the current run.
- Multiline commit messages, regular commits, and amend. Failed commits preserve the message, and commits include only the index. Local hooks and signing behavior are preserved.
- Click a local branch, remote branch, or tag to browse its reachable commits without switching the checkout. Double-click a branch to switch, or use the searchable branch menu in the top navigation or Git toolbar (⌘⌥B / Ctrl+Alt+B). Recently used branches are prioritized; keyboard selection keeps its branch identity across refreshes, and an existing local tracking branch is reused for remote targets. The picker preserves the active module and shows checkout errors without discarding local changes. Right-click or open the history toolbar’s action menu for merge, rebase, rename, and delete. Section Plus buttons open compact branch/tag creation forms.
- Commit graphs show parent relationships, forks, and merges alongside branch/tag labels. Filter by author / message / ref / hash; gaps mark commits hidden by the filter. History loads up to 2,000 commits, preserving lanes across scrolling and pagination. Parent links open commit details even beyond the loaded page. Commit details include file navigation and merge changes relative to the first parent; full metadata remains expandable.
- Create / switch / rename / delete local branches, track remote branches, and create / delete tags.
- Worktrees share one project entry, with independent directory state and a collapsible Git sidebar list. The main directory has a separate badge that remains visible beside long branch names. Right-click a worktree to open it, copy its path, reveal it in the file manager, or lock/unlock it. **Remove** is separated at the bottom and confirms the full target path. The main directory has no lock action; opening the current directory and removing protected directories are disabled. The management page retains the full set of operations. Create from a new or existing branch, or detached HEAD; open, lock/unlock, and prune missing registrations. Removal keeps the branch and checks Git changes and unsaved editor drafts; force removal is not exposed.
- Add / remove remotes, update URLs, fetch, fast-forward pull, merge / rebase pull, push, and set an upstream for the first time.
- Save stashes, including untracked files; preview, apply, pop, and drop them. Operations are bound to object IDs, and deletion validates the target while holding a Git reference lock. Currently supports the files reference storage backend; other formats preserve the entry and prompt you to handle it in the terminal.
- Merge / rebase status, viewing all three conflict stages, editing the result, marking conflicts resolved, continuing, and aborting. Deleting a file is also supported as a resolution.
- The Git page refreshes in the background every 3 seconds, when opened, and after operations complete. Background results retain their project association, and stale diff / file reads are rejected.

Git uses your existing configuration, SSH, credential helpers, and signing programs. Interactive authentication can be completed in the terminal. The editor is limited to UTF-8 text and files up to 4 MiB. Non-UTF-8 diffs display a notice, while whole-file staging preserves the original bytes. Git output is capped at 8 MiB. Interactive rebase and PR / Issue / CI integration are planned for later.

When creating a terminal, the app removes `NO_COLOR` inherited from the launcher and declares `xterm-256color` / `truecolor` support, so launching from a command environment without colors does not cause TUIs to disable colors automatically. Your shell configuration or individual commands can still explicitly set `NO_COLOR`.

The terminal currently focuses on standard xterm / VT interaction. Image protocols and the complete Kitty keyboard protocol are not yet implemented. Compatibility needs to be verified for individual plugins and applications.

## Keyboard shortcuts

The native menu bar includes File, View and Terminal menus for opening folders/files, file and project text search, saving and managing tabs, switching modules/themes, and creating, splitting, clearing or copying terminal output. Menus follow the selected interface language and disable unavailable actions. Clear and Copy Output target the focused terminal pane.

| macOS | Windows / Linux | Action |
| --- | --- | --- |
| ⌘O | Ctrl+Shift+O | Open folder |
| ⌘B | Ctrl+Shift+B | Expand / collapse the project rail |
| ⌘J | Ctrl+Shift+J | Show / hide the bottom terminal in Git / Files |
| ⌘F in Terminal | Ctrl+Shift+F in Terminal | Find in terminal output and scrollback |
| ⌘1 / ⌘2 / ⌘3 | Ctrl+1 / Ctrl+2 / Ctrl+3 | Terminal / Git / Files |
| ⌘T | Ctrl+Shift+T | New terminal in the current context; enter a terminal from welcome |
| ⌘Shift+N | Ctrl+Shift+N | New local terminal |
| ⌘S | Ctrl+S | Save the current file to disk |
| ⌘W | Ctrl+W | Close the current file tab |
| ⌘W | Ctrl+Shift+W | Close the current terminal tab; projects retain their final terminal |
| Ctrl+Tab / Ctrl+Shift+Tab | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous file tab |
| ⌘Shift+T | Ctrl+Shift+T | Reopen the last closed file tab |
| ⌘F | Ctrl+F | Find in the editor |
| ⌘⇧P | Ctrl+Shift+P | Quick actions |
| ⌘⌥B | Ctrl+Alt+B | Search and switch Git branches |
| ⌘⌥P | Ctrl+Alt+P | Search projects and working directories |
| ⌘, | Ctrl+, | Open the separate settings window |
| ⌘C / ⌘V | Ctrl+Shift+C / Ctrl+Shift+V | Copy the terminal selection / paste into the terminal |
| ⌘⇧C | Ctrl+Shift+C | Copy selected code with its path and line numbers in Files / Git Diff |
| ⌘K | Ctrl+Shift+K | Clear the focused terminal's screen and scrollback, and request a redraw |
| Ctrl+K | Ctrl+K | Send to the shell / TUI, preserving the application's shortcut |
| ⌘Backspace | Ctrl+U (Bash/Zsh) or Ctrl+Home (PowerShell) | Invoke shell line deletion; clears the current command when the cursor is at the end |
| ⌘Left / ⌘Right | Home / End | Move to the start / end using the shell’s editing keys |
| Option+Backspace | Ctrl+Backspace | Delete the preceding word; boundaries follow the shell’s keymap |
| — | Ctrl+Left / Ctrl+Right / Ctrl+Delete | Send modified cursor / Delete sequences to the shell or TUI |
| ⇧PageUp / ⇧PageDown | Shift+PageUp / Shift+PageDown | Page through terminal scrollback |
| Esc | Esc | Send to the application when the terminal is focused; elsewhere, close popovers first, then the window |

## Verification

```sh
python3 -B -m unittest discover -s tests -p 'test_*.py'
cargo test --no-default-features
cargo test --features native
cargo test --features ui-tests
cargo fmt --all -- --check
cargo clippy --features native --all-targets -- -D warnings
cargo build --features native
```

## Project structure

- `src/config.rs`, `assets/config.toml`: User configuration, validation and comment-preserving persistence.
- `src/state.rs`: Real project, terminal, and editing draft state, independent of GPUI.
- `src/workspace.rs`: Atomic project-history persistence and startup restoration.
- `src/terminal.rs`, `src/pty.rs`: Terminal parsing, key encoding, selection, and the real PTY lifecycle.
- `src/locale.rs`, `assets/locales.json`: System language matching and interface text.
- `src/agent.rs`: Pure path-based rules for identifying foreground agents.
- `src/platform.rs`, `src/process.rs`: Platform fonts, shells, shortcuts, and background process queries.
- `src/ui/console.rs`: Native GPUI terminal rendering, input methods, and interaction.
- `src/git/`: Git commands and output, status parsing, diffs, repository snapshots, and write operations.
- `src/files/`: Disk reads / writes, scope checks, and version guards for asynchronous reads.
- `src/ui/`: The application shell, terminal, Git, file views, and visual styles.
- `tests/git.rs`: Git integration tests using only synthetic temporary repositories and local bare remotes.
- `tests/reads.rs`: Tests for asynchronous reloading and draft protection.
- `tests/workspace.rs`: Tests for project isolation and terminal / workspace lifecycles.
- `PRODUCT.md`, `DESIGN.md`: Confirmed product boundaries and interface design.
