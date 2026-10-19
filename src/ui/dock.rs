use super::*;
use gpui_kit::component::resizable::{resizable_panel, v_resizable};

impl Tervia {
    pub(super) fn toggle_terminal_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.has_project() || self.state.project().tab == Tab::Terminal {
            return;
        }
        let visible = !self.state.project().dock.visible;
        self.state.project_mut().dock.visible = visible;
        if visible {
            self.focus_terminal(window, cx);
        } else if self.state.project().tab == Tab::Files
            && self.state.project().current_file().is_some()
        {
            self.focus_file_document(
                self.state.active_project,
                self.state.project().active_file,
                window,
                cx,
            );
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn workspace_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project = self.state.active_project;
        let tab = self.state.active_tab();
        if tab == Tab::Terminal {
            return self.terminal(window, cx).into_any_element();
        }
        let body = match tab {
            Tab::Git => self.git(window, cx).into_any_element(),
            Tab::Files => self.files(window, cx).into_any_element(),
            Tab::Terminal => unreachable!(),
        };
        if !self.state.project().dock.visible {
            return body;
        }
        let height = self.state.project().dock.size;
        let terminal = self.terminal(window, cx);
        v_resizable(("workspace-terminal-dock", project))
            .child(
                resizable_panel()
                    .size_range(px(240.0)..Pixels::MAX)
                    .child(body),
            )
            .child(
                resizable_panel()
                    .size(px(height as f32))
                    .size_range(px(160.0)..px(800.0))
                    .child(
                        div()
                            .debug_selector(|| "terminal-dock".into())
                            .size_full()
                            .flex()
                            .child(terminal),
                    ),
            )
            .on_resize(cx.listener(
                move |this,
                      sizes: &Entity<gpui_kit::component::resizable::ResizableState>,
                      _,
                      cx| {
                    if let Some(size) = sizes.read(cx).sizes().get(1) {
                        this.state.projects[project].dock.size =
                            (f32::from(*size) as u32).clamp(160, 800);
                        cx.notify();
                    }
                },
            ))
            .into_any_element()
    }

    pub(super) fn project_attention(&self, parent: usize, cx: &App) -> bool {
        self.terminals.iter().enumerate().any(|(index, terminals)| {
            self.state.project_visible(index)
                && (self.state.folder(index) == Some(parent)
                    || self.state.same_project(index, parent))
                && terminals
                    .iter()
                    .flatten()
                    .any(|console| console.read(cx).needs_attention())
        })
    }
}
