mod branches;
mod changes;
mod commits;
mod graph;
mod history;
mod navigation;
mod remotes;
mod review;
mod scans;
pub(super) use scans::Scans;
mod sidebar;
mod stash;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;
mod viewer;
mod views;
mod worktrees;

use super::{
    Tervia,
    controls::{icon_style, label_button},
};
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    diff::{DiffFile, DiffMode, DiffState},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::Arc;
use tervia::git::{FileStatus, Operation, Repository, Side, Snapshot};
use tervia::state::Tab;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Changes,
    History,
    Branches,
    Remotes,
    Stash,
    Worktrees,
}

#[derive(Clone, PartialEq, Eq)]
enum Detail {
    File(FileStatus, Side),
    Commit(String),
    Stash(String),
    Conflict(PathBuf),
}

pub(super) struct GitUi {
    pub snapshot: Option<Arc<Snapshot>>,
    workspace: Option<Arc<tervia::git::Discovery>>,
    changes_scroll: UniformListScrollHandle,
    history_scroll: UniformListScrollHandle,
    history_graph: Arc<tervia::git::graph::Graph>,
    sidebar_scroll: UniformListScrollHandle,
    branches_scroll: UniformListScrollHandle,
    raw_scroll: UniformListScrollHandle,
    metadata_scroll: UniformListScrollHandle,
    commit_files_scroll: UniformListScrollHandle,
    stash_scroll: UniformListScrollHandle,
    worktrees: worktrees::WorktreesUi,
    collapsed_changes: std::collections::BTreeSet<u8>,
    collapsed_folders: std::collections::BTreeSet<(u8, PathBuf)>,
    collapsed_sections: std::collections::BTreeSet<&'static str>,
    change_rows: Arc<[changes::ChangeRow]>,
    #[cfg(all(test, feature = "ui-tests"))]
    rendered_rows: std::cell::Cell<usize>,
    pub(super) repository: Option<Repository>,
    mode: Mode,
    branch: Option<String>,
    busy: bool,
    pub(super) error: String,
    output: String,
    detail: String,
    raw_lines: Arc<[SharedString]>,
    metadata_lines: Arc<[SharedString]>,
    commit: Option<tervia::git::Commit>,
    commit_file: Option<SharedString>,
    selected: Option<Detail>,
    generation: usize,
    detail_loading: bool,
    limit: usize,
    first: Entity<InputState>,
    second: Entity<InputState>,
    message: Entity<TextareaState>,
    diff: Entity<DiffState>,
    review: review::Review,
}

impl GitUi {
    fn reconcile_snapshot(&mut self, snapshot: &Snapshot, cx: &mut Context<Tervia>) {
        if !self.snapshot.as_ref().is_some_and(|previous| {
            previous.commits.len() == snapshot.commits.len()
                && previous
                    .commits
                    .iter()
                    .zip(&snapshot.commits)
                    .all(|(old, new)| old.hash == new.hash && old.parents == new.parents)
        }) {
            // Lay out the whole loaded DAG once; viewport boundaries must not reset its lanes.
            self.history_graph = Arc::new(tervia::git::graph::Graph::new(&snapshot.commits));
        }
        if !self
            .snapshot
            .as_ref()
            .is_some_and(|previous| previous.status.files == snapshot.status.files)
        {
            self.review.files = review::files(snapshot);
            self.change_rows = changes::change_rows(
                &snapshot.status.files,
                &self.collapsed_changes,
                &self.collapsed_folders,
            )
            .into();
        }
        let selected = navigation::choose_detail(self.mode, self.selected.as_ref(), snapshot);
        if selected != self.selected {
            // Invalidate before assigning an automatic replacement; otherwise begin_detail
            // would mistake the new selection for the owner of the previous patch.
            self.clear_detail(cx);
            self.selected = selected;
        }
    }

    fn clear_detail(&mut self, cx: &mut Context<Tervia>) {
        self.prepare_diff(None, cx);
        self.generation += 1;
        self.selected = None;
        self.detail.clear();
        self.raw_lines = Arc::from([]);
        self.metadata_lines = Arc::from([]);
        self.commit = None;
        self.commit_file = None;
        self.detail_loading = false;
    }
    fn begin_detail(&mut self, detail: Detail, cx: &mut Context<Tervia>) -> usize {
        if self.selected.as_ref() != Some(&detail) {
            self.prepare_diff(Some(&detail), cx);
            self.detail.clear();
            self.raw_lines = Arc::from([]);
            self.metadata_lines = Arc::from([]);
            self.commit = None;
            self.commit_file = None;
        }
        if let Detail::Commit(hash) = &detail
            && let Some(commit) = self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.commits.iter().find(|commit| &commit.hash == hash))
        {
            self.commit = Some(commit.clone());
        }
        self.generation += 1;
        self.selected = Some(detail);
        self.detail_loading = true;
        self.generation
    }

    fn install_commit(&mut self, generation: usize, commit: tervia::git::Commit) {
        // Metadata and the patch share the same owner, including navigation beyond a loaded page.
        if self.generation == generation {
            self.commit = Some(commit);
        }
    }

    fn install_detail(
        &mut self,
        generation: usize,
        result: Result<String, String>,
        cx: &mut Context<Tervia>,
    ) {
        // A slow request for a previous selection must never change the new viewer.
        if self.generation != generation {
            return;
        }
        self.detail_loading = false;
        match result {
            Ok(detail) => {
                if self.detail != detail {
                    let files = DiffFile::parse(&detail).unwrap_or_default();
                    // Reuse a position only after the fresh patch matches its saved document.
                    if let Some(saved) = self
                        .review
                        .restore
                        .take()
                        .filter(|saved| saved.patch == detail)
                    {
                        let mode = self.diff.read(cx).mode();
                        self.diff = saved.diff;
                        self.diff.update(cx, |state, cx| state.set_mode(mode, cx));
                        self.observe_diff(cx);
                        self.commit_file = saved.file;
                        self.review.range = saved.range;
                    } else {
                        self.review.range = None;
                        self.commit_file = files.first().map(|file| file.path().clone());
                        self.diff.update(cx, |state, cx| state.set_files(files, cx));
                    }
                    let commit = matches!(self.selected, Some(Detail::Commit(_)));
                    let body = if commit {
                        viewer::commit_patch(&detail)
                    } else {
                        &detail
                    };
                    self.raw_lines = body
                        .lines()
                        .map(|line| SharedString::from(line.to_owned()))
                        .collect::<Vec<_>>()
                        .into();
                    self.metadata_lines = if commit {
                        detail
                            .split("\ndiff --git ")
                            .next()
                            .unwrap_or_default()
                            .lines()
                            .take(1000)
                            .map(|line| SharedString::from(line.to_owned()))
                            .collect::<Vec<_>>()
                            .into()
                    } else {
                        Arc::from([])
                    };
                    self.detail = detail;
                }
            }
            Err(error) => {
                self.review.range = None;
                self.detail.clear();
                self.raw_lines = Arc::from([]);
                self.metadata_lines = Arc::from([]);
                self.diff.update(cx, |state, cx| state.set_files([], cx));
                self.error = error;
            }
        }
    }
    pub fn subscribe(&self, subscriptions: &mut Vec<Subscription>, cx: &mut Context<Tervia>) {
        self.worktrees.subscribe(subscriptions, cx);
        for input in [&self.first, &self.second] {
            subscriptions.push(cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()));
        }
        subscriptions.push(cx.subscribe(&self.message, |_, _, _: &InputEvent, cx| cx.notify()));
    }
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        let mut ui = Self {
            snapshot: None,
            workspace: None,
            changes_scroll: UniformListScrollHandle::new(),
            history_scroll: UniformListScrollHandle::new(),
            history_graph: Arc::new(tervia::git::graph::Graph::default()),
            sidebar_scroll: UniformListScrollHandle::new(),
            branches_scroll: UniformListScrollHandle::new(),
            raw_scroll: UniformListScrollHandle::new(),
            metadata_scroll: UniformListScrollHandle::new(),
            commit_files_scroll: UniformListScrollHandle::new(),
            stash_scroll: UniformListScrollHandle::new(),
            worktrees: worktrees::WorktreesUi::new(window, cx),
            collapsed_changes: Default::default(),
            collapsed_folders: Default::default(),
            collapsed_sections: std::collections::BTreeSet::from(["git_commit_metadata"]),
            change_rows: Arc::from([]),
            #[cfg(all(test, feature = "ui-tests"))]
            rendered_rows: std::cell::Cell::new(0),
            repository: None,
            mode: Mode::Changes,
            branch: None,
            busy: false,
            error: String::new(),
            output: String::new(),
            detail: String::new(),
            raw_lines: Arc::from([]),
            metadata_lines: Arc::from([]),
            commit: None,
            commit_file: None,
            selected: None,
            generation: 0,
            detail_loading: false,
            limit: 100,
            first: cx.new(|cx| InputState::new(window, cx)),
            second: cx.new(|cx| InputState::new(window, cx)),
            message: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(3)
                    .placeholder(tervia::locale::Language::System.text("commit_message"))
            }),
            diff: cx.new(|cx| DiffState::new([], cx)),
            review: review::Review::default(),
        };
        ui.observe_diff(cx);
        ui
    }
}

impl Tervia {
    pub(super) fn choose_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.t("git_open_repository").into()),
        });
        cx.spawn_in(window, async move |weak, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let opened = cx
                    .background_executor()
                    .spawn(async move { tervia::files::directory(&path, std::path::Path::new("")) })
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| match opened {
                    Ok(root) => this.open_directory(root, window, cx),
                    Err(error) => {
                        this.state.notice = error;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn open_repository(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.worktree_removing(&root) {
            return;
        }
        self.navigation.cancel();
        for ui in &mut self.git_states {
            ui.cancel_worktree_navigation();
        }
        let count = self.state.projects.len();
        let index = self.state.open_directory(root);
        if index == count {
            self.init_project(index, window, cx);
        }
        let parent = self.attach_navigation(index);
        if !self.navigation.roots.contains_key(&parent) {
            self.restore_navigation(parent, window, cx);
        }
        self.state.set_tab(Tab::Git);
        self.refresh_git(index, window, cx);
        cx.notify();
    }

    fn open_workspace(
        &mut self,
        discovery: tervia::git::Discovery,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.state.active_project;
        let root = discovery.root.clone();
        if activate
            && let Some(index) = self
                .state
                .projects
                .iter()
                .position(|project| project.root == root)
            && self
                .state
                .folder(index)
                .is_some_and(|parent| parent != index)
        {
            // Reopening a child must keep its enclosing scan and project boundary.
            self.change_project(index, window, cx);
            return;
        }
        self.git_scans.invalidate(&root);
        let new_root = !self
            .state
            .projects
            .iter()
            .any(|project| project.root == root);
        let group = Arc::new(discovery);
        let mut members = Vec::new();
        for directory in std::iter::once(&root).chain(group.repositories.iter()) {
            if self.worktree_removing(directory) {
                continue;
            }
            let count = self.state.projects.len();
            let index = self.state.register_directory(directory.clone());
            members.push(index);
            if index == count {
                self.init_project(index, window, cx);
                self.state.projects[index].tab = Tab::Terminal;
            }
            self.git_states[index].workspace = Some(group.clone());
            if let Some(identity) = group.identities.get(directory) {
                self.state
                    .associate_repository(index, identity.clone(), None);
            }
            if index == count {
                self.refresh_git(index, window, cx);
            }
        }
        let parent = members.first().copied();
        if let Some(parent) = parent {
            self.state.group_folder(parent, &members[1..]);
            if !self.navigation.roots.contains_key(&parent) {
                self.restore_navigation(parent, window, cx);
            }
        }
        // Removed repositories retain their drafts, but must receive the group's new membership too.
        for ui in &mut self.git_states {
            if ui
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.root == root)
            {
                ui.workspace = Some(group.clone());
            }
        }
        if activate {
            if let Some(parent) = parent {
                if new_root {
                    self.state.projects[parent].tab = Tab::Terminal;
                }
                self.change_project(parent, window, cx);
            }
        } else {
            // A background rescan must preserve whichever project the user selected meanwhile.
            self.state.active_project = previous;
        }
        cx.notify();
    }

    pub(super) fn scanning_project(&self, project: usize) -> bool {
        let root = self.git_states[project]
            .workspace
            .as_ref()
            .map(|workspace| workspace.root.as_path())
            .unwrap_or(&self.state.projects[project].root);
        self.git_scans.running(root)
    }

    fn open_scan(
        &mut self,
        scan: tervia::git::Scan,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = scan.discovery.root.clone();
        self.open_workspace(scan.discovery, activate, window, cx);
        self.git_scans.notices.insert(root, scan.notices);
    }

    pub(super) fn scan_notices(&self, project: usize) -> Vec<String> {
        use tervia::git::ScanLimit;
        let workspace = self.git_states[project].workspace.as_ref();
        let root = workspace
            .map(|workspace| &workspace.root)
            .unwrap_or(&self.state.projects[project].root);
        if let Some(notices) = self.git_scans.notices.get(root) {
            notices
                .iter()
                .map(|notice| match notice {
                    ScanLimit::Time(seconds) => {
                        self.f("git_scan_time", &[("seconds", &seconds.to_string())])
                    }
                    ScanLimit::Directories(count) => {
                        self.f("git_scan_directories", &[("count", &count.to_string())])
                    }
                    ScanLimit::Repositories(count) => {
                        self.f("git_scan_count", &[("count", &count.to_string())])
                    }
                    ScanLimit::Depth(depth) => {
                        self.f("git_scan_depth", &[("depth", &depth.to_string())])
                    }
                    ScanLimit::Unreadable => self.t("git_scan_unreadable").to_owned(),
                })
                .collect()
        } else if workspace.is_some_and(|workspace| workspace.limited) {
            vec![self.t("git_scan_limited").to_owned()]
        } else {
            Vec::new()
        }
    }

    pub(super) fn rescan_repositories(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.git_states[project]
            .workspace
            .as_ref()
            .map(|workspace| workspace.root.clone())
            .unwrap_or_else(|| self.state.projects[project].root.clone());
        let Some(generation) = self.git_scans.begin(root.clone()) else {
            return;
        };
        let captured = root.clone();
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Repository::scan(&captured) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // All siblings share one scan; a superseded completion must not reinstall an old group.
                if !this.git_scans.finish(&root, generation) {
                    return;
                }
                match result {
                    Ok(scan) => this.open_scan(scan, false, window, cx),
                    Err(error) => this.git_states[project].error = error,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_git_file(
        &mut self,
        project: usize,
        path: PathBuf,
        range: Option<gpui_kit::component::diff::DiffLineRange>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repository) = self.git_states[project].repository.clone() else {
            return;
        };
        // A parent can become a Git repository after a plain folder was opened.
        // Keep existing file drafts anchored to their original project directory.
        let owner = if repository.root != self.state.projects[project].root {
            self.open_repository(repository.root, window, cx);
            self.state.active_project
        } else {
            project
        };
        self.open_real_file(owner, path.clone(), window, cx);
        self.remember_git_file_jump(project, owner, path, range);
    }

    pub(super) fn refresh_git(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_job(project, None, false, window, cx);
    }

    fn git_job(
        &mut self,
        project: usize,
        operation: Option<Operation>,
        initialize: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.state.projects[project].root.clone();
        if self.git_states[project].busy
            || self.git_mutating
            || ((self.saving_file || self.file_reads.has_reloads())
                && (operation.is_some() || initialize))
        {
            return;
        }
        let mutating = operation.is_some() || initialize;
        let commit = matches!(operation, Some(Operation::Commit { .. }));
        let limit = self.git_states[project].limit;
        self.git_states[project].busy = true;
        if mutating {
            self.git_mutating = true;
            self.git_states[project].error.clear();
        }
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let (repository, result, snapshot) = cx
                .background_executor()
                .spawn(async move {
                    let repository = if initialize {
                        Repository::initialize(&root)
                    } else {
                        Repository::open(&root)
                    };
                    match repository {
                        Ok(repository) => {
                            let result = operation.map(|operation| repository.execute(operation));
                            // A failed merge/rebase can still change Git's operation state.
                            let snapshot = repository.snapshot(limit);
                            (Some(repository), result, snapshot)
                        }
                        Err(error) => (None, None, Err(error)),
                    }
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if mutating {
                    this.git_mutating = false;
                }
                // An unavailable portable worktree keeps its known project group and drafts.
                if let Some(repository) = &repository
                    && let Ok(identity) = repository.common_directory()
                {
                    let main = snapshot
                        .as_ref()
                        .ok()
                        .and_then(|snapshot| snapshot.worktrees.iter().find(|tree| tree.main))
                        .map(|tree| tree.path.clone());
                    this.state.associate_repository(project, identity, main);
                }
                let ui = &mut this.git_states[project];
                ui.busy = false;
                ui.repository = repository;
                let success = matches!(&result, Some(Ok(_)));
                if let Some(result) = result {
                    match result {
                        Ok(output) => ui.output = output,
                        Err(error) => ui.error = error,
                    }
                }
                match snapshot {
                    Ok(snapshot) => {
                        this.state.projects[project].branch = snapshot.status.branch.clone();
                        ui.reconcile_snapshot(&snapshot, cx);
                        ui.snapshot = Some(Arc::new(snapshot));
                    }
                    Err(error) => {
                        // A passive probe outside a worktree is an empty state, not an error.
                        ui.error = if !mutating
                            && ui.repository.is_none()
                            && error.starts_with(
                                "git rev-parse: fatal: not a git repository (or any of the parent directories):",
                            )
                        {
                            String::new()
                        } else {
                            error
                        };
                        ui.snapshot = None;
                    }
                }
                if commit && success {
                    ui.message
                        .update(cx, |input, cx| input.set_value("", window, cx));
                }
                if let Some(detail) = ui.selected.clone() {
                    this.git_detail(project, detail, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn git_detail(
        &mut self,
        project: usize,
        detail: Detail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repository) = self.git_states[project].repository.clone() else {
            return;
        };
        let ui = &mut self.git_states[project];
        let generation = ui.begin_detail(detail.clone(), cx);
        let cached_commit = ui.commit.clone();
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let commit = match &detail {
                        Detail::Commit(hash) => Some(match cached_commit {
                            Some(commit) => commit,
                            None => repository.commit(hash)?,
                        }),
                        _ => None,
                    };
                    let result = match detail {
                        Detail::File(file, side) => repository.diff(&file, side),
                        Detail::Commit(hash) => repository.show_commit(&hash),
                        Detail::Stash(reference) => repository.show_stash(&reference),
                        Detail::Conflict(path) => {
                            let mut output = String::new();
                            for (stage, title) in [
                                (1, "BASE"),
                                (2, "OURS (index stage 2)"),
                                (3, "THEIRS (index stage 3)"),
                            ] {
                                output.push_str(&format!(
                                    "{title}\n{}\n\n",
                                    repository
                                        .conflict_content(&path, stage)
                                        .unwrap_or_else(|_| "(not present in this side)".into())
                                ));
                            }
                            Ok(output)
                        }
                    };
                    result.map(|detail| (detail, commit))
                })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let ui = &mut this.git_states[project];
                match result {
                    Ok((detail, commit)) => {
                        ui.install_detail(generation, Ok(detail), cx);
                        if let Some(commit) = commit {
                            ui.install_commit(generation, commit);
                        }
                    }
                    Err(error) => ui.install_detail(generation, Err(error), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn git_action(
        &self,
        id: impl Into<ElementId>,
        key: &'static str,
        operation: Operation,
        cx: &mut Context<Self>,
    ) -> Button {
        let project = self.state.active_project;
        self.git_action_for(project, id, key, operation, cx)
    }

    fn git_action_for(
        &self,
        project: usize,
        id: impl Into<ElementId>,
        key: &'static str,
        operation: Operation,
        cx: &mut Context<Self>,
    ) -> Button {
        let confirm = matches!(
            operation,
            Operation::DeleteBranch(_)
                | Operation::DeleteTag(_)
                | Operation::RemoveRemote(_)
                | Operation::StashDrop { .. }
                | Operation::Abort
                | Operation::Commit { amend: true, .. }
        );
        let target = match &operation {
            Operation::DeleteBranch(name)
            | Operation::DeleteTag(name)
            | Operation::RemoveRemote(name) => name.clone(),
            Operation::StashDrop {
                reference,
                expected,
            } => format!("{} · {}", reference, &expected[..expected.len().min(12)]),
            _ => self.state.projects[project].branch.clone(),
        };
        label_button(id, self.t(key), self.git_p())
            .custom(icon_style(self.git_p(), cx))
            .disabled(
                self.git_states[project].busy
                    || self.git_mutating
                    || self.saving_file
                    || self.file_reads.has_reloads(),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if confirm {
                    let receiver = window.prompt(
                        PromptLevel::Warning,
                        &format!("{}: {}", this.t(key), target),
                        Some(this.t("git_confirm_hint")),
                        &[this.t("git_cancel"), this.t("git_confirm")],
                        cx,
                    );
                    let operation = operation.clone();
                    this.git_mutating = true;
                    cx.spawn_in(window, async move |weak, cx| {
                        let accepted = receiver.await == Ok(1);
                        let _ = weak.update_in(cx, |this, window, cx| {
                            this.git_mutating = false;
                            if accepted {
                                this.git_job(project, Some(operation), false, window, cx);
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                    cx.notify();
                } else {
                    this.git_job(project, Some(operation.clone()), false, window, cx);
                }
            }))
    }

    fn clone_repository(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.git_states[project].first.read(cx).value().to_string();
        self.clone_url(Some(project), url, window, cx);
    }

    pub(super) fn clone_url(
        &mut self,
        project: Option<usize>,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if url.trim().is_empty() || self.git_mutating {
            return;
        }
        let root = project
            .map(|project| self.state.projects[project].root.clone())
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "/".into())
            });
        let destination = cx.prompt_for_new_path(&root, Some(Repository::clone_name(&url)));
        self.git_mutating = true;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = match destination.await {
                Ok(Ok(Some(path))) => Some(
                    cx.background_executor()
                        .spawn(async move { Repository::clone_into(&url, &path) })
                        .await,
                ),
                _ => None,
            };
            let _ = weak.update_in(cx, |this, window, cx| {
                this.git_mutating = false;
                match result {
                    Some(Ok(repository)) => this.open_repository(repository.root, window, cx),
                    Some(Err(error)) => {
                        if let Some(project) = project {
                            this.git_states[project].error = error;
                        } else {
                            this.state.notice = error;
                        }
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
