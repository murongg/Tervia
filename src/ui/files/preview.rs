use super::*;
use gpui_kit::base::TextView;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(in crate::ui) struct Previews {
    images: HashMap<usize, Arc<Image>>,
    sources: std::collections::HashSet<usize>,
}

impl Previews {
    pub(in crate::ui) fn reading(&self, file: &File, index: usize) -> bool {
        file.is_image() || (file.language == "markdown" && !self.sources.contains(&index))
    }
}

pub(super) enum Content {
    Text(String),
    Image(Arc<Image>),
}

pub(super) fn load(root: &std::path::Path, path: &std::path::Path) -> Result<Content, String> {
    let format = tervia::files::image_mime(path).and_then(ImageFormat::from_mime_type);
    match format {
        Some(format) => Ok(Content::Image(Arc::new(Image::from_bytes(
            format,
            tervia::files::read(root, path)?,
        )))),
        None => tervia::files::text(root, path).map(Content::Text),
    }
}

impl Tervia {
    pub(in crate::ui) fn prune_previews(&mut self, project: usize) {
        // Browsing should not retain every image's encoded bytes after its temporary tab is replaced.
        let opened = &self.state.projects[project].open_files;
        self.explorers[project]
            .previews
            .images
            .retain(|index, _| opened.contains(index));
    }
    pub(in crate::ui) fn show_file_source(&mut self, project: usize, index: usize) {
        self.explorers[project].previews.sources.insert(index);
    }

    pub(in crate::ui) fn focus_file_document(
        &self,
        project: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorers[project]
            .previews
            .reading(&self.state.projects[project].files[index], index)
        {
            self.focus.focus(window, cx);
        } else {
            self.editors[project][index].update(cx, |editor, cx| editor.focus(window, cx));
        }
    }
    pub(super) fn install_image(&mut self, project: usize, index: usize, image: Arc<Image>) {
        self.explorers[project].previews.images.insert(index, image);
    }

    pub(super) fn source_toggle(
        &self,
        project: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::button::Button {
        let source = self.explorers[project].previews.sources.contains(&index);
        label_button(
            "files-source-toggle",
            self.t(if source {
                "files_reading"
            } else {
                "files_source"
            }),
            super::super::theme::Palette::workbench(self.appearance.theme),
        )
        .debug_selector(|| "files-source-toggle".into())
        .on_click(cx.listener(move |this, _, window, cx| {
            let sources = &mut this.explorers[project].previews.sources;
            if !sources.remove(&index) {
                sources.insert(index);
                this.editors[project][index].update(cx, |editor, cx| editor.focus(window, cx));
            } else {
                this.focus.focus(window, cx);
            }
            cx.notify();
        }))
    }

    pub(super) fn file_preview(&self, project: usize, index: usize) -> Option<AnyElement> {
        let file = &self.state.projects[project].files[index];
        let previews = &self.explorers[project].previews;
        let p = super::super::theme::Palette::workbench(self.appearance.theme);
        if let Some(image) = previews.images.get(&index) {
            let failed = self.t("files_image_failed");
            return Some(
                div()
                    .debug_selector(|| "files-image-preview".into())
                    .size_full()
                    .p(px(24.0))
                    .child(
                        img(image.clone())
                            .size_full()
                            .object_fit(ObjectFit::Contain)
                            .with_fallback(move || {
                                div()
                                    .text_color(p.secondary)
                                    .child(failed)
                                    .into_any_element()
                            }),
                    )
                    .into_any_element(),
            );
        }
        if file.language == "markdown" && !previews.sources.contains(&index) {
            let root = self.state.projects[project].root.clone();
            let parent = file
                .disk_path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_path_buf();
            return Some(
                div()
                    .debug_selector(|| "files-markdown-preview".into())
                    .size_full()
                    .p(px(24.0))
                    .text_color(p.text)
                    .child(
                        TextView::markdown(("file-markdown", index), file.draft.clone())
                            .h_full()
                            .w_full()
                            .scrollable(true)
                            .image_source(move |url| {
                                let path = parent.join(url.as_ref());
                                let allowed = !path.is_absolute()
                                    && root.join(&path).canonicalize().ok().is_some_and(|file| {
                                        root.canonicalize()
                                            .ok()
                                            .is_some_and(|root| file.starts_with(root))
                                    })
                                    && std::fs::metadata(root.join(&path))
                                        .is_ok_and(|meta| meta.len() <= 4 * 1024 * 1024);
                                if allowed {
                                    root.join(path).into()
                                } else {
                                    Arc::new(Image::empty()).into()
                                }
                            }),
                    )
                    .into_any_element(),
            );
        }
        None
    }
}
