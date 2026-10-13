pub(crate) mod console;
mod controls;
mod explorer;
mod files;
mod git;
mod monitor;
mod preferences;
mod shell;
mod tabs;
mod terminal;
mod theme;
mod welcome;

use std::path::PathBuf;

use gpui_kit::base::actions::Cancel;
use gpui_kit::component::input::{EditorState, Escape, InputEvent, InputState, TabSize};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::config::Config;
use tervia::locale::Language;
use tervia::platform::Platform;
use tervia::preferences::Preferences;
use tervia::state::{Tab, Workspace};

use theme::Palette;

pub(crate) fn window_title(title: &str) -> SharedString {
    title.to_owned().into()
}

pub(crate) fn menus(language: Language) -> Vec<Menu> {
    vec![Menu {
        name: "Tervia".into(),
        items: vec![
            MenuItem::action(format!("{}…", language.text("settings")), OpenPreferences),
            MenuItem::separator(),
            MenuItem::action(format!("{} Tervia", language.text("quit")), Quit),
        ],
        disabled: false,
    }]
}

gpui_kit::actions!(
    tervia,
    [
        Quit,
        ToggleSidebar,
        TerminalTab,
        GitTab,
        FilesTab,
        SaveFile,
        NewSession,
        TogglePalette,
        OpenPreferences,
        CloseFileTab,
        NextFileTab,
        PreviousFileTab,
        ReopenFileTab
    ]
);

pub struct Tervia {
    pub focus: FocusHandle,
    state: Workspace,
    preferences: Entity<Preferences>,
    appearance: Preferences,
    config: Option<Config>,
    config_error: Entity<Option<String>>,
    preferences_window: Option<AnyWindowHandle>,
    editors: Vec<Vec<Entity<EditorState>>>,
    explorers: Vec<explorer::Explorer>,
    monitors: Vec<monitor::Monitor>,
    terminals: Vec<Vec<Option<Entity<console::Console>>>>,
    git_states: Vec<git::GitUi>,
    git_scans: git::Scans,
    git_mutating: bool,
    saving_file: bool,
    file_closing: bool,
    file_reads: tervia::files::Reads,
    welcome_url: Entity<InputState>,
    git_poll: Option<Task<()>>,
    file_poll: Option<Task<()>>,
    files_active: bool,
    subscriptions: Vec<Subscription>,
}

impl Tervia {
    fn t(&self, key: &'static str) -> &'static str {
        self.appearance.language.text(key)
    }

    fn f(&self, key: &str, arguments: &[(&str, &str)]) -> String {
        self.appearance.language.format(key, arguments)
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(Preferences::default(), None, None, window, cx)
    }

    pub fn configured(
        config: Result<Config, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (appearance, config, error) = match config {
            Ok(mut config) => {
                let result = config.load();
                match result {
                    Ok(preferences) => (preferences, Some(config), None),
                    Err(error) => (Preferences::default(), Some(config), Some(error)),
                }
            }
            Err(error) => (Preferences::default(), None, Some(error)),
        };
        Self::build(appearance, config, error, window, cx)
    }

    fn build(
        appearance: Preferences,
        config: Option<Config>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        preferences::apply(&appearance, cx);
        let language = appearance.language;
        let mut state = Workspace::empty();
        if let Some(error) = &error {
            state.notice = format!("{}\n{error}", language.text("config_error"));
        }
        let mut view = Self {
            focus: cx.focus_handle(),
            state,
            preferences: cx.new(|_| appearance.clone()),
            appearance,
            config,
            config_error: cx.new(|_| error),
            preferences_window: None,
            editors: Vec::new(),
            explorers: Vec::new(),
            monitors: Vec::new(),
            terminals: Vec::new(),
            git_states: Vec::new(),
            git_scans: git::Scans::default(),
            git_mutating: false,
            saving_file: false,
            file_closing: false,
            file_reads: tervia::files::Reads::default(),
            welcome_url: cx
                .new(|cx| InputState::new(window, cx).placeholder(language.text("git_clone_url"))),
            git_poll: None,
            file_poll: None,
            files_active: window.is_window_active(),
            subscriptions: Vec::new(),
        };
        view.subscriptions
            .push(cx.subscribe(&view.welcome_url, |_, _, _: &InputEvent, cx| cx.notify()));
        view.subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                this.monitor_activation(window.is_window_active(), window, cx);
            }));
        view.subscriptions.push(cx.observe_in(
            &view.preferences,
            window,
            |this, preferences, _window, cx| {
                this.appearance = preferences.read(cx).clone();
                preferences::apply(&this.appearance, cx);
                if let Some(config) = &mut this.config {
                    let error = config.save(&this.appearance).err();
                    if let Some(error) = &error {
                        this.state.notice = format!("{}\n{error}", this.t("config_error"));
                    }
                    this.config_error.update(cx, |current, cx| {
                        *current = error;
                        cx.notify();
                    });
                }
                cx.notify();
            },
        ));
        for index in 0..view.state.projects.len() {
            view.init_project(index, window, cx);
        }
        view.git_poll = Some(cx.spawn_in(window, async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                if weak
                    .update_in(cx, |this, window, cx| {
                        if this.files_active
                            && !this.state.projects.is_empty()
                            && this.state.project().tab == Tab::Git
                        {
                            this.refresh_git(this.state.active_project, window, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        view.file_poll = Some(cx.spawn_in(window, async move |weak, cx| {
            let mut tick = 0;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                tick += 1;
                if weak
                    .update_in(cx, |this, window, cx| {
                        this.poll_files(tick % 4 == 0, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        view.focus.focus(window, cx);
        view
    }

    fn init_project(&mut self, project_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.state.projects[project_index].clone();
        let mut editors = Vec::new();
        for (file_index, file) in project.files.iter().enumerate() {
            let editor = cx.new(|cx| {
                EditorState::new(window, cx)
                    .language(file.language.clone())
                    .line_number(true)
                    .folding(true)
                    .auto_close(true)
                    .smart_indent(true)
                    .tab_size(TabSize {
                        tab_size: 4,
                        hard_tabs: false,
                    })
                    .default_value(file.draft.clone())
            });
            // Capture the owner: a late input event must never edit the newly selected project.
            self.subscriptions.push(cx.subscribe_in(
                &editor,
                window,
                move |this, editor, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.state.projects[project_index].files[file_index].draft =
                            editor.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ));
            editors.push(editor);
        }
        self.editors.push(editors);
        self.explorers.push(explorer::Explorer::default());
        self.monitors.push(monitor::Monitor::default());
        self.terminals.push(vec![None; project.sessions.len()]);
        let git = git::GitUi::new(window, cx);
        git.subscribe(&mut self.subscriptions, cx);
        self.git_states.push(git);
    }

    fn ensure_console(
        &mut self,
        session: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<console::Console> {
        let project = self.state.active_project;
        if let Some(console) = &self.terminals[project][session] {
            return console.clone();
        }
        let project_path = self.state.projects[project].root.as_path();
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let cwd = tervia::pty::initial_directory(project_path, &home);
        let preferences = self.preferences.clone();
        let console = cx.new(|cx| console::Console::new(cwd, preferences, window, cx));
        self.subscriptions.push(cx.subscribe(
            &console,
            move |this, _, _: &console::IdentityChanged, cx| {
                if this.state.active_project == project {
                    cx.notify();
                }
            },
        ));
        self.subscriptions.push(cx.subscribe(
            &console,
            move |this, console, _: &console::Focused, cx| {
                if this.state.active_project == project
                    && let Some(index) = this.terminals[project].iter().position(|entry| {
                        entry
                            .as_ref()
                            .is_some_and(|entry| entry.entity_id() == console.entity_id())
                    })
                {
                    // Focus routes toolbar actions without changing the fixed split-pane order.
                    this.state.focus_session(index);
                    cx.notify();
                }
            },
        ));
        self.terminals[project][session] = Some(console.clone());
        // Register ownership before focus callbacks can route toolbar actions to this entity.
        if session == self.state.project().active_session {
            console.read(cx).focus.clone().focus(window, cx);
        }
        console
    }

    pub(crate) fn shutdown_terminals(&mut self, cx: &mut Context<Self>) {
        for console in self.terminals.iter().flatten().flatten() {
            console.update(cx, |console, _| console.shutdown());
        }
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.new_session();
        let project = self.state.active_project;
        self.terminals[project].push(None);
        let session = self.state.project().active_session;
        self.ensure_console(session, window, cx)
            .read(cx)
            .focus
            .clone()
            .focus(window, cx);
        cx.notify();
    }

    fn change_project(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.project_visible(index)
            || self.worktree_removing(&self.state.projects[index].root)
        {
            return;
        }
        for ui in &mut self.git_states {
            ui.cancel_worktree_navigation();
        }
        self.state.switch_project(index);
        self.refresh_git(index, window, cx);
        if self.state.project().tab == Tab::Files {
            self.load_directory(index, PathBuf::new(), false, window, cx);
        }
        if self.state.project().tab == Tab::Terminal {
            self.ensure_console(self.state.project().active_session, window, cx)
                .read(cx)
                .focus
                .clone()
                .focus(window, cx);
        }
        cx.notify();
    }

    fn change_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        for ui in &mut self.git_states {
            ui.cancel_worktree_navigation();
        }
        if self.state.project().tab != tab {
            // Loading a file cannot override a module the user selected afterwards.
            self.file_reads.select(self.state.active_project);
        }
        self.state.set_tab(tab);
        if tab == Tab::Git {
            self.refresh_git(self.state.active_project, window, cx);
        }
        if tab == Tab::Files {
            self.load_directory(self.state.active_project, PathBuf::new(), false, window, cx);
        }
        self.focus.focus(window, cx);
        if tab == Tab::Terminal {
            self.ensure_console(self.state.project().active_session, window, cx)
                .read(cx)
                .focus
                .clone()
                .focus(window, cx);
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_real_file(window, cx);
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.dismiss_palette() {
            cx.notify();
        } else {
            window.remove_window();
        }
    }

    fn toggle_theme(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.update(cx, |prefs, cx| {
            prefs.light_theme = !prefs.light_theme;
            cx.notify();
        });
    }

    pub(crate) fn open_preferences(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.preferences_window
            && cx
                .update_window(handle, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let preferences = self.preferences.clone();
        let config_path = self.config.as_ref().map(|config| config.path().to_owned());
        let config_error = self.config_error.clone();
        let bounds = Bounds::centered(None, size(px(760.0), px(640.0)), cx);
        match gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(window_title(&format!("Tervia {}", self.t("settings")))),
                    appears_transparent: true,
                    traffic_light_position: (Platform::CURRENT == Platform::Mac)
                        .then(|| point(px(16.0), px(18.0))),
                }),
                window_min_size: Some(size(px(700.0), px(580.0))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    preferences::PreferencesWindow::new(
                        preferences,
                        config_path,
                        config_error,
                        window,
                        cx,
                    )
                })
            },
        ) {
            Ok((handle, _)) => {
                self.preferences_window = Some(handle);
            }
            Err(_) => {
                self.state.notice = "settings_open_failed".into();
                cx.notify();
            }
        }
    }

    fn p(&self) -> Palette {
        Palette::new(self.appearance.light_theme)
    }
}

impl Render for Tervia {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p();
        if self.state.projects.is_empty() {
            return self.welcome(cx).into_any_element();
        }
        div()
            .id("tervia-workspace")
            .relative()
            .key_context(if self.state.project().tab == Tab::Files {
                "Tervia FileTabs"
            } else {
                "Tervia"
            })
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::workspace_background(self.appearance.light_theme))
            .text_color(p.text)
            .font_family(Platform::CURRENT.ui_font())
            .text_size(px(self.appearance.ui_size as f32))
            // Inputs and menus use different Escape actions; both bubble only after
            // their own transient UI has been dismissed.
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.state.toggle_sidebar();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TerminalTab, window, cx| {
                this.change_tab(Tab::Terminal, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &GitTab, window, cx| this.change_tab(Tab::Git, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FilesTab, window, cx| {
                    this.change_tab(Tab::Files, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SaveFile, window, cx| this.save(window, cx)))
            .on_action(cx.listener(|this, _: &CloseFileTab, window, cx| {
                this.close_active_file_tab(window, cx)
            }))
            .on_action(cx.listener(|this, _: &NextFileTab, window, cx| {
                this.cycle_file_tab(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PreviousFileTab, window, cx| {
                this.cycle_file_tab(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ReopenFileTab, window, cx| {
                this.reopen_file_tab(this.state.active_project, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session(window, cx)))
            .on_action(cx.listener(|this, _: &TogglePalette, window, cx| {
                this.state.palette_open = !this.state.palette_open;
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenPreferences, _, cx| this.open_preferences(cx)))
            .child(self.titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(self.navigation(cx))
                            .child(match self.state.project().tab {
                                Tab::Terminal => self.terminal(window, cx).into_any_element(),
                                Tab::Git => self.git(window, cx).into_any_element(),
                                Tab::Files => self.files(window, cx).into_any_element(),
                            }),
                    ),
            )
            .child(self.statusbar(cx))
            .when(self.state.palette_open, |this| this.child(self.palette(cx)))
            .into_any_element()
    }
}
