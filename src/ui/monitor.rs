use super::Tervia;
use gpui_kit::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use tervia::files::{Update, Watch};

type DiskUpdates = Vec<(usize, PathBuf, Result<String, String>)>;

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

#[derive(Default)]
pub(super) struct Monitor {
    pub enabled: bool,
    watch: Option<Watch>,
    epoch: usize,
    starting: bool,
    fallback: bool,
    registered: BTreeSet<PathBuf>,
    syncing: Option<usize>,
    sync_again: bool,
    tree_pending: bool,
    generation: usize,
    pub external: BTreeMap<PathBuf, &'static str>,
}

impl Tervia {
    pub(super) fn stop_project_monitor(&mut self, project: usize, cx: &mut Context<Self>) {
        let monitor = &mut self.monitors[project];
        monitor.enabled = false;
        monitor.epoch += 1;
        monitor.generation += 1;
        monitor.syncing = None;
        monitor.sync_again = false;
        monitor.tree_pending = false;
        monitor.starting = false;
        monitor.registered.clear();
        if let Some(watch) = monitor.watch.take() {
            dispose_watch(watch, cx);
        }
    }
    pub(super) fn monitor_activation(
        &mut self,
        active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.files_active == active {
            return;
        }
        self.files_active = active;
        for project in 0..self.monitors.len() {
            let monitor = &mut self.monitors[project];
            monitor.epoch += 1;
            monitor.generation += 1;
            monitor.syncing = None;
            monitor.sync_again = false;
            monitor.tree_pending = false;
            monitor.starting = false;
            monitor.registered.clear();
            if let Some(watch) = monitor.watch.take() {
                dispose_watch(watch, cx);
            }
            if active && self.monitors[project].enabled {
                self.monitors[project].fallback = false;
                self.ensure_watch(project, window, cx);
                // Reconcile immediately on foreground entry; do not wait for the event timer.
                self.refresh_explorer(project, window, cx);
                self.sync_open_files(project, window, cx);
            }
        }
        if active
            && !self.state.projects.is_empty()
            && self.state.project().tab == tervia::state::Tab::Git
        {
            self.refresh_git(self.state.active_project, window, cx);
        }
    }

    pub(super) fn ensure_watch(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.project_visible(project) {
            return;
        }
        let monitor = &mut self.monitors[project];
        monitor.enabled = true;
        if !self.files_active || monitor.watch.is_some() || monitor.starting || monitor.fallback {
            return;
        }
        monitor.starting = true;
        let epoch = monitor.epoch;
        let root = self.state.projects[project].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let watch = cx
                .background_executor()
                .spawn(async move { Watch::new(&root) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if !this.files_active || this.monitors[project].epoch != epoch {
                    if let Ok(watch) = watch {
                        dispose_watch(watch, cx);
                    }
                    return;
                }
                this.monitors[project].starting = false;
                match watch {
                    Ok(watch) => {
                        this.monitors[project].watch = Some(watch);
                        this.monitors[project].registered.insert(PathBuf::new());
                        let mut directories = this.explorers[project].tree.directories();
                        directories.extend(
                            this.state.projects[project]
                                .files
                                .iter()
                                .filter_map(|file| file.disk_path.parent().map(PathBuf::from)),
                        );
                        for directory in directories {
                            this.register_directory(project, directory, window, cx);
                        }
                        // Reconcile once after subscribing, covering changes during watcher setup.
                        this.request_tree_sync(project, window, cx);
                        this.sync_open_files(project, window, cx);
                    }
                    Err(_) => {
                        this.monitors[project].fallback = true;
                        if this.state.active_project == project {
                            this.state.notice = "files_watch_fallback".into();
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn register_directory(
        &mut self,
        project: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let monitor = &mut self.monitors[project];
        let Some(watch) = monitor.watch.clone() else {
            return;
        };
        if !monitor.registered.insert(path.clone()) {
            return;
        }
        let epoch = monitor.epoch;
        cx.spawn_in(window, async move |weak, cx| {
            let directory = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { watch.watch_directory(&directory) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.finish_directory_watch(project, path, epoch, result, window, cx)
            });
        })
        .detach();
    }

    fn finish_directory_watch(
        &mut self,
        project: usize,
        path: PathBuf,
        epoch: usize,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.monitors[project].epoch != epoch || !self.files_active {
            return;
        }
        if result.is_ok() {
            // The first listing preceded subscription; reconcile that otherwise unwatched gap.
            self.load_directory(project, path, true, window, cx);
            self.sync_open_files(project, window, cx);
        } else {
            self.monitors[project].registered.remove(&path);
            self.monitors[project].fallback = true;
            if self.state.active_project == project {
                self.state.notice = "files_watch_fallback".into();
            }
            cx.notify();
        }
    }

    pub(super) fn record_file_saved(&mut self, project: usize, index: usize, contents: String) {
        // Reads started during explicit IO may predate its new baseline, even if its start
        // already invalidated an older automatic read. Invalidate at completion as well.
        self.invalidate_disk_sync(project);
        self.file_reads.invalidate(
            project,
            &self.state.projects[project].files[index].disk_path,
        );
        self.state.projects[project].files[index].saved = contents;
        self.monitors[project]
            .external
            .remove(&self.state.projects[project].files[index].disk_path);
    }

    pub(super) fn poll_files(
        &mut self,
        fallback_tick: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_active {
            return;
        }
        for project in 0..self.monitors.len() {
            let changes = self.monitors[project].watch.as_ref().map(Watch::drain);
            let monitor = &mut self.monitors[project];
            let mut changed = monitor.fallback && fallback_tick;
            if let Some(changes) = changes {
                changed |= changes.rescan || !changes.paths.is_empty();
                if changes.error.is_some() {
                    monitor.fallback = true;
                }
                if changes.rescan {
                    monitor.registered.clear();
                }
                for removed in changes.removed {
                    monitor
                        .registered
                        .retain(|path| !path.starts_with(&removed));
                }
            }
            if changed {
                self.request_tree_sync(project, window, cx);
                self.sync_open_files(project, window, cx);
            }
        }
    }

    fn request_tree_sync(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.files_active {
            return;
        }
        if self.explorers[project].tree.loading() {
            self.monitors[project].tree_pending = true;
        } else {
            self.refresh_explorer(project, window, cx);
        }
    }

    pub(super) fn tree_settled(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_active {
            return;
        }
        if self.monitors[project].tree_pending && !self.explorers[project].tree.loading() {
            self.monitors[project].tree_pending = false;
            self.refresh_explorer(project, window, cx);
        }
    }

    pub(super) fn invalidate_disk_sync(&mut self, project: usize) {
        self.monitors[project].generation += 1;
        if self.monitors[project].syncing.is_some() {
            self.monitors[project].sync_again = true;
        }
    }

    pub(super) fn sync_open_files(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_active {
            return;
        }
        let monitor = &mut self.monitors[project];
        if monitor.syncing.is_some() {
            monitor.sync_again = true;
            return;
        }
        let files: Vec<_> = self.state.projects[project]
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| !file.is_image())
            .map(|(index, file)| (index, file.disk_path.clone()))
            .collect();
        if files.is_empty() {
            return;
        }
        monitor.generation += 1;
        let generation = monitor.generation;
        monitor.syncing = Some(generation);
        let root = self.state.projects[project].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let updates = cx
                .background_executor()
                .spawn(async move {
                    files
                        .into_iter()
                        .map(|(index, path)| {
                            let contents = tervia::files::text(&root, &path);
                            (index, path, contents)
                        })
                        .collect::<DiskUpdates>()
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.finish_disk_sync(project, generation, updates, window, cx)
            });
        })
        .detach();
    }

    fn finish_disk_sync(
        &mut self,
        project: usize,
        generation: usize,
        updates: DiskUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let monitor = &mut self.monitors[project];
        // A paused session's completion must not release a newer foreground read.
        if monitor.syncing != Some(generation) || !self.files_active {
            return;
        }
        monitor.syncing = None;
        if monitor.sync_again || monitor.generation != generation {
            monitor.sync_again = false;
            self.sync_open_files(project, window, cx);
            return;
        }
        self.apply_disk_updates(project, updates, window, cx);
    }

    pub(super) fn apply_disk_updates(
        &mut self,
        project: usize,
        updates: DiskUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (index, path, contents) in updates {
            let file = &mut self.state.projects[project].files[index];
            match tervia::files::reconcile(&file.saved, &file.draft, contents) {
                Update::Unchanged => {
                    self.monitors[project].external.remove(&path);
                }
                Update::Replace(contents) => {
                    let changed = file.draft != contents;
                    file.saved = contents.clone();
                    file.draft = contents.clone();
                    self.monitors[project].external.remove(&path);
                    // Avoid resetting the cursor/selection for our own save or unchanged text.
                    if changed {
                        self.editors[project][index]
                            .update(cx, |editor, cx| editor.set_value(contents, window, cx));
                    }
                }
                Update::Conflict => {
                    self.monitors[project]
                        .external
                        .insert(path, "files_disk_changed");
                }
                Update::Unavailable(_) => {
                    self.monitors[project]
                        .external
                        .insert(path, "files_disk_unavailable");
                }
            }
        }
        cx.notify();
    }
}

fn dispose_watch(watch: Watch, cx: &Context<Tervia>) {
    // Native backends may join their worker during teardown; keep that off the UI thread.
    cx.background_executor()
        .spawn(async move {
            drop(watch);
        })
        .detach();
}
