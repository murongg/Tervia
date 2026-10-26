use super::{Tervia, console};
use gpui_kit::*;
use tervia::state::Screen;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Local,
    Project(usize),
}

pub(super) fn discard_exited(state: &mut tervia::state::Workspace, root: &std::path::Path) -> bool {
    let mut changed = false;
    for terminal in state
        .projects
        .iter_mut()
        .map(|project| &mut project.terminal)
        .chain(std::iter::once(&mut state.local_terminal))
    {
        changed |= terminal.retain(|session| {
            session.connection.as_ref().is_none_or(|id| {
                let target = tervia::sessions::Target::new(
                    root.to_owned(),
                    id.clone(),
                    std::path::PathBuf::new(),
                );
                // Completed tasks keep their output and explicit rerun action across UI restarts.
                (session.task.is_some() && target.directory().join("finished.json").is_file())
                    || !target.has_exited()
            })
        });
    }
    changed
}

impl Tervia {
    pub(super) fn focus_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.terminals().sessions.is_empty() {
            self.focus.focus(window, cx);
        } else {
            self.ensure_console(self.state.terminals().active_session, window, cx)
                .read(cx)
                .focus
                .clone()
                .focus(window, cx);
        }
    }

    pub(super) fn console_owner(&self) -> Owner {
        if self.state.has_project() {
            Owner::Project(self.state.active_project)
        } else {
            Owner::Local
        }
    }

    fn consoles_for(&self, owner: Owner) -> &[Option<Entity<console::Console>>] {
        match owner {
            Owner::Local => &self.local_terminals,
            Owner::Project(project) => &self.terminals[project],
        }
    }

    pub(super) fn consoles(&self) -> &[Option<Entity<console::Console>>] {
        self.consoles_for(self.console_owner())
    }

    fn consoles_mut(&mut self) -> &mut Vec<Option<Entity<console::Console>>> {
        match self.console_owner() {
            Owner::Local => &mut self.local_terminals,
            Owner::Project(project) => &mut self.terminals[project],
        }
    }

    pub(super) fn ensure_console(
        &mut self,
        session: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<console::Console> {
        self.ensure_console_with_shell(session, self.preferences.read(cx).shell.clone(), window, cx)
    }

    fn ensure_console_with_shell(
        &mut self,
        session: usize,
        profile: tervia::shell::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<console::Console> {
        if let Some(console) = self.consoles().get(session).and_then(Option::as_ref) {
            return console.clone();
        }
        let owner = self.console_owner();
        let remote = self.state.terminals().sessions[session].ssh.is_some();
        let home = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" }));
        let cwd = match owner {
            Owner::Local => self
                .launch_directory
                .as_deref()
                .map(|directory| tervia::pty::initial_directory(directory, &home))
                .unwrap_or(home),
            Owner::Project(project) if self.state.terminals().sessions[session].task.is_some() => {
                // A missing task directory must fail at spawn, never fall back to running in the user's home.
                self.state.projects[project].root.clone()
            }
            Owner::Project(project) => {
                tervia::pty::initial_directory(&self.state.projects[project].root, &home)
            }
        };
        let preferences = self.preferences.clone();
        let history = self.command_history.clone();
        let existing = self.state.terminals().sessions[session]
            .connection
            .is_some();
        let offset = self.state.terminals().sessions[session].scroll_offset;
        let target = (|| -> Result<_, String> {
            let id = match self.state.terminals().sessions[session].connection.clone() {
                Some(id) => id,
                None => tervia::sessions::Target::identifier()?,
            };
            let mut target = self
                .session_target(id.clone())?
                .with_shell(profile.resolve());
            if let Some(arguments) = self.preferences.read(cx).shell_args_for(&profile) {
                target = target.with_shell_args(arguments.to_vec());
            }
            if let Some(task) = &self.state.terminals().sessions[session].task {
                target = target.with_task(task.clone());
            }
            if let Some(host) = &self.state.terminals().sessions[session].ssh {
                target = target.with_ssh(host.clone());
            }
            self.state.terminals_mut().sessions[session].connection = Some(id);
            // Record ownership before starting a host; a fast exit/update must not orphan a live shell.
            if let Some(store) = &mut self.workspace {
                store.save(&self.state)?;
            }
            Ok((target, existing))
        })();
        let console = cx
            .new(|cx| console::Console::new(cwd, preferences, history, target, offset, window, cx));
        self.subscriptions.push(cx.subscribe_in(
            &console,
            window,
            |this, console, event: &console::ActivityChanged, window, cx| {
                if let Some(snapshot) = &event.0 {
                    this.agent_notification(console, snapshot, window, cx);
                }
                cx.notify();
            },
        ));
        self.subscriptions.push(
            cx.subscribe(&console, |_, _, _: &console::LifecycleChanged, cx| {
                cx.notify()
            }),
        );
        self.subscriptions.push(
            cx.subscribe(&console, |_, _, _: &console::AttentionChanged, cx| {
                cx.notify()
            }),
        );
        self.subscriptions.push(cx.subscribe_in(
            &console,
            window,
            move |this, _, link: &tervia::terminal::FileLink, window, cx| {
                // A remote path may coincidentally name a local file; never open it against the local project.
                if remote || this.console_owner() != owner || this.state.screen == Screen::Welcome {
                    return;
                }
                match owner {
                    Owner::Project(project) => {
                        this.open_terminal_link(project, link.clone(), window, cx)
                    }
                    Owner::Local => {
                        this.state.notice = "local_terminal_files_hint".into();
                        cx.notify();
                    }
                }
            },
        ));
        self.subscriptions.push(cx.subscribe(
            &console,
            move |this, _, _: &console::IdentityChanged, cx| {
                if this.console_owner() == owner {
                    cx.notify();
                }
            },
        ));
        self.subscriptions.push(cx.subscribe(
            &console,
            move |this, console, _: &console::Focused, cx| {
                cx.dismiss_system_notification(&super::agents::tag(&console));
                // Both owners can have session zero; route delayed events by owner and entity identity.
                if this.console_owner() == owner
                    && this.state.screen != Screen::Welcome
                    && let Some(index) = this.consoles_for(owner).iter().position(|entry| {
                        entry
                            .as_ref()
                            .is_some_and(|entry| entry.entity_id() == console.entity_id())
                    })
                {
                    this.state.focus_session(index);
                    cx.notify();
                }
            },
        ));
        let count = self.state.terminals().sessions.len();
        self.consoles_mut().resize_with(count, || None);
        self.consoles_mut()[session] = Some(console.clone());
        if session == self.state.terminals().active_session {
            console.read(cx).focus.clone().focus(window, cx);
        }
        console
    }

    pub(super) fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_session_with_shell(self.preferences.read(cx).shell.clone(), window, cx);
    }

    pub(super) fn new_session_with_shell(
        &mut self,
        profile: tervia::shell::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_rename = None;
        if self.state.screen == Screen::Welcome {
            self.open_local_terminal(window, cx);
            return;
        }
        self.state.new_session();
        let count = self.state.terminals().sessions.len();
        self.consoles_mut().resize_with(count, || None);
        let session = self.state.terminals().active_session;
        self.ensure_console_with_shell(session, profile, window, cx)
            .read(cx)
            .focus
            .clone()
            .focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_terminal_session(
        &mut self,
        session: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if session >= self.state.terminals().sessions.len() {
            return;
        }
        // Session indices shift on removal; an outstanding rename must never target its neighbor.
        self.terminal_rename = None;
        let connection = self.state.terminals().sessions[session].connection.clone();
        self.capture_terminal_positions(cx);
        let previous = self.state.terminals().clone();
        self.state.close_session(session);
        // Forget the tab durably before destroying its process or view, including on an immediate crash.
        if let Some(store) = &mut self.workspace
            && let Err(error) = store.save(&self.state)
        {
            *self.state.terminals_mut() = previous;
            self.state.notice = format!("{}\n{error}", self.t("workspace_error"));
            cx.notify();
            return;
        }
        if let Some(console) = self.consoles_mut().remove(session) {
            cx.dismiss_system_notification(&super::agents::tag(&console));
            if let Err(error) = console.update(cx, |console, _| console.shutdown()) {
                self.state.notice = format!("{}: {error}", self.t("connection_error"));
            }
        } else if let Some(id) = connection {
            self.end_session(id, cx);
        }
        if self.state.active_tab() == tervia::state::Tab::Ssh {
            self.focus.focus(window, cx);
        } else {
            self.focus_terminal(window, cx);
        }
        cx.notify();
    }

    pub(super) fn session_target(&self, id: String) -> Result<tervia::sessions::Target, String> {
        let config = self
            .config
            .as_ref()
            .ok_or("Terminal recovery requires a valid configuration location")?;
        Ok(tervia::sessions::Target::new(
            config.path().with_file_name("sessions"),
            id,
            std::env::current_exe().map_err(|error| error.to_string())?,
        ))
    }

    pub(super) fn end_session(&self, id: String, cx: &mut Context<Self>) {
        if let Ok(target) = self.session_target(id) {
            cx.spawn(async move |weak, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { target.terminate() })
                    .await;
                if let Err(error) = result {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.notice = format!("{}: {error}", this.t("connection_error"));
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    }

    pub(super) fn capture_terminal_positions(&mut self, cx: &App) {
        for (sessions, consoles) in self
            .state
            .projects
            .iter_mut()
            .map(|project| &mut project.terminal.sessions)
            .zip(&self.terminals)
            .chain(std::iter::once((
                &mut self.state.local_terminal.sessions,
                &self.local_terminals,
            )))
        {
            for (session, console) in sessions.iter_mut().zip(consoles) {
                if let Some(console) = console {
                    session.scroll_offset = console.read(cx).scroll_offset();
                }
            }
        }
    }

    pub(super) fn flush_terminals(&mut self, cx: &mut Context<Self>) -> bool {
        for console in self
            .terminals
            .iter()
            .flatten()
            .chain(&self.local_terminals)
            .flatten()
        {
            if let Err(error) = console.read(cx).flush() {
                self.state.notice = format!("{}: {error}", self.t("input_failed"));
                cx.notify();
                return false;
            }
        }
        true
    }

    fn leave_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tasks.cancel();
        self.terminal_rename = None;
        if self.navigation.switcher.open {
            self.close_project_switcher(window, cx);
        }
        self.navigation.cancel();
        if self.state.has_project() {
            self.file_reads.select(self.state.active_project);
        }
        for ui in &mut self.git_states {
            ui.close_branch_picker();
            ui.cancel_worktree_navigation();
        }
        self.state.palette_open = false;
        self.welcome_cloning = false;
    }

    pub(super) fn open_local_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture_reading_positions(cx);
        self.leave_screen(window, cx);
        self.state.show_local_terminal();
        let count = self.state.local_terminal.sessions.len();
        self.local_terminals.resize_with(count, || None);
        self.ensure_console(self.state.local_terminal.active_session, window, cx)
            .read(cx)
            .focus
            .clone()
            .focus(window, cx);
        self.persist_workspace(cx);
        cx.notify();
    }

    pub(super) fn new_local_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing = !self.state.local_terminal.sessions.is_empty();
        self.open_local_terminal(window, cx);
        if existing {
            self.new_session(window, cx);
        }
    }

    pub(super) fn open_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture_reading_positions(cx);
        self.leave_screen(window, cx);
        self.state.show_welcome();
        self.persist_workspace(cx);
        self.focus.focus(window, cx);
        cx.notify();
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests;
