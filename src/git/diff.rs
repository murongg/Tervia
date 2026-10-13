use super::{FileStatus, Repository, Side, arguments, command, text};

impl Repository {
    pub(super) fn apply_hunk(
        &self,
        file: &FileStatus,
        side: Side,
        index: usize,
        expected: &str,
    ) -> Result<String, String> {
        if file.conflicted || (side == Side::Worktree && file.worktree == '?') {
            return Err("Stage this file as a whole".into());
        }
        // Context and line offsets come from a particular index/worktree pair.
        // Never apply a selection after an external editor or terminal changed it.
        if self.diff(file, side)? != expected {
            return Err("The diff changed. Refresh it before staging this hunk.".into());
        }
        // Patch bytes carry CRLF and trailing whitespace that belong to the
        // source. Rebuilding lines with lines() silently changes that content.
        let lines: Vec<_> = expected.split_inclusive('\n').collect();
        let starts: Vec<_> = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.starts_with("@@ ").then_some(index))
            .collect();
        let start = *starts.get(index).ok_or("Hunk no longer exists")?;
        let end = starts.get(index + 1).copied().unwrap_or(lines.len());
        let patch = lines[..starts[0]]
            .iter()
            .chain(&lines[start..end])
            .copied()
            .collect::<String>();
        let mut args = arguments(&["apply", "--cached", "--whitespace=nowarn"]);
        if side == Side::Index {
            args.push("--reverse".into());
        }
        command::run_input(&self.root, &args, Some(patch.into_bytes())).map(text)
    }
}
