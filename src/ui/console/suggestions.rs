use super::super::controls::icon_button;
use super::*;
use gpui_kit::component::{
    Selectable, Sizable,
    button::{Button, ButtonVariants},
};
use tervia::{
    suggestions::{History, Input, Shell, Source, Suggestion, Worker},
    terminal::shell::{Event, Tracker},
};

async fn history_reply<T: Send + 'static>(
    reply: async_channel::Receiver<Result<T, String>>,
    executor: BackgroundExecutor,
) -> Result<T, String> {
    // The worker is an ordinary OS thread. Receive on GPUI's background executor so
    // its reply never wakes or drops a foreground task on the worker's thread.
    executor
        .spawn(async move {
            reply
                .recv_blocking()
                .unwrap_or_else(|_| Err("Command history worker stopped".into()))
        })
        .await
}

pub(super) struct State {
    pub tracker: Option<Tracker>,
    pub candidates: Vec<Suggestion>,
    pub input: Input,
    pub generation: u64,
    pub viewport: Bounds<Pixels>,
    history_file: Option<PathBuf>,
    selected: Option<usize>,
    worker: Worker,
    session: String,
    error: Option<String>,
    request: Option<Task<()>>,
}

impl State {
    pub fn new(worker: Worker) -> Self {
        let mut state = Self {
            tracker: None,
            candidates: Vec::new(),
            input: Input::default(),
            generation: 0,
            viewport: Bounds::default(),
            history_file: None,
            selected: None,
            worker,
            session: String::new(),
            error: None,
            request: None,
        };
        state.new_session();
        state
    }

    pub fn new_session(&mut self) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        self.session = format!("{timestamp}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    }
}

impl Console {
    pub(super) fn receive_output(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let events = self.suggestions.tracker.as_mut().map_or_else(
            || vec![Event::Output(bytes.to_vec())],
            |tracker| tracker.feed(bytes),
        );
        for event in events {
            match event {
                Event::Output(bytes) => {
                    self.buffer.feed(&bytes);
                    self.handle_events(cx);
                }
                Event::Started => {
                    self.suggestions.input.started();
                    self.invalidate_suggestion();
                }
                Event::Prompt(cwd) => {
                    self.cwd = cwd;
                    self.suggestions.input.prompt();
                    self.invalidate_suggestion();
                }
                Event::History(path) => {
                    if self.suggestions.history_file != path {
                        self.suggestions.history_file = path;
                        self.invalidate_suggestion();
                        self.refresh_suggestions(cx);
                    }
                }
                Event::Completed(command)
                    if self.appearance.command_suggestions && !self.replaying =>
                {
                    // Keep recording in PTY order, but never show a post-command prediction.
                    let response = self
                        .suggestions
                        .worker
                        .submit(self.suggestions.session.clone(), command);
                    match response {
                        Ok(reply) => cx
                            .spawn(async move |weak, cx| {
                                let result =
                                    history_reply(reply, cx.background_executor().clone()).await;
                                let _ = weak.update(cx, |this, cx| {
                                    this.suggestions.error = result.err();
                                    cx.notify();
                                });
                            })
                            .detach(),
                        Err(error) => {
                            self.suggestions.error = Some(error);
                            self.suggestions.new_session();
                        }
                    }
                }
                Event::Completed(_) => {}
            }
        }
    }

    pub(super) fn invalidate_suggestion(&mut self) {
        self.suggestions.generation = self.suggestions.generation.wrapping_add(1);
        self.suggestions.candidates.clear();
        self.suggestions.selected = None;
        self.suggestions.request.take();
    }

    pub(super) fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        if !self.suggestions_available() {
            return;
        }
        let prefix = self.suggestions.input.prefix().unwrap().to_owned();
        let shell = self
            .suggestions
            .tracker
            .as_ref()
            .map_or(Shell::Other, Tracker::shell);
        self.suggestions.candidates = History::default().complete(&prefix, &self.cwd, shell);
        let generation = self.suggestions.generation;
        let worker = self.suggestions.worker.clone();
        let cwd = self.cwd.clone();
        let source = self.suggestions.history_file.clone();
        self.suggestions.request = Some(cx.spawn(async move |weak, cx| {
            // A cancelled debounce never queues disk work for an obsolete keystroke.
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let result = match worker.complete_from(prefix, cwd, shell, source) {
                Ok(reply) => history_reply(reply, cx.background_executor().clone()).await,
                Err(error) => Err(error),
            };
            let _ = weak.update(cx, |this, cx| match result {
                Ok(candidates) => this.apply_suggestions(generation, candidates, None, cx),
                Err(error) => this.apply_suggestions(
                    generation,
                    this.suggestions.candidates.clone(),
                    Some(error),
                    cx,
                ),
            });
        }));
        cx.notify();
    }

    pub(super) fn apply_suggestions(
        &mut self,
        generation: u64,
        candidates: Vec<Suggestion>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.suggestions.generation != generation || !self.suggestions_available() {
            return;
        }
        // Once navigation starts, freeze the list until the prefix changes. A history
        // top-six result may omit the chosen command entirely, so retaining its index is unsafe.
        if self.suggestions.selected.is_none() {
            self.suggestions.candidates = candidates;
        }
        if error.is_some() {
            self.suggestions.error = error;
        }
        cx.notify();
    }

    fn suggestions_available(&self) -> bool {
        self.appearance.command_suggestions
            && self.suggestions.input.prefix().is_some()
            && self.agent.is_none()
            && self.status.is_none()
            && self.buffer.term.grid().display_offset() == 0
            && !self.buffer.term.mode().contains(TermMode::ALT_SCREEN)
            && !self.search.as_ref().is_some_and(|search| search.open)
            && self.composition.is_empty()
    }

    pub(super) fn suggestion_visible(&self) -> bool {
        !self.suggestions.candidates.is_empty() && self.suggestion_layout().is_some()
    }

    fn suggestion_row_height(&self) -> Pixels {
        (self.cell_height + px(8.0)).max(px(30.0))
    }

    fn suggestion_layout(&self) -> Option<(Bounds<Pixels>, usize)> {
        if !self.suggestions_available()
            || self.suggestions.viewport.size.height <= px(0.0)
            || self.suggestions.candidates.is_empty()
        {
            return None;
        }
        let cursor = self.buffer.term.grid().cursor.point;
        if cursor.line.0 < 0 || cursor.line.0 as usize >= self.buffer.size().1 {
            return None;
        }
        let columns = self
            .suggestions
            .candidates
            .iter()
            .map(|candidate| {
                candidate
                    .command
                    .chars()
                    .map(|c| if c.is_ascii() { 1 } else { 2 })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(24);
        let width = (self.cell_width * columns as f32 + px(100.0))
            .max(px(320.0))
            .min(self.bounds.size.width);
        let row_height = self.suggestion_row_height();
        // Footer, padding and both border edges all consume space in the fixed overlay.
        let chrome_height = px(38.0);
        let full_height = row_height * self.suggestions.candidates.len() as f32 + chrome_height;
        let top = self.cell_height * cursor.line.0 as f32;
        let below = top + self.cell_height + px(6.0);
        let below_space = (self.bounds.size.height - below).max(px(0.0));
        let above_space = (top - px(6.0)).max(px(0.0));
        let use_below = below_space >= full_height || below_space >= above_space;
        let space = if use_below { below_space } else { above_space };
        let rows = ((f32::from(space - chrome_height) / f32::from(row_height))
            .floor()
            .max(0.0) as usize)
            .min(self.suggestions.candidates.len());
        if rows == 0 {
            return None;
        }
        let height = row_height * rows as f32 + chrome_height;
        let y = if use_below {
            below
        } else {
            top - height - px(6.0)
        };
        let prefix_columns = self
            .suggestions
            .input
            .prefix()
            .unwrap()
            .chars()
            .map(|c| if c.is_ascii() { 1 } else { 2 })
            .sum::<usize>();
        let x = (self.cell_width * cursor.column.0.saturating_sub(prefix_columns) as f32)
            .min((self.bounds.size.width - width).max(px(0.0)));
        let inset = self.bounds.origin - self.suggestions.viewport.origin;
        Some((Bounds::new(inset + point(x, y), size(width, height)), rows))
    }

    pub(super) fn suggestion_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.suggestion_visible() {
            return false;
        }
        match key {
            "up" | "down" => {
                let len = self.suggestions.candidates.len();
                let selected = if key == "up" {
                    self.suggestions
                        .selected
                        .map_or(len - 1, |index| (index + len - 1) % len)
                } else {
                    self.suggestions
                        .selected
                        .map_or(0, |index| (index + 1) % len)
                };
                self.suggestions.selected = Some(selected);
                cx.notify();
            }
            // A merely visible suggestion must never change what Enter executes.
            "enter" if self.suggestions.selected.is_none() => return false,
            "enter" | "tab" => self.accept_suggestion(window, cx),
            _ => return false,
        }
        true
    }

    pub(super) fn accept_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.suggestion_visible() {
            return;
        }
        let candidate = &self.suggestions.candidates[self.suggestions.selected.unwrap_or(0)];
        let Some(suffix) = candidate
            .command
            .strip_prefix(self.suggestions.input.prefix().unwrap())
        else {
            return;
        };
        // Only append the missing suffix; never replace shell-owned text or send Enter.
        let bytes = paste_bytes(suffix, *self.buffer.term.mode());
        self.send(&bytes, cx);
        self.invalidate_suggestion();
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn suggestion_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (bounds, rows) = self.suggestion_layout()?;
        let p = Palette::new(self.appearance.light_theme);
        let language = self.appearance.language;
        let row_height = self.suggestion_row_height();
        let first = self
            .suggestions
            .selected
            .unwrap_or(0)
            .saturating_sub(rows - 1);
        let mut list = div()
            .id("command-completions")
            .role(Role::List)
            .debug_selector(|| "terminal-suggestion".into())
            .absolute()
            .left(bounds.left())
            .top(bounds.top())
            .w(bounds.size.width)
            .h(bounds.size.height)
            .min_w_0()
            .flex()
            .flex_col()
            .p(px(4.0))
            .bg(p.sidebar)
            .rounded(px(6.0))
            .border_1()
            .border_color(p.border)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
        for (index, candidate) in self
            .suggestions
            .candidates
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
        {
            let command = candidate.command.clone();
            let source = language.text(if candidate.source == Source::History {
                "suggestion_history"
            } else {
                "suggestion_common"
            });
            let selected = self.suggestions.selected == Some(index);
            list = list.child(
                div()
                    // Press state belongs to a command even if history reorders its row.
                    .id(SharedString::from(format!("completion-option:{command}")))
                    .role(Role::ListItem)
                    .aria_selected(selected)
                    .w_full()
                    .h(row_height)
                    .flex_shrink_0()
                    .child(
                        Button::new(SharedString::from(command.clone()))
                            .ghost()
                            .small()
                            .debug_selector(move || format!("terminal-suggestion-row-{index}"))
                            .selected(selected)
                            .w_full()
                            .h(row_height)
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(0.0))
                            .overflow_hidden()
                            .when(selected, |row| row.bg(p.selected))
                            .text_color(p.text)
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .font_family(self.appearance.font_family.clone())
                                            .text_size(px(self.appearance.code_size as f32))
                                            .line_height(self.cell_height)
                                            .child(candidate.command.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(11.0))
                                            .text_color(p.secondary)
                                            .child(source),
                                    ),
                            )
                            .accessibility_label(format!(
                                "{}: {} · {}",
                                language.text("suggestion_insert"),
                                candidate.command,
                                source
                            ))
                            .tooltip(candidate.command.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                // A history result can reorder rows between pointer down and click.
                                let Some(index) = this
                                    .suggestions
                                    .candidates
                                    .iter()
                                    .position(|candidate| candidate.command == command)
                                else {
                                    return;
                                };
                                this.suggestions.selected = Some(index);
                                this.accept_suggestion(window, cx);
                            })),
                    ),
            );
        }
        let mut footer = div()
            .debug_selector(|| "terminal-suggestion-footer".into())
            .w_full()
            .h(px(28.0))
            .flex_shrink_0()
            .min_w_0()
            .flex()
            .items_center()
            .px(px(6.0))
            .gap(px(4.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(if self.suggestions.selected.is_some() {
                        "↑↓ · Tab / Enter · Esc"
                    } else {
                        "↑↓ · Tab · Esc"
                    }),
            );
        if let Some(error) = self.suggestions.error.clone() {
            footer = footer.child(
                icon_button(
                    "command-history-error",
                    gpui_kit::assets::IconName::Info,
                    language.text("suggestion_history_error"),
                    p,
                    cx,
                )
                .tooltip(error.clone())
                .on_click(move |_, window, cx| {
                    drop(window.prompt(
                        PromptLevel::Warning,
                        language.text("suggestion_history_error"),
                        Some(&error),
                        &[language.text("close")],
                        cx,
                    ));
                }),
            );
        }
        Some(list.child(footer).into_any_element())
    }
}
