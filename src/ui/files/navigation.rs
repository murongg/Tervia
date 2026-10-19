use super::*;
use gpui_kit::component::input::Position;
use tervia::{platform::Platform, state::ReadingPosition};

gpui_kit::actions!(tervia, [FileBack, FileForward]);

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

#[derive(Clone)]
enum Kind {
    Jump(Location),
    Step(Step),
}

#[derive(Clone)]
struct Guard {
    index: usize,
    path: PathBuf,
    text: SharedString,
    cursor: Position,
}

#[derive(Clone)]
pub(super) struct Travel {
    pub target: Location,
    kind: Kind,
    guard: Option<Guard>,
}

impl Travel {
    pub(super) fn jump(from: Location, target: Location) -> Self {
        Self {
            target,
            kind: Kind::Jump(from),
            guard: None,
        }
    }

    pub(super) fn current(&self, view: &Tervia, project: usize, cx: &App) -> bool {
        let Some(guard) = &self.guard else {
            return false;
        };
        if let Kind::Step(step) = &self.kind
            && !view.explorers[project].file_history.is_current(step)
        {
            return false;
        }
        view.state.has_project()
            && view.state.active_project == project
            && view.state.project_visible(project)
            && view.state.project().tab == Tab::Files
            && view.state.project().active_file == guard.index
            && view.state.project().open_files.contains(&guard.index)
            && view.state.project().files[guard.index].disk_path == guard.path
            && view.editors[project][guard.index].read(cx).value() == guard.text
            && view.editors[project][guard.index]
                .read(cx)
                .cursor_position()
                == guard.cursor
    }
}

pub(in crate::ui) fn shortcuts(platform: Platform) -> (&'static str, &'static str) {
    match platform {
        Platform::Mac => ("ctrl--", "ctrl-shift--"),
        Platform::Windows => ("alt-left", "alt-right"),
        Platform::Linux => ("ctrl-alt--", "ctrl-shift--"),
    }
}

impl Tervia {
    pub(in crate::ui) fn file_location(&self, project: usize, cx: &App) -> Option<Location> {
        let owner = self.state.projects.get(project)?;
        let index = owner.active_file;
        let file = owner.current_file()?;
        let editor = self.editors[project][index].read(cx);
        let cursor = editor.cursor_position();
        let scroll = editor.scroll_offset();
        Some(Location {
            path: file.disk_path.clone(),
            position: ReadingPosition {
                line: cursor.line,
                column: cursor.character,
                scroll_x: f32::from(scroll.x),
                scroll_y: f32::from(scroll.y),
                source: !self.explorers[project].previews.reading(file, index),
            },
        })
    }

    pub(in crate::ui) fn navigate_file(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.has_project() || self.state.project().tab != Tab::Files || self.file_closing
        {
            return;
        }
        let project = self.state.active_project;
        let Some(current) = self.file_location(project, cx) else {
            return;
        };
        let Some(step) = self.explorers[project]
            .file_history
            .begin(direction, current)
        else {
            return;
        };
        self.open_file_location(
            project,
            Travel {
                target: step.target.clone(),
                kind: Kind::Step(step),
                guard: None,
            },
            window,
            cx,
        );
    }

    pub(super) fn open_file_location(
        &mut self,
        project: usize,
        mut travel: Travel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.has_project()
            || self.state.active_project != project
            || !self.state.project_visible(project)
            || self.file_closing
            || self.worktree_removing(&self.state.projects[project].root.join(&travel.target.path))
        {
            self.abandon_file_travel(project, &travel);
            cx.notify();
            return;
        }
        self.dismiss_definition_preview(cx);
        let index = self.state.projects[project].active_file;
        travel.guard = Some(Guard {
            index,
            path: self.state.projects[project].files[index].disk_path.clone(),
            text: self.editors[project][index].read(cx).value(),
            cursor: self.editors[project][index].read(cx).cursor_position(),
        });
        let existing = self.state.projects[project]
            .open_files
            .iter()
            .copied()
            .find(|index| {
                self.state.projects[project].files[*index].disk_path == travel.target.path
            });
        if let Some(index) = existing {
            self.state.projects[project].pin_file(index);
            self.select_open_file(project, index, window, cx);
            self.finish_file_travel(project, index, &travel, window, cx);
        } else {
            self.read_real_file(
                project,
                travel.target.path.clone(),
                Open::Travel(Box::new(travel)),
                window,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn finish_file_travel(
        &mut self,
        project: usize,
        index: usize,
        travel: &Travel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = travel.target.position;
        self.explorers[project]
            .previews
            .set_source(index, position.source);
        if !self.state.projects[project].files[index].is_image() {
            self.editors[project][index].update(cx, |editor, cx| {
                editor.set_cursor_position(
                    Position::new(position.line, position.column),
                    window,
                    cx,
                );
                if matches!(travel.kind, Kind::Step(_)) {
                    editor
                        .set_scroll_offset(point(px(position.scroll_x), px(position.scroll_y)), cx);
                }
            });
        }
        match &travel.kind {
            Kind::Jump(from) => {
                if let Some(to) = self.file_location(project, cx) {
                    self.explorers[project]
                        .file_history
                        .record(from.clone(), to);
                }
            }
            Kind::Step(step) => {
                self.explorers[project].file_history.finish(step);
            }
        }
        self.focus_file_document(project, index, window, cx);
        cx.notify();
    }

    pub(super) fn abandon_file_travel(&mut self, project: usize, travel: &Travel) {
        if let Kind::Step(step) = &travel.kind {
            self.explorers[project].file_history.abandon(step);
        }
    }

    pub(super) fn reject_file_travel(&mut self, project: usize, travel: &Travel) {
        if let Kind::Step(step) = &travel.kind {
            self.explorers[project].file_history.reject(step);
        }
    }

    pub(super) fn file_navigation_buttons(&self, cx: &mut Context<Self>) -> Div {
        let p = super::super::theme::Palette::workbench(self.appearance.light_theme);
        let history = &self.explorers[self.state.active_project].file_history;
        let (back, forward) = match Platform::CURRENT {
            Platform::Mac => ("Ctrl+-", "Ctrl+Shift+-"),
            Platform::Windows => ("Alt+Left", "Alt+Right"),
            Platform::Linux => ("Ctrl+Alt+-", "Ctrl+Shift+-"),
        };
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                icon_button(
                    "file-back",
                    IconName::ArrowLeft,
                    format!("{} ({back})", self.t("file_back")),
                    p,
                    cx,
                )
                .debug_selector(|| "file-back".into())
                .accessibility_label(self.t("file_back"))
                .disabled(!history.can_back() || self.file_closing)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.navigate_file(Direction::Back, window, cx)
                })),
            )
            .child(
                icon_button(
                    "file-forward",
                    IconName::ArrowRight,
                    format!("{} ({forward})", self.t("file_forward")),
                    p,
                    cx,
                )
                .debug_selector(|| "file-forward".into())
                .accessibility_label(self.t("file_forward"))
                .disabled(!history.can_forward() || self.file_closing)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.navigate_file(Direction::Forward, window, cx)
                })),
            )
    }
}
