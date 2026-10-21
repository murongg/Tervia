# Changelog

## v0.17.1 (2026-10-21)

### Fixes

- **terminal:** initialize missing macOS locale before shell startup (67f0b1d0)
- **ui:** prevent Escape from closing the workspace (adc57337)

### Other changes

- **github:** add issue and pull request templates (5096b976)
- change project license to MIT (c18ec3f8)

## v0.17.0 (2026-10-21)

### Features

- **appearance:** add custom theme editor (51998ccd)

### Fixes

- **projects:** hide worktrees from the directory picker (a303a1f4)

## v0.16.0 (2026-10-20)

### Features

- **appearance:** add theme presets and system mode (cedb5540)
- **projects:** add an instant hover project panel (6c08f2b7)

### Fixes

- **projects:** separate pin and search click targets (4e49ab6c)

## v0.15.1 (2026-10-19)

### Performance

- reduce terminal and workspace refresh overhead (5cb9dba1)

## v0.15.0 (2026-10-19)

### Features

- **agents:** add opt-in status and notifications (776b2038)
- **tasks:** add project task launcher (5b6e5127)

### Other changes

- **settings:** reveal toggles before clicking (e7b025a7)
- **windows:** compare packaged license with source bytes (6917dfe7)
- license project under AGPL-3.0-only (84d08e86)

## v0.14.1 (2026-10-18)

### Fixes

- **git:** unify hover across composite rows (40eb31fa)
- **autostart:** handle missing Windows registry entries (f8084bd7)
- **windows:** restore titlebar dragging and double-click restore (d2190bf6)

## v0.14.0 (2026-10-18)

### Features

- **terminal:** add Windows shell selection (005220de)

### Fixes

- **git:** add a close button to error banners (5ddf5c81)

## v0.13.2 (2026-10-18)

### Fixes

- **packaging:** size macOS images from apparent file sizes (5f5806c5)

## v0.13.1 (2026-10-18)

### Fixes

- **linux:** add client-side window controls (d7db2b68)
- **windows:** add missing window controls (4029f142)
- **windows:** prevent extra console windows on startup (d7be41db)

### Other changes

- **packaging:** add desktop distribution formats (9e086d35)

## v0.13.0 (2026-10-18)

### Features

- **files:** add basic file management (257d40f0)
- **settings:** organize preferences into focused categories (0c62077b)

### Other changes

- **files:** compare relocated paths across platforms (a96e6672)

## v0.12.2 (2026-10-17)

### Fixes

- **terminal:** render empty project status bars safely (96ed2476)

## v0.12.1 (2026-10-17)

### Fixes

- **terminal:** restore only surviving terminal tabs (5906951d)
- **terminal:** preserve signed macOS host bundles (44bfea8e)
- **sessions:** retry transient executable busy errors (5c2567d4)
- **git:** match sidebar divider to workbench borders (931b29d7)
- **terminal:** retry busy Linux session hosts (283dc9b8)

## v0.12.0 (2026-10-17)

### Features

- **terminal:** preserve sessions across app restarts and updates (e04b6c92)

### Fixes

- **git:** make navigation sidebar resizable (44a5cd75)
- **files:** make ignored explorer entries visibly dimmer (3beb1056)

### Other changes

- **terminal:** handle replay events in Windows PTY checks (e130e51b)

## v0.11.0 (2026-10-17)

### Features

- **terminal:** add persistent tab renaming (bc1ff8bf)
- **terminal:** add local command completion and shell history (57789cc0)

### Fixes

- **terminal:** preserve typed commands when pressing Enter (22de6b07)
- **terminal:** avoid selecting cells on a single click (3ee03190)
- **build:** use MSVC-compatible SCSS dependency (d746a820)
- **files:** highlight more languages and file types (536deb5e)

## v0.10.0 (2026-10-17)

### Features

- **terminal:** add local terminal and startup restoration (67cd6134)
- **git:** enrich worktree context menus (06e1cbc6)
- **git:** add worktree removal menu and main badge (45eaba29)
- **autostart:** add launch at login setting (8d0c185b)

### Other changes

- **settings:** reveal file nesting toggle before clicking (f442c738)

## v0.9.0 (2026-10-16)

### Features

- **git:** add resizable collapsible sections (4e01d26a)

### Fixes

- **git:** restore branch switcher popup rendering (de3d810b)

### Other changes

- **git:** check compact sections across window heights (d30f4194)

## v0.8.0 (2026-10-16)

### Features

- **files:** add Material Icon Theme file icons (8d6483f4)
- **files:** dim Git-ignored explorer entries (93388f85)

## v0.7.0 (2026-10-15)

### Features

- **git:** add file and bulk discard actions (3065de05)

### Fixes

- **ui:** restore window maximization after restart (25ea8267)

### Documentation

- **git:** use the correct discard screenshot extension (907e7f5d)

### Other changes

- **git:** make discard fixture line endings deterministic (87234cde)

## v0.6.0 (2026-10-15)

### Features

- **projects:** add switching, favorites, and close/reopen (30b2d53b)
- **files:** add optional related-file nesting (1828ec03)

### Fixes

- **ui:** enable terminal drag scrolling and auto-hide scrollbars (cccb6a01)
- **updates:** correct primary action button colors (e84249f5)

### Documentation

- **readme:** introduce the product and move detailed guides (8742faab)
- **ui:** show transient terminal scrollbars (605324d3)

### Other changes

- **projects:** match shortcut initialization to app startup (0d05ace5)

## v0.5.0 (2026-10-15)

### Features

- **workspace:** streamline AI CLI coding workflows (03475cbb)

### Fixes

- **files:** expand syntax highlighting coverage (f9ab0308)

## v0.4.0 (2026-10-15)

### Features

- **git:** add searchable quick branch switching (76c37681)
- **git:** browse branch history without switching checkouts (28bf9706)
- **workspace:** move working directory switching to the titlebar (a0af4f8e)

## v0.3.1 (2026-10-15)

### Fixes

- **git:** show stage icons in diff hunk headers (1b2c17c4)
- **updates:** enable packaged updates and show circular progress (88daf614)

### Documentation

- **git:** add clean hunk header preview (1ea62ddf)

## v0.3.0 (2026-10-14)

### Features

- **ui:** add native workspace menus (c0ddaa9d)
- **workspace:** improve review and file browsing (43284137)
- **git:** separate untracked changes in working copy (c3dd4ab9)
- **git:** add commit graph and parent navigation (2a03efe7)

### Fixes

- **git:** centralize repository probing and error handling (0819d6c7)
- **gpui:** retain row height estimates across layout changes (f562fae7)
- **files:** align workbench dividers (5ef86b79)
- **git:** hide expected non-repository probe errors (d5164d1a)

### Other changes

- **search:** compare native paths across platforms (22fcfe52)

## v0.2.0 (2026-10-14)

### Features

- **workspace:** unify directory navigation (ab428dfe)
- **updates:** schedule configurable automatic checks (7b0452c9)
- **updates:** add verified desktop updates and restart (9f8c5d7d)

### Fixes

- **files:** start explorer at directory contents (ddac56ce)
- **workspace:** restore open projects after restart (38fae62f)
- **ui:** simplify welcome screen styling (72f4c622)
- **ui:** toggle window zoom on titlebar double-click (ac39ae72)

### Documentation

- **updates:** show automatic check settings (d4cd323d)

### Other changes

- **workspace:** make nested directory selectors portable (c5ac7c61)
- add pull request checks across desktop platforms (e73f0c1b)
- restrict native workflow triggers to main pushes (78c61db5)
- **workspace:** make save failure checks portable (d74dc5d7)
- **updates:** use host-native paths in bundle checks (79ed794c)

## v0.1.2 (2026-10-13)

### Fixes

- **release:** register the temporary signing keychain (c4ec8b72)

### Other changes

- allow validating one desktop platform (552e4b68)
- cache Rust dependencies across desktop builds (7d51ffbf)

## v0.1.1 (2026-10-13)

### Fixes

- **git:** normalize repository and worktree identities (1ed6dfc8)
- **build:** avoid duplicate Windows manifests (14b0e666)

### Other changes

- **windows:** isolate the ConPTY round-trip script (86b602ed)
- **windows:** complete the ConPTY cursor handshake (f4742810)
- **windows:** resolve the isolated ConPTY executable (58db5aab)

## v0.1.0 (2026-10-13)

### Features

- add native terminal, Git and file workspace (9fb565ef)

### Other changes

- automate signed desktop releases (ebf161c7)
