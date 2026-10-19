use super::*;
use gpui_kit::component::input::{DefinitionProvider, Rope};
use lsp_types::{LocationLink, ShowDocumentParams};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tervia::lsp::tools::{Service, Stage};
use tervia::lsp::{Document, Query, Server};

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

#[derive(Clone)]
struct Origin {
    project: usize,
    index: usize,
    selection: usize,
    navigation: u64,
    path: PathBuf,
    text: String,
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
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Vec<LocationLink>>> {
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
                };
                let client = this.explorers[project].definitions.clone();
                Some((client, query, origin))
            })
            .ok()
            .flatten();
        let Some((client, query, origin)) = snapshot else {
            return Task::ready(Ok(Vec::new()));
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let latest = self.generation.clone();
        let ready = self.origin.clone();
        ready.replace(None);
        let view = self.view.clone();
        let request = client.definition_request(&server, query);
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { request?.run() })
                .await;
            let result = view
                .update(cx, |this, cx| {
                    if latest.get() != generation || !origin.current(this, cx) {
                        return Vec::new();
                    }
                    match result {
                        Ok(locations) => {
                            if locations.is_empty() {
                                this.state.notice = "definition_not_found".into();
                            } else {
                                ready.replace(Some(origin));
                                this.state.notice.clear();
                            }
                            cx.notify();
                            locations
                        }
                        Err(error) => {
                            if this.explorers[project]
                                .definitions
                                .status()
                                .is_some_and(|status| matches!(status.stage, Stage::Runtime(_)))
                            {
                                this.state.notice.clear();
                            } else {
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
                    let path = tervia::lsp::target_path(&root, params.uri.as_str())?;
                    if open_paths.contains(&path) {
                        return Ok::<_, String>(path);
                    }
                    let canonical = root.join(&path).canonicalize().map_err(|e| e.to_string())?;
                    // Servers may normalize a symlink URI. Resolve open identities
                    // off the UI thread before falling back to a fresh disk read.
                    Ok(open_paths
                        .into_iter()
                        .find(|open| {
                            root.join(open)
                                .canonicalize()
                                .is_ok_and(|open| open == canonical)
                        })
                        .unwrap_or(path))
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
                        if let Some(index) = this.state.projects[project]
                            .files
                            .iter()
                            .position(|file| file.disk_path == path)
                            && this.state.projects[project].open_files.contains(&index)
                        {
                            // Reuse open buffers: reading from disk here could erase an
                            // unsaved target, and the server resolved its draft positions.
                            this.state.projects[project].pin_file(index);
                            this.select_open_file(project, index, window, cx);
                            this.editors[project][index].update(cx, |editor, cx| {
                                editor.set_cursor_position(position, window, cx)
                            });
                        } else {
                            this.open_file_match(
                                project,
                                path,
                                position.line as usize + 1,
                                position.character as usize,
                                window,
                                cx,
                            );
                        }
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
