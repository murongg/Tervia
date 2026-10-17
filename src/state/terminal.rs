use super::{Session, Tab, Workspace};

impl Session {
    pub fn valid_custom_name(name: &str) -> bool {
        name.len() <= 1024 && !name.chars().any(char::is_control)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Welcome,
    Local,
    Project,
}

#[derive(Clone, Debug)]
pub struct Terminals {
    pub sessions: Vec<Session>,
    pub active_session: usize,
    pub focused_session: usize,
    pub split_terminal: bool,
}

impl Default for Terminals {
    fn default() -> Self {
        Self {
            sessions: vec![Session {
                name: "shell".into(),
                custom_name: None,
            }],
            active_session: 0,
            focused_session: 0,
            split_terminal: false,
        }
    }
}

impl Terminals {
    pub fn rename(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        if !Session::valid_custom_name(name) {
            return false;
        }
        let Some(session) = self.sessions.get_mut(index) else {
            return false;
        };
        // Keep the automatic shell name so clearing an alias restores normal Agent detection.
        session.custom_name = (!name.is_empty()).then(|| name.to_owned());
        true
    }

    pub fn empty() -> Self {
        Self {
            sessions: Vec::new(),
            ..Self::default()
        }
    }

    fn add(&mut self) {
        let index = self.sessions.len();
        self.sessions.push(Session {
            name: if index == 0 {
                "shell".into()
            } else {
                format!("shell {}", index + 1)
            },
            custom_name: None,
        });
        self.active_session = index;
        self.focused_session = index;
    }

    fn close(&mut self, index: usize) {
        if index >= self.sessions.len() {
            return;
        }
        self.sessions.remove(index);
        let last = self.sessions.len().saturating_sub(1);
        self.active_session = self
            .active_session
            .saturating_sub(usize::from(index <= self.active_session))
            .min(last);
        self.focused_session = self
            .focused_session
            .saturating_sub(usize::from(index <= self.focused_session))
            .min(last);
        if self.sessions.len() < 2 {
            self.split_terminal = false;
        }
    }
}

impl Workspace {
    pub fn has_project(&self) -> bool {
        self.screen == Screen::Project && self.project_visible(self.active_project)
    }

    pub fn active_tab(&self) -> Tab {
        if self.has_project() {
            self.project().tab
        } else {
            Tab::Terminal
        }
    }

    pub fn show_local_terminal(&mut self) {
        self.screen = Screen::Local;
        if self.local_terminal.sessions.is_empty() {
            self.local_terminal.add();
        }
        self.notice.clear();
    }

    pub fn show_welcome(&mut self) {
        self.screen = Screen::Welcome;
        self.notice.clear();
        self.palette_open = false;
    }

    pub fn terminals(&self) -> &Terminals {
        if self.screen == Screen::Project {
            &self.project().terminal
        } else {
            &self.local_terminal
        }
    }

    pub fn terminals_mut(&mut self) -> &mut Terminals {
        if self.screen == Screen::Project {
            &mut self.project_mut().terminal
        } else {
            &mut self.local_terminal
        }
    }

    pub fn toggle_split(&mut self) {
        let terminal = self.terminals_mut();
        if terminal.sessions.len() > 1 {
            terminal.split_terminal = !terminal.split_terminal;
        }
    }

    pub fn new_session(&mut self) {
        if self.screen == Screen::Welcome {
            self.show_local_terminal();
        } else if self.terminals().sessions.len() >= 64 {
            self.notice = "terminal_limit".into();
        } else {
            self.terminals_mut().add();
        }
    }

    pub fn select_session(&mut self, index: usize) {
        let terminal = self.terminals_mut();
        if index < terminal.sessions.len() {
            terminal.active_session = index;
            terminal.focused_session = index;
        }
    }

    pub fn focus_session(&mut self, index: usize) {
        let terminal = self.terminals_mut();
        if index < terminal.sessions.len() {
            terminal.focused_session = index;
        }
    }

    pub fn command_session(&self) -> usize {
        self.terminals()
            .focused_session
            .min(self.terminals().sessions.len().saturating_sub(1))
    }

    pub fn close_session(&mut self, index: usize) {
        // Project docks retain their last surface; a local terminal may be explicitly left empty.
        if self.screen != Screen::Project || self.terminals().sessions.len() > 1 {
            self.terminals_mut().close(index);
        }
    }
}
