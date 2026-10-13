use super::*;

pub(super) fn choose_detail(
    mode: Mode,
    selected: Option<&Detail>,
    snapshot: &Snapshot,
) -> Option<Detail> {
    let retained = selected.and_then(|selected| match selected {
        Detail::File(old, side) => snapshot
            .status
            .files
            .iter()
            .find(|file| {
                file.path == old.path
                    && !file.conflicted
                    && if *side == Side::Index {
                        file.index != '.'
                    } else {
                        file.worktree != '.'
                    }
            })
            .map(|file| Detail::File(file.clone(), *side)),
        Detail::Conflict(path) => snapshot
            .status
            .files
            .iter()
            .find(|file| &file.path == path && file.conflicted)
            .map(|file| Detail::Conflict(file.path.clone())),
        Detail::Stash(hash) => snapshot
            .stashes
            .iter()
            .find(|stash| &stash.hash == hash)
            .map(|stash| Detail::Stash(stash.hash.clone())),
        Detail::Commit(hash) => Some(Detail::Commit(hash.clone())),
    });
    if retained.is_some() {
        return retained;
    }
    match mode {
        Mode::Changes => snapshot
            .status
            .files
            .iter()
            .find(|file| file.conflicted)
            .map(|file| Detail::Conflict(file.path.clone()))
            .or_else(|| {
                snapshot
                    .status
                    .files
                    .iter()
                    .find(|file| file.worktree != '.')
                    .map(|file| Detail::File(file.clone(), Side::Worktree))
            })
            .or_else(|| {
                snapshot
                    .status
                    .files
                    .iter()
                    .find(|file| file.index != '.')
                    .map(|file| Detail::File(file.clone(), Side::Index))
            }),
        Mode::History => snapshot
            .commits
            .first()
            .map(|commit| Detail::Commit(commit.hash.clone())),
        Mode::Stash => snapshot
            .stashes
            .first()
            .map(|stash| Detail::Stash(stash.hash.clone())),
        Mode::Branches | Mode::Remotes | Mode::Worktrees => None,
    }
}

pub(super) fn branch_target<'a>(
    selected: Option<&str>,
    snapshot: &'a Snapshot,
) -> Option<&'a tervia::git::Branch> {
    snapshot
        .branches
        .iter()
        .find(|branch| Some(branch.name.as_str()) == selected)
        .or_else(|| snapshot.branches.iter().find(|branch| branch.current))
}

impl Tervia {
    pub(super) fn select_git_mode(
        &mut self,
        project: usize,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        // Even reselecting the current view supersedes a pending asynchronous directory open.
        ui.cancel_worktree_navigation();
        if ui.mode == mode {
            return;
        }
        ui.mode = mode;
        ui.clear_detail(cx);
        let language = self.appearance.language;
        ui.first.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(
                language.text(match mode {
                    Mode::Branches => "git_name",
                    Mode::Remotes => "git_remote_name",
                    Mode::Stash => "git_stash_message",
                    _ => "git_filter",
                }),
                window,
                cx,
            );
        });
        ui.second.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(
                language.text(if mode == Mode::Remotes {
                    "git_remote_url"
                } else {
                    "git_new_name"
                }),
                window,
                cx,
            );
        });
        if let Some(detail) = ui
            .snapshot
            .as_ref()
            .and_then(|snapshot| choose_detail(mode, None, snapshot))
        {
            self.git_detail(project, detail, window, cx);
        }
        cx.notify();
    }
}
