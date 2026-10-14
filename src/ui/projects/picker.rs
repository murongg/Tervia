use super::*;

impl Tervia {
    pub(super) fn dismiss_directory_picker(
        close: &Option<WeakEntity<PopoverState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(close) = close {
            let _ = close.update(cx, |state, cx| state.dismiss(window, cx));
        }
    }

    pub(super) fn directory_picker(
        &self,
        parent: usize,
        close: WeakEntity<PopoverState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.navigation.rows(parent, true);
        let ui = &self.navigation.roots[&parent];
        let p = self.p();
        let next = close.clone();
        let previous = close.clone();
        let expand = close.clone();
        let collapse = close.clone();
        let select = close.clone();
        div()
            .id(("directory-picker-content", parent))
            .debug_selector(move || format!("directory-picker-content-{parent}"))
            .w(px(300.0))
            .h(px((rows.len() as f32 * 40.0 + 26.0).min(360.0)))
            .flex()
            .flex_col()
            .min_h_0()
            .text_color(p.text)
            .key_context("DirectoryPicker")
            .track_focus(&ui.focus)
            .on_action(cx.listener(move |this, _: &NextDirectory, window, cx| {
                this.directory_picker_key(parent, "down", next.clone(), window, cx)
            }))
            .on_action(cx.listener(move |this, _: &PreviousDirectory, window, cx| {
                this.directory_picker_key(parent, "up", previous.clone(), window, cx)
            }))
            .on_action(cx.listener(move |this, _: &ExpandDirectory, window, cx| {
                this.directory_picker_key(parent, "right", expand.clone(), window, cx)
            }))
            .on_action(cx.listener(move |this, _: &CollapseDirectory, window, cx| {
                this.directory_picker_key(parent, "left", collapse.clone(), window, cx)
            }))
            .on_action(cx.listener(move |this, _: &SelectDirectory, window, cx| {
                this.directory_picker_key(parent, "enter", select.clone(), window, cx)
            }))
            .child(
                div()
                    .px(px(8.0))
                    .pb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t("project_choose_directory_hint")),
            )
            .child(
                uniform_list(
                    ("directory-picker-list", parent),
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.navigation_row(&rows[index], Some(close.clone()), cx))
                            .collect()
                    }),
                )
                .size_full()
                .track_scroll(&ui.scroll),
            )
            .into_any_element()
    }

    fn directory_picker_key(
        &mut self,
        parent: usize,
        key: &str,
        close: WeakEntity<PopoverState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.navigation.rows(parent, true);
        let paths: Vec<_> = rows
            .iter()
            .filter_map(|row| match row {
                Node::Directory { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        let ui = self.navigation.roots.get_mut(&parent).unwrap();
        let current = ui
            .cursor
            .as_ref()
            .and_then(|path| paths.iter().position(|candidate| candidate == path));
        match key {
            "down" | "up" => {
                let index = match (key, current) {
                    ("up", Some(index)) => index.saturating_sub(1),
                    ("up", None) => paths.len() - 1,
                    (_, Some(index)) => (index + 1).min(paths.len() - 1),
                    _ => 0,
                };
                ui.cursor = Some(paths[index].clone());
                if let Some(row) = rows.iter().position(
                    |row| matches!(row, Node::Directory {path, ..} if *path == paths[index]),
                ) {
                    ui.scroll.scroll_to_item(row, ScrollStrategy::Top);
                }
            }
            "enter" => {
                let path = ui.cursor.clone().unwrap_or_default();
                Self::dismiss_directory_picker(&Some(close), window, cx);
                self.activate_directory(parent, path, window, cx);
            }
            "right" => {
                if let Some(path) = ui.cursor.clone()
                    && !path.as_os_str().is_empty()
                    && !ui.tree.is_expanded(&path)
                {
                    self.toggle_navigation(parent, path, window, cx);
                }
            }
            "left" => {
                if let Some(path) = ui.cursor.clone()
                    && !path.as_os_str().is_empty()
                {
                    if ui.tree.is_expanded(&path) {
                        self.toggle_navigation(parent, path, window, cx);
                    } else {
                        ui.cursor = path.parent().map(PathBuf::from);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }
}
