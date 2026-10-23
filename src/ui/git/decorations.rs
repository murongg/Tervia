use super::{FileStatus, GitUi};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) fn collect(files: &[FileStatus]) -> BTreeMap<PathBuf, char> {
    let mut statuses = BTreeMap::new();
    for file in files {
        let status = if file.conflicted {
            '!'
        } else if file.worktree != '.' {
            file.worktree
        } else {
            file.index
        };
        let status = match status {
            '.' => continue,
            '?' => 'U',
            status => status,
        };
        statuses.insert(file.path.clone(), status);
        // Renames also change the old directory; copies leave their source untouched.
        for path in std::iter::once(file.path.as_path()).chain(
            file.original
                .as_deref()
                .filter(|_| file.index == 'R' || file.worktree == 'R'),
        ) {
            for parent in path.ancestors().skip(1) {
                if parent.as_os_str().is_empty() {
                    break;
                }
                statuses
                    .entry(parent.into())
                    .and_modify(|current| {
                        if status == '!' {
                            *current = '!';
                        }
                    })
                    .or_insert(if status == '!' { '!' } else { 'M' });
            }
        }
    }
    statuses
}

impl GitUi {
    pub(in crate::ui) fn file_decoration(&self, path: &Path) -> Option<char> {
        self.snapshot.as_ref()?;
        // Git paths stay relative to the repository, even when Files opens a subdirectory.
        let path = path.strip_prefix(&self.repository.as_ref()?.root).ok()?;
        self.decorations.get(path).copied()
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::path::Path;
    use tervia::git::FileStatus;

    fn file(path: &str, index: char, worktree: char) -> FileStatus {
        FileStatus {
            path: path.into(),
            original: None,
            index,
            worktree,
            conflicted: false,
        }
    }

    #[test]
    fn explorer_decorations_cover_index_worktree_untracked_and_conflicts() {
        let mut conflict = file("src/conflict.rs", 'A', 'A');
        conflict.conflicted = true;
        let statuses = collect(&[
            file("src/staged.rs", 'A', '.'),
            file("src/modified.rs", 'A', 'M'),
            file("src/untracked.rs", '?', '?'),
            file("src/deleted.rs", '.', 'D'),
            file("src/type.rs", '.', 'T'),
            conflict,
            file("src/clean.rs", '.', '.'),
        ]);
        for (path, status) in [
            ("src/staged.rs", 'A'),
            ("src/modified.rs", 'M'),
            ("src/untracked.rs", 'U'),
            ("src/deleted.rs", 'D'),
            ("src/type.rs", 'T'),
            ("src/conflict.rs", '!'),
            ("src", '!'),
        ] {
            assert_eq!(statuses.get(Path::new(path)), Some(&status), "{path}");
        }
        assert!(!statuses.contains_key(Path::new("src/clean.rs")));
        assert!(!statuses.contains_key(Path::new("src-other")));
    }

    #[test]
    fn explorer_decorations_mark_rename_parents_but_keep_copy_sources_clean() {
        let mut renamed = file("new/deep/renamed.rs", 'R', '.');
        renamed.original = Some("old/deep/original.rs".into());
        let mut copied = file("copies/copy.rs", 'C', '.');
        copied.original = Some("unchanged/source.rs".into());
        let statuses = collect(&[renamed, copied]);
        for path in ["old", "old/deep", "new", "new/deep", "copies"] {
            assert_eq!(statuses.get(Path::new(path)), Some(&'M'), "{path}");
        }
        assert_eq!(statuses.get(Path::new("new/deep/renamed.rs")), Some(&'R'));
        assert_eq!(statuses.get(Path::new("copies/copy.rs")), Some(&'C'));
        assert!(!statuses.contains_key(Path::new("unchanged")));
        assert!(collect(&[]).is_empty());
    }
}
