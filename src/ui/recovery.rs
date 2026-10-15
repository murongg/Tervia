use super::*;
use tervia::state::ReadingPosition;

impl Tervia {
    pub(super) fn schedule_workspace_save(&mut self, cx: &mut Context<Self>) {
        if self.workspace.is_none() || self.workspace_save.is_some() {
            return;
        }
        // Coalesce keystrokes without allowing later render notifications to postpone the flush.
        self.workspace_save = Some(cx.spawn(async move |weak, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.workspace_save.take();
                this.persist_workspace(cx);
            });
        }));
    }

    pub(super) fn capture_reading_positions(&mut self, cx: &App) {
        for project in 0..self.state.projects.len() {
            let opened = self.state.projects[project].open_files.clone();
            for index in opened {
                let file = &self.state.projects[project].files[index];
                if self.state.projects[project]
                    .restoring
                    .contains(&file.disk_path)
                    || file.is_image()
                {
                    continue;
                }
                let editor = self.editors[project][index].read(cx);
                if editor.line_height().is_none() {
                    continue;
                }
                let cursor = editor.cursor_position();
                let scroll = editor.scroll_offset();
                let position = ReadingPosition {
                    line: cursor.line,
                    column: cursor.character,
                    scroll_x: f32::from(scroll.x),
                    scroll_y: f32::from(scroll.y),
                    source: !self.explorers[project].previews.reading(file, index),
                };
                let path = file.disk_path.clone();
                self.state.projects[project]
                    .positions
                    .insert(path, position);
            }
        }
    }

    pub(super) fn restore_reading_position(
        &mut self,
        project: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let file = &self.state.projects[project].files[index];
        if file.is_image() {
            return;
        }
        let Some(position) = self.state.projects[project]
            .positions
            .get(&file.disk_path)
            .copied()
        else {
            return;
        };
        if position.source {
            self.show_file_source(project, index);
        }
        self.editors[project][index].update(cx, |editor, cx| {
            editor.set_cursor_position(
                gpui_kit::component::input::Position::new(position.line, position.column),
                window,
                cx,
            );
            editor.set_scroll_offset(point(px(position.scroll_x), px(position.scroll_y)), cx);
        });
    }

    pub(super) fn restore_documents(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = self.state.projects[project].open_files.clone();
        for index in opened {
            let path = self.state.projects[project].files[index].disk_path.clone();
            if self.state.projects[project].restoring.contains(&path) {
                self.restore_clean_file(project, path, window, cx);
            } else {
                self.restore_reading_position(project, index, window, cx);
            }
        }
    }
}
