use super::{
    Tervia,
    controls::{icon, icon_button, label_button, list_row},
};
use gpui_kit::component::{
    Disableable, Sizable,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::tasks::{Catalog, Task as ProjectTask};

mod picker;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;

pub(super) struct Launcher {
    pub input: Entity<InputState>,
    pub open: bool,
    owner: Option<usize>,
    pub request: u64,
    catalog: Catalog,
    loading: bool,
    selected: usize,
    scroll: UniformListScrollHandle,
    previous_focus: Option<FocusHandle>,
}

impl Launcher {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx)),
            open: false,
            owner: None,
            request: 0,
            catalog: Catalog::default(),
            loading: false,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            previous_focus: None,
        }
    }

    pub fn cancel(&mut self) {
        self.open = false;
        self.request = self.request.wrapping_add(1);
    }
}

impl Tervia {
    pub(super) fn subscribe_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions.push(cx.subscribe_in(
            &self.tasks.input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Change => {
                    this.tasks.selected = 0;
                    this.tasks.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.confirm_project_task(window, cx),
                _ => {}
            },
        ));
    }

    pub(super) fn show_project_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.has_project() {
            return;
        }
        self.navigation.cancel();
        self.navigation.switcher.open = false;
        self.state.palette_open = false;
        for ui in &mut self.git_states {
            ui.close_branch_picker();
        }
        if !self.tasks.open {
            self.tasks.previous_focus = window.focused(cx);
        }
        self.tasks.open = true;
        self.tasks.owner = Some(self.state.active_project);
        self.tasks.selected = 0;
        let placeholder = self.t("tasks_search");
        self.tasks.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(placeholder, window, cx);
            input.focus(window, cx);
        });
        self.discover_tasks(cx);
    }

    fn discover_tasks(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.tasks.owner else {
            return;
        };
        let root = self.state.projects[owner].root.clone();
        self.tasks.request = self.tasks.request.wrapping_add(1);
        let request = self.tasks.request;
        self.tasks.loading = true;
        self.tasks.catalog = Catalog::default();
        cx.spawn(async move |weak, cx| {
            let catalog = cx
                .background_executor()
                .spawn(async move { tervia::tasks::discover(&root) })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.install_tasks(owner, request, catalog, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn install_tasks(
        &mut self,
        owner: usize,
        request: u64,
        catalog: Catalog,
        cx: &mut Context<Self>,
    ) -> bool {
        // A directory switch or newer scan wins, including when the old result arrives after reopening the picker.
        if !self.tasks.open
            || self.tasks.owner != Some(owner)
            || self.tasks.request != request
            || !self.state.has_project()
            || self.state.active_project != owner
        {
            return false;
        }
        self.tasks.catalog = catalog;
        self.tasks.loading = false;
        self.tasks.selected = 0;
        cx.notify();
        true
    }

    pub(super) fn close_project_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tasks.cancel();
        self.tasks
            .previous_focus
            .take()
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        cx.notify();
    }

    fn task_rows(&self, cx: &App) -> Vec<usize> {
        let query = self.tasks.input.read(cx).value().trim().to_lowercase();
        self.tasks
            .catalog
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| {
                (query.is_empty()
                    || format!("{} {} {}", task.label, task.display(), task.source)
                        .to_lowercase()
                        .contains(&query))
                .then_some(index)
            })
            .collect()
    }

    fn confirm_project_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tasks.open
            || self.tasks.loading
            || !self.state.has_project()
            || self.tasks.owner != Some(self.state.active_project)
        {
            return;
        }
        if let Some(task) = self
            .task_rows(cx)
            .get(self.tasks.selected)
            .map(|index| self.tasks.catalog.tasks[*index].clone())
        {
            self.close_project_tasks(window, cx);
            self.run_project_task(task, window, cx);
        }
    }

    fn run_project_task(&mut self, task: ProjectTask, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.has_project() {
            return;
        }
        if self.state.terminals().sessions.len() >= 64 {
            self.state.notice = self.t("tasks_terminal_limit").into();
            cx.notify();
            return;
        }
        self.terminal_rename = None;
        self.state.new_session();
        let session = self.state.terminals().active_session;
        self.state.terminals_mut().sessions[session].name = task.label.clone();
        self.state.terminals_mut().sessions[session].task = Some(task);
        self.change_tab(tervia::state::Tab::Terminal, window, cx);
        self.persist_workspace(cx);
    }

    fn rerun_project_task(&mut self, session: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.has_project()
            || !self
                .consoles()
                .get(session)
                .and_then(Option::as_ref)
                .is_some_and(|console| console.read(cx).can_restart())
        {
            return;
        }
        if let Some(task) = self
            .state
            .terminals()
            .sessions
            .get(session)
            .and_then(|session| session.task.clone())
        {
            self.run_project_task(task, window, cx);
        }
    }

    pub(super) fn task_run_summary(&self, session: usize, cx: &mut Context<Self>) -> Option<Div> {
        let task = self
            .state
            .terminals()
            .sessions
            .get(session)?
            .task
            .as_ref()?;
        let console = self.consoles().get(session)?.as_ref()?.read(cx);
        let status = console.task_status();
        let can_rerun = console.can_restart();
        let p = self.p();
        Some(
            div()
                .debug_selector(move || format!("task-run-summary-{session}"))
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(12.0))
                .px(px(16.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(p.chrome_border())
                .child(icon(gpui_kit::assets::IconName::Play, p.accent))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(div().text_ellipsis().child(task.display()))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(p.secondary)
                                .text_ellipsis()
                                .child(status),
                        ),
                )
                .child(
                    label_button(("task-rerun", session), self.t("tasks_rerun"), p, cx)
                        .debug_selector(move || format!("task-rerun-{session}"))
                        .icon(icon(gpui_kit::assets::IconName::RotateCcw, p.secondary))
                        .disabled(!can_rerun)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.rerun_project_task(session, window, cx)
                        })),
                ),
        )
    }
}
