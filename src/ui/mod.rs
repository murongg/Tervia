mod agents;
pub(crate) mod console;
mod context;
mod controls;
mod dock;
mod explorer;
mod files;
mod git;
mod icons;
mod menus;
mod monitor;
mod preferences;
mod projects;
mod recovery;
mod scroll;
mod sessions;
mod shell;
mod tabs;
mod tasks;
mod terminal;
mod theme;
mod updates;
mod welcome;

use std::path::PathBuf;

use gpui_kit::base::actions::Cancel;
use gpui_kit::component::input::{EditorState, Escape, InputEvent, InputState, TabSize};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::config::Config;
use tervia::platform::Platform;
use tervia::preferences::Preferences;
use tervia::state::{Screen, Tab, Workspace};
use tervia::workspace::Store;

pub(crate) use menus::bind_menu_keys;
use menus::menus;
use theme::Palette;

pub(crate) fn window_title(title: &str) -> SharedString {
    title.to_owned().into()
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
        NewLocalTerminal,
        CloseSession,
        Welcome,
        TogglePalette,
        OpenPreferences,
        OpenProject,
        OpenFile,
        ToggleTheme,
        SplitTerminal,
        ClearTerminal,
        CopyTerminalOutput,
        CloseFileTab,
        NextFileTab,
        PreviousFileTab,
        ReopenFileTab,
        QuickOpenFile,
        SearchProjectText,
        CopyCodeContext,
        ToggleTerminalDock,
        RunProjectTask
    ]
);

pub struct Tervia {
    pub focus: FocusHandle,
    state: Workspace,
    preferences: Entity<Preferences>,
    appearance: Preferences,
    config: Option<Config>,
    workspace: Option<Store>,
    workspace_save: Option<Task<()>>,
    workspace_revision: u64,
    config_error: Entity<Option<String>>,
    preferences_window: Option<AnyWindowHandle>,
    updater: Entity<updates::Updater>,
    update_restarting: bool,
    editors: Vec<Vec<Entity<EditorState>>>,
    explorers: Vec<explorer::Explorer>,
    navigation: projects::Navigation,
    monitors: Vec<monitor::Monitor>,
    terminals: Vec<Vec<Option<Entity<console::Console>>>>,
    local_terminals: Vec<Option<Entity<console::Console>>>,
    shell_profiles: Vec<tervia::shell::Profile>,
    terminal_name: Entity<InputState>,
    terminal_rename: Option<(sessions::Owner, usize)>,
    command_history: tervia::suggestions::Worker,
    tasks: tasks::Launcher,
    git_states: Vec<git::GitUi>,
    git_scans: git::Scans,
    git_mutating: bool,
    saving_file: bool,
    file_closing: bool,
    file_reads: tervia::files::Reads,
    welcome_url: Entity<InputState>,
    welcome_cloning: bool,
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
        Self::build(
            Preferences::default(),
            None,
            None,
            Workspace::empty(),
            None,
            window,
            cx,
        )
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
        let mut workspace = config
            .as_ref()
            .map(|config| Store::new(config.path().with_file_name("workspace.json")));
        let state = workspace.as_mut().map(Store::load).transpose();
        let mut state = match state {
            Ok(state) => state.unwrap_or_else(Workspace::empty),
            Err(error) => {
                let mut state = Workspace::empty();
                state.notice = format!("{}\n{error}", appearance.language.text("workspace_error"));
                state
            }
        };
        if let Some(config) = &config {
            let root = config.path().with_file_name("sessions");
            if sessions::discard_exited(&mut state, &root)
                && let Some(store) = &mut workspace
                && let Err(error) = store.save(&state)
            {
                state.notice = format!("{}\n{error}", appearance.language.text("workspace_error"));
            }
        }
        Self::build(appearance, config, error, state, workspace, window, cx)
    }

    fn build(
        appearance: Preferences,
        config: Option<Config>,
        error: Option<String>,
        mut state: Workspace,
        workspace: Option<Store>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        preferences::apply(&appearance, cx);
        projects::init(cx);
        git::init(cx);
        let language = appearance.language;
        if let Some(error) = &error {
            state.notice = format!("{}\n{error}", language.text("config_error"));
        }
        let preferences = cx.new(|_| appearance.clone());
        let updater = cx.new(|cx| updates::Updater::new(preferences.clone(), cx));
        let local_terminals = vec![None; state.local_terminal.sessions.len()];
        let command_history = tervia::suggestions::Worker::new(
            config
                .as_ref()
                .map(|config| config.path().with_file_name("command-history.json")),
        );
        let mut view = Self {
            focus: cx.focus_handle(),
            state,
            preferences,
            appearance,
            config,
            workspace,
            workspace_save: None,
            workspace_revision: 0,
            config_error: cx.new(|_| error),
            preferences_window: None,
            updater,
            update_restarting: false,
            editors: Vec::new(),
            explorers: Vec::new(),
            navigation: projects::Navigation::new(window, cx),
            monitors: Vec::new(),
            terminals: Vec::new(),
            local_terminals,
            shell_profiles: tervia::shell::Profile::available(),
            terminal_name: cx.new(|cx| {
                InputState::new(window, cx)
                    .validate(|value, _| tervia::state::Session::valid_custom_name(value))
            }),
            terminal_rename: None,
            command_history,
            tasks: tasks::Launcher::new(window, cx),
            git_states: Vec::new(),
            git_scans: git::Scans::default(),
            git_mutating: false,
            saving_file: false,
            file_closing: false,
            file_reads: tervia::files::Reads::default(),
            welcome_url: cx.new(|cx| {
                InputState::new(window, cx).placeholder(language.text("git_clone_placeholder"))
            }),
            welcome_cloning: false,
            git_poll: None,
            file_poll: None,
            files_active: window.is_window_active(),
            subscriptions: Vec::new(),
        };
        view.subscribe_tasks(window, cx);
        view.subscribe_agent_notifications(window, cx);
        view.subscriptions.push(cx.subscribe_in(
            &view.terminal_name,
            window,
            |this, _, event, _, cx| {
                if matches!(event, InputEvent::Blur) && this.terminal_rename.take().is_some() {
                    cx.notify();
                }
            },
        ));
        view.subscriptions.push(cx.subscribe_in(
            &view.updater,
            window,
            |this, _, _: &updates::Restart, window, cx| this.restart_after_update(window, cx),
        ));
        view.subscriptions
            .push(cx.observe(&view.updater, |_, _, cx| cx.notify()));
        view.subscriptions.push(cx.subscribe_in(
            &view.navigation.switcher.input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.navigation.switcher.selected = 0;
                    this.navigation
                        .switcher
                        .scroll
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    // Native inputs consume Enter and emit an event before workspace actions.
                    this.confirm_project_switcher(window, cx);
                }
            },
        ));
        view.subscriptions.push(cx.subscribe_in(
            &view.navigation.input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.navigation.selected = 0;
                    this.navigation
                        .scroll
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) && this.navigation.open {
                    let parent = this.working_directory_owner();
                    if let Some(index) = this
                        .working_directory_rows(parent, cx)
                        .get(this.navigation.selected)
                        .copied()
                    {
                        this.select_working_directory(parent, index, window, cx);
                    }
                }
            },
        ));
        if !cfg!(test) {
            view.updater
                .update(cx, |updater, cx| updater.start_automatic(cx));
        }
        view.subscriptions
            .push(cx.subscribe(&view.welcome_url, |_, _, _: &InputEvent, cx| cx.notify()));
        view.subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                this.monitor_activation(window.is_window_active(), window, cx);
            }));
        view.subscriptions
            .push(cx.observe_window_bounds(window, |this, window, cx| {
                // Fullscreen has its own size; retain the zoom state to restore on the next launch.
                if window.is_fullscreen() || window.is_simple_fullscreen() {
                    return;
                }
                // macOS reports zoomed bounds as Windowed, so query the native maximization state.
                let maximized = window.is_maximized();
                if this.state.window_maximized != maximized {
                    this.state.window_maximized = maximized;
                    this.schedule_workspace_save(cx);
                }
            }));
        view.subscriptions.push(cx.observe_in(
            &view.preferences,
            window,
            |this, preferences, _window, cx| {
                this.appearance = preferences.read(cx).clone();
                for explorer in &mut this.explorers {
                    explorer.tree.set_nesting(this.appearance.file_nesting);
                }
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
        for index in 0..view.state.projects.len() {
            if view.state.project_visible(index) {
                view.restore_documents(index, window, cx);
            }
        }
        for index in 0..view.state.projects.len() {
            if !view.state.project_visible(index) {
                continue;
            }
            view.refresh_git(index, window, cx);
            let root = &view.state.projects[index].root;
            // Separate child scans would replace the enclosing workspace's repository grouping.
            if view
                .state
                .folder(index)
                .is_none_or(|parent| parent == index)
                && !view
                    .state
                    .projects
                    .iter()
                    .any(|project| project.root != *root && root.starts_with(&project.root))
            {
                view.rescan_repositories(index, window, cx);
            }
        }
        for parent in view.state.project_rows() {
            view.state.attach_directory(parent, parent);
        }
        view.subscriptions
            .push(cx.observe_self(|this, cx| this.schedule_workspace_save(cx)));
        view.git_poll = Some(cx.spawn_in(window, async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                if weak
                    .update_in(cx, |this, window, cx| {
                        if this.files_active
                            && this.state.has_project()
                            && this.state.project().tab == Tab::Git
                        {
                            this.poll_git(this.state.active_project, window, cx);
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
        if view.state.window_maximized {
            // Restore after the window and its bounds observer are registered.
            window.defer(cx, |window, _| {
                if !window.is_maximized() {
                    window.zoom_window();
                }
            });
        }
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
            self.install_definitions(project_index, file_index, &editor, cx);
            self.subscriptions.push(cx.observe(&editor, |this, _, cx| {
                if this.state.project().tab == Tab::Files {
                    cx.notify();
                }
            }));
            // Capture the owner: a late input event must never edit the newly selected project.
            self.subscriptions.push(cx.subscribe_in(
                &editor,
                window,
                move |this, editor, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.state.projects[project_index].files[file_index].draft =
                            editor.read(cx).value().to_string();
                        if this.state.projects[project_index].files[file_index].dirty() {
                            this.state.projects[project_index].pin_file(file_index);
                        }
                        cx.notify();
                    }
                },
            ));
            editors.push(editor);
        }
        self.editors.push(editors);
        let mut explorer = explorer::Explorer::new(window, cx);
        explorer.tree.set_nesting(self.appearance.file_nesting);
        self.subscriptions.push(cx.subscribe_in(
            &explorer.finder.input,
            window,
            move |this, _, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_file_search(project_index, window, cx);
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    this.open_search_selection(project_index, window, cx);
                }
            },
        ));
        self.explorers.push(explorer);
        self.monitors.push(monitor::Monitor::default());
        self.terminals
            .push(vec![None; project.terminal.sessions.len()]);
        let git = git::GitUi::new(window, cx);
        git.subscribe(project_index, &mut self.subscriptions, cx);
        self.git_states.push(git);
    }

    pub(crate) fn shutdown_terminals(&mut self, cx: &mut Context<Self>) {
        for explorer in &self.explorers {
            explorer.definitions.shutdown();
        }
        self.workspace_save.take();
        self.persist_workspace(cx);
        self.detach_terminals(cx);
    }

    pub(super) fn detach_terminals(&mut self, cx: &mut Context<Self>) {
        for slot in self
            .terminals
            .iter_mut()
            .flatten()
            .chain(self.local_terminals.iter_mut())
        {
            if let Some(console) = slot.take() {
                console.update(cx, |console, _| console.detach());
            }
        }
    }

    fn persist_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace.is_none() {
            return;
        }
        self.capture_reading_positions(cx);
        self.capture_terminal_positions(cx);
        if let Some(workspace) = &mut self.workspace
            && let Err(error) = workspace.save(&self.state)
            && self.state.notice.is_empty()
        {
            self.state.notice = format!("{}\n{error}", self.t("workspace_error"));
            cx.notify();
        }
    }

    fn change_project(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.project_visible(index)
            || self.worktree_removing(&self.state.projects[index].root)
        {
            return;
        }
        self.terminal_rename = None;
        self.tasks.cancel();
        self.dismiss_definition_preview(cx);
        if self.navigation.switcher.open {
            self.close_project_switcher(window, cx);
        }
        self.navigation.cancel();
        for ui in &mut self.git_states {
            ui.close_branch_picker();
            ui.cancel_worktree_navigation();
        }
        self.state.switch_project(index);
        self.persist_workspace(cx);
        self.refresh_git(index, window, cx);
        if self.state.active_tab() == Tab::Files {
            self.load_directory(index, PathBuf::new(), false, window, cx);
        }
        if self.state.project().tab == Tab::Terminal {
            self.focus_terminal(window, cx);
        }
        cx.notify();
    }

    fn change_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.has_project() {
            return;
        }
        self.dismiss_definition_preview(cx);
        self.terminal_rename = None;
        if self.navigation.switcher.open {
            self.close_project_switcher(window, cx);
        }
        self.navigation.cancel();
        for ui in &mut self.git_states {
            ui.close_branch_picker();
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
            self.focus_terminal(window, cx);
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_real_file(window, cx);
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tasks.open {
            self.close_project_tasks(window, cx);
            return;
        }
        if self.welcome_cloning {
            self.welcome_cloning = false;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        // Input Escape can bubble past the popover's own Cancel action.
        if self.dismiss_definition_preview(cx) {
            return;
        }
        if self.navigation.switcher.open {
            self.close_project_switcher(window, cx);
        } else if self.navigation.open {
            self.navigation.open = false;
            cx.notify();
        } else if (!self.state.projects.is_empty()
            && self.git_states[self.state.active_project].close_branch_picker())
            || self.state.dismiss_palette()
        {
            cx.notify();
        } else if !self.state.projects.is_empty()
            && self.state.active_tab() == Tab::Files
            && self.explorers[self.state.active_project].finder.mode != files::search::Mode::Browse
        {
            self.show_file_search(files::search::Mode::Browse, window, cx);
        } else if self.try_quit(cx) {
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
        let updater = self.updater.clone();
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
                        updater,
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
        if self.state.screen == Screen::Welcome {
            return self.welcome(window, cx).into_any_element();
        }
        div()
            .id("tervia-workspace")
            .relative()
            .key_context(match self.state.active_tab() {
                Tab::Files => "Tervia FileTabs",
                Tab::Terminal => "Tervia Terminal",
                Tab::Git => "Tervia",
            })
            .track_focus(&self.focus)
            .on_modifiers_changed(cx.listener(Self::definition_modifiers))
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
            .on_action(
                cx.listener(|this, _: &projects::QuickSwitchProject, window, cx| {
                    this.show_project_switcher(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &projects::ReopenProject, window, cx| {
                    this.show_project_switcher(true, window, cx)
                }),
            )
            .when(self.state.previous_project().is_some(), |workspace| {
                workspace.on_action(cx.listener(
                    |this, _: &projects::PreviousProject, window, cx| {
                        if let Some(index) = this.state.previous_project() {
                            this.change_project(index, window, cx);
                        }
                    },
                ))
            })
            .when(self.state.has_project(), |workspace| {
                workspace
                    .on_action(cx.listener(|this, _: &projects::CloseProject, window, cx| {
                        this.request_project_close(this.working_directory_owner(), window, cx)
                    }))
                    .on_action(cx.listener(
                        |this, _: &projects::SwitchWorkingDirectory, window, cx| {
                            this.toggle_directory_picker(!this.navigation.open, window, cx);
                        },
                    ))
                    .on_action(cx.listener(|this, _: &git::QuickSwitchBranch, window, cx| {
                        this.toggle_branch_picker(git::SwitchAnchor::Navigation, window, cx);
                    }))
            })
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.state.toggle_sidebar();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewLocalTerminal, window, cx| {
                this.new_local_terminal(window, cx)
            }))
            .on_action(cx.listener(|this, _: &Welcome, window, cx| this.open_welcome(window, cx)))
            .when(self.state.has_project(), |workspace| {
                workspace
                    .on_action(cx.listener(|this, _: &TerminalTab, window, cx| {
                        this.change_tab(Tab::Terminal, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &GitTab, window, cx| {
                        this.change_tab(Tab::Git, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &FilesTab, window, cx| {
                        this.change_tab(Tab::Files, window, cx)
                    }))
            })
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.choose_repository(window, cx)),
            )
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(
                    cx.listener(|this, _: &OpenFile, window, cx| this.choose_file(window, cx)),
                )
            })
            .when(self.can_save_file(), |workspace| {
                workspace
                    .on_action(cx.listener(|this, _: &SaveFile, window, cx| this.save(window, cx)))
            })
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(cx.listener(|this, _: &QuickOpenFile, window, cx| {
                    this.show_file_search(files::search::Mode::Files, window, cx)
                }))
            })
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(cx.listener(|this, _: &SearchProjectText, window, cx| {
                    this.show_file_search(files::search::Mode::Text, window, cx)
                }))
            })
            .when(
                self.state.active_tab() == Tab::Files
                    && !self.state.project().open_files.is_empty()
                    && self.can_close_file_tabs(),
                |workspace| {
                    workspace.on_action(cx.listener(|this, _: &CloseFileTab, window, cx| {
                        this.close_active_file_tab(window, cx)
                    }))
                },
            )
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(cx.listener(|this, _: &NextFileTab, window, cx| {
                    this.cycle_file_tab(false, window, cx)
                }))
            })
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(cx.listener(|this, _: &PreviousFileTab, window, cx| {
                    this.cycle_file_tab(true, window, cx)
                }))
            })
            .when(
                self.state.has_project()
                    && self.state.active_tab() == Tab::Files
                    && self.state.project().current_file().is_some()
                    && self.explorers[self.state.active_project]
                        .file_history
                        .can_back(),
                |workspace| {
                    workspace.on_action(cx.listener(|this, _: &files::FileBack, window, cx| {
                        this.navigate_file(files::Direction::Back, window, cx)
                    }))
                },
            )
            .when(
                self.state.has_project()
                    && self.state.active_tab() == Tab::Files
                    && self.state.project().current_file().is_some()
                    && self.explorers[self.state.active_project]
                        .file_history
                        .can_forward(),
                |workspace| {
                    workspace.on_action(cx.listener(|this, _: &files::FileForward, window, cx| {
                        this.navigate_file(files::Direction::Forward, window, cx)
                    }))
                },
            )
            .when(
                self.state.active_tab() == Tab::Files
                    && !self.state.project().closed_files.is_empty()
                    && !self.file_closing,
                |workspace| {
                    workspace.on_action(cx.listener(|this, _: &ReopenFileTab, window, cx| {
                        this.reopen_file_tab(this.state.active_project, window, cx)
                    }))
                },
            )
            .on_action(cx.listener(|this, _: &NewSession, window, cx| {
                if this.state.has_project() && !this.state.project().dock.visible {
                    this.change_tab(Tab::Terminal, window, cx);
                }
                this.new_session(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SplitTerminal, window, cx| this.split_terminal(window, cx)),
            )
            .when(
                !self.state.terminals().sessions.is_empty()
                    && (self.state.active_tab() == Tab::Terminal
                        || self.state.project().dock.visible),
                |workspace| {
                    workspace
                        .on_action(
                            cx.listener(|this, _: &ClearTerminal, _, cx| this.clear_terminal(cx)),
                        )
                        .on_action(cx.listener(|this, _: &CopyTerminalOutput, _, cx| {
                            this.copy_terminal_output(cx)
                        }))
                },
            )
            .on_action(
                cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)),
            )
            .on_action(cx.listener(|this, _: &TogglePalette, window, cx| {
                this.state.palette_open = !this.state.palette_open;
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .when(self.state.active_tab() != Tab::Terminal, |workspace| {
                workspace.on_action(cx.listener(|this, _: &ToggleTerminalDock, window, cx| {
                    this.toggle_terminal_dock(window, cx)
                }))
            })
            .when(self.state.active_tab() != Tab::Terminal, |workspace| {
                workspace.on_action(
                    cx.listener(|this, _: &CopyCodeContext, _, cx| this.copy_code_context(cx)),
                )
            })
            .on_action(cx.listener(|this, _: &OpenPreferences, _, cx| this.open_preferences(cx)))
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                this.close_terminal_session(this.state.command_session(), window, cx)
            }))
            .child(self.titlebar(window, cx))
            .when(self.state.has_project(), |workspace| {
                workspace.on_action(cx.listener(|this, _: &RunProjectTask, window, cx| {
                    this.show_project_tasks(window, cx)
                }))
            })
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
                            .when(self.state.has_project(), |body| {
                                body.child(self.navigation(cx))
                            })
                            .child(self.workspace_body(window, cx)),
                    ),
            )
            .child(self.statusbar(cx))
            .when(self.state.palette_open, |this| this.child(self.palette(cx)))
            .when(self.navigation.switcher.open, |this| {
                this.child(self.project_switcher(cx))
            })
            .when(self.tasks.open && self.state.has_project(), |this| {
                this.child(self.project_tasks(cx))
            })
            .into_any_element()
    }
}
