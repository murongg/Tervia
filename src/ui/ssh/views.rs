use super::super::theme::Palette;
use super::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};

impl Tervia {
    pub(in crate::ui) fn ssh_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = Palette::workbench(&self.appearance.theme);
        let query = self.ssh.search.read(cx).value().to_lowercase();
        let rows = self
            .ssh
            .hosts
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                format!("{} {} {}", h.name, h.address, h.user)
                    .to_lowercase()
                    .contains(&query)
                    .then_some(i)
            })
            .collect::<Vec<_>>();
        let sidebar = div()
            .w(px(220.0))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border)
            .child(
                toolbar(p)
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.t("ssh_hosts")),
                    )
                    .child(
                        icon_button(
                            "ssh-import",
                            IconName::Download,
                            self.t("ssh_import"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("ssh_import"))
                        .on_click(cx.listener(|this, _, _, cx| this.import_ssh_hosts(cx))),
                    )
                    .child(
                        icon_button("ssh-add", IconName::Plus, self.t("ssh_add"), p, cx)
                            .accessibility_label(self.t("ssh_add"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.edit_ssh_host(None, window, cx)
                            })),
                    ),
            )
            .child(
                div().px(px(12.0)).py(px(8.0)).flex_shrink_0().child(
                    Input::new(&self.ssh.search)
                        .w_full()
                        .prefix(icon(IconName::Search, p.secondary))
                        .aria_label(self.t("ssh_search")),
                ),
            )
            .child(if rows.is_empty() {
                div()
                    .p(px(14.0))
                    .text_color(p.secondary)
                    .text_size(px(12.0))
                    .child(self.t("ssh_empty"))
                    .into_any_element()
            } else {
                uniform_list(
                    "ssh-hosts",
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let index = rows[row];
                                let host = &this.ssh.hosts[index];
                                let target = host.clone();
                                let owner = cx.weak_entity();
                                list_row(
                                    ("ssh-host", index),
                                    this.ssh.active_host == Some(index),
                                    p,
                                    cx,
                                )
                                .debug_selector(move || format!("ssh-host-{index}"))
                                .h(px(52.0))
                                .accessibility_label(format!("{} · {}", host.name, host.address))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .px(px(10.0))
                                        .child(icon(IconName::Server, p.secondary))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.0))
                                                .child(
                                                    div().text_ellipsis().child(host.name.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(p.secondary)
                                                        .text_ellipsis()
                                                        .child(host.address.clone()),
                                                ),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_ssh_host(index, window, cx)
                                }))
                                .context_menu(
                                    move |menu, _, cx| {
                                        if let Some(view) = owner.upgrade() {
                                            view.read(cx).ssh_host_menu(
                                                &target,
                                                menu,
                                                owner.clone(),
                                            )
                                        } else {
                                            menu
                                        }
                                    },
                                )
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .into_any_element()
            });
        let session = self.selected_ssh_session();
        let content = if let Some(host) = self.ssh.selected().cloned() {
            let toolbar = toolbar(p)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{} · {}", host.name, host.address)),
                )
                .child(
                    icon_button(
                        "ssh-reconnect",
                        IconName::RefreshCw,
                        self.t("ssh_reconnect"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("ssh_reconnect"))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.connect_ssh_terminal(window, cx)),
                    ),
                )
                .child(
                    icon_button(
                        "ssh-search-terminal",
                        IconName::Search,
                        self.t("terminal_search"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("terminal_search"))
                    .disabled(session.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(session) = this.selected_ssh_session() {
                            this.ensure_console(session, window, cx)
                                .update(cx, |console, cx| console.open_search(window, cx));
                        }
                    })),
                )
                .child(
                    icon_button("ssh-edit-host", IconName::Pencil, self.t("ssh_edit"), p, cx)
                        .accessibility_label(self.t("ssh_edit"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_ssh_host(this.ssh.active_host, window, cx)
                        })),
                )
                .child(
                    icon_button(
                        "ssh-remove-host",
                        IconName::Trash,
                        self.t("ssh_remove"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("ssh_remove"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.remove_ssh_host(host.clone(), window, cx)
                    })),
                )
                .child(
                    icon_button(
                        "ssh-disconnect",
                        IconName::X,
                        self.t("ssh_disconnect"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("ssh_disconnect"))
                    .disabled(session.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(session) = this.selected_ssh_session() {
                            this.close_terminal_session(session, window, cx);
                        }
                    })),
                );
            let terminal = if let Some(session) = session {
                self.terminal_pane(session, window, cx)
            } else {
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        label_button("ssh-connect", self.t("ssh_connect"), p, cx)
                            .icon(icon(IconName::SquareTerminal, p.accent))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.connect_ssh_terminal(window, cx)
                            })),
                    )
            };
            div()
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .child(toolbar)
                .child(
                    div()
                        .debug_selector(|| "ssh-terminal".into())
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(terminal),
                )
        } else {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.0))
                .child(icon(IconName::Server, p.secondary).size(px(28.0)))
                .child(self.t("ssh_choose"))
                .child(
                    label_button("ssh-first-host", self.t("ssh_add"), p, cx).on_click(
                        cx.listener(|this, _, window, cx| this.edit_ssh_host(None, window, cx)),
                    ),
                )
        };
        div()
            .debug_selector(|| "ssh-workspace".into())
            .size_full()
            .min_h_0()
            .flex()
            .child(sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when_some(
                        self.ssh.error.clone().filter(|_| self.ssh.form.is_none()),
                        |body, error| {
                            body.child(
                                div()
                                    .id("ssh-error")
                                    .max_h(px(80.0))
                                    .overflow_y_scroll()
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .text_size(px(12.0))
                                    .text_color(p.red)
                                    .child(error),
                            )
                        },
                    )
                    .child(div().flex_1().min_h_0().child(content)),
            )
    }

    fn ssh_host_menu(&self, host: &Host, menu: PopupMenu, view: WeakEntity<Self>) -> PopupMenu {
        let p = self.p();
        let connecting = view.clone();
        let editing = view.clone();
        let connect = host.clone();
        let edit = host.clone();
        let remove = host.clone();
        menu.item(
            PopupMenuItem::new(self.t("ssh_connect"))
                .icon(icon(IconName::SquareTerminal, p.secondary))
                .on_click(move |_, window, cx| {
                    let target = connect.clone();
                    let _ = connecting.update(cx, |_, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            // Host removal can shift row indices while a menu is open.
                            if let Some(index) =
                                this.ssh.hosts.iter().position(|host| host == &target)
                            {
                                this.select_ssh_host(index, window, cx);
                            }
                        });
                    });
                }),
        )
        .item(
            PopupMenuItem::new(self.t("ssh_edit"))
                .icon(icon(IconName::Pencil, p.secondary))
                .on_click(move |_, window, cx| {
                    let target = edit.clone();
                    let _ = editing.update(cx, |_, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            if let Some(index) =
                                this.ssh.hosts.iter().position(|host| host == &target)
                            {
                                this.edit_ssh_host(Some(index), window, cx);
                            }
                        });
                    });
                }),
        )
        .separator()
        .item(
            PopupMenuItem::new(self.t("ssh_remove"))
                .icon(icon(IconName::Trash, p.secondary))
                .on_click(move |_, window, cx| {
                    let target = remove.clone();
                    let _ = view.update(cx, |_, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            this.remove_ssh_host(target, window, cx);
                        });
                    });
                }),
        )
    }
}
