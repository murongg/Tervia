use super::super::Tervia;
use gpui_kit::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tervia::files::{EntryKind, Row, Tree};

#[derive(Default)]
pub(in crate::ui) struct Navigation {
    pub roots: BTreeMap<usize, DirectoryTree>,
    pub scroll: UniformListScrollHandle,
    generation: u64,
    #[cfg(all(test, feature = "ui-tests"))]
    pub rendered_rows: std::cell::Cell<usize>,
}

pub(in crate::ui) struct DirectoryTree {
    pub tree: Tree,
    pub focus: FocusHandle,
    pub cursor: Option<PathBuf>,
    pub scroll: UniformListScrollHandle,
    reveal: Option<PathBuf>,
    refresh_pending: bool,
}

impl DirectoryTree {
    pub fn new(cx: &mut App) -> Self {
        Self {
            tree: Tree::default(),
            focus: cx.focus_handle(),
            cursor: None,
            scroll: UniformListScrollHandle::new(),
            reveal: None,
            refresh_pending: false,
        }
    }
}

#[derive(Clone)]
pub(super) enum Node {
    Directory {
        parent: usize,
        path: PathBuf,
        depth: usize,
    },
    Status {
        parent: usize,
        row: Row,
    },
}

impl Navigation {
    pub fn cancel(&mut self) {
        self.generation += 1;
    }

    pub(super) fn rows(&self, parent: usize, expanded: bool) -> Vec<Node> {
        let mut rows = vec![Node::Directory {
            parent,
            path: PathBuf::new(),
            depth: 0,
        }];
        if expanded && let Some(ui) = self.roots.get(&parent) {
            for row in ui.tree.rows().iter() {
                match row {
                    Row::Entry { entry, depth } => rows.push(Node::Directory {
                        parent,
                        path: entry.path.clone(),
                        depth: depth + 1,
                    }),
                    row => rows.push(Node::Status {
                        parent,
                        row: row.clone(),
                    }),
                }
            }
        }
        rows
    }
}

impl Tervia {
    pub(in crate::ui) fn attach_navigation(&mut self, index: usize) -> usize {
        let root = &self.state.projects[index].root;
        let parent = self
            .state
            .project_rows()
            .into_iter()
            .filter(|parent| {
                *parent != index && root.starts_with(&self.state.projects[*parent].root)
            })
            .max_by_key(|parent| self.state.projects[*parent].root.components().count())
            .or_else(|| self.state.folder(index))
            .unwrap_or(index);
        self.state.attach_directory(parent, index);
        parent
    }

    pub(in crate::ui) fn open_directory(
        &mut self,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_removing(&root) {
            return;
        }
        let count = self.state.projects.len();
        let index = self.state.register_directory(root);
        if index == count {
            self.init_project(index, window, cx);
            self.state.projects[index].tab = tervia::state::Tab::Terminal;
        }
        let parent = self.attach_navigation(index);
        self.change_project(index, window, cx);
        self.restore_navigation(parent, window, cx);
        self.rescan_repositories(parent, window, cx);
    }

    pub(in crate::ui) fn load_navigation(
        &mut self,
        parent: usize,
        path: PathBuf,
        refresh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation
            .roots
            .entry(parent)
            .or_insert_with(|| DirectoryTree::new(cx));
        self.ensure_watch(parent, window, cx);
        let Some(token) = self
            .navigation
            .roots
            .get_mut(&parent)
            .unwrap()
            .tree
            .begin(path.clone(), refresh)
        else {
            return;
        };
        self.request_navigation(parent, path, token, window, cx);
    }

    fn request_navigation(
        &mut self,
        parent: usize,
        path: PathBuf,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.state.projects[parent].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let relative = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    tervia::files::list(&root, &relative).map(|entries| {
                        entries
                            .into_iter()
                            .filter(|entry| entry.kind == EntryKind::Directory)
                            .collect()
                    })
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let Some(ui) = this.navigation.roots.get_mut(&parent) else {
                    return;
                };
                if !ui.tree.finish(&path, token, result) {
                    return;
                }
                this.register_directory(parent, path, window, cx);
                this.reveal_navigation(parent, window, cx);
                let ui = this.navigation.roots.get_mut(&parent).unwrap();
                if ui.refresh_pending && !ui.tree.loading() {
                    ui.refresh_pending = false;
                    this.refresh_navigation(parent, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::ui) fn refresh_navigation(
        &mut self,
        parent: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_active || !self.state.project_rows().contains(&parent) {
            return;
        }
        let Some(ui) = self.navigation.roots.get_mut(&parent) else {
            return;
        };
        if ui.tree.loading() {
            ui.refresh_pending = true;
            return;
        }
        let requests = ui.tree.refresh();
        for (path, token) in requests {
            self.request_navigation(parent, path, token, window, cx);
        }
    }

    pub(in crate::ui) fn toggle_navigation(
        &mut self,
        parent: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if path.as_os_str().is_empty() {
            self.state.toggle_folder(parent);
            if self.state.folder_expanded(parent) {
                self.load_navigation(parent, path, false, window, cx);
            }
        } else if let Some(ui) = self.navigation.roots.get_mut(&parent) {
            ui.reveal = None;
            if ui.tree.toggle(&path) {
                self.load_navigation(parent, path, false, window, cx);
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn restore_navigation(
        &mut self,
        parent: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_navigation(parent, PathBuf::new(), false, window, cx);
        let active = self.state.project_target(parent);
        if let Ok(relative) = self.state.projects[active]
            .root
            .strip_prefix(&self.state.projects[parent].root)
        {
            self.navigation.roots.get_mut(&parent).unwrap().reveal =
                relative.parent().map(PathBuf::from);
            self.reveal_navigation(parent, window, cx);
        }
    }

    fn reveal_navigation(&mut self, parent: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ui = self.navigation.roots.get_mut(&parent).unwrap();
        let Some(target) = ui.reveal.clone() else {
            return;
        };
        let mut path = PathBuf::new();
        for component in target.components() {
            if ui.tree.entries(&path).is_none() {
                return;
            }
            path.push(component);
            if !ui.tree.is_expanded(&path) {
                if ui.tree.toggle(&path) {
                    self.load_navigation(parent, path, false, window, cx);
                } else {
                    ui.reveal = None;
                }
                return;
            }
        }
        ui.reveal = None;
    }

    pub(in crate::ui) fn activate_directory(
        &mut self,
        parent: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.cancel();
        for ui in &mut self.git_states {
            ui.cancel_worktree_navigation();
        }
        let generation = self.navigation.generation;
        let root = self.state.projects[parent].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { tervia::files::directory(&root, &path) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // A slow directory validation must not replace a later project or module selection.
                if this.navigation.generation != generation || !this.state.project_visible(parent) {
                    return;
                }
                match result {
                    Ok(root) => {
                        if this.worktree_removing(&root) {
                            return;
                        }
                        let count = this.state.projects.len();
                        let index = this.state.register_directory(root);
                        this.state.attach_directory(parent, index);
                        if index == count {
                            this.init_project(index, window, cx);
                            this.state.projects[index].tab = tervia::state::Tab::Terminal;
                        }
                        // Directory selection is physical; shared Git identity does not redirect to a checkout root.
                        this.change_project(index, window, cx);
                    }
                    Err(error) => {
                        this.state.notice = error;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(in crate::ui) fn directory_index(&self, parent: usize, path: &Path) -> Option<usize> {
        let absolute = self.state.projects[parent].root.join(path);
        self.state
            .projects
            .iter()
            .enumerate()
            .find_map(|(index, project)| {
                (project.root == absolute && self.state.project_visible(index)).then_some(index)
            })
    }
}
