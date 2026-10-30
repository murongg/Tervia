use super::*;
use gpui_kit::base::TextView;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(in crate::ui) struct Previews {
    images: HashMap<usize, ImagePreview>,
    sources: std::collections::HashSet<usize>,
}

struct ImagePreview {
    image: Result<Arc<RenderImage>, String>,
    zoom: Option<f32>,
    scroll: ScrollHandle,
}

impl ImagePreview {
    fn scale(&self, size: Size<Pixels>) -> f32 {
        self.zoom.unwrap_or_else(|| {
            let available = self.scroll.bounds().size;
            ((available.width - px(48.0)).max(px(1.0)) / size.width)
                .min((available.height - px(48.0)).max(px(1.0)) / size.height)
                .min(1.0)
        })
    }
}

impl Previews {
    pub(in crate::ui) fn reading(&self, file: &File, index: usize) -> bool {
        file.is_image() || (file.language == "markdown" && !self.sources.contains(&index))
    }
}

pub(super) enum Content {
    Text(String),
    Image(Result<Arc<RenderImage>, String>),
}

pub(super) fn load(
    root: &std::path::Path,
    path: &std::path::Path,
    renderer: SvgRenderer,
) -> Result<Content, String> {
    let format = tervia::files::image_mime(path).and_then(ImageFormat::from_mime_type);
    match format {
        Some(format) => Ok(Content::Image(
            Image::from_bytes(format, tervia::files::read(root, path)?)
                .to_image_data(renderer)
                .map_err(|error| error.to_string()),
        )),
        None => tervia::files::text(root, path).map(Content::Text),
    }
}

impl Tervia {
    pub(in crate::ui) fn prune_previews(&mut self, project: usize) {
        // Browsing must release decoded frames when a temporary image tab is replaced.
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
    pub(super) fn install_image(
        &mut self,
        project: usize,
        index: usize,
        image: Result<Arc<RenderImage>, String>,
    ) {
        let previews = &mut self.explorers[project].previews;
        if let Some(preview) = previews.images.get_mut(&index) {
            preview.image = image;
        } else {
            previews.images.insert(
                index,
                ImagePreview {
                    image,
                    zoom: None,
                    scroll: ScrollHandle::new(),
                },
            );
        }
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
            super::super::theme::Palette::workbench(&self.appearance.theme),
            cx,
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

    pub(super) fn file_preview(
        &self,
        project: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let file = &self.state.projects[project].files[index];
        let previews = &self.explorers[project].previews;
        let p = super::super::theme::Palette::workbench(&self.appearance.theme);
        if let Some(preview) = previews.images.get(&index) {
            let Ok(image) = &preview.image else {
                return Some(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(p.secondary)
                        .child(self.t("files_image_failed"))
                        .into_any_element(),
                );
            };
            // GPUI rasterizes SVGs at 2x for smoothing; 100% must use their logical dimensions.
            let raster_scale =
                if tervia::files::image_mime(&file.disk_path) == Some("image/svg+xml") {
                    SMOOTH_SVG_SCALE_FACTOR
                } else {
                    1.0
                };
            let natural = image
                .size(0)
                .map(|value| px(u32::from(value) as f32 / raster_scale));
            let mut controls = toolbar(p).h(px(36.0)).justify_center();
            for (id, glyph, key, factor) in [
                (
                    "files-image-zoom-out",
                    IconName::ZoomOut,
                    "files_image_zoom_out",
                    0.8,
                ),
                (
                    "files-image-zoom-in",
                    IconName::ZoomIn,
                    "files_image_zoom_in",
                    1.25,
                ),
            ] {
                if factor > 1.0 {
                    controls = controls.child(
                        div()
                            .min_w(px(72.0))
                            .text_center()
                            .text_size(px(11.0))
                            .text_color(p.secondary)
                            .child(
                                preview
                                    .zoom
                                    .map(|zoom| format!("{:.0}%", zoom * 100.0))
                                    .unwrap_or_else(|| self.t("files_image_fit").to_owned()),
                            ),
                    );
                }
                controls = controls.child(
                    icon_button(id, glyph, self.t(key), p, cx)
                        .debug_selector(move || id.into())
                        .accessibility_label(self.t(key))
                        .disabled(if factor < 1.0 {
                            preview.scale(natural) <= 0.01
                        } else {
                            preview.scale(natural) >= 16.0
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(preview) =
                                this.explorers[project].previews.images.get_mut(&index)
                            {
                                preview.zoom =
                                    Some((preview.scale(natural) * factor).clamp(0.01, 16.0));
                                cx.notify();
                            }
                        })),
                );
            }
            controls = controls
                .child(
                    label_button("files-image-original", "100%", p, cx)
                        .debug_selector(|| "files-image-original".into())
                        .accessibility_label(self.t("files_image_original"))
                        .tooltip(self.t("files_image_original").to_owned())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(preview) =
                                this.explorers[project].previews.images.get_mut(&index)
                            {
                                preview.zoom = Some(1.0);
                                cx.notify();
                            }
                        })),
                )
                .child(
                    icon_button(
                        "files-image-fit",
                        IconName::Maximize,
                        self.t("files_image_fit"),
                        p,
                        cx,
                    )
                    .debug_selector(|| "files-image-fit".into())
                    .accessibility_label(self.t("files_image_fit"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(preview) =
                            this.explorers[project].previews.images.get_mut(&index)
                        {
                            preview.zoom = None;
                            preview.scroll.set_offset(point(px(0.0), px(0.0)));
                            cx.notify();
                        }
                    })),
                );
            return Some(
                div()
                    .debug_selector(|| "files-image-preview".into())
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(controls)
                    .child(
                        // Keep intrinsic image dimensions out of the flex pane's size calculation.
                        div().flex_1().min_h_0().min_w_0().relative().child(
                            div()
                                .id(("image-viewport", index))
                                .debug_selector(|| "files-image-viewport".into())
                                .absolute()
                                .inset_0()
                                .overflow_scroll()
                                .track_scroll(&preview.scroll)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .p(px(24.0))
                                        .when(preview.zoom.is_none(), |body| body.size_full())
                                        .when_some(preview.zoom, |body, zoom| {
                                            // The scroll surface must include both edges of a centered, enlarged image.
                                            body.w(natural.width * zoom + px(48.0))
                                                .h(natural.height * zoom + px(48.0))
                                                .min_w_full()
                                                .min_h_full()
                                        })
                                        .child(
                                            img(image.clone())
                                                .id(("preview-image", index))
                                                .debug_selector(|| "files-preview-image".into())
                                                .flex_shrink_0()
                                                .w(natural.width * preview.zoom.unwrap_or(1.0))
                                                .h(natural.height * preview.zoom.unwrap_or(1.0))
                                                .when(preview.zoom.is_none(), |image| {
                                                    image.max_w_full().max_h_full()
                                                })
                                                .object_fit(if preview.zoom.is_some() {
                                                    ObjectFit::Contain
                                                } else {
                                                    ObjectFit::ScaleDown
                                                }),
                                        ),
                                ),
                        ),
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
