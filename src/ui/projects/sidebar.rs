use super::*;
use crate::ui::controls::icon_style;
use gpui_kit::component::menu::ContextMenuExt as _;
use std::time::Duration;

#[derive(Clone)]
struct ProjectDrag {
    owner: usize,
    name: String,
    light: bool,
    favorite: bool,
}
impl Render for ProjectDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = crate::ui::theme::Palette::new(self.light);
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .bg(p.raised)
            .text_color(p.text)
            .child(self.name.clone())
    }
}

#[derive(Default)]
pub(in crate::ui) struct Sidebar {
    scroll: UniformListScrollHandle,
    hovered: bool,
    peek: bool,
    pending: Option<Task<()>>,
}

impl Sidebar {
    pub(super) fn collapse(&mut self) {
        self.pending = None;
        self.peek = false;
        self.hovered = false;
    }
}

impl Tervia {
    fn hover_project_sidebar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let sidebar = &mut self.navigation.sidebar;
        sidebar.hovered = hovered;
        // Reentry cancels the delayed close before it can hide the panel under the pointer.
        sidebar.pending = None;
        if self.state.sidebar_expanded || sidebar.peek == hovered {
            return;
        }
        if hovered {
            sidebar.peek = true;
            cx.notify();
            return;
        }
        let delay = Duration::from_millis(150);
        sidebar.pending = Some(cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor().timer(delay).await;
                let dragging = weak.update(cx, |this, cx| {
                    let sidebar = &mut this.navigation.sidebar;
                    if sidebar.hovered != hovered || this.state.sidebar_expanded {
                        return false;
                    }
                    // GPUI suppresses hover during a drag; keep the drop targets in place until it ends.
                    if !hovered && cx.has_active_drag() {
                        return true;
                    }
                    sidebar.peek = hovered;
                    cx.notify();
                    false
                });
                if !dragging.unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    pub(in crate::ui) fn toggle_project_sidebar(&mut self, cx: &mut Context<Self>) {
        self.navigation.sidebar.collapse();
        self.state.toggle_sidebar();
        self.persist_workspace(cx);
        cx.notify();
    }

    pub(in crate::ui) fn dismiss_project_sidebar(&mut self, cx: &mut Context<Self>) -> bool {
        if self.state.sidebar_expanded || !self.navigation.sidebar.peek {
            return false;
        }
        self.navigation.sidebar.collapse();
        cx.notify();
        true
    }

    fn project_row(&self, parent: usize, expanded: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p();
        let active = self.state.has_project() && self.working_directory_owner() == parent;
        let name = self.state.project_label(parent);
        let favorite = self.state.project_favorite(parent);
        let drag = ProjectDrag {
            owner: parent,
            name: name.clone(),
            light: self.appearance.light_theme,
            favorite,
        };
        let target = self.state.project_target(parent);
        let branch = self.state.projects[target].branch.clone();
        let button = Button::new(("project", parent))
            .custom(icon_style(p, cx))
            .compact()
            .w_full()
            .h(px(40.0))
            .px_0()
            .rounded(px(6.0))
            .bg(if active {
                p.selected
            } else {
                p.sidebar.opacity(0.0)
            })
            .debug_selector(move || format!("project-root-{parent}"))
            .accessibility_label(self.f("switch_project", &[("name", &name)]))
            .tooltip(self.state.projects[parent].path.clone())
            .relative()
            .when(self.project_attention(parent, cx), |button| {
                button
                    .accessibility_label(format!(
                        "{} · {}",
                        self.f("switch_project", &[("name", &name)]),
                        self.t("terminal_attention")
                    ))
                    .tooltip(format!(
                        "{} · {}",
                        self.state.projects[parent].path,
                        self.t("terminal_attention")
                    ))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.0))
                            .right(px(3.0))
                            .size(px(6.0))
                            .rounded_full()
                            .bg(p.accent),
                    )
            })
            .child(if expanded {
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .pr(px(22.0))
                    .child(
                        div()
                            .w(px(40.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon(
                                    IconName::Folder,
                                    if active { p.accent } else { p.secondary },
                                )
                                .size(px(14.0)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .font_weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(p.text)
                            .child(name),
                    )
                    .when(branch != "—", |row| {
                        row.child(
                            div()
                                .max_w(px(68.0))
                                .flex()
                                .items_center()
                                .gap(px(3.0))
                                .text_size(px(11.0))
                                .text_color(p.secondary)
                                .child(icon(IconName::GitBranch, p.secondary).size(px(11.0)))
                                .child(div().min_w_0().text_ellipsis().child(branch)),
                        )
                    })
            } else {
                div()
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(if active { p.accent } else { p.secondary })
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(13.0))
                    .child(
                        name.chars()
                            .next()
                            .unwrap_or('T')
                            .to_uppercase()
                            .to_string(),
                    )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.change_project(this.state.project_target(parent), window, cx);
            }));
        let owner = cx.weak_entity();
        div()
            .id(("project-row", parent))
            .debug_selector(move || format!("project-row-{parent}"))
            .h(px(40.0))
            .w_full()
            .relative()
            .child(button)
            .when(expanded, |row| {
                row.child(
                    Button::new(("favorite-project", parent))
                        .custom(icon_style(p, cx))
                        .compact()
                        .absolute()
                        .top(px(8.0))
                        .right(px(4.0))
                        .w(px(24.0))
                        .h(px(24.0))
                        .icon(
                            icon(
                                IconName::Star,
                                if self.state.project_favorite(parent) {
                                    p.accent
                                } else {
                                    p.secondary.opacity(0.65)
                                },
                            )
                            .size(px(12.0)),
                        )
                        .debug_selector(move || format!("favorite-project-{parent}"))
                        .accessibility_label(self.t(if self.state.project_favorite(parent) {
                            "project_unfavorite"
                        } else {
                            "project_favorite"
                        }))
                        .tooltip(self.t(if self.state.project_favorite(parent) {
                            "project_unfavorite"
                        } else {
                            "project_favorite"
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.state.toggle_project_favorite(parent);
                            this.persist_workspace(cx);
                            cx.notify();
                        })),
                )
            })
            .when(!expanded && self.state.project_favorite(parent), |row| {
                row.child(
                    div()
                        .absolute()
                        .bottom(px(2.0))
                        .right(px(3.0))
                        .child(icon(IconName::Star, p.accent).size(px(9.0))),
                )
            })
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .drag_over::<ProjectDrag>(move |style, drag, _, _| {
                if drag.favorite == favorite {
                    style.bg(p.raised).border_b_1().border_color(p.accent)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(move |this, drag: &ProjectDrag, _, cx| {
                this.move_sidebar_project(drag.owner, parent, cx)
            }))
            .context_menu(move |menu, _, cx| {
                if let Some(view) = owner.upgrade() {
                    view.read(cx).project_menu(parent, menu, owner.clone())
                } else {
                    menu
                }
            })
            .into_any_element()
    }

    fn project_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p();
        let pinned = self.state.sidebar_expanded;
        let expanded = pinned || self.navigation.sidebar.peek;
        let projects = self.state.project_rows();
        let height = projects.len() as f32 * 40.0 + if expanded { 40.0 } else { 32.0 };
        let search = div()
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                Button::new("search-projects")
                    .custom(icon_style(p, cx))
                    .compact()
                    .w_full()
                    // Separate hitboxes keep pinning from also invoking project search.
                    .when(expanded, |button| button.w_auto().flex_1().min_w_0())
                    .h(px(32.0))
                    .debug_selector(|| "project-search".into())
                    .px_0()
                    .accessibility_label(self.t("project_quick_switch"))
                    .tooltip(self.t("project_quick_switch"))
                    .child(if expanded {
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .text_color(p.secondary)
                            .child(
                                div()
                                    .w(px(40.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(IconName::Search, p.secondary).size(px(14.0))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.t("projects")),
                            )
                            .child(
                                div()
                                    .ml(px(6.0))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::NORMAL)
                                    .child(projects.len().to_string()),
                            )
                    } else {
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(IconName::Search, p.secondary))
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_project_switcher(false, window, cx)
                    })),
            )
            .when(expanded, |row| {
                let label = self.t(if pinned {
                    "sidebar_unpin"
                } else {
                    "sidebar_pin"
                });
                row.child(
                    Button::new("pin-project-sidebar")
                        .custom(icon_style(p, cx))
                        .compact()
                        .mr(px(4.0))
                        .size(px(24.0))
                        .rounded(px(6.0))
                        .bg(if pinned {
                            p.selected
                        } else {
                            p.sidebar.opacity(0.0)
                        })
                        .icon(
                            icon(IconName::Pin, if pinned { p.accent } else { p.secondary })
                                .size(px(13.0)),
                        )
                        .accessibility_label(label)
                        .tooltip(label)
                        .debug_selector(|| "pin-project-sidebar".into())
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_project_sidebar(cx))),
                )
            });
        div()
            .id("project-navigation")
            .debug_selector(|| "project-navigation".into())
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(height))
            .max_h_full()
            .flex()
            .flex_col()
            .when(expanded, |panel| {
                // Expand only the project area, below module navigation; global tools stay in the rail.
                panel
                    .left(px(-8.0))
                    .w(px(232.0))
                    .px(px(8.0))
                    .pb(px(8.0))
                    .rounded(px(12.0))
                    .bg(if self.appearance.light_theme {
                        p.background
                    } else {
                        p.toolbar
                    })
                    .shadow(vec![BoxShadow {
                        color: black().opacity(if self.appearance.light_theme {
                            0.10
                        } else {
                            0.30
                        }),
                        offset: point(px(0.0), px(6.0)),
                        blur_radius: px(20.0),
                        spread_radius: px(-8.0),
                        inset: false,
                    }])
            })
            .occlude()
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|this, hovered, _, cx| this.hover_project_sidebar(*hovered, cx)))
            .child(search)
            .child(
                div().flex_1().min_h_0().child(
                    uniform_list(
                        "project-list",
                        projects.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.project_row(projects[index], expanded, cx))
                                .collect()
                        }),
                    )
                    .size_full()
                    .scrollbar(&self.navigation.sidebar.scroll),
                ),
            )
    }

    fn rail_button(
        &self,
        id: &'static str,
        glyph: IconName,
        label: &'static str,
        height: f32,
    ) -> Button {
        Button::new(id)
            .ghost()
            .compact()
            .h(px(height))
            .w_full()
            .icon(icon(glyph, self.p().secondary))
            .debug_selector(move || id.into())
            .accessibility_label(self.t(label))
            .tooltip(self.t(label))
    }

    pub(in crate::ui) fn sidebar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p();
        let local = self.state.screen == tervia::state::Screen::Local;
        let attention = self
            .local_terminals
            .iter()
            .flatten()
            .any(|console| console.read(cx).needs_attention());
        let content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(8.0))
            .py(px(12.0))
            .child(
                Button::new("local-terminal-entry")
                    .ghost()
                    .compact()
                    .w_full()
                    .h(px(36.0))
                    .rounded(px(7.0))
                    .bg(if local {
                        p.selected
                    } else {
                        p.sidebar.opacity(0.0)
                    })
                    .icon(icon(
                        IconName::SquareTerminal,
                        if local { p.accent } else { p.secondary },
                    ))
                    .debug_selector(|| "local-terminal-entry".into())
                    .accessibility_label(self.t("local_terminal"))
                    .tooltip(self.t("local_terminal"))
                    .when(attention, |button| {
                        button.child(icon(IconName::Bell, p.accent).size(px(11.0)))
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_local_terminal(window, cx)),
                    ),
            )
            .child(
                div()
                    .h(px(1.0))
                    .flex_shrink_0()
                    .bg(p.chrome_border())
                    .mx(px(8.0))
                    .my(px(16.0)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.project_panel(cx)),
            );
        let tools = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(8.0))
            .pb(px(12.0))
            .child(
                self.rail_button("open-welcome", IconName::House, "welcome_home", 36.0)
                    .on_click(cx.listener(|this, _, window, cx| this.open_welcome(window, cx))),
            )
            .child(
                self.rail_button("add-project", IconName::Plus, "git_open_repository", 36.0)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.choose_repository(window, cx)),
                    ),
            )
            .child(
                self.rail_button(
                    "recent-projects",
                    IconName::RotateCcw,
                    "project_recent_closed",
                    32.0,
                )
                .on_click(
                    cx.listener(|this, _, window, cx| this.show_project_switcher(true, window, cx)),
                ),
            )
            .child(
                self.rail_button("settings", IconName::Settings, "settings", 36.0)
                    .tooltip(self.t("appearance_shortcuts"))
                    .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
            );
        div()
            .id("project-sidebar")
            .absolute()
            .top_0()
            .left_0()
            // The working surface always starts after this rail, including when a project panel is pinned.
            .w(px(56.0))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.chrome_border().opacity(0.0))
            .occlude()
            // Paint the rail seam before its floating child so it cannot cut through the panel.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(1.0))
                    .bg(p.chrome_border()),
            )
            .child(content)
            .child(tools)
    }
}
