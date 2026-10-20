use super::*;
use gpui_kit::component::input::InputState;

pub(in crate::ui) struct Navigation {
    pub input: Entity<InputState>,
    pub open: bool,
    pub selected: usize,
    pub scroll: UniformListScrollHandle,
    pub(super) sidebar: super::sidebar::Sidebar,
    pub(super) generation: u64,
    pub(in crate::ui) switcher: super::switcher::Switcher,
    pub(super) closing: bool,
}

impl Navigation {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx)),
            open: false,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            sidebar: super::sidebar::Sidebar::default(),
            generation: 0,
            switcher: super::switcher::Switcher::new(window, cx),
            closing: false,
        }
    }
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.open = false;
        // A temporary overlay belongs to this navigation, not the next screen or project.
        self.sidebar.collapse();
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Tervia {
    pub(in crate::ui) fn working_directory_owner(&self) -> usize {
        let active = self.state.active_project;
        self.state.project_owner(active).unwrap_or(active)
    }

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
        if let Some(owner) = self.state.project_owner(index)
            && self.state.project_closed(owner)
        {
            self.state.reopen_project(owner);
        }
        let parent = self.attach_navigation(index);
        self.change_project(index, window, cx);
        self.rescan_repositories(parent, window, cx);
    }

    pub(in crate::ui) fn choose_working_directory(
        &mut self,
        parent: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.cancel();
        let generation = self.navigation.generation;
        let root = self.state.projects[parent].root.clone();
        let outside = self.t("working_directory_outside").to_owned();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.t("working_directory_add").into()),
        });
        cx.spawn_in(window, async move |weak, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let path = tervia::files::directory(&path, std::path::Path::new(""))?;
                        path.strip_prefix(&root)
                            .map(PathBuf::from)
                            .map_err(|_| outside)
                    })
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| {
                    if this.navigation.generation != generation
                        || !this.state.project_visible(parent)
                    {
                        return;
                    }
                    match result {
                        Ok(path) => this.activate_directory(parent, path, window, cx),
                        Err(error) => {
                            this.state.notice = error;
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
        cx.notify();
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
                // Validation belongs to the captured project; a later navigation always wins.
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
}
