use super::*;
use gpui_kit::component::input::{DefinitionProvider, Rope, RopeExt as _};
use lsp_types::{LocationLink, ShowDocumentParams};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use tervia::lsp::tools::{Service, Stage};
use tervia::lsp::{Document, Query, Server};

mod preview;
#[cfg(all(test, feature = "ui-tests"))]
pub(in crate::ui::files) mod tests;
pub(in crate::ui) use preview::Peek;

#[derive(Clone)]
struct Origin {
    project: usize,
    index: usize,
    selection: usize,
    navigation: u64,
    path: PathBuf,
    text: String,
    offset: usize,
}
impl Origin {
    fn current(&self, view: &Tervia, cx: &App) -> bool {
        view.definition_source(self.project, self.index)
            && view.file_reads.selected(self.project, self.selection)
            && view.navigation.generation() == self.navigation
            && view.state.projects[self.project].files[self.index].disk_path == self.path
            && view.editors[self.project][self.index]
                .read(cx)
                .value()
                .as_str()
                == self.text
    }
}

struct Definitions {
    view: WeakEntity<Tervia>,
    project: usize,
    index: usize,
    server: Server,
    generation: Rc<Cell<u64>>,
    origin: Rc<RefCell<Option<Origin>>>,
}
impl Definitions {
    fn attach(
        view: WeakEntity<Tervia>,
        project: usize,
        index: usize,
        server: Server,
        editor: &Entity<EditorState>,
        cx: &mut App,
    ) {
        let origin = Rc::new(RefCell::new(None::<Origin>));
        let provider = Rc::new(Self {
            view: view.clone(),
            project,
            index,
            server,
            generation: Rc::new(Cell::new(0)),
            origin: origin.clone(),
        });
        editor.update(cx, |editor, _| {
            editor.lsp_mut().definition_provider = Some(provider);
            editor.lsp_mut().show_document = Some(Rc::new(move |params, window, cx| {
                let params = params.clone();
                let origin = origin.borrow().clone();
                let view = view.clone();
                // The hook runs inside an editor update. Defer before reading or
                // focusing any editor, including same-file definition targets.
                window.defer(cx, move |window, cx| {
                    if let Some(origin) = origin {
                        let _ = view.update(cx, |this, cx| {
                            this.follow_definition(origin, params, window, cx)
                        });
                    }
                });
                // Always consume this hook: the default handler would interpret
                // a cross-file target's coordinates within the current buffer.
                true
            }));
        });
    }
}
impl DefinitionProvider for Definitions {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Vec<LocationLink>>> {
        let symbol = text.word_range(offset).unwrap_or(offset..offset);
        let modifier_hover = window.modifiers().secondary() && !window.modifiers().alt;
        let text = text.to_string();
        let project = self.project;
        let index = self.index;
        let server = self.server.clone();
        let snapshot = self
            .view
            .update(cx, |this, _| {
                if !this.definition_source(project, index) {
                    return None;
                }
                let owner = &this.state.projects[project];
                let family = Server::for_language(&owner.files[index].language);
                let mut documents = owner
                    .open_files
                    .iter()
                    .copied()
                    .filter(|file| *file != index)
                    .filter_map(|file| {
                        let source = &owner.files[file];
                        (Server::for_language(&source.language) == family).then(|| Document {
                            path: source.disk_path.clone(),
                            language: source.language.clone(),
                            text: source.draft.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                let source = &owner.files[index];
                documents.push(Document {
                    path: source.disk_path.clone(),
                    language: source.language.clone(),
                    text: text.clone(),
                });
                let query = Query {
                    root: owner.root.clone(),
                    path: source.disk_path.clone(),
                    documents,
                    offset,
                };
                let origin = Origin {
                    project,
                    index,
                    selection: this.file_reads.selection(project),
                    navigation: this.navigation.generation(),
                    path: source.disk_path.clone(),
                    text,
                    offset,
                };
                let client = this.explorers[project].definitions.clone();
                let peek = &mut this.explorers[project].definition_preview;
                let explicit = peek.take_explicit();
                let token =
                    (modifier_hover && !explicit).then(|| peek.begin(index, symbol.clone()));
                Some((client, query, origin, token))
            })
            .ok()
            .flatten();
        let Some((client, query, origin, token)) = snapshot else {
            return Task::ready(Ok(Vec::new()));
        };
        let hovering = token.is_some();
        let generation = if hovering {
            self.generation.get()
        } else {
            let generation = self.generation.get().wrapping_add(1);
            self.generation.set(generation);
            generation
        };
        let latest = self.generation.clone();
        let ready = self.origin.clone();
        ready.replace(None);
        let view = self.view.clone();
        let query = Arc::new(query);
        let request = client.definition_request(&server, query.clone());
        window.spawn(cx, async move |cx| {
            // The provider is called inside an editor update. Source editor
            // reads and hover-cache dismissal must wait until that borrow ends.
            let hover = view
                .update_in(cx, |this, _, cx| match token {
                    Some(token) => Some(preview::Hover {
                        token,
                        range: symbol.clone(),
                        scroll: this.editors[project][index].read(cx).scroll_offset(),
                    }),
                    None => {
                        this.dismiss_definition_preview(cx);
                        None
                    }
                })
                .unwrap_or_default();
            if hovering && hover.is_none() {
                return Ok(Vec::new());
            }
            if let Some(hover) = &hover {
                // Replaced hover tasks are dropped by the editor: pointer travel
                // should not start service preparation or a lookup for every pixel.
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let current = view
                    .update_in(cx, |this, window, cx| {
                        origin.current(this, cx) && hover.current(this, &origin, window, cx)
                    })
                    .unwrap_or(false);
                if !current {
                    let _ = view.update_in(cx, |this, _, _| {
                        this.explorers[project]
                            .definition_preview
                            .cancel(&hover.token);
                    });
                    return Ok(Vec::new());
                }
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let locations = request?.run()?;
                    let preview = if hovering {
                        locations
                            .first()
                            .and_then(|location| preview::prepare(&query, location).ok())
                    } else {
                        None
                    };
                    Ok::<_, String>((locations, preview))
                })
                .await;
            let result = view
                .update_in(cx, |this, window, cx| {
                    if (!hovering && latest.get() != generation)
                        || !origin.current(this, cx)
                        || hover
                            .as_ref()
                            .is_some_and(|hover| !hover.current(this, &origin, window, cx))
                    {
                        if let Some(hover) = &hover {
                            this.explorers[project]
                                .definition_preview
                                .cancel(&hover.token);
                        }
                        return Vec::new();
                    }
                    match result {
                        Ok((locations, prepared)) => {
                            if hovering
                                && prepared.as_ref().is_some_and(|prepared| {
                                    !preview::target_current(prepared, this, project, cx)
                                })
                            {
                                // Do not leave an underlined link with coordinates
                                // resolved against a draft that changed during IO.
                                this.explorers[project].definition_preview.clear();
                                return Vec::new();
                            }
                            if locations.is_empty() {
                                if let Some(hover) = &hover {
                                    this.explorers[project]
                                        .definition_preview
                                        .cancel(&hover.token);
                                }
                                if !hovering {
                                    this.state.notice = "definition_not_found".into();
                                }
                            } else {
                                if let Some(hover) = &hover {
                                    let preview = prepared.map(|prepared| {
                                        preview::Preview::new(
                                            origin.clone(),
                                            symbol,
                                            locations[0].clone(),
                                            prepared,
                                            hover.scroll,
                                        )
                                    });
                                    this.explorers[project]
                                        .definition_preview
                                        .complete(&hover.token, preview);
                                }
                                ready.replace(Some(origin));
                                this.state.notice.clear();
                            }
                            cx.notify();
                            locations
                        }
                        Err(error) => {
                            if let Some(hover) = &hover {
                                this.explorers[project]
                                    .definition_preview
                                    .cancel(&hover.token);
                            }
                            if this.explorers[project]
                                .definitions
                                .status()
                                .is_some_and(|status| matches!(status.stage, Stage::Runtime(_)))
                            {
                                this.state.notice.clear();
                            } else if !hovering {
                                this.state.notice =
                                    this.f("definition_failed", &[("error", &error)]);
                            }
                            cx.notify();
                            Vec::new()
                        }
                    }
                })
                .unwrap_or_default();
            Ok(result)
        })
    }
}

impl Tervia {
    pub(in crate::ui) fn definition_setup(
        &self,
        project: usize,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let status = self.explorers[project].definitions.status()?;
        let file = self.state.projects[project].current_file()?;
        if Server::for_language(&file.language)
            .as_ref()
            .and_then(Service::from_server)
            != Some(status.service)
        {
            return None;
        }
        let service = status.service.label();
        let p = super::super::theme::Palette::workbench(self.appearance.light_theme);
        let runtime = match status.stage {
            Stage::Runtime(runtime) => Some(runtime),
            _ => None,
        };
        let message = match status.stage {
            Stage::Checking => self.f("definition_checking", &[("service", service)]),
            Stage::Waiting => self.f("definition_waiting", &[("service", service)]),
            Stage::Installing => self.f("definition_installing", &[("service", service)]),
            Stage::Downloading { received, total } => {
                let progress = total
                    .filter(|total| *total > 0)
                    .map(|total| {
                        format!(
                            "{:.0}%",
                            (received as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
                        )
                    })
                    .unwrap_or_else(|| "…".into());
                self.f(
                    "definition_downloading",
                    &[("service", service), ("progress", &progress)],
                )
            }
            Stage::Runtime(runtime) => self.f(
                "definition_runtime",
                &[("runtime", runtime.name), ("version", runtime.minimum)],
            ),
        };
        Some(
            div()
                .debug_selector(|| "definition-setup".into())
                .h(px(28.0))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(11.0))
                .text_color(p.secondary)
                .child(icon(IconName::Download, p.accent))
                .child(div().flex_1().min_w_0().text_ellipsis().child(message))
                .when_some(runtime, |bar, runtime| {
                    bar.child(
                        label_button(
                            "definition-runtime",
                            &self.f("definition_get_runtime", &[("runtime", runtime.name)]),
                            p,
                        )
                        .debug_selector(|| "definition-runtime".into())
                        .on_click(move |_, _, cx| cx.open_url(runtime.url)),
                    )
                    .child(
                        label_button("definition-retry", self.t("retry"), p)
                            .debug_selector(|| "definition-retry".into())
                            .on_click(cx.listener(|this, _, window, cx| {
                                let project = this.state.active_project;
                                let index = this.state.project().active_file;
                                if this.definition_source(project, index) {
                                    this.editors[project][index]
                                        .update(cx, |editor, cx| editor.focus(window, cx));
                                    window.dispatch_action(
                                        Box::new(gpui_kit::component::input::GoToDefinition),
                                        cx,
                                    );
                                }
                            })),
                    )
                }),
        )
    }

    fn definition_source(&self, project: usize, index: usize) -> bool {
        self.state.has_project()
            && self.state.active_project == project
            && self.state.project_visible(project)
            && self.state.projects[project].tab == Tab::Files
            && self.state.projects[project].active_file == index
            && self.state.projects[project].open_files.contains(&index)
            && !self.explorers[project]
                .previews
                .reading(&self.state.projects[project].files[index], index)
    }

    pub(in crate::ui) fn install_definitions(
        &self,
        project: usize,
        index: usize,
        editor: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        editor.update(cx, |editor, cx| {
            editor.clear_hover_state(cx);
            editor.lsp_mut().definition_provider = None;
            editor.lsp_mut().show_document = None;
        });
        if let Some(server) =
            Server::for_language(&self.state.projects[project].files[index].language)
        {
            Definitions::attach(cx.weak_entity(), project, index, server, editor, cx);
        }
    }

    fn follow_definition(
        &mut self,
        origin: Origin,
        params: ShowDocumentParams,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !origin.current(self, cx) {
            return;
        }
        self.dismiss_definition_preview(cx);
        let project = origin.project;
        let root = self.state.projects[project].root.clone();
        let open_paths = self.state.projects[project]
            .open_files
            .iter()
            .map(|index| self.state.projects[project].files[*index].disk_path.clone())
            .collect::<Vec<_>>();
        let cursor = self.editors[project][origin.index]
            .read(cx)
            .cursor_position();
        cx.spawn_in(window, async move |weak, cx| {
            let target = cx
                .background_executor()
                .spawn(async move {
                    preview::resolve(&root, params.uri.as_str(), open_paths.into_iter())
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if !origin.current(this, cx)
                    || this.editors[project][origin.index]
                        .read(cx)
                        .cursor_position()
                        != cursor
                {
                    return;
                }
                match target {
                    Ok(path) => {
                        if this.worktree_removing(&this.state.projects[project].root.join(&path)) {
                            return;
                        }
                        let position = params.selection.unwrap_or_default().start;
                        let Some(mut from) = this.file_location(project, cx) else {
                            return;
                        };
                        // A modifier click can precede caret movement. Back should
                        // return to that reference, not an unrelated old caret.
                        if let Ok(reference) = tervia::lsp::position(&origin.text, origin.offset) {
                            from.position.line = reference.line;
                            from.position.column = reference.character;
                        }
                        let target = Location {
                            path,
                            position: tervia::state::ReadingPosition {
                                line: position.line,
                                column: position.character,
                                source: true,
                                ..Default::default()
                            },
                        };
                        this.open_file_location(project, Travel::jump(from, target), window, cx);
                    }
                    Err(error) => {
                        this.state.notice = this.f("definition_failed", &[("error", &error)]);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}
