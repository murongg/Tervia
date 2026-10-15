use super::*;
use gpui_kit::component::diff::{DiffSide, DiffState};
use std::path::Path;

pub(super) fn diff_context(diff: &DiffState, cx: &App) -> Option<String> {
    let excerpts = diff.selected_excerpts(cx);
    if excerpts.is_empty() {
        return None;
    }
    Some(
        excerpts
            .iter()
            .map(|part| {
                tervia::files::code_context(
                    Path::new(part.path.as_ref()),
                    part.start,
                    part.end,
                    Some(match part.side {
                        DiffSide::Original => "original",
                        DiffSide::Modified => "modified",
                    }),
                    &part.text,
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

impl Tervia {
    pub(super) fn file_context(&self, project: usize, index: usize, cx: &App) -> Option<String> {
        let file = self.state.projects.get(project)?.files.get(index)?;
        if self.explorers[project].previews.reading(file, index) {
            return None;
        }
        let editor = self.editors.get(project)?.get(index)?.read(cx);
        let selection = editor.selected_range();
        if selection.is_empty() {
            return None;
        }
        tervia::files::selection_context(&file.disk_path, &editor.value(), selection)
    }

    fn selected_code_context(&self, cx: &App) -> Option<String> {
        let project = self.state.active_project;
        match self.state.project().tab {
            Tab::Files => self.file_context(project, self.state.project().active_file, cx),
            Tab::Git => self.git_states[project].code_context(cx),
            Tab::Terminal => None,
        }
    }

    pub(super) fn copy_code_context(&mut self, cx: &mut Context<Self>) {
        if let Some(context) = self.selected_code_context(cx) {
            self.copy_context(context, cx);
        }
    }

    pub(super) fn copy_context(&mut self, context: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(context));
        self.state.notice = "code_context_copied".into();
        cx.notify();
    }
}
