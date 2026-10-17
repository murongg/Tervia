use super::{Tervia, console};
use gpui_kit::*;
use tervia::state::Screen;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Local,
    Project(usize),
}

impl Tervia {
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
        if let Some(console) = self.consoles().get(session).and_then(Option::as_ref) {
            return console.clone();
        }
        let owner = self.console_owner();
        let home = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" }));
        let cwd = match owner {
            Owner::Local => home,
            Owner::Project(project) => {
                tervia::pty::initial_directory(&self.state.projects[project].root, &home)
            }
        };
        let preferences = self.preferences.clone();
        let history = self.command_history.clone();
        let console = cx.new(|cx| console::Console::new(cwd, preferences, history, window, cx));
        self.subscriptions.push(
            cx.subscribe(&console, |_, _, _: &console::AttentionChanged, cx| {
                cx.notify()
            }),
        );
        self.subscriptions.push(cx.subscribe_in(
            &console,
            window,
            move |this, _, link: &tervia::terminal::FileLink, window, cx| {
                if this.console_owner() != owner || this.state.screen == Screen::Welcome {
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
        self.terminal_rename = None;
        if self.state.screen == Screen::Welcome {
            self.open_local_terminal(window, cx);
            return;
        }
        self.state.new_session();
        let count = self.state.terminals().sessions.len();
        self.consoles_mut().resize_with(count, || None);
        let session = self.state.terminals().active_session;
        self.ensure_console(session, window, cx)
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
        if session >= self.state.terminals().sessions.len()
            || (self.state.has_project() && self.state.terminals().sessions.len() == 1)
        {
            return;
        }
        // Session indices shift on removal; an outstanding rename must never target its neighbor.
        self.terminal_rename = None;
        self.state.close_session(session);
        if let Some(console) = self.consoles_mut().remove(session) {
            console.update(cx, |console, _| console.shutdown());
        }
        if self.state.terminals().sessions.is_empty() {
            self.focus.focus(window, cx);
        } else {
            self.ensure_console(self.state.terminals().active_session, window, cx)
                .read(cx)
                .focus
                .clone()
                .focus(window, cx);
        }
        cx.notify();
    }

    fn leave_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
