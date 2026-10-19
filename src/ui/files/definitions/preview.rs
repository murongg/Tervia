use super::*;
use gpui_kit::base::TextView;
use std::{ops::Range, path::Path};

struct Session {
    source: usize,
    range: Range<usize>,
    token: Arc<()>,
    ready: bool,
    preview: Option<Preview>,
}

#[derive(Default)]
pub(in crate::ui) struct Peek {
    session: Option<Session>,
    explicit: bool,
}

impl Peek {
    pub(super) fn begin(&mut self, source: usize, range: Range<usize>) -> Arc<()> {
        let token = Arc::new(());
        self.session = Some(Session {
            source,
            range,
            token: token.clone(),
            ready: false,
            preview: None,
        });
        token
    }

    pub(super) fn current(&self, token: &Arc<()>) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| Arc::ptr_eq(&session.token, token))
    }

    pub(super) fn cancel(&mut self, token: &Arc<()>) {
        if self.current(token) {
            self.clear();
        }
    }

    pub(super) fn complete(&mut self, token: &Arc<()>, preview: Option<Preview>) {
        if let Some(session) = self
            .session
            .as_mut()
            .filter(|session| Arc::ptr_eq(&session.token, token))
        {
            session.ready = true;
            session.preview = preview;
        }
    }

    fn get(&self) -> Option<&Preview> {
        self.session.as_ref()?.preview.as_ref()
    }

    pub(super) fn clear(&mut self) -> Option<usize> {
        self.session.take().map(|session| session.source)
    }

    pub(super) fn take_explicit(&mut self) -> bool {
        std::mem::take(&mut self.explicit)
    }
}

pub(super) struct Hover {
    pub token: Arc<()>,
    pub range: Range<usize>,
    pub scroll: Point<Pixels>,
}

impl Hover {
    pub(super) fn current(
        &self,
        view: &Tervia,
        origin: &Origin,
        window: &Window,
        cx: &App,
    ) -> bool {
        view.explorers[origin.project]
            .definition_preview
            .current(&self.token)
            && window.modifiers().secondary()
            && !window.modifiers().alt
            && view.editors[origin.project][origin.index]
                .read(cx)
                .scroll_offset()
                == self.scroll
            && view.editors[origin.project][origin.index]
                .read(cx)
                .range_to_bounds(&self.range)
                .is_some_and(|bounds| bounds.contains(&window.mouse_position()))
    }
}

pub(super) struct Prepared {
    path: PathBuf,
    text: String,
    was_open: bool,
    code: String,
    language: String,
    line: u32,
}

pub(in crate::ui) struct Preview {
    origin: Origin,
    range: Range<usize>,
    location: LocationLink,
    prepared: Prepared,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: Point<Pixels>,
}

impl Preview {
    pub(super) fn new(
        origin: Origin,
        range: Range<usize>,
        location: LocationLink,
        prepared: Prepared,
        scroll: Point<Pixels>,
    ) -> Self {
        Self {
            origin,
            range,
            location,
            prepared,
            bounds: Rc::default(),
            scroll,
        }
    }
}

pub(super) fn resolve(
    root: &Path,
    uri: &str,
    open: impl Iterator<Item = PathBuf>,
) -> Result<PathBuf, String> {
    let path = tervia::lsp::target_path(root, uri)?;
    let open = open.collect::<Vec<_>>();
    if open.contains(&path) {
        return Ok(path);
    }
    let canonical = root.join(&path).canonicalize().map_err(|e| e.to_string())?;
    Ok(open
        .into_iter()
        .find(|open| {
            root.join(open)
                .canonicalize()
                .is_ok_and(|open| open == canonical)
        })
        .unwrap_or(path))
}

pub(super) fn prepare(query: &Query, location: &LocationLink) -> Result<Prepared, String> {
    let path = resolve(
        &query.root,
        location.target_uri.as_str(),
        query.documents.iter().map(|doc| doc.path.clone()),
    )?;
    let open = query.documents.iter().find(|doc| doc.path == path);
    let text = match open {
        Some(doc) => doc.text.clone(),
        None => tervia::files::text(&query.root, &path)?,
    };
    let line = location.target_selection_range.start.line;
    let start = line.saturating_sub(1) as usize;
    let code = text
        .lines()
        .skip(start)
        .take(8)
        .map(|line| {
            let mut clipped = line.chars().take(160).collect::<String>();
            if line.chars().count() > 160 {
                clipped.push('…');
            }
            clipped
        })
        .collect::<Vec<_>>()
        .join("\n");
    let language = open.map(|doc| doc.language.clone()).unwrap_or_else(|| {
        gpui_kit::component::highlighter::language_for_path(&path.to_string_lossy()).to_string()
    });
    Ok(Prepared {
        path,
        text,
        was_open: open.is_some(),
        code,
        language,
        line,
    })
}

pub(super) fn target_current(prepared: &Prepared, view: &Tervia, project: usize, cx: &App) -> bool {
    let owner = &view.state.projects[project];
    match owner
        .open_files
        .iter()
        .copied()
        .find(|index| owner.files[*index].disk_path == prepared.path)
    {
        Some(index) => view.editors[project][index].read(cx).text() == prepared.text.as_str(),
        None => !prepared.was_open,
    }
}

impl Tervia {
    pub(in crate::ui) fn definition_modifiers(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.modifiers.secondary() || event.modifiers.alt {
            self.dismiss_definition_preview(cx);
            return;
        }
        if !self.state.has_project() || self.state.project().tab != Tab::Files {
            return;
        }
        let project = self.state.active_project;
        let index = self.state.project().active_file;
        let position = window.mouse_position();
        if !self.definition_source(project, index)
            || !self.editors[project][index]
                .read(cx)
                .input_bounds()
                .contains(&position)
        {
            return;
        }
        let selection = self.file_reads.selection(project);
        let navigation = self.navigation.generation();
        let weak = cx.weak_entity();
        // Base currently hit-tests definitions only on mouse movement. Reuse
        // that path for modifier-only changes until the editor handles them.
        // Defer so neither entity event handler is borrowed during redispatch.
        window.defer(cx, move |window, cx| {
            let modifiers = window.modifiers();
            if !modifiers.secondary() || modifiers.alt || window.mouse_position() != position {
                return;
            }
            let current = weak
                .update(cx, |this, _| {
                    this.definition_source(project, index)
                        && this.file_reads.selected(project, selection)
                        && this.navigation.generation() == navigation
                })
                .unwrap_or(false);
            if current {
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers,
                    }),
                    cx,
                );
            }
        });
    }

    pub(in crate::ui) fn dismiss_definition_preview(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for project in 0..self.explorers.len() {
            if let Some(source) = self.explorers[project].definition_preview.clear() {
                changed = true;
                // The editor caches same-symbol results. Clear its public hover
                // state too so a later modifier hover can reopen this preview.
                self.editors[project][source].update(cx, |editor, cx| editor.clear_hover_state(cx));
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::ui) fn definition_pointer(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.state.has_project() {
            return;
        }
        let project = self.state.active_project;
        let Some(session) = &self.explorers[project].definition_preview.session else {
            return;
        };
        let trigger = self.editors[project][session.source]
            .read(cx)
            .range_to_bounds(&session.range);
        let overlay = session
            .preview
            .as_ref()
            .and_then(|preview| preview.bounds.get());
        let keep = trigger.map(|trigger| overlay.map_or(trigger, |bounds| trigger.union(&bounds)));
        if !event.modifiers.secondary()
            || event.modifiers.alt
            || !keep.is_some_and(|bounds| bounds.contains(&event.position))
        {
            self.dismiss_definition_preview(cx);
        }
    }

    pub(in crate::ui) fn prepare_definition_action(&mut self) {
        if self.state.has_project() {
            let project = self.state.active_project;
            let index = self.state.project().active_file;
            self.explorers[project].definition_preview.explicit = self
                .definition_source(project, index)
                && Server::for_language(&self.state.projects[project].files[index].language)
                    .is_some();
        }
    }

    pub(in crate::ui) fn definition_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left
            || !event.modifiers.secondary()
            || event.modifiers.alt
            || !self.state.has_project()
        {
            return;
        }
        let project = self.state.active_project;
        let index = self.state.project().active_file;
        if !self.definition_source(project, index) {
            return;
        }
        let editor = self.editors[project][index].read(cx);
        if self.explorers[project]
            .definition_preview
            .get()
            .and_then(|preview| preview.bounds.get())
            .is_some_and(|bounds| bounds.contains(&event.position))
        {
            return;
        }
        if editor.lsp().definition_provider.is_none()
            || !editor.input_bounds().contains(&event.position)
        {
            return;
        }
        if self.explorers[project]
            .definition_preview
            .session
            .as_ref()
            .is_some_and(|session| {
                session.source == index
                    && session.ready
                    && editor
                        .range_to_bounds(&session.range)
                        .is_some_and(|range| range.contains(&event.position))
            })
        {
            return;
        }
        let selection = self.file_reads.selection(project);
        let navigation = self.navigation.generation();
        let weak = cx.weak_entity();
        // Let native input handling place the caret first. Dispatching now would
        // query the previous caret, and a ready native link already owns its click.
        window.defer(cx, move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.definition_source(project, index)
                    && this.file_reads.selected(project, selection)
                    && this.navigation.generation() == navigation
                {
                    this.dismiss_definition_preview(cx);
                    this.editors[project][index].update(cx, |editor, cx| editor.focus(window, cx));
                    window
                        .dispatch_action(Box::new(gpui_kit::component::input::GoToDefinition), cx);
                }
            });
        });
    }

    pub(in crate::ui) fn render_definition_preview(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let project = self.state.active_project;
        let preview = self.explorers[project].definition_preview.get()?;
        if !window.modifiers().secondary()
            || window.modifiers().alt
            || !preview.origin.current(self, cx)
            || self.editors[project][preview.origin.index]
                .read(cx)
                .scroll_offset()
                != preview.scroll
            || !target_current(&preview.prepared, self, project, cx)
        {
            let token = self.explorers[project]
                .definition_preview
                .session
                .as_ref()?
                .token
                .clone();
            cx.defer_in(window, move |this, _, cx| {
                if this.explorers[project].definition_preview.current(&token) {
                    this.dismiss_definition_preview(cx);
                }
            });
            return None;
        }
        let trigger = self.editors[project][preview.origin.index]
            .read(cx)
            .range_to_bounds(&preview.range)?;
        // Choose a side before fitting the popup: snapping a below-symbol
        // popup upward could otherwise cover the reference's click target.
        let above = (trigger.top() - px(12.0)).max(px(0.0));
        let below = (window.viewport_size().height - trigger.bottom() - px(12.0)).max(px(0.0));
        let show_above = below < px(280.0) && above > below;
        let (anchor, position, available) = if show_above {
            (
                Anchor::BottomLeft,
                trigger.origin - point(px(0.0), px(4.0)),
                above,
            )
        } else {
            (
                Anchor::TopLeft,
                trigger.bottom_left() + point(px(0.0), px(4.0)),
                below,
            )
        };
        if available < px(40.0) {
            return None;
        }
        let p = super::super::super::theme::Palette::workbench(self.appearance.light_theme);
        let bounds = preview.bounds.clone();
        let origin = preview.origin.clone();
        let location = preview.location.clone();
        let token = self.explorers[project]
            .definition_preview
            .session
            .as_ref()?
            .token
            .clone();
        let code = &preview.prepared.code;
        // Arbitrary source can contain Markdown fences. A longer fence keeps it
        // literal code and prevents project text from becoming links or images.
        let fence = "`".repeat(
            code.split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0)
                .max(2)
                + 1,
        );
        let markdown = format!("{fence}{}\n{code}\n{fence}", preview.prepared.language);
        Some(
            deferred(
                anchored()
                    .anchor(anchor)
                    .position(position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .id("definition-preview")
                            .debug_selector(|| "definition-preview".into())
                            .relative()
                            .w(px(520.0).min(window.viewport_size().width - px(24.0)))
                            .max_h(px(280.0).min(available))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .bg(p.sidebar)
                            .border_1()
                            .border_color(p.border)
                            .rounded(px(5.0))
                            .on_mouse_move(|_, _, cx| cx.stop_propagation())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(
                                div()
                                    .h(px(32.0))
                                    .flex_shrink_0()
                                    .px(px(10.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(file_icon(
                                        &preview.prepared.path,
                                        self.appearance.light_theme,
                                        None,
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .text_size(px(11.0))
                                            .text_color(p.secondary)
                                            .child(format!(
                                                "{}:{}",
                                                preview.prepared.path.display(),
                                                preview.prepared.line + 1
                                            )),
                                    )
                                    .child(
                                        label_button(
                                            "definition-preview-open",
                                            self.t("go_to_definition"),
                                            p,
                                        )
                                        .debug_selector(|| "definition-preview-open".into())
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                let current = this.explorers[project]
                                                    .definition_preview
                                                    .current(&token)
                                                    && this.explorers[project]
                                                        .definition_preview
                                                        .get()
                                                        .is_some_and(|preview| {
                                                            target_current(
                                                                &preview.prepared,
                                                                this,
                                                                project,
                                                                cx,
                                                            )
                                                        });
                                                if !current {
                                                    this.dismiss_definition_preview(cx);
                                                    return;
                                                }
                                                this.follow_definition(
                                                    origin.clone(),
                                                    ShowDocumentParams {
                                                        uri: location.target_uri.clone(),
                                                        external: Some(false),
                                                        take_focus: Some(true),
                                                        selection: Some(
                                                            location.target_selection_range,
                                                        ),
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(10.0))
                                    .pb(px(8.0))
                                    .min_h_0()
                                    .overflow_hidden()
                                    .text_size(px(12.0))
                                    .child(
                                        TextView::markdown("definition-preview-code", markdown)
                                            .w_full()
                                            .scrollable(false),
                                    ),
                            )
                            .child(
                                canvas(
                                    move |actual, _, _| {
                                        bounds.set(Some(actual));
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
