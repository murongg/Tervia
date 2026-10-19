use super::{Tervia, console, sessions::Owner};
use gpui_kit::*;
use tervia::{agent::activity::Snapshot, state::Tab};

pub(super) fn tag(console: &Entity<console::Console>) -> String {
    format!("tervia-agent-{}", console.entity_id().as_u64())
}

impl Tervia {
    pub(super) fn subscribe_agent_notifications(&self, window: &Window, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let handle = window.window_handle();
        cx.on_system_notification_response(move |response, cx| {
            if response.action_id.as_deref().is_some_and(|id| id != "open") {
                return;
            }
            let _ = handle.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.open_agent_notification(&response.tag, window, cx)
                });
            });
        });
    }

    fn locate_agent_console(&self, notification: &str) -> Option<(Owner, usize)> {
        self.local_terminals
            .iter()
            .enumerate()
            .find_map(|(index, console)| {
                console
                    .as_ref()
                    .filter(|console| tag(console) == notification)
                    .map(|_| (Owner::Local, index))
            })
            .or_else(|| {
                self.terminals
                    .iter()
                    .enumerate()
                    .filter(|(project, _)| self.state.project_visible(*project))
                    .find_map(|(project, consoles)| {
                        consoles.iter().enumerate().find_map(|(index, console)| {
                            console
                                .as_ref()
                                .filter(|console| tag(console) == notification)
                                .map(|_| (Owner::Project(project), index))
                        })
                    })
            })
    }

    pub(super) fn agent_notification(
        &self,
        console: &Entity<console::Console>,
        snapshot: &Snapshot,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if window.is_window_active() || !self.preferences.read(cx).agent_notifications {
            return;
        }
        // Settings is another Tervia window. On macOS the platform's main-window handle
        // can remain set while the app is in the background, so check actual activation.
        let current = window.window_handle();
        if cx
            .windows()
            .into_iter()
            .filter(|handle| *handle != current)
            .any(|handle| {
                handle
                    .update(cx, |_, window, _| window.is_window_active())
                    .unwrap_or(false)
            })
        {
            return;
        }
        let notification = tag(console);
        let Some((owner, index)) = self.locate_agent_console(&notification) else {
            return;
        };
        let (project, session) = match owner {
            Owner::Local => (
                self.t("local_terminal").to_owned(),
                self.state.local_terminal.sessions.get(index),
            ),
            Owner::Project(project) => (
                self.state.projects[project].name.clone(),
                self.state.projects[project].terminal.sessions.get(index),
            ),
        };
        let Some(session) = session else {
            return;
        };
        let session_name = session.custom_name.as_ref().unwrap_or(&session.name);
        cx.show_system_notification(SystemNotification {
            tag: notification.into(),
            title: format!(
                "{} · {}",
                snapshot.agent.name(),
                self.t(snapshot.phase.key())
            )
            .into(),
            body: format!("{project} · {session_name}").into(),
            actions: vec![SystemNotificationAction {
                id: "open".into(),
                label: self.t("agent_open_terminal").into(),
            }],
        });
    }

    pub(super) fn open_agent_notification(
        &mut self,
        notification: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Resolve the live entity each time: indices can shift after tab closure, reordering or project removal.
        let Some((owner, index)) = self.locate_agent_console(notification) else {
            cx.dismiss_system_notification(notification);
            return;
        };
        match owner {
            Owner::Local => self.open_local_terminal(window, cx),
            Owner::Project(project) => {
                if self.worktree_removing(&self.state.projects[project].root) {
                    return;
                }
                self.change_project(project, window, cx);
                self.change_tab(Tab::Terminal, window, cx);
            }
        }
        self.state.select_session(index);
        self.ensure_console(index, window, cx)
            .read(cx)
            .focus
            .clone()
            .focus(window, cx);
        cx.dismiss_system_notification(notification);
        window.activate_window();
        cx.activate(true);
        self.persist_workspace(cx);
        cx.notify();
    }
}
