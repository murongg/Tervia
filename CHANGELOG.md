# Changelog

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
