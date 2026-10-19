//! Native presentation of explicit agent lifecycle snapshots.
use super::*;
use tervia::agent::activity::{Phase, Snapshot};

impl Console {
    pub(in crate::ui) fn observe_session(
        &mut self,
        agent: Option<Agent>,
        snapshot: Option<Snapshot>,
        cx: &mut Context<Self>,
    ) {
        // Retire the observed process first so a newly read SessionStart cannot be discarded with it.
        self.observe_agent(agent, cx);
        self.observe_activity(snapshot, false, cx);
    }
    pub(in crate::ui) fn observe_agent(&mut self, agent: Option<Agent>, cx: &mut Context<Self>) {
        if self.agent == agent {
            return;
        }
        // A later instance of the same executable needs a fresh hook, not its predecessor's state.
        if self.agent.is_some_and(|previous| {
            self.activity
                .as_ref()
                .is_some_and(|snapshot| snapshot.agent == previous)
        }) {
            let snapshot = self.activity.as_ref().unwrap();
            self.retired_epoch = Some(snapshot.epoch.clone());
            self.agent_attention = false;
            cx.emit(AttentionChanged);
        }
        self.agent = agent;
        cx.emit(IdentityChanged);
        cx.notify();
    }
    pub(in crate::ui) fn needs_attention(&self) -> bool {
        self.attention || self.agent_attention
    }

    pub(in crate::ui) fn observe_activity(
        &mut self,
        snapshot: Option<Snapshot>,
        historical: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = snapshot else {
            return;
        };
        if self.activity.as_ref() == Some(&snapshot) {
            return;
        }
        // Startup epochs survive coalescing; the UI need not observe a transient Ready event.
        if snapshot.phase == Phase::Exited {
            self.retired_epoch = Some(snapshot.epoch.clone());
        }
        let alert = snapshot.phase.alerts()
            && self.activity_is_current(&snapshot)
            && !historical
            && !self.replaying
            && self
                .activity
                .as_ref()
                .is_none_or(|previous| !previous.same_alert(&snapshot));
        if matches!(
            snapshot.phase,
            Phase::Working | Phase::Ready | Phase::Exited
        ) {
            self.agent_attention = false;
        }
        if alert && !self.buffer.term.is_focused {
            self.agent_attention = true;
        }
        self.activity = Some(snapshot.clone());
        cx.emit(ActivityChanged(alert.then_some(snapshot)));
        cx.emit(AttentionChanged);
        cx.notify();
    }

    pub(in crate::ui) fn agent_status(&self) -> Option<(Agent, &'static str)> {
        if let Some(agent) = self.agent {
            if !matches!(agent, Agent::Claude | Agent::Codex) {
                return None;
            }
            return Some((
                agent,
                self.activity
                    .as_ref()
                    .filter(|snapshot| {
                        self.activity_is_current(snapshot) && snapshot.agent == agent
                    })
                    .map_or("agent_untracked", |snapshot| snapshot.phase.key()),
            ));
        }
        self.activity
            .as_ref()
            .filter(|snapshot| snapshot.phase == Phase::Exited)
            .map(|snapshot| (snapshot.agent, snapshot.phase.key()))
    }

    fn activity_is_current(&self, snapshot: &Snapshot) -> bool {
        self.retired_epoch.as_deref() != Some(snapshot.epoch.as_str())
    }

    pub(super) fn agent_status_bar(&self) -> Option<AnyElement> {
        let (agent, key) = self.agent_status()?;
        let p = Palette::new(self.appearance.light_theme);
        let color = match key {
            "agent_failed" => p.red,
            "agent_complete" => p.green,
            "agent_permission" | "agent_input" => p.accent,
            _ => p.secondary,
        };
        Some(
            div()
                .id("agent-status")
                .debug_selector(|| "agent-status".into())
                .role(gpui_kit::accesskit::Role::Status)
                .aria_label(format!(
                    "{} {}",
                    agent.name(),
                    self.appearance.language.text(key)
                ))
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(30.0))
                .px(px(14.0))
                .gap(px(8.0))
                .border_b_1()
                .border_color(p.chrome_border())
                .text_size(px(11.0))
                .child(div().text_color(p.secondary).child(agent.name()))
                .child(
                    div()
                        .text_color(color)
                        .child(self.appearance.language.text(key)),
                )
                .when(key == "agent_untracked", |row| {
                    row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_right()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(p.secondary)
                            .child(self.appearance.language.text("agent_setup_hint")),
                    )
                })
                .into_any_element(),
        )
    }
}
