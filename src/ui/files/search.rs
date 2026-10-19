use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::component::input::{Input, InputState};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tervia::files::search;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum Mode {
    Browse,
    Files,
    Text,
}
#[derive(Clone)]
pub(in crate::ui) enum Row {
    File(PathBuf),
    Text(search::Hit),
}

pub(in crate::ui) struct Finder {
    pub mode: Mode,
    pub input: Entity<InputState>,
    pub rows: Arc<Vec<Row>>,
    index: Option<Arc<search::Names>>,
    generation: usize,
    loading: bool,
    limited: bool,
    error: String,
    selected: usize,
    cancel: Arc<AtomicBool>,
    scroll: UniformListScrollHandle,
    task: Option<Task<()>>,
}
impl Finder {
    pub(in crate::ui) fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            mode: Mode::Browse,
            input: cx.new(|cx| InputState::new(window, cx)),
            rows: Arc::default(),
            index: None,
            generation: 0,
            loading: false,
            limited: false,
            error: String::new(),
            selected: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            scroll: UniformListScrollHandle::new(),
            task: None,
        }
    }
}
impl Tervia {
    pub(in crate::ui) fn show_file_search(
        &mut self,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.projects.is_empty() {
            return;
        }
        self.change_tab(Tab::Files, window, cx);
        let project = self.state.active_project;
        let placeholder = self.t(if mode == Mode::Text {
            "files_search_text"
        } else {
            "files_find_name"
        });
        let finder = &mut self.explorers[project].finder;
        finder.cancel.store(true, Ordering::Relaxed);
        finder.mode = mode;
        finder.index = None;
        finder.rows = Arc::default();
        finder.selected = 0;
        finder.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(placeholder, window, cx);
            if mode != Mode::Browse {
                input.focus(window, cx);
            }
        });
        self.refresh_file_search(project, window, cx);
        cx.notify();
    }
    pub(in crate::ui) fn refresh_file_search(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let finder = &mut self.explorers[project].finder;
        finder.cancel.store(true, Ordering::Relaxed);
        finder.generation += 1;
        let generation = finder.generation;
        let mode = finder.mode;
        if mode == Mode::Browse {
            finder.loading = false;
            finder.task = None;
            return;
        }
        let query = finder.input.read(cx).value().to_string();
        if mode == Mode::Text && query.trim().is_empty() {
            finder.rows = Arc::default();
            finder.loading = false;
            return;
        }
        finder.cancel = Arc::new(AtomicBool::new(false));
        let cancel = finder.cancel.clone();
        let root = self.state.projects[project].root.clone();
        let index = finder.index.clone();
        finder.loading = true;
        finder.error.clear();
        finder.rows = Arc::default();
        finder.selected = 0;
        finder.task = Some(cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if mode == Mode::Files {
                        let index = match index {
                            Some(index) => index,
                            None => Arc::new(search::Names::new(search::index(&root, &cancel)?)),
                        };
                        let rows = index
                            .find(&query)
                            .into_iter()
                            .map(Row::File)
                            .collect::<Vec<_>>();
                        let limited = index.limited || rows.len() == 500;
                        Ok::<_, String>((rows, Some(index), limited))
                    } else {
                        let results = search::text(&root, &query, &cancel)?;
                        Ok((
                            results.hits.into_iter().map(Row::Text).collect(),
                            None,
                            results.limited,
                        ))
                    }
                })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let finder = &mut this.explorers[project].finder;
                // Query and owner are captured: superseded searches never reinstall their results.
                if finder.generation != generation || finder.mode != mode {
                    return;
                }
                finder.loading = false;
                match result {
                    Ok((rows, index, limited)) => {
                        finder.rows = Arc::new(rows);
                        finder.index = index;
                        finder.limited = limited;
                    }
                    Err(error) => finder.error = error,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(in crate::ui) fn open_search_selection(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let finder = &self.explorers[project].finder;
        let Some(row) = finder.rows.get(finder.selected).cloned() else {
            return;
        };
        match row {
            Row::File(path) => self.open_real_file(project, path, window, cx),
            Row::Text(hit) => {
                self.open_file_match(project, hit.path, hit.line, hit.column, window, cx)
            }
        }
    }
    pub(in crate::ui) fn search_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let finder = &self.explorers[project].finder;
        let p = super::super::theme::Palette::workbench(self.appearance.light_theme);
        let rows = finder.rows.clone();
        let generation = finder.generation;
        let status = if finder.loading {
            self.t("files_searching").to_owned()
        } else if !finder.error.is_empty() {
            finder.error.clone()
        } else if finder.limited {
            self.f(
                "files_search_limited",
                &[("count", &rows.len().to_string())],
            )
        } else {
            self.f("files_search_count", &[("count", &rows.len().to_string())])
        };
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers != Modifiers::default() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        let finder = &mut this.explorers[project].finder;
                        finder.selected = if event.keystroke.key == "up" {
                            finder.selected.saturating_sub(1)
                        } else {
                            (finder.selected + 1).min(finder.rows.len().saturating_sub(1))
                        };
                        finder
                            .scroll
                            .scroll_to_item(finder.selected, ScrollStrategy::Center);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    "escape" => {
                        this.show_file_search(Mode::Browse, window, cx);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .h(px(44.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        label_button("files-search-back", self.t("file_explorer"), p)
                            .icon(icon(IconName::ArrowLeft, p.secondary))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_file_search(Mode::Browse, window, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_button(
                            "files-search-switch",
                            if finder.mode == Mode::Files {
                                IconName::Search
                            } else {
                                IconName::FileSearch
                            },
                            self.t(if finder.mode == Mode::Files {
                                "files_search_text"
                            } else {
                                "files_find_name"
                            }),
                            p,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let mode = if this.explorers[project].finder.mode == Mode::Files {
                                    Mode::Text
                                } else {
                                    Mode::Files
                                };
                                this.show_file_search(mode, window, cx);
                            },
                        )),
                    ),
            )
            .child(
                div().p(px(8.0)).child(
                    Input::new(&finder.input)
                        .aria_label(self.t(if finder.mode == Mode::Text {
                            "files_search_text"
                        } else {
                            "files_find_name"
                        }))
                        .w_full(),
                ),
            )
            .child(
                div()
                    .px(px(12.0))
                    .pb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.muted)
                    .child(status),
            )
            .child(
                uniform_list(
                    ("file-search-rows", project),
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let row = rows[index].clone();
                                let file_path = match &row {
                                    Row::File(path) => path,
                                    Row::Text(hit) => &hit.path,
                                };
                                let (path, text) = match &row {
                                    Row::File(path) => (
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned(),
                                        Some(path.to_string_lossy().into_owned()),
                                    ),
                                    Row::Text(hit) => (
                                        format!("{}:{}", hit.path.display(), hit.line),
                                        Some(hit.text.clone()),
                                    ),
                                };
                                label_button(("files-search-result", index), "", p)
                                    .debug_selector(move || format!("files-search-result-{index}"))
                                    .w_full()
                                    .h(px(if text.is_some() { 54.0 } else { 36.0 }))
                                    .rounded(px(0.0))
                                    .bg(if this.explorers[project].finder.selected == index {
                                        p.selected
                                    } else {
                                        p.background
                                    })
                                    .accessibility_label(path.clone())
                                    .tooltip(path.clone())
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .items_start()
                                            .text_size(px(12.0))
                                            .child(
                                                div()
                                                    .w_full()
                                                    .min_w_0()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .child(file_icon(
                                                        file_path,
                                                        this.appearance.light_theme,
                                                        None,
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_ellipsis()
                                                            .child(path),
                                                    ),
                                            )
                                            .when_some(text, |item, text| {
                                                item.child(
                                                    div()
                                                        .w_full()
                                                        .pl(px(22.0))
                                                        .text_ellipsis()
                                                        .text_size(px(11.0))
                                                        .text_color(p.muted)
                                                        .child(text),
                                                )
                                            }),
                                    )
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, window, cx| {
                                            if this.explorers[project].finder.generation
                                                != generation
                                            {
                                                return;
                                            }
                                            this.explorers[project].finder.selected = index;
                                            match &row {
                                                Row::File(path) if event.click_count() < 2 => this
                                                    .open_preview_file(
                                                        project,
                                                        path.clone(),
                                                        window,
                                                        cx,
                                                    ),
                                                Row::File(path) => this.open_real_file(
                                                    project,
                                                    path.clone(),
                                                    window,
                                                    cx,
                                                ),
                                                Row::Text(hit) => this.open_file_match(
                                                    project,
                                                    hit.path.clone(),
                                                    hit.line,
                                                    hit.column,
                                                    window,
                                                    cx,
                                                ),
                                            }
                                        },
                                    ))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .scrollbar(&finder.scroll),
            )
    }
}
