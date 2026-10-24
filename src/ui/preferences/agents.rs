use super::*;
use gpui_kit::base::Disableable;
use std::sync::Arc;
use tervia::agent::Agent;

pub(super) type Action = Arc<dyn Fn(Agent, bool) -> Result<PathBuf, String> + Send + Sync>;

impl PreferencesWindow {
    pub(super) fn configure_agent(&mut self, agent: Agent, enabled: bool, cx: &mut Context<Self>) {
        if self.agent_busy.is_some() {
            return;
        }
        self.agent_busy = Some(agent);
        self.agent_result = None;
        let action = self.agent_action.clone();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action(agent, enabled) })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.agent_busy = None;
                this.agent_result = Some(result.map(|path| (agent, enabled, path)));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn agent_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(&self.current.theme);
        let mut group = div()
            .flex()
            .flex_col()
            .child(self.heading("settings_agents"))
            .child(
                self.row(
                    "agent_notifications",
                    "agent-notifications-control",
                    div().flex().items_center().h_full().child(
                        Switch::new("agent-notifications")
                            .checked(self.current.agent_notifications)
                            .accessibility_label(self.t("agent_notifications"))
                            .on_change(cx.listener(|this, enabled, _, cx| {
                                this.preferences.update(cx, |prefs, cx| {
                                    prefs.agent_notifications = *enabled;
                                    cx.notify();
                                });
                            })),
                    ),
                ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t("agent_notifications_hint")),
            );
        for (index, (name, agent)) in [("Claude Code", Agent::Claude), ("Codex", Agent::Codex)]
            .into_iter()
            .enumerate()
        {
            group = group.child(
                self.row(
                    if agent == Agent::Claude {
                        "agent_claude"
                    } else {
                        "agent_codex"
                    },
                    if agent == Agent::Claude {
                        "agent-claude-controls"
                    } else {
                        "agent-codex-controls"
                    },
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(
                            label_button(("agent-connect", index), self.t("agent_connect"), p, cx)
                                .disabled(self.agent_busy.is_some())
                                .accessibility_label(format!("{name} {}", self.t("agent_connect")))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.configure_agent(agent, true, cx)
                                })),
                        )
                        .child(
                            label_button(("agent-remove", index), self.t("agent_remove"), p, cx)
                                .disabled(self.agent_busy.is_some())
                                .accessibility_label(format!("{name} {}", self.t("agent_remove")))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.configure_agent(agent, false, cx)
                                })),
                        ),
                ),
            );
        }
        group = group.child(
            div()
                .text_size(px(11.0))
                .text_color(p.secondary)
                .child(self.t(if self.agent_busy.is_some() {
                    "agent_busy"
                } else {
                    "agent_integration_hint"
                })),
        );
        if let Some(result) = &self.agent_result {
            let (message, error) = match result {
                Ok((agent, enabled, path)) => (
                    format!(
                        "{}: {}\n{}",
                        agent.name(),
                        self.t(if *enabled {
                            "agent_connect_done"
                        } else {
                            "agent_remove_done"
                        }),
                        path.display()
                    ),
                    false,
                ),
                Err(error) => (format!("{} {error}", self.t("agent_error")), true),
            };
            group = group.child(
                div()
                    .debug_selector(|| "agent-integration-result".into())
                    .mt(px(8.0))
                    .text_size(px(11.0))
                    .text_color(if error { p.red } else { p.secondary })
                    .child(message),
            );
        }
        group
    }
}
