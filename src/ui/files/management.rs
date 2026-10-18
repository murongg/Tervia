use super::*;
use gpui_kit::component::{
    Sizable as _,
    input::{Input, InputState},
};
use std::path::{Component, Path};
use tervia::files::Operation;

fn operation_paths(operation: &Operation) -> Vec<&Path> {
    match operation {
        Operation::Move { from, to } => vec![from, to],
        Operation::Copy { to, .. }
        | Operation::CreateFile(to)
        | Operation::CreateDirectory(to)
        | Operation::Trash(to) => vec![to],
    }
}

#[derive(Clone)]
pub(in crate::ui) enum Edit {
    File(PathBuf),
    Directory(PathBuf),
    Rename(PathBuf),
    Move(PathBuf),
    Copy(PathBuf),
}

impl Edit {
    fn title(&self) -> &'static str {
        match self {
            Self::File(_) => "files_new_file",
            Self::Directory(_) => "files_new_folder",
            Self::Rename(_) => "files_rename",
            Self::Move(_) => "files_move",
            Self::Copy(_) => "files_copy",
        }
    }

    fn operation(&self, value: &str) -> Result<Operation, &'static str> {
        let name = Path::new(value.trim());
        if name.as_os_str().is_empty() {
            return Err("files_name_required");
        }
        if matches!(self, Self::Rename(_))
            && (name.components().count() != 1
                || !matches!(name.components().next(), Some(Component::Normal(_))))
        {
            return Err("files_name_invalid");
        }
        let name: PathBuf = name.components().collect();
        Ok(match self {
            Self::File(parent) => Operation::CreateFile(parent.join(&name)),
            Self::Directory(parent) => Operation::CreateDirectory(parent.join(&name)),
            Self::Rename(from) => Operation::Move {
                from: from.clone(),
                to: from.parent().unwrap_or(Path::new("")).join(&name),
            },
            Self::Move(from) => Operation::Move {
                from: from.clone(),
                to: name,
            },
            Self::Copy(from) => Operation::Copy {
                from: from.clone(),
                to: name,
            },
        })
    }
}

pub(in crate::ui) struct Management {
    pub input: Entity<InputState>,
    pub edit: Option<Edit>,
    error: String,
    _subscription: Subscription,
}

impl Management {
    pub(in crate::ui) fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let subscription = cx.observe(&input, |_, _, cx| cx.notify());
        Self {
            input,
            edit: None,
            error: String::new(),
            _subscription: subscription,
        }
    }
}

impl Tervia {
    pub(in crate::ui) fn can_manage_files(&self) -> bool {
        self.state.has_project()
            && !self.git_mutating
            && !self.saving_file
            && !self.file_closing
            && !self.file_reads.has_reloads()
    }

    pub(in crate::ui) fn edit_entry(
        &mut self,
        project: usize,
        edit: Edit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_manage_files() || self.state.active_project != project {
            return;
        }
        let value = match &edit {
            Edit::Rename(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Edit::Move(path) | Edit::Copy(path) => path.to_string_lossy().into_owned(),
            _ => String::new(),
        };
        let placeholder = self.t(if matches!(edit, Edit::Move(_) | Edit::Copy(_)) {
            "files_destination"
        } else {
            "git_name"
        });
        let management = &mut self.explorers[project].management;
        management.edit = Some(edit);
        management.error.clear();
        management.input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.set_placeholder(placeholder, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    pub(in crate::ui) fn cancel_entry(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving_file {
            return;
        }
        self.explorers[project].management.edit = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn entry_form(&self, project: usize, cx: &mut Context<Self>) -> Div {
        let management = &self.explorers[project].management;
        let Some(edit) = &management.edit else {
            return div();
        };
        let p = super::super::theme::Palette::workbench(self.appearance.light_theme);
        let context = match edit {
            Edit::File(parent) | Edit::Directory(parent) => {
                if parent.as_os_str().is_empty() {
                    self.state.projects[project].name.clone()
                } else {
                    parent.display().to_string()
                }
            }
            Edit::Rename(path) | Edit::Move(path) | Edit::Copy(path) => path.display().to_string(),
        };
        div()
            .debug_selector(|| "file-entry-form".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(8.0))
            .p(px(12.0))
            .border_b_1()
            .border_color(p.border)
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    this.submit_entry(project, window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::component::input::Enter, window, cx| {
                    this.submit_entry(project, window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    this.cancel_entry(project, window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.cancel_entry(project, window, cx);
                    cx.stop_propagation();
                },
            ))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.t(edit.title())),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .text_ellipsis()
                    .child(context),
            )
            .child(
                Input::new(&management.input)
                    .small()
                    .aria_label(self.t(edit.title()))
                    .disabled(self.saving_file)
                    .w_full(),
            )
            .when(matches!(edit, Edit::Move(_) | Edit::Copy(_)), |form| {
                form.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.secondary)
                        .child(self.t("files_destination_hint")),
                )
            })
            .when(!management.error.is_empty(), |form| {
                form.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.red)
                        .child(management.error.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.0))
                    .child(
                        label_button("file-entry-cancel", self.t("git_cancel"), p)
                            .disabled(self.saving_file)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.cancel_entry(project, window, cx)
                            })),
                    )
                    .child(
                        label_button("file-entry-submit", self.t("git_confirm"), p)
                            .disabled(
                                !self.can_manage_files()
                                    || management.input.read(cx).value().trim().is_empty(),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.submit_entry(project, window, cx)
                            })),
                    ),
            )
    }

    pub(in crate::ui) fn submit_entry(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_manage_files() {
            return;
        }
        let management = &self.explorers[project].management;
        let Some(edit) = &management.edit else {
            return;
        };
        let operation = edit.operation(&management.input.read(cx).value());
        match operation {
            Ok(operation) => self.run_file_operation(project, operation, window, cx),
            Err(key) => {
                self.explorers[project].management.error = self.t(key).into();
                cx.notify();
            }
        }
    }

    fn file_operation_guard(
        &self,
        project: usize,
        operation: &Operation,
    ) -> Result<(), &'static str> {
        let root = &self.state.projects[project].root;
        let source = match operation {
            Operation::Move { from, .. } | Operation::Trash(from) => root.join(from),
            _ => return Ok(()),
        };
        if self.worktree_removing(&source) {
            return Err("git_running");
        }
        for owner in &self.state.projects {
            // A working directory owns terminal processes whose cwd cannot be rebased by a rename.
            if owner.root.starts_with(&source) {
                return Err("files_directory_in_use");
            }
            for file in &owner.files {
                let path = owner.root.join(&file.disk_path);
                if matches!(operation, Operation::Trash(_))
                    && path.starts_with(&source)
                    && file.dirty()
                {
                    return Err("files_manage_dirty");
                }
                if let Operation::Move { to, .. } = operation {
                    let destination = root.join(to);
                    if path.starts_with(&source) && !destination.starts_with(&owner.root) {
                        return Err("files_directory_in_use");
                    }
                    if path.starts_with(&destination) && file.dirty() {
                        return Err("files_manage_dirty");
                    }
                }
            }
        }
        Ok(())
    }

    pub(in crate::ui) fn trash_entry(
        &mut self,
        project: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_manage_files() {
            return;
        }
        let operation = Operation::Trash(path.clone());
        if let Err(key) = self.file_operation_guard(project, &operation) {
            self.state.notice = key.into();
            cx.notify();
            return;
        }
        let detail = self.f("files_trash_hint", &[("path", &path.to_string_lossy())]);
        let answer = window.prompt(
            PromptLevel::Warning,
            self.t("files_trash"),
            Some(&detail),
            &[self.t("git_cancel"), self.t("files_trash")],
            cx,
        );
        self.file_closing = true;
        cx.spawn_in(window, async move |weak, cx| {
            let accepted = answer.await == Ok(1);
            let _ = weak.update_in(cx, |this, window, cx| {
                this.file_closing = false;
                if accepted && this.can_manage_files() {
                    this.run_file_operation(project, operation, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn run_file_operation(
        &mut self,
        project: usize,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(key) = self.file_operation_guard(project, &operation) {
            self.explorers[project].management.error = self.t(key).into();
            self.state.notice = key.into();
            cx.notify();
            return;
        }
        let root = self.state.projects[project].root.clone();
        self.saving_file = true;
        for owner in 0..self.state.projects.len() {
            let owner_root = &self.state.projects[owner].root;
            if root.starts_with(owner_root) || owner_root.starts_with(&root) {
                self.invalidate_disk_sync(owner);
            }
        }
        let navigation = self.file_reads.selection(project);
        cx.spawn_in(window, async move |weak, cx| {
            let task_root = root.clone();
            let task_operation = operation.clone();
            let result = cx
                .background_executor()
                .spawn(async move { tervia::files::apply(&task_root, &task_operation) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.saving_file = false;
                match result {
                    Ok(()) => {
                        this.finish_file_operation(project, &operation, window, cx);
                        this.explorers[project].management.edit = None;
                        if this.state.active_project == project {
                            this.state.notice = "files_manage_done".into();
                            if this.file_reads.selected(project, navigation)
                                && this.state.project().tab == Tab::Files
                            {
                                this.focus.focus(window, cx);
                                if let Operation::CreateFile(path) = operation {
                                    this.open_real_file(project, path, window, cx);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        this.explorers[project].management.error = error.clone();
                        if this.state.active_project == project {
                            this.state.notice = error;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_file_operation(
        &mut self,
        project: usize,
        operation: &Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.state.projects[project].root.clone();
        for owner in 0..self.state.projects.len() {
            let owner_root = self.state.projects[owner].root.clone();
            if !root.starts_with(&owner_root) && !owner_root.starts_with(&root) {
                continue;
            }
            self.invalidate_disk_sync(owner);
            // Cancel only reads whose paths changed; unrelated navigation/recovery still owns its result.
            for path in operation_paths(operation) {
                if let Ok(path) = root.join(path).strip_prefix(&owner_root) {
                    self.file_reads.invalidate_tree(owner, path);
                    self.monitors[owner]
                        .external
                        .retain(|key, _| !key.starts_with(path));
                }
            }
            match operation {
                Operation::Move { from, to } => {
                    if let (Ok(from), Ok(to)) = (
                        root.join(from).strip_prefix(&owner_root),
                        root.join(to).strip_prefix(&owner_root),
                    ) {
                        self.state.projects[owner].relocate_files(from, to);
                        for (index, file) in self.state.projects[owner].files.iter_mut().enumerate()
                        {
                            if file.disk_path.starts_with(to) && !file.is_image() {
                                let language =
                                    gpui_kit::component::highlighter::language_for_path(&file.path)
                                        .to_string();
                                file.language = language.clone();
                                self.editors[owner][index]
                                    .update(cx, |editor, cx| editor.set_highlighter(language, cx));
                            }
                        }
                    }
                }
                Operation::Trash(path) => {
                    if let Ok(path) = root.join(path).strip_prefix(&owner_root) {
                        self.state.projects[owner].forget_files(path);
                        self.prune_previews(owner);
                    }
                }
                _ => {}
            }
            self.refresh_explorer(owner, window, cx);
            self.refresh_file_search(owner, window, cx);
            self.refresh_git(owner, window, cx);
            self.sync_open_files(owner, window, cx);
            let restoring: Vec<_> = self.state.projects[owner]
                .restoring
                .iter()
                .cloned()
                .collect();
            for path in restoring {
                self.restore_clean_file(owner, path, window, cx);
            }
        }
        self.persist_workspace(cx);
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests;
