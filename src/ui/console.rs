use std::{ops::Range, path::PathBuf, sync::Arc, time::Duration};

use alacritty_terminal::{
    event::{Event, WindowSize},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point as CellPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags},
    vte::ansi::{ClearMode, Color, CursorShape, Handler, NamedColor, Rgb},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use portable_pty::PtySize;
use tervia::{
    agent::Agent,
    locale::Language,
    platform::Platform,
    preferences::Preferences,
    pty::PtyEvent,
    sessions::{Connection, Target},
    terminal::{FileLink, TerminalBuffer, key_bytes, mouse_bytes, paste_bytes},
};

use super::theme::Palette;
mod activity;
mod render;
mod scroll;
mod search;
mod suggestions;

gpui_kit::actions!(
    console,
    [
        CopySelection,
        Paste,
        Escape,
        SelectAll,
        Clear,
        Find,
        CloseSearch,
        NextMatch,
        PreviousMatch
    ]
);

#[derive(Clone, Debug, Default, PartialEq, Action)]
#[action(namespace = console, no_json)]
pub struct SendKey {
    key: &'static str,
    control: bool,
    alt: bool,
    shift: bool,
}

pub struct Focused;
impl EventEmitter<Focused> for Console {}
pub struct IdentityChanged;
impl EventEmitter<IdentityChanged> for Console {}
impl EventEmitter<FileLink> for Console {}
pub struct AttentionChanged;
impl EventEmitter<AttentionChanged> for Console {}
pub struct LifecycleChanged;
impl EventEmitter<LifecycleChanged> for Console {}
pub(super) struct ActivityChanged(pub Option<tervia::agent::activity::Snapshot>);
impl EventEmitter<ActivityChanged> for Console {}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            if Platform::CURRENT == Platform::Mac {
                "cmd-f"
            } else {
                "ctrl-shift-f"
            },
            Find,
            Some("Console"),
        ),
        KeyBinding::new("escape", CloseSearch, Some("ConsoleSearch > Input")),
        KeyBinding::new("enter", NextMatch, Some("ConsoleSearch > Input")),
        KeyBinding::new("shift-enter", PreviousMatch, Some("ConsoleSearch > Input")),
        KeyBinding::new(Platform::CURRENT.copy_key(), CopySelection, Some("Console")),
        KeyBinding::new(Platform::CURRENT.paste_key(), Paste, Some("Console")),
        KeyBinding::new("escape", Escape, Some("Console")),
        KeyBinding::new(Platform::CURRENT.select_key(), SelectAll, Some("Console")),
        KeyBinding::new(Platform::CURRENT.clear_key(), Clear, Some("Console")),
    ]);
    // GPUI's Root handles Tab before raw key-down listeners. Explicit terminal
    // bindings must win for every non-text key, not merely for Escape and Copy.
    let mut bindings = Vec::new();
    for key in [
        "enter",
        "tab",
        "backspace",
        "delete",
        "insert",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "f1",
        "f2",
        "f3",
        "f4",
        "f5",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "f11",
        "f12",
    ] {
        for flags in 0..8 {
            let control = flags & 4 != 0;
            let alt = flags & 2 != 0;
            let shift = flags & 1 != 0;
            let chord = format!(
                "{}{}{}{key}",
                if control { "ctrl-" } else { "" },
                if alt { "alt-" } else { "" },
                if shift { "shift-" } else { "" }
            );
            bindings.push(KeyBinding::new(
                &chord,
                SendKey {
                    key,
                    control,
                    alt,
                    shift,
                },
                Some("Console"),
            ));
        }
    }
    for key in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z", "space", "[", "]", "\\",
    ] {
        for (control, alt) in [(true, false), (false, true), (true, true)] {
            let chord = format!(
                "{}{}{key}",
                if control { "ctrl-" } else { "" },
                if alt { "alt-" } else { "" }
            );
            bindings.push(KeyBinding::new(
                &chord,
                SendKey {
                    key,
                    control,
                    alt,
                    shift: false,
                },
                Some("Console"),
            ));
        }
    }
    cx.bind_keys(bindings);
    if Platform::CURRENT == Platform::Mac {
        // Translate macOS editing chords into shell-owned control keys. Clearing
        // input must not use the separate action that erases terminal scrollback.
        cx.bind_keys(
            [
                ("cmd-backspace", "u"),
                ("cmd-left", "a"),
                ("cmd-right", "e"),
            ]
            .into_iter()
            .map(|(chord, key)| {
                KeyBinding::new(
                    chord,
                    SendKey {
                        key,
                        control: true,
                        alt: false,
                        shift: false,
                    },
                    Some("Console"),
                )
            }),
        );
    }
}

pub struct Console {
    pub focus: FocusHandle,
    pub cwd: PathBuf,
    pub agent: Option<Agent>,
    pub(super) attention: bool,
    activity: Option<tervia::agent::activity::Snapshot>,
    retired_epoch: Option<String>,
    agent_attention: bool,
    search: Option<search::Search>,
    suggestions: suggestions::State,
    buffer: TerminalBuffer,
    glyphs: render::Glyphs,
    session: Option<Connection>,
    target: Option<Target>,
    replaying: bool,
    restored_scroll: usize,
    terminated: std::sync::Arc<std::sync::atomic::AtomicBool>,
    appearance: Preferences,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    selecting: bool,
    mouse_reporting: bool,
    selection_scroll: Option<Task<()>>,
    scrollbar: scroll::TerminalScroll,
    link_hover: bool,
    link_pressed: bool,
    composition: String,
    status: Option<Status>,
    cursor_on: bool,
    _blink: Option<Task<()>>,
    _reader: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    #[cfg(test)]
    transmitted: Vec<Vec<u8>>,
}

enum Status {
    Restoring,
    Exited(String),
    Connection(String),
    Spawn(String),
    Input(String),
    Resize(String),
}

impl Status {
    fn text(&self, language: Language) -> String {
        match self {
            Self::Restoring => language.text("terminal_restoring").into(),
            Self::Exited(status) => format!(
                "{} · {status} · ⌘T {}",
                language.text("session_ended"),
                language.text("new_terminal")
            ),
            Self::Connection(error) => format!(
                "{}: {error} · ⌘T {}",
                language.text("connection_error"),
                language.text("retry")
            ),
            Self::Spawn(error) => format!(
                "{}: {error} · ⌘T {}",
                language.text("shell_start_failed"),
                language.text("retry")
            ),
            Self::Input(error) => format!("{}: {error}", language.text("input_failed")),
            Self::Resize(error) => format!("{}: {error}", language.text("resize_failed")),
        }
    }
}

impl Console {
    pub(super) fn can_rerun_task(&self) -> bool {
        matches!(self.status, Some(Status::Exited(_) | Status::Spawn(_)))
    }

    pub(super) fn task_status(&self) -> String {
        let language = self.appearance.language;
        match &self.status {
            Some(Status::Restoring) => language.text("tasks_starting").into(),
            Some(Status::Exited(status)) => {
                format!("{} · {status}", language.text("tasks_finished"))
            }
            Some(Status::Spawn(error)) => format!("{} · {error}", language.text("tasks_failed")),
            Some(Status::Connection(error)) => {
                format!("{} · {error}", language.text("connection_error"))
            }
            _ => language.text("tasks_running").into(),
        }
    }

    pub(super) fn finish_process(&mut self, status: String, cx: &mut Context<Self>) {
        self.finish_replay(cx);
        self.status = Some(Status::Exited(status));
        self.session.take();
        self.invalidate_suggestion();
        cx.emit(LifecycleChanged);
        cx.notify();
    }

    pub fn shell(&self) -> String {
        self.session
            .as_ref()
            .map(|session| session.shell().to_owned())
            .or_else(|| {
                self.target
                    .as_ref()
                    .map(|target| target.shell().to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| {
                self.appearance
                    .shell
                    .resolve()
                    .to_string_lossy()
                    .into_owned()
            })
    }

    pub fn new(
        cwd: PathBuf,
        preferences: Entity<Preferences>,
        history: tervia::suggestions::Worker,
        target: Result<(Target, bool), String>,
        restored_scroll: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = preferences.read(cx).clone();
        let mut console = Self {
            focus: cx.focus_handle(),
            cwd: cwd.clone(),
            agent: None,
            attention: false,
            activity: None,
            retired_epoch: None,
            agent_attention: false,
            search: None,
            suggestions: suggestions::State::new(history),
            buffer: TerminalBuffer::new(80, 24),
            glyphs: Default::default(),
            session: None,
            target: target.as_ref().ok().map(|(target, _)| target.clone()),
            replaying: !cfg!(test) && target.is_ok(),
            restored_scroll,
            terminated: std::sync::Arc::default(),
            appearance,
            bounds: Bounds::default(),
            cell_width: px(8.0),
            cell_height: px(23.0),
            selecting: false,
            mouse_reporting: false,
            selection_scroll: None,
            scrollbar: scroll::TerminalScroll::default(),
            link_hover: false,
            link_pressed: false,
            composition: String::new(),
            status: None,
            cursor_on: true,
            _blink: None,
            _reader: None,
            _subscriptions: Vec::new(),
            #[cfg(test)]
            transmitted: Vec::new(),
        };
        console
            ._subscriptions
            .push(cx.observe(&preferences, |this, preferences, cx| {
                let appearance = preferences.read(cx).clone();
                if this.appearance.command_suggestions != appearance.command_suggestions {
                    this.invalidate_suggestion();
                    this.suggestions.new_session();
                }
                // Presets in the same light/dark mode still change terminal color queries.
                let theme_changed = this.appearance.theme.current() != appearance.theme.current();
                this.appearance = appearance;
                if theme_changed
                    && let Some(session) = &this.session
                    && let Err(error) = session.colors(this.palette_colors())
                {
                    this.status = Some(Status::Connection(error));
                }
                cx.notify();
            }));
        console
            ._subscriptions
            .push(cx.on_focus(&console.focus, window, |this, _, cx| {
                this.focus_changed(true, cx)
            }));
        console
            ._subscriptions
            .push(cx.on_blur(&console.focus, window, |this, _, cx| {
                this.focus_changed(false, cx)
            }));
        console
            ._subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                this.focus_changed(
                    window.is_window_active() && this.focus.is_focused(window),
                    cx,
                );
            }));
        console._blink = Some(cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if weak
                    .update(cx, |this, cx| {
                        let agent = this.session.as_ref().and_then(Connection::foreground_agent);
                        let activity = this.session.as_ref().and_then(Connection::activity);
                        this.observe_session(agent, activity, cx);
                        if this.buffer.term.is_focused && this.buffer.term.cursor_style().blinking {
                            this.cursor_on = !this.cursor_on;
                            cx.notify();
                        } else if !this.cursor_on {
                            this.cursor_on = true;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        // Native UI tests use an in-memory terminal: OS reader threads cannot wake
        // GPUI’s deterministic test scheduler. Real PTY transport has integration coverage.
        if !cfg!(test) {
            let size = PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            };
            let colors = console.palette_colors();
            console.status = Some(Status::Restoring);
            let terminated = console.terminated.clone();
            console._reader = Some(cx.spawn(async move |weak, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let (target, existing) = target?;
                        let target = target.with_colors(colors);
                        let result = if existing {
                            target.reconnect()
                        } else {
                            target.open(&cwd, size)
                        };
                        // Explicit close may race a slow host launch; it must not leave a new orphan PTY.
                        if terminated.load(std::sync::atomic::Ordering::Acquire) {
                            if let Ok((session, _, _)) = &result {
                                let _ = session.terminate();
                            }
                            return Err("Terminal was closed".into());
                        }
                        result
                    })
                    .await;
                let receiver = match weak.update(cx, |this, cx| match result {
                    Ok((session, receiver, tracker)) => {
                        this.observe_activity(session.activity(), true, cx);
                        this.session = Some(session);
                        this.suggestions.tracker = tracker;
                        cx.notify();
                        Some(receiver)
                    }
                    Err(error) => {
                        this.replaying = false;
                        this.status = Some(Status::Spawn(error));
                        cx.emit(LifecycleChanged);
                        cx.notify();
                        None
                    }
                }) {
                    Ok(Some(receiver)) => receiver,
                    _ => return,
                };
                while let Ok(event) = receiver.recv().await {
                    let mut batch = vec![event];
                    while batch.len() < 32 {
                        match receiver.try_recv() {
                            Ok(event) => batch.push(event),
                            Err(_) => break,
                        }
                    }
                    if weak
                        .update(cx, |this, cx| {
                            for event in batch {
                                match event {
                                    PtyEvent::Data(bytes) => {
                                        this.receive_output(&bytes, cx);
                                    }
                                    PtyEvent::Resized { columns, rows } => {
                                        this.buffer.resize(columns, rows);
                                    }
                                    PtyEvent::ReplayComplete => this.finish_replay(cx),
                                    PtyEvent::Exited(status) => {
                                        this.finish_process(status, cx);
                                    }
                                    PtyEvent::Error(error) => {
                                        this.replaying = false;
                                        this.status = Some(Status::Connection(error));
                                        cx.emit(LifecycleChanged);
                                        this.invalidate_suggestion();
                                    }
                                }
                            }
                            this.refresh_search(cx);
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                    // Bound each update and yield so a noisy PTY cannot starve keyboard input or painting.
                    cx.background_executor()
                        .timer(Duration::from_millis(8))
                        .await;
                }
            }));
        }
        console
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        self.terminated
            .store(true, std::sync::atomic::Ordering::Release);
        let result = if let Some(session) = &self.session {
            session.terminate()
        } else if let Some(target) = &self.target {
            target.terminate()
        } else {
            Ok(())
        };
        self.detach();
        result
    }

    pub fn detach(&mut self) {
        self.invalidate_suggestion();
        self.stop_selecting();
        self.session.take();
        self._reader.take();
    }

    pub fn flush(&self) -> Result<(), String> {
        if let Some(session) = &self.session {
            session.flush()
        } else if self.replaying {
            Err(self.appearance.language.text("terminal_restoring").into())
        } else {
            Ok(())
        }
    }

    pub fn scroll_offset(&self) -> usize {
        if self.replaying {
            self.restored_scroll
        } else {
            self.buffer.term.grid().display_offset()
        }
    }

    fn finish_replay(&mut self, cx: &mut Context<Self>) {
        if !self.replaying {
            return;
        }
        self.replaying = false;
        self.buffer.events();
        // A restored shell may already contain partially edited input; wait for its next prompt before suggesting suffixes.
        self.suggestions.input.started();
        if self.buffer.term.mode().contains(TermMode::FOCUS_IN_OUT)
            && let Some(session) = &self.session
        {
            let _ = session.write(if self.buffer.term.is_focused {
                b"\x1b[I"
            } else {
                b"\x1b[O"
            });
        }
        self.buffer
            .term
            .scroll_display(Scroll::Delta(self.restored_scroll.min(10000) as i32));
        if matches!(self.status, Some(Status::Restoring)) {
            self.status = None;
            cx.emit(LifecycleChanged);
        }
        cx.notify();
    }

    fn focus_changed(&mut self, focused: bool, cx: &mut Context<Self>) {
        if !focused {
            self.stop_selecting();
            self.mouse_reporting = false;
        }
        self.buffer.term.is_focused = focused;
        if focused {
            if self.needs_attention() {
                self.attention = false;
                self.agent_attention = false;
                cx.emit(AttentionChanged);
            }
            cx.emit(Focused);
        }
        if self.buffer.term.mode().contains(TermMode::FOCUS_IN_OUT)
            && !self.replaying
            && let Some(session) = &self.session
        {
            let _ = session.write(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
        cx.notify();
    }

    pub fn output(&self) -> String {
        self.buffer.text()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.replaying {
            return;
        }
        if let Some(session) = &self.session {
            let _ = session.clear();
        }
        self.stop_selecting();
        // Clear the viewport first: Alacritty moves it into history on the main
        // screen, so clearing saved lines first would leave old output behind.
        self.buffer.term.clear_screen(ClearMode::All);
        self.buffer.term.clear_screen(ClearMode::Saved);
        self.send(b"\x0c", cx);
        cx.notify();
    }

    fn clear_action(&mut self, _: &Clear, _: &mut Window, cx: &mut Context<Self>) {
        self.clear(cx);
    }

    fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.replaying {
            return;
        }
        self.suggestions.input.send(bytes);
        self.invalidate_suggestion();
        #[cfg(test)]
        self.transmitted.push(bytes.to_vec());
        self.cursor_on = true;
        if let Some(session) = &self.session {
            if let Err(error) = session.write(bytes) {
                self.status = Some(Status::Input(error));
            }
            self.buffer.term.scroll_display(Scroll::Bottom);
            self.buffer.term.selection = None;
            cx.notify();
        }
        self.refresh_suggestions(cx);
    }

    fn handle_events(&mut self, cx: &mut Context<Self>) {
        if self.replaying {
            self.buffer.events();
            return;
        }
        for event in self.buffer.events() {
            match event {
                Event::PtyWrite(reply) => {
                    if self.target.is_some() {
                        continue;
                    }
                    if let Some(session) = &self.session {
                        let _ = session.write(reply.as_bytes());
                    }
                }
                Event::ClipboardStore(_, text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text))
                }
                // OSC 52 queries must not disclose the host clipboard to an arbitrary shell program.
                Event::ClipboardLoad(_, _) => {}
                Event::Bell if !self.buffer.term.is_focused && !self.attention => {
                    self.attention = true;
                    cx.emit(AttentionChanged);
                    cx.notify();
                }
                Event::ColorRequest(index, formatter) => {
                    if self.target.is_some() {
                        continue;
                    }
                    let color = self.index_color(index);
                    if let Some(session) = &self.session {
                        let _ = session.write(formatter(color).as_bytes());
                    }
                }
                Event::TextAreaSizeRequest(formatter) => {
                    if self.target.is_some() {
                        continue;
                    }
                    let (cols, rows) = self.buffer.size();
                    let size = WindowSize {
                        num_lines: rows as u16,
                        num_cols: cols as u16,
                        cell_width: f32::from(self.cell_width) as u16,
                        cell_height: f32::from(self.cell_height) as u16,
                    };
                    if let Some(session) = &self.session {
                        let _ = session.write(formatter(size).as_bytes());
                    }
                }
                _ => {}
            }
        }
    }

    fn copy(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.buffer.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx
            .read_from_clipboard()
            .and_then(|clipboard| clipboard.text())
        {
            let bytes = paste_bytes(&text, *self.buffer.term.mode());
            self.send(&bytes, cx);
        }
    }

    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.suggestion_visible() {
            self.invalidate_suggestion();
            cx.notify();
            return;
        }
        self.send(b"\x1b", cx);
    }

    fn send_key(&mut self, key: &SendKey, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        if !self.composition.is_empty() {
            cx.propagate();
            return;
        }
        if !key.alt && !key.control && !key.shift && self.suggestion_key(key.key, window, cx) {
            return;
        }
        if key.shift && matches!(key.key, "pageup" | "pagedown") {
            self.buffer.term.scroll_display(if key.key == "pageup" {
                Scroll::PageUp
            } else {
                Scroll::PageDown
            });
            cx.notify();
        } else if let Some(bytes) = key_bytes(
            key.key,
            key.control,
            key.alt,
            key.shift,
            *self.buffer.term.mode(),
        ) {
            self.send(&bytes, cx);
        } else {
            cx.propagate();
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select(
            CellPoint::new(self.buffer.term.topmost_line(), Column(0)),
            CellPoint::new(
                self.buffer.term.bottommost_line(),
                self.buffer.term.last_column(),
            ),
        );
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        let key = &event.keystroke;
        if key.modifiers.platform || !self.composition.is_empty() {
            return;
        }
        if !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.shift
            && self.suggestion_key(&key.key, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if key.modifiers.shift && key.key == "pageup" {
            self.buffer.term.scroll_display(Scroll::PageUp);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if key.modifiers.shift && key.key == "pagedown" {
            self.buffer.term.scroll_display(Scroll::PageDown);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if let Some(bytes) = key_bytes(
            &key.key,
            key.modifiers.control,
            key.modifiers.alt,
            key.modifiers.shift,
            *self.buffer.term.mode(),
        ) {
            self.send(&bytes, cx);
            cx.stop_propagation();
        }
    }

    fn viewport_point(&self, point: Point<Pixels>) -> (usize, usize) {
        let (cols, rows) = self.buffer.size();
        let column = (f32::from(point.x - self.bounds.left()) / f32::from(self.cell_width))
            .floor()
            .max(0.0) as usize;
        let row = (f32::from(point.y - self.bounds.top()) / f32::from(self.cell_height))
            .floor()
            .max(0.0) as usize;
        (column.min(cols - 1), row.min(rows - 1))
    }

    fn grid_point(&self, point: Point<Pixels>) -> CellPoint {
        let (column, row) = self.viewport_point(point);
        CellPoint::new(
            Line(row as i32 - self.buffer.term.grid().display_offset() as i32),
            Column(column),
        )
    }

    fn selection_anchor(&self, position: Point<Pixels>) -> (CellPoint, Side) {
        let point = self.grid_point(position);
        // Both endpoints use the pointer's cell half so a stationary click leaves no selection.
        let middle = self.bounds.left() + self.cell_width * (point.column.0 as f32 + 0.5);
        let side = if position.x < middle {
            Side::Left
        } else {
            Side::Right
        };
        (point, side)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_selecting();
        self.mouse_reporting = false;
        self.link_pressed = false;
        self.focus.focus(window, cx);
        let open_link = if Platform::CURRENT == Platform::Mac {
            event.modifiers.platform
        } else {
            event.modifiers.control
        };
        // Modifier-click opens source without consuming ordinary selection or a TUI's mouse reports.
        if open_link
            && !event.modifiers.shift
            && let Some(link) = self.buffer.link_at(self.grid_point(event.position))
        {
            self.selecting = false;
            self.link_hover = false;
            self.link_pressed = true;
            cx.emit(link);
            cx.stop_propagation();
            return;
        }
        if self.buffer.term.mode().intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            self.mouse_reporting = true;
            let (column, row) = self.viewport_point(event.position);
            self.send(
                &mouse_bytes(0, column, row, false, *self.buffer.term.mode()),
                cx,
            );
            return;
        }
        self.selecting = true;
        let ty = match event.click_count {
            2 => SelectionType::Semantic,
            3.. => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        let (point, side) = self.selection_anchor(event.position);
        self.buffer.term.selection = Some(Selection::new(ty, point, side));
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.selecting && event.pressed_button != Some(MouseButton::Left) {
            self.stop_selecting();
            return;
        }
        if self.link_pressed {
            return;
        }
        let modifier = if Platform::CURRENT == Platform::Mac {
            event.modifiers.platform
        } else {
            event.modifiers.control
        };
        let hover = modifier
            && !event.modifiers.shift
            && !self.selecting
            && self
                .buffer
                .link_at(self.grid_point(event.position))
                .is_some();
        if self.link_hover != hover {
            self.link_hover = hover;
            cx.notify();
        }
        if self.selecting {
            self.update_selection(event.position);
            self.start_selection_scroll(event.position, window, cx);
            cx.notify();
        } else if self.mouse_reporting
            && event.pressed_button == Some(MouseButton::Left)
            && self
                .buffer
                .term
                .mode()
                .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
        {
            let (column, row) = self.viewport_point(event.position);
            self.send(
                &mouse_bytes(32, column, row, false, *self.buffer.term.mode()),
                cx,
            );
        }
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        // A link consumed the press, so its release must not reach the terminal program either.
        if std::mem::take(&mut self.link_pressed) {
            return;
        }
        // Only a press delivered to the program may generate a release (scrollbar gestures never do).
        if std::mem::take(&mut self.mouse_reporting)
            && self.buffer.term.mode().intersects(TermMode::MOUSE_MODE)
        {
            let (column, row) = self.viewport_point(event.position);
            self.send(
                &mouse_bytes(0, column, row, true, *self.buffer.term.mode()),
                cx,
            );
        }
        if self.selecting {
            self.update_selection(event.position);
        }
        self.stop_selecting();
        cx.notify();
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = f32::from(event.delta.pixel_delta(self.cell_height).y);
        let lines = (delta / f32::from(self.cell_height)).round() as i32;
        if lines == 0 {
            return;
        }
        let mode = *self.buffer.term.mode();
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            let (column, row) = self.viewport_point(event.position);
            for _ in 0..lines.unsigned_abs().min(20) {
                self.send(
                    &mouse_bytes(if lines > 0 { 64 } else { 65 }, column, row, false, mode),
                    cx,
                );
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !event.modifiers.shift
        {
            let bytes = key_bytes(
                if lines > 0 { "up" } else { "down" },
                false,
                false,
                false,
                mode,
            )
            .unwrap();
            for _ in 0..lines.unsigned_abs().min(20) {
                self.send(&bytes, cx);
            }
        } else {
            self.buffer.term.scroll_display(Scroll::Delta(lines));
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn palette_colors(&self) -> Vec<[u8; 3]> {
        (0..269)
            .map(|index| {
                let color = self.index_color(index);
                [color.r, color.g, color.b]
            })
            .collect()
    }

    fn index_color(&self, index: usize) -> Rgb {
        if let Some(color) = self.buffer.term.colors()[index] {
            return color;
        }
        let p = Palette::new(self.appearance.theme);
        let color = match index {
            256 => p.text.to_rgb(),
            257 => p.background.to_rgb(),
            258 => p.text.to_rgb(),
            0..=15 => rgb(super::theme::terminal_ansi(self.appearance.theme)[index]),
            16..=231 => {
                let cube = index - 16;
                let channel = |value| if value == 0 { 0 } else { 55 + value * 40 };
                return Rgb {
                    r: channel(cube / 36) as u8,
                    g: channel(cube / 6 % 6) as u8,
                    b: channel(cube % 6) as u8,
                };
            }
            232..=255 => {
                let value = (8 + (index - 232) * 10) as u8;
                return Rgb {
                    r: value,
                    g: value,
                    b: value,
                };
            }
            _ => p.text.to_rgb(),
        };
        Rgb {
            r: (color.r * 255.0) as u8,
            g: (color.g * 255.0) as u8,
            b: (color.b * 255.0) as u8,
        }
    }

    fn color(&self, color: Color) -> Hsla {
        let color = match color {
            Color::Spec(color) => color,
            Color::Named(color) => self.index_color(color as usize),
            Color::Indexed(index) => self.index_color(index as usize),
        };
        rgb((color.r as u32) << 16 | (color.g as u32) << 8 | color.b as u32).into()
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[gpui_test::test]
    fn precise_agent_attention_deduplicates_and_replay_never_alerts(cx: &mut TestAppContext) {
        use tervia::agent::activity::{Phase, Snapshot};
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let console = cx.new(mock_console);
            console.update(cx, |this, cx| {
                let snapshot = |phase, revision: &str| Snapshot {
                    epoch: "mock-epoch".into(),
                    agent: Agent::Codex,
                    phase,
                    session: "mock-session".into(),
                    turn: "mock-turn".into(),
                    revision: revision.into(),
                };
                this.buffer.term.is_focused = false;
                this.observe_activity(Some(snapshot(Phase::Complete, "mock-1")), true, cx);
                assert!(!this.needs_attention());
                this.observe_activity(Some(snapshot(Phase::Permission, "mock-2")), false, cx);
                assert!(this.needs_attention());
                this.focus_changed(true, cx);
                assert!(!this.needs_attention());
                this.focus_changed(false, cx);
                this.observe_activity(Some(snapshot(Phase::Permission, "mock-3")), false, cx);
                assert!(!this.needs_attention());
                this.observe_activity(Some(snapshot(Phase::Working, "mock-4")), false, cx);
                this.observe_activity(Some(snapshot(Phase::Complete, "mock-5")), false, cx);
                assert!(this.needs_attention());
                this.focus_changed(true, cx);
                this.replaying = true;
                this.focus_changed(false, cx);
                this.observe_activity(Some(snapshot(Phase::Failed, "mock-6")), false, cx);
                assert!(!this.needs_attention());
            });
        });
    }

    #[gpui_test::test]
    fn restarting_the_same_agent_without_new_hooks_does_not_reuse_its_old_status(
        cx: &mut TestAppContext,
    ) {
        use tervia::agent::activity::{Phase, Snapshot};
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let console = cx.new(mock_console);
            console.update(cx, |this, cx| {
                this.observe_agent(Some(Agent::Claude), cx);
                this.observe_activity(
                    Some(Snapshot {
                        epoch: "mock-epoch".into(),
                        agent: Agent::Claude,
                        phase: Phase::Working,
                        session: "mock-old-session".into(),
                        turn: "mock-old-turn".into(),
                        revision: "mock-old-event".into(),
                    }),
                    true,
                    cx,
                );
                let late = Snapshot {
                    epoch: "mock-epoch".into(),
                    agent: Agent::Claude,
                    phase: Phase::Permission,
                    session: "mock-old-session".into(),
                    turn: "mock-old-turn".into(),
                    revision: "mock-late-event".into(),
                };
                this.observe_session(None, Some(late.clone()), cx);
                this.observe_session(Some(Agent::Claude), Some(late), cx);
                assert_eq!(
                    this.agent_status(),
                    Some((Agent::Claude, "agent_untracked"))
                );
                assert!(!this.needs_attention());
                let ended = Snapshot {
                    epoch: "mock-epoch".into(),
                    agent: Agent::Claude,
                    phase: Phase::Exited,
                    session: "mock-old-session".into(),
                    turn: "mock-old-turn".into(),
                    revision: "mock-ended-event".into(),
                };
                this.observe_session(None, Some(ended.clone()), cx);
                this.observe_session(Some(Agent::Claude), Some(ended), cx);
                assert_eq!(
                    this.agent_status(),
                    Some((Agent::Claude, "agent_untracked"))
                );
                this.observe_activity(
                    Some(Snapshot {
                        epoch: "mock-resumed-epoch".into(),
                        agent: Agent::Claude,
                        phase: Phase::Ready,
                        session: "mock-old-session".into(),
                        turn: "mock-new-turn".into(),
                        revision: "mock-new-event".into(),
                    }),
                    false,
                    cx,
                );
                assert_eq!(this.agent_status(), Some((Agent::Claude, "agent_ready")));
            });
        });
    }

    #[gpui_test::test]
    fn resumed_session_state_survives_when_polling_skips_its_ready_event(cx: &mut TestAppContext) {
        use tervia::agent::activity::{Event, Phase, Snapshot};
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let console = cx.new(mock_console);
            console.update(cx, |this, cx| {
                let event = |name| {
                    Event::parse(
                        Agent::Claude,
                        &serde_json::json!({"session_id":"mock-session", "hook_event_name":name}),
                    )
                    .unwrap()
                };
                let old =
                    Snapshot::advance(None, event("UserPromptSubmit"), "mock-old-epoch".into())
                        .unwrap();
                this.observe_session(Some(Agent::Claude), Some(old.clone()), cx);
                this.observe_session(None, None, cx);
                let start =
                    Snapshot::advance(Some(&old), event("SessionStart"), "mock-new-epoch".into())
                        .unwrap();
                let working = Snapshot::advance(
                    Some(&start),
                    event("UserPromptSubmit"),
                    "mock-latest".into(),
                )
                .unwrap();
                this.observe_session(Some(Agent::Claude), Some(working), cx);
                assert_eq!(
                    this.agent_status(),
                    Some((Agent::Claude, Phase::Working.key()))
                );
            });
        });
    }

    #[gpui_test::test]
    fn a_fresh_hook_survives_a_foreground_transition_in_the_same_poll(cx: &mut TestAppContext) {
        use tervia::agent::activity::{Phase, Snapshot};
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let console = cx.new(mock_console);
            console.update(cx, |this, cx| {
                let snapshot = |session: &str, phase, revision: &str| Snapshot {
                    epoch: session.into(),
                    agent: Agent::Claude,
                    phase,
                    session: session.into(),
                    turn: revision.into(),
                    revision: revision.into(),
                };
                this.observe_agent(Some(Agent::Claude), cx);
                this.observe_activity(
                    Some(snapshot("mock-old", Phase::Working, "mock-1")),
                    true,
                    cx,
                );
                let fresh = snapshot("mock-new", Phase::Ready, "mock-2");
                this.observe_session(None, Some(fresh.clone()), cx);
                this.observe_session(Some(Agent::Claude), Some(fresh), cx);
                assert_eq!(this.agent_status(), Some((Agent::Claude, "agent_ready")));
                this.observe_agent(Some(Agent::Gemini), cx);
                assert_eq!(this.agent_status(), None);
            });
        });
    }

    #[gpui_test::test]
    fn theme_changes_update_existing_terminal_colors_without_replacing_output(
        cx: &mut TestAppContext,
    ) {
        use tervia::preferences::Theme;
        cx.update(gpui_kit::init);
        let preferences = cx.update(|cx| cx.new(|_| Preferences::default()));
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                Console::new(
                    "/mock-project".into(),
                    preferences.clone(),
                    tervia::suggestions::Worker::new(None),
                    Err("mock offline".into()),
                    0,
                    window,
                    cx,
                )
            });
            entity.update(cx, |console, cx| {
                console.receive_output(b"mock output\r\n", cx)
            });
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        for theme in Theme::ALL {
            let before = cx.update(|_, cx| console.read(cx).output());
            cx.update(|_, cx| {
                preferences.update(cx, |preferences, cx| {
                    preferences.theme = theme.into();
                    cx.notify();
                });
            });
            cx.update(|_, cx| {
                let console = console.read(cx);
                let p = Palette::new(theme);
                let expected = |color: Hsla| {
                    let color = color.to_rgb();
                    Rgb {
                        r: (color.r * 255.0) as u8,
                        g: (color.g * 255.0) as u8,
                        b: (color.b * 255.0) as u8,
                    }
                };
                assert_eq!(console.appearance.theme.current(), theme);
                assert_eq!(console.index_color(256), expected(p.text));
                assert_eq!(console.index_color(257), expected(p.background));
                let ansi = match theme {
                    Theme::Darcula => Some((1, 0xf0524f)),
                    Theme::Monokai => Some((9, 0xf92672)),
                    Theme::SolarizedDark | Theme::SolarizedLight => Some((4, 0x268bd2)),
                    Theme::QuietLight => Some((4, 0x405bb0)),
                    _ => None,
                };
                if let Some((index, color)) = ansi {
                    assert_eq!(
                        console.index_color(index),
                        Rgb {
                            r: (color >> 16) as u8,
                            g: (color >> 8) as u8,
                            b: color as u8
                        },
                        "{theme:?}: ANSI color"
                    );
                }
                assert_eq!(console.output(), before);
            });
        }
    }

    #[gpui_test::test]
    fn agent_status_stays_above_the_terminal_in_constrained_panes_and_both_themes(
        cx: &mut TestAppContext,
    ) {
        use tervia::agent::activity::{Phase, Snapshot};
        let (console, cx) = scroll_fixture(cx);
        cx.simulate_resize(size(px(360.0), px(320.0)));
        for light in [false, true] {
            console.update(cx, |this, cx| {
                this.agent = Some(Agent::Codex);
                this.appearance.theme = light.into();
                this.appearance.language = Language::Chinese;
                this.observe_activity(
                    Some(Snapshot {
                        epoch: "mock-epoch".into(),
                        agent: Agent::Codex,
                        phase: Phase::Permission,
                        session: "mock-session".into(),
                        turn: "mock-turn".into(),
                        revision: "mock-revision".into(),
                    }),
                    true,
                    cx,
                );
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let status = cx.debug_bounds("agent-status").unwrap();
            assert_eq!(status.size.height, px(30.0));
            console.update(cx, |this, _| {
                assert!(status.bottom() <= this.bounds.top());
                assert!(status.left() <= this.bounds.left());
                assert!(status.right() >= this.bounds.right());
                assert!(status.left() >= px(0.0) && status.right() <= px(360.0));
                assert_eq!(
                    this.agent_status(),
                    Some((Agent::Codex, "agent_permission"))
                );
            });
        }
    }

    #[gpui_test::test]
    fn replay_suppresses_historical_bells_clipboard_and_input(cx: &mut TestAppContext) {
        let (console, cx) = scroll_fixture(cx);
        console.update(cx, |this, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(
                "synthetic-current-clipboard".into(),
            ));
            this.replaying = true;
            this.buffer.term.is_focused = false;
            this.receive_output(b"\x07\x1b]52;c;c3ludGhldGljLW9sZA==\x07mock replay", cx);
            this.send(b"must-not-run", cx);
            assert!(!this.attention);
            assert!(this.transmitted.is_empty());
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "synthetic-current-clipboard"
            );
            assert!(this.output().contains("mock replay"));
            this.finish_replay(cx);
            this.send(b"mock-live", cx);
            assert_eq!(this.transmitted, [b"mock-live".to_vec()]);
        });
    }

    struct ScrollFixture(Entity<Console>);

    impl Render for ScrollFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p(px(40.0)).child(
                div()
                    .w_full()
                    .max_w(px(420.0))
                    .h(px(240.0))
                    .child(self.0.clone()),
            )
        }
    }

    fn scroll_fixture(cx: &mut TestAppContext) -> (Entity<Console>, &mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::preferences::apply(&Preferences::default(), cx);
        });
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            console = Some(entity.clone());
            let fixture = cx.new(|_| ScrollFixture(entity));
            gpui_kit::base::Root::new(fixture, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            console.update(cx, |this, cx| {
                for row in 0..200 {
                    this.buffer
                        .feed(format!("synthetic row {row:03}\r\n").as_bytes());
                }
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        (console, cx)
    }

    fn hold_drag(cx: &mut VisualTestContext) {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui_test::test]
    fn terminal_single_click_clears_selection_without_highlighting_a_cell(cx: &mut TestAppContext) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.buffer = TerminalBuffer::new(80, 24);
                this.buffer.feed(b"mock> ");
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        for light in [false, true] {
            for (column, row) in [
                (0.2, 0.5),
                (0.8, 0.5),
                (7.2, 0.5),
                (7.8, 0.5),
                (3.2, 1.5),
                (3.8, 1.5),
            ] {
                let (position, cursor) = cx.update(|_, cx| {
                    console.update(cx, |this, _| {
                        this.appearance.theme = light.into();
                        this.buffer.select(
                            CellPoint::new(Line(0), Column(0)),
                            CellPoint::new(Line(0), Column(3)),
                        );
                        (
                            this.bounds.origin
                                + point(this.cell_width * column, this.cell_height * row),
                            this.buffer.term.grid().cursor.point,
                        )
                    })
                });
                cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
                cx.simulate_mouse_move(position, Some(MouseButton::Left), Modifiers::default());
                cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
                cx.update(|window, cx| {
                    let this = console.read(cx);
                    assert!(
                        this.buffer.term.renderable_content().selection.is_none(),
                        "A stationary click must not select a character cell"
                    );
                    assert!(this.buffer.selected_text().is_empty());
                    assert_eq!(this.buffer.term.grid().cursor.point, cursor);
                    assert!(this.focus.is_focused(window));
                    assert!(this.transmitted.is_empty());
                });
            }
        }
    }

    #[gpui_test::test]
    fn terminal_drag_selection_uses_the_same_cell_edges_in_both_directions(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.buffer = TerminalBuffer::new(80, 24);
                this.buffer.feed(b"mock> ");
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        let (start, end) = cx.update(|_, cx| {
            let this = console.read(cx);
            let at = |column| {
                this.bounds.origin + point(this.cell_width * column, this.cell_height * 0.5)
            };
            (at(0.9), at(2.9))
        });
        for (from, to) in [(start, end), (end, start)] {
            cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
            cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
            cx.update(|_, cx| assert_eq!(console.read(cx).buffer.selected_text(), "oc"));
        }
    }

    #[gpui_test::test]
    fn terminal_double_and_triple_click_still_select_words_and_lines(cx: &mut TestAppContext) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.buffer = TerminalBuffer::new(80, 24);
                this.buffer.feed(b"mock alpha beta");
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        let position = cx.update(|_, cx| {
            let this = console.read(cx);
            this.bounds.origin + point(this.cell_width * 6.8, this.cell_height * 0.5)
        });
        for (count, expected) in [(2, "alpha"), (3, "mock alpha beta\n")] {
            cx.simulate_event(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: count,
                modifiers: Modifiers::default(),
                first_mouse: false,
            });
            cx.simulate_event(MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: count,
                modifiers: Modifiers::default(),
            });
            cx.update(|_, cx| assert_eq!(console.read(cx).buffer.selected_text(), expected));
        }
    }

    #[gpui_test::test]
    fn terminal_drag_selection_scrolls_outside_both_edges_and_stops_on_release(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        let (bounds, output) = cx.update(|_, cx| {
            let this = console.read(cx);
            (this.bounds, this.output())
        });
        let start = bounds.center();
        let above = point(start.x, bounds.top() - px(30.0));
        let below = point(start.x, bounds.bottom() + px(30.0));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(above, Some(MouseButton::Left), Modifiers::default());
        hold_drag(cx);
        let (offset, selected) = cx.update(|_, cx| {
            let this = console.read(cx);
            (
                this.buffer.term.grid().display_offset(),
                this.buffer.selected_text(),
            )
        });
        assert!(
            offset > 0,
            "Holding above the pane must scroll into history"
        );
        hold_drag(cx);
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert!(this.buffer.term.grid().display_offset() > offset);
            assert!(this.buffer.selected_text().len() > selected.len());
            assert!(this.buffer.selected_text().ends_with(&selected));
        });
        cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
        let offset = cx.update(|_, cx| console.read(cx).buffer.term.grid().display_offset());
        hold_drag(cx);
        cx.update(|_, cx| assert_eq!(console.read(cx).buffer.term.grid().display_offset(), offset));
        cx.simulate_mouse_move(below, Some(MouseButton::Left), Modifiers::default());
        hold_drag(cx);
        cx.update(|_, cx| assert!(console.read(cx).buffer.term.grid().display_offset() < offset));
        cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
        let (offset, selected) = cx.update(|_, cx| {
            let this = console.read(cx);
            assert!(
                !this.selecting,
                "Releasing outside the pane ends the selection"
            );
            assert_eq!(this.output(), output);
            assert!(this.transmitted.is_empty());
            (
                this.buffer.term.grid().display_offset(),
                this.buffer.selected_text(),
            )
        });
        hold_drag(cx);
        cx.simulate_mouse_move(above, None, Modifiers::default());
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.buffer.term.grid().display_offset(), offset);
            assert_eq!(this.buffer.selected_text(), selected);
        });
    }

    #[gpui_test::test]
    fn terminal_scrollbar_drags_history_without_selecting_or_sending_mouse_input(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        let bar = cx
            .debug_bounds("terminal-scrollbar")
            .expect("Terminal history needs a scrollbar");
        cx.simulate_event(ScrollWheelEvent {
            position: bar.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(24.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| {
            console.update(cx, |this, _| this.buffer.feed(b"\x1b[?1002h\x1b[?1006h"));
        });
        let bottom = point(bar.right() - px(8.0), bar.bottom() - px(24.0));
        let top = point(bottom.x, bar.top() + px(24.0));
        cx.simulate_mouse_down(bottom, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(top, Some(MouseButton::Left), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(top, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(
                this.buffer.term.grid().display_offset(),
                this.buffer.term.history_size()
            );
            assert!(!this.selecting);
            assert!(this.buffer.selected_text().is_empty());
            assert!(this.transmitted.is_empty());
        });
        cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(bottom, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.buffer.term.grid().display_offset(), 0);
            assert!(this.transmitted.is_empty());
        });
    }

    #[gpui_test::test]
    fn terminal_selection_scrolling_preserves_tui_mouse_mode_and_shift_override(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        let bounds = cx.update(|_, cx| {
            console.update(cx, |this, _| this.buffer.feed(b"\x1b[?1002h\x1b[?1006h"));
            console.read(cx).bounds
        });
        let start = bounds.center();
        let above = point(start.x, bounds.top() - px(30.0));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(above, Some(MouseButton::Left), Modifiers::default());
        hold_drag(cx);
        cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert!(!this.selecting);
            assert_eq!(this.buffer.term.grid().display_offset(), 0);
            assert!(this.buffer.selected_text().is_empty());
            assert!(!this.transmitted.is_empty());
        });
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        cx.simulate_mouse_down(start, MouseButton::Left, shift);
        cx.simulate_mouse_move(above, Some(MouseButton::Left), shift);
        hold_drag(cx);
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert!(this.buffer.term.grid().display_offset() > 0);
            assert!(!this.buffer.selected_text().is_empty());
        });
        cx.update(|_, cx| console.update(cx, |this, cx| this.focus_changed(false, cx)));
        let offset = cx.update(|_, cx| console.read(cx).buffer.term.grid().display_offset());
        hold_drag(cx);
        cx.update(|_, cx| {
            assert!(!console.read(cx).selecting);
            assert_eq!(console.read(cx).buffer.term.grid().display_offset(), offset);
        });
    }

    #[gpui_test::test]
    fn terminal_search_input_handles_enter_and_escape_without_sending_keys_to_the_shell(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
        });
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            entity.update(cx, |this, cx| {
                this.buffer.feed(b"synthetic one\r\nsynthetic two");
                this.open_search(window, cx);
            });
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            console.update(cx, |this, cx| this.open_search(window, cx));
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("terminal-search").is_some());
        cx.simulate_input("synthetic");
        cx.update(|_, cx| assert_eq!(console.read(cx).buffer.selected_text(), "synthetic"));
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            let this = console.read(cx);
            assert!(!this.search.as_ref().unwrap().open);
            assert!(this.transmitted.is_empty());
            assert!(this.focus.is_focused(window));
        });
    }

    #[gpui_test::test]
    fn terminal_attention_uses_explicit_bells_and_focus_acknowledges_them(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let console = cx.new(mock_console);
            console.update(cx, |this, cx| {
                this.buffer.term.is_focused = false;
                this.buffer.feed(b"\x07\x07");
                this.handle_events(cx);
                assert!(this.attention);
                this.focus_changed(true, cx);
                assert!(!this.attention);
                this.buffer.feed(b"\x07");
                this.handle_events(cx);
                assert!(!this.attention);
            });
        });
    }

    #[gpui_test::test]
    fn terminal_link_click_preserves_tui_mouse_protocol(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut console = mock_console(cx);
                console.buffer.feed(b"src/mock.rs:4\x1b[?1002h\x1b[?1006h");
                console
            });
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        let position = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let console = console.read(cx);
            console.bounds.origin + point(console.cell_width * 3.5, console.cell_height * 0.5)
        });
        let modifiers = Modifiers {
            platform: Platform::CURRENT == Platform::Mac,
            control: Platform::CURRENT != Platform::Mac,
            ..Modifiers::default()
        };
        cx.simulate_mouse_down(position, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(
            position + point(px(1.0), px(0.0)),
            Some(MouseButton::Left),
            modifiers,
        );
        cx.simulate_mouse_up(position, MouseButton::Left, modifiers);
        cx.update(|_, cx| assert!(console.read(cx).transmitted.is_empty()));
        cx.simulate_click(position, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted,
                [b"\x1b[<0;4;1M".to_vec(), b"\x1b[<0;4;1m".to_vec()]
            )
        });
    }

    #[gpui_test::test]
    fn terminal_file_links_open_source_lines_without_overwriting_drafts(cx: &mut TestAppContext) {
        use super::super::Tervia;
        use std::fs;
        use tervia::state::Tab;
        cx.update(gpui_kit::init);
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("mock.rs"), "first\nsecond\nthird\n").unwrap();
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut this = Tervia::new(window, cx);
                let project = this.state.open_directory(root.path().into());
                this.state.projects[project]
                    .files
                    .push(tervia::state::File {
                        path: "mock.rs".into(),
                        disk_path: "mock.rs".into(),
                        language: "rust".into(),
                        saved: "first\nsecond\nthird\n".into(),
                        draft: "draft first\ndraft second\ndraft third\n".into(),
                    });
                this.init_project(project, window, cx);
                this.state.set_tab(Tab::Terminal);
                this.ensure_console(0, window, cx).update(cx, |console, _| {
                    console
                        .buffer
                        .feed(b"see mock.rs:2:3\x1b[?1000h\x1b[?1006h")
                });
                this
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        let position = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let console = view.read(cx).terminals[0][0].as_ref().unwrap().read(cx);
            console.bounds.origin + point(console.cell_width * 6.5, console.cell_height * 0.5)
        });
        let modifiers = Modifiers {
            platform: Platform::CURRENT == Platform::Mac,
            control: Platform::CURRENT != Platform::Mac,
            ..Modifiers::default()
        };
        cx.simulate_click(position, modifiers);
        cx.run_until_parked();
        cx.update(|_, cx| {
            let this = view.read(cx);
            assert_eq!(this.state.project().tab, Tab::Files);
            assert_eq!(this.state.project().current_file().unwrap().path, "mock.rs");
            assert_eq!(
                this.editors[0][0].read(cx).cursor_position(),
                gpui_kit::component::input::Position::new(1, 2)
            );
            assert_eq!(
                this.state.project().current_file().unwrap().draft,
                "draft first\ndraft second\ndraft third\n"
            );
            assert!(
                this.terminals[0][0]
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .transmitted
                    .is_empty(),
                "Link clicks must not send unmatched mouse releases to a TUI"
            );
        });
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.open_terminal_link(
                    0,
                    FileLink {
                        path: "mock.rs".into(),
                        line: 3,
                        column: 1,
                    },
                    window,
                    cx,
                );
                this.change_tab(Tab::Git, window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx).state.project().tab,
                Tab::Git,
                "Later navigation wins over pending link resolution"
            )
        });
    }

    #[gpui_test::test]
    fn mac_command_editing_shortcuts_reach_the_shell_and_dismiss_completion(
        cx: &mut TestAppContext,
    ) {
        if Platform::CURRENT != Platform::Mac {
            return;
        }
        let (console, cx) = scroll_fixture(cx);
        let output = cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.suggestions.input.prompt();
                this.send(b"git", cx);
                assert!(this.suggestion_visible());
                this.output()
            })
        });
        cx.simulate_keystrokes("cmd-backspace");
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.transmitted.concat(), b"git\x15");
            assert!(!this.suggestion_visible());
            assert_eq!(
                this.output(),
                output,
                "Deleting input must not clear terminal output or scrollback"
            );
        });
        cx.simulate_input("git");
        cx.update(|_, cx| {
            assert!(
                console.read(cx).suggestion_visible(),
                "Completion should work again after clearing a known input line"
            )
        });
        cx.simulate_keystrokes("cmd-left cmd-right alt-backspace ctrl-u ctrl-w");
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted.concat(),
                b"git\x15git\x01\x05\x1b\x7f\x15\x17"
            );
        });
    }

    #[gpui_test::test]
    fn terminal_editing_shortcuts_leave_search_and_ime_input_to_their_editors(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| this.open_search(window, cx));
        });
        cx.simulate_input("syntheticquery");
        cx.simulate_keystrokes(if Platform::CURRENT == Platform::Mac {
            "cmd-backspace"
        } else {
            "ctrl-backspace"
        });
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert!(
                this.search
                    .as_ref()
                    .unwrap()
                    .input
                    .read(cx)
                    .value()
                    .is_empty()
            );
            assert!(this.transmitted.is_empty());
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.composition = "合成".into();
            })
        });
        cx.simulate_keystrokes(if Platform::CURRENT == Platform::Mac {
            "cmd-backspace"
        } else {
            "ctrl-backspace"
        });
        cx.update(|_, cx| {
            assert!(
                console.read(cx).transmitted.is_empty(),
                "IME editing must not send control bytes to the shell"
            )
        });
    }

    #[gpui_test::test]
    fn enter_runs_the_typed_command_when_completion_has_not_been_chosen(cx: &mut TestAppContext) {
        let (console, cx) = ls_fixture(cx);
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted.concat(),
                b"ls\r",
                "An unchosen lsof suggestion must not replace ls"
            );
            assert!(!console.read(cx).suggestion_visible());
        });
    }

    #[gpui_test::test]
    fn first_arrow_selects_a_candidate_before_enter_fills_without_executing(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = ls_fixture(cx);
        cx.simulate_keystrokes("down enter");
        cx.update(|_, cx| assert_eq!(console.read(cx).transmitted.concat(), b"lsof -nP -iTCP"));
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(console.read(cx).transmitted.concat(), b"lsof -nP -iTCP\r"));
    }

    #[gpui_test::test]
    fn tab_can_fill_the_first_candidate_without_arrow_selection(cx: &mut TestAppContext) {
        let (console, cx) = ls_fixture(cx);
        cx.simulate_keystrokes("tab");
        cx.update(|_, cx| assert_eq!(console.read(cx).transmitted.concat(), b"lsof -nP -iTCP"));
    }

    #[gpui_test::test]
    fn editing_the_prefix_clears_the_explicit_choice_before_enter(cx: &mut TestAppContext) {
        use tervia::suggestions::{Source, Suggestion};
        let (console, cx) = ls_fixture(cx);
        cx.simulate_keystrokes("down");
        cx.simulate_input("o");
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.apply_suggestions(
                    this.suggestions.generation,
                    vec![Suggestion {
                        command: "lsof -nP -iTCP".into(),
                        source: Source::History,
                    }],
                    None,
                    cx,
                );
            })
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(console.read(cx).transmitted.concat(), b"lso\r"));
    }

    fn ls_fixture(cx: &mut TestAppContext) -> (Entity<Console>, &mut VisualTestContext) {
        use tervia::suggestions::{Source, Suggestion};
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.suggestions.input.prompt();
                this.send(b"ls", cx);
                this.apply_suggestions(
                    this.suggestions.generation,
                    vec![
                        Suggestion {
                            command: "lsof -nP -iTCP".into(),
                            source: Source::History,
                        },
                        Suggestion {
                            command: "ls -la".into(),
                            source: Source::Common,
                        },
                    ],
                    None,
                    cx,
                );
                assert!(this.suggestion_visible());
            });
        });
        (console, cx)
    }

    #[gpui_test::test]
    fn typing_a_prefix_opens_a_keyboard_selectable_list_and_enter_only_fills(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.buffer.feed(b"\x1b[2J\x1b[3J\x1b[Hmock> ");
                this.suggestions.input.prompt();
                assert!(!this.suggestion_visible());
                this.send(b"git", cx);
                this.buffer.feed(b"git");
                assert_eq!(this.suggestions.candidates.len(), 6);
                assert_eq!(this.suggestions.candidates[0].command, "git status");
            });
        });
        cx.update(|window, cx| {
            for _ in 0..2 {
                window.draw(cx).clear(cx);
            }
        });
        assert!(cx.debug_bounds("terminal-suggestion-row-0").is_some());
        assert!(cx.debug_bounds("terminal-suggestion-row-2").is_some());
        cx.simulate_keystrokes("down down down up down enter");
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.transmitted, [b"git".to_vec(), b" commit".to_vec()]);
            assert!(!this.suggestion_visible());
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(console.read(cx).transmitted.last().unwrap(), b"\r"));
    }

    #[gpui_test::test]
    fn prefix_changes_filter_the_list_and_reject_late_history_results(cx: &mut TestAppContext) {
        use tervia::suggestions::{Source, Suggestion};
        let (console, cx) = scroll_fixture(cx);
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.suggestions.input.prompt();
                this.send(b"git", cx);
                let previous = this.suggestions.generation;
                this.send(b" sta", cx);
                assert_eq!(
                    this.suggestions
                        .candidates
                        .iter()
                        .map(|s| s.command.as_str())
                        .collect::<Vec<_>>(),
                    ["git status", "git stash"]
                );
                this.apply_suggestions(
                    previous,
                    vec![Suggestion {
                        command: "git mock-stale".into(),
                        source: Source::History,
                    }],
                    None,
                    cx,
                );
                assert_eq!(this.suggestions.candidates.len(), 2);
                this.send(b"t", cx);
                assert_eq!(this.suggestions.candidates[0].command, "git status");
                assert_eq!(this.suggestions.candidates.len(), 1);
                this.send(b"\x7f", cx);
                assert_eq!(this.suggestions.candidates.len(), 2);
                this.send(b"\r", cx);
                this.suggestions.input.prompt();
                assert!(
                    !this.suggestion_visible(),
                    "Nothing appears after Enter on an empty prompt"
                );
            });
        });
    }

    #[gpui_test::test]
    fn changing_shell_history_source_invalidates_results_from_the_previous_file(
        cx: &mut TestAppContext,
    ) {
        use tervia::{
            suggestions::{Source, Suggestion},
            terminal::shell::Tracker,
        };
        let (console, cx) = scroll_fixture(cx);
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.suggestions.tracker = Some(Tracker::new("mock-token".into(), "zsh"));
                this.suggestions.input.prompt();
                this.send(b"git", cx);
                let old = this.suggestions.generation;
                let path = if cfg!(windows) {
                    r"C:\synthetic\custom-history"
                } else {
                    "/synthetic/custom-history"
                };
                let encoded = path
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                this.receive_output(
                    format!("\x1b]777;tervia;mock-token;H;{encoded}\x07").as_bytes(),
                    cx,
                );
                assert!(
                    this.suggestions.generation > old,
                    "Changing the source must cancel its outstanding prefix query"
                );
                this.apply_suggestions(
                    old,
                    vec![Suggestion {
                        command: "git obsolete-source".into(),
                        source: Source::History,
                    }],
                    None,
                    cx,
                );
                assert!(
                    !this
                        .suggestions
                        .candidates
                        .iter()
                        .any(|s| s.command == "git obsolete-source")
                );
            })
        });
    }

    #[gpui_test::test]
    fn escape_releases_arrows_and_tab_to_the_shell(cx: &mut TestAppContext) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.suggestions.input.prompt();
                this.send(b"git", cx);
            });
        });
        cx.simulate_keystrokes("escape up tab");
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(
                this.transmitted,
                [b"git".to_vec(), b"\x1b[A".to_vec(), b"\t".to_vec()]
            );
            assert!(!this.suggestion_visible());
        });
    }

    #[gpui_test::test]
    fn late_history_results_keep_the_chosen_command_and_tab_fills_it(cx: &mut TestAppContext) {
        use tervia::suggestions::{Source, Suggestion};
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            init(cx);
            console.update(cx, |this, cx| {
                this.focus.focus(window, cx);
                this.suggestions.input.prompt();
                this.send(b"git", cx);
                this.suggestion_key("down", window, cx);
                this.suggestion_key("down", window, cx);
                this.apply_suggestions(
                    this.suggestions.generation,
                    (0..6)
                        .map(|index| Suggestion {
                            command: format!("git mock-history-{index}"),
                            source: Source::History,
                        })
                        .collect(),
                    None,
                    cx,
                );
            });
        });
        cx.simulate_keystrokes("tab");
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted,
                [b"git".to_vec(), b" stash".to_vec()]
            )
        });
    }

    #[gpui_test::test]
    fn late_history_reordering_cannot_change_a_pressed_completion(cx: &mut TestAppContext) {
        use tervia::suggestions::{Source, Suggestion};
        let (console, cx) = scroll_fixture(cx);
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.buffer.feed(b"\x1b[2J\x1b[3J\x1b[Hmock> git");
                this.suggestions.input.prompt();
                this.send(b"git", cx);
            })
        });
        cx.update(|window, cx| {
            for _ in 0..2 {
                window.draw(cx).clear(cx);
            }
        });
        let pressed = cx
            .debug_bounds("terminal-suggestion-row-1")
            .unwrap()
            .center();
        cx.simulate_mouse_down(pressed, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.apply_suggestions(
                    this.suggestions.generation,
                    vec![
                        Suggestion {
                            command: "git mock-history".into(),
                            source: Source::History,
                        },
                        Suggestion {
                            command: "git status".into(),
                            source: Source::Common,
                        },
                        Suggestion {
                            command: "git stash".into(),
                            source: Source::Common,
                        },
                    ],
                    None,
                    cx,
                );
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(pressed, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted,
                [b"git".to_vec()],
                "A press on stash must never fill the status row that replaced it"
            )
        });
    }

    #[gpui_test::test]
    fn clicking_a_completion_inserts_only_its_suffix_without_selecting_output(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.buffer
                    .feed(b"\x1b[2J\x1b[3J\x1b[Hmock> git\x1b[?1000h");
                this.suggestions.input.prompt();
                this.send(b"git", cx);
            });
        });
        cx.update(|window, cx| {
            for _ in 0..2 {
                window.draw(cx).clear(cx);
            }
        });
        let button = cx
            .debug_bounds("terminal-suggestion-row-1")
            .expect("Stash candidate");
        cx.simulate_click(button.center(), Modifiers::default());
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.transmitted, [b"git".to_vec(), b" stash".to_vec()]);
            assert!(this.buffer.term.selection.is_none());
        });
    }

    #[gpui_test::test]
    fn completion_list_follows_input_and_fits_small_panes_without_resizing_the_pty(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        let before = cx.update(|_, cx| console.read(cx).buffer.size());
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.buffer.feed(b"\x1b[2J\x1b[3J\x1b[Hmock> git");
                this.suggestions.input.prompt();
                this.send(b"git", cx);
            })
        });
        cx.update(|window, cx| {
            for _ in 0..3 {
                window.draw(cx).clear(cx);
            }
        });
        let popup = cx
            .debug_bounds("terminal-suggestion")
            .expect("List below input");
        let footer = cx.debug_bounds("terminal-suggestion-footer").unwrap();
        assert!(
            footer.bottom() <= popup.bottom() - px(4.0),
            "The keyboard hints must fit inside the list border"
        );
        cx.update(|_, cx| {
            let this = console.read(cx);
            assert_eq!(this.buffer.size(), before);
            let prompt_bottom = this.bounds.top() + this.cell_height;
            assert!(popup.top() >= prompt_bottom && popup.top() <= prompt_bottom + px(8.0));
            assert!(popup.left() >= this.bounds.left() && popup.right() <= this.bounds.right());
            assert!(popup.bottom() <= this.bounds.bottom());
        });
        cx.simulate_resize(size(px(320.0), px(280.0)));
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                this.appearance.code_size = 24;
                this.buffer.feed(b"\x1b[999;999H");
                cx.notify();
            })
        });
        cx.update(|window, cx| {
            for _ in 0..3 {
                window.draw(cx).clear(cx);
            }
        });
        let popup = cx
            .debug_bounds("terminal-suggestion")
            .expect("List above bottom input");
        cx.update(|_, cx| {
            let this = console.read(cx);
            let prompt_top = this.bounds.top()
                + this.cell_height * this.buffer.term.grid().cursor.point.line.0 as f32;
            assert!(popup.bottom() <= prompt_top);
            assert!(popup.top() >= this.bounds.top());
            assert!(popup.left() >= this.bounds.left() && popup.right() <= this.bounds.right());
        });
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.suggestion_key("up", window, cx);
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("terminal-suggestion-row-5").is_some(),
            "Arrow navigation reveals the last row in a short pane"
        );
    }

    #[gpui_test::test]
    fn completion_does_not_capture_keys_during_search_composition_scrollback_or_fullscreen(
        cx: &mut TestAppContext,
    ) {
        let (console, cx) = scroll_fixture(cx);
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                this.suggestions.input.prompt();
                this.send(b"git", cx);
                this.buffer.feed(b"\x1b[?1049h");
                this.accept_suggestion(window, cx);
                assert_eq!(this.transmitted, [b"git".to_vec()]);
                this.buffer.feed(b"\x1b[?1049l");
                this.open_search(window, cx);
                assert!(!this.suggestion_visible());
                this.search.as_mut().unwrap().open = false;
                this.composition = "mock".into();
                assert!(!this.suggestion_visible());
                this.composition.clear();
                this.buffer.term.scroll_display(Scroll::Top);
                assert!(!this.suggestion_visible());
                this.buffer.term.scroll_display(Scroll::Bottom);
                this.appearance.command_suggestions = false;
                this.accept_suggestion(window, cx);
                assert_eq!(this.transmitted, [b"git".to_vec()]);
            })
        });
    }

    #[gpui_test::test]
    fn typed_ahead_input_blocks_completion_until_a_fresh_shell_prompt(cx: &mut TestAppContext) {
        use tervia::terminal::shell::Tracker;
        let (console, cx) = scroll_fixture(cx);
        cx.update(|_, cx| {
            console.update(cx, |this, cx| {
                let cwd = if cfg!(windows) {
                    r"C:\synthetic"
                } else {
                    "/synthetic"
                };
                let hex = |text: &str| {
                    text.as_bytes()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                };
                let marker = |body: String| format!("\x1b]777;tervia;mock-token;{body}\x07");
                this.suggestions.tracker = Some(Tracker::new("mock-token".into(), "zsh"));
                this.suggestions.input.prompt();
                this.send(b"sleep 1", cx);
                this.send(b"\r", cx);
                this.receive_output(
                    marker(format!("C;{};{}", hex(cwd), hex("sleep 1"))).as_bytes(),
                    cx,
                );
                this.send(b"git", cx);
                this.receive_output(marker("D;0".into()).as_bytes(), cx);
                this.receive_output(marker(format!("P;{}", hex(cwd))).as_bytes(), cx);
                assert!(!this.suggestion_visible());
                this.send(b"\r", cx);
                this.receive_output(
                    marker(format!("C;{};{}", hex(cwd), hex("git"))).as_bytes(),
                    cx,
                );
                this.receive_output(marker("D;0".into()).as_bytes(), cx);
                this.receive_output(marker(format!("P;{}", hex(cwd))).as_bytes(), cx);
                assert!(!this.suggestion_visible());
                this.send(b"git", cx);
                assert!(this.suggestion_visible());
            })
        });
    }

    #[gpui_test::test]
    fn terminal_reuses_glyphs_without_reusing_stale_colors_fonts_or_combining_text(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(900.0), px(500.0)));
                this.layout(bounds, window, cx);
                this.buffer.feed(b"aaaaaaaaaaaaaaaaaaaa");
                let first = this.layout(bounds, window, cx);
                let second = this.layout(bounds, window, cx);
                assert_eq!(first.text.len(), second.text.len());
                for ((a, origin_a), (b, origin_b)) in first.text.iter().zip(&second.text) {
                    assert_eq!(origin_a, origin_b);
                    assert!(std::sync::Arc::ptr_eq(a, b));
                }
                assert!(std::sync::Arc::ptr_eq(&first.text[0].0, &first.text[1].0));
                this.buffer.feed(b"\x1b]10;#ff0000\x07");
                let recolored = this.layout(bounds, window, cx);
                assert!(!std::sync::Arc::ptr_eq(
                    &first.text[0].0,
                    &recolored.text[0].0
                ));
                this.appearance.code_size += 2;
                let resized = this.layout(bounds, window, cx);
                assert!(!std::sync::Arc::ptr_eq(
                    &recolored.text[0].0,
                    &resized.text[0].0
                ));
                this.buffer
                    .feed("\r\n\x1b[1;3;4m合成😀e\u{301}\x1b[0m".as_bytes());
                let unicode = this.layout(bounds, window, cx);
                assert!(
                    unicode
                        .text
                        .iter()
                        .any(|(line, _)| line.text.as_ref() == "e\u{301}")
                );
                assert!(
                    unicode
                        .text
                        .iter()
                        .any(|(line, _)| line.text.as_ref() == "😀")
                );
            })
        });
    }

    #[gpui_test::test]
    #[ignore = "manual terminal layout benchmark"]
    fn terminal_layout_benchmark(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            console.update(cx, |this, cx| {
                let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1800.0), px(1200.0)));
                this.layout(bounds, window, cx);
                let (columns, rows) = this.buffer.size();
                let row: String = (0..columns)
                    .map(|column| (b'!' + (column % 90) as u8) as char)
                    .collect();
                this.buffer
                    .feed(format!("{row}\r\n").repeat(rows).as_bytes());
                for _ in 0..3 {
                    std::hint::black_box(this.layout(bounds, window, cx));
                }
                let mut times = Vec::new();
                let mut cells = 0;
                for _ in 0..60 {
                    let started = std::time::Instant::now();
                    let frame = this.layout(bounds, window, cx);
                    times.push(started.elapsed().as_secs_f64() * 1000.0);
                    cells = frame.text.len();
                    std::hint::black_box(frame);
                }
                times.sort_by(f64::total_cmp);
                println!(
                    "terminal_layout: median_ms={:.3} p95_ms={:.3} glyphs={cells} samples=60",
                    times[30], times[56]
                );
            })
        });
    }

    fn mock_console(cx: &mut Context<Console>) -> Console {
        Console {
            focus: cx.focus_handle(),
            cwd: "/mock-home".into(),
            agent: None,
            attention: false,
            activity: None,
            retired_epoch: None,
            agent_attention: false,
            search: None,
            suggestions: suggestions::State::new(tervia::suggestions::Worker::new(None)),
            buffer: TerminalBuffer::new(80, 24),
            glyphs: Default::default(),
            session: None,
            target: None,
            replaying: false,
            restored_scroll: 0,
            terminated: std::sync::Arc::default(),
            appearance: Preferences {
                language: Language::English,
                ..Preferences::default()
            },
            bounds: Bounds::default(),
            cell_width: px(8.0),
            cell_height: px(23.0),
            selecting: false,
            mouse_reporting: false,
            selection_scroll: None,
            scrollbar: scroll::TerminalScroll::default(),
            link_hover: false,
            link_pressed: false,
            composition: String::new(),
            status: None,
            cursor_on: true,
            _blink: None,
            _reader: None,
            _subscriptions: Vec::new(),
            transmitted: Vec::new(),
        }
    }

    #[gpui_test::test]
    fn terminal_tab_reaches_the_pty_instead_of_changing_ui_focus(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
        });
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            entity.read(cx).focus.clone().focus(window, cx);
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            assert_eq!(console.read(cx).transmitted, vec![vec![9]]);
            assert!(console.read(cx).focus.is_focused(window));
        });
        cx.update(|_, cx| {
            console.update(cx, |console, _| console.buffer.feed(b"\x1b[?1h"));
        });
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("ctrl-c");
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert_eq!(
                console.read(cx).transmitted,
                vec![vec![9], b"\x1bOA".to_vec(), vec![3], vec![27]]
            );
            console.update(cx, |console, _| {
                console.buffer.feed(b"copy-first\r\ncopy-second")
            });
        });
        let (bounds, width, height) = cx.update(|window, cx| {
            let _ = window.draw(cx);
            let console = console.read(cx);
            (console.bounds, console.cell_width, console.cell_height)
        });
        let from = bounds.origin + point(px(1.0), height * 0.5);
        let to = bounds.origin + point(width * 10.5, height * 1.5);
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        cx.simulate_keystrokes(Platform::CURRENT.copy_key());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "copy-first\ncopy-second"
            );
        });
    }

    #[gpui_test::test]
    fn native_menu_output_actions_follow_the_focused_split_pane(cx: &mut TestAppContext) {
        use super::super::Tervia;
        use tervia::state::Tab;

        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
            super::super::bind_menu_keys(cx);
        });
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut view = Tervia::new(window, cx);
                let project = view.state.open_directory("/synthetic/menu-output".into());
                view.init_project(project, window, cx);
                view.state.set_tab(Tab::Terminal);
                view.new_session(window, cx);
                view.state.select_session(0);
                view.state.toggle_split();
                view.ensure_console(0, window, cx).update(cx, |console, _| {
                    console.buffer.feed(b"synthetic-primary");
                });
                view.ensure_console(1, window, cx).update(cx, |console, _| {
                    console.buffer.feed(b"synthetic-secondary");
                });
                view
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        // GPUI dispatches focus events only for an active window.
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let second = cx.update(|_, cx| {
            view.read(cx).terminals[0][1]
                .as_ref()
                .unwrap()
                .read(cx)
                .bounds
        });
        cx.simulate_click(second.center(), Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(view.read(cx).state.command_session(), 1);
        });
        for key in ["copy_terminal", "clear_terminal"] {
            let action = super::super::menus(Language::English)
                .into_iter()
                .flat_map(|menu| menu.items)
                .find_map(|item| match item {
                    MenuItem::Action { name, action, .. }
                        if name.as_ref() == Language::English.text(key) =>
                    {
                        Some(action)
                    }
                    _ => None,
                })
                .unwrap();
            cx.update(|window, cx| window.dispatch_action(action, cx));
        }
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap().trim(),
                "synthetic-secondary"
            );
            assert_eq!(view.state.project().terminal.active_session, 0);
            assert_eq!(view.state.command_session(), 1);
            let primary = view.terminals[0][0].as_ref().unwrap().read(cx);
            let secondary = view.terminals[0][1].as_ref().unwrap().read(cx);
            assert_eq!(primary.output().trim(), "synthetic-primary");
            assert!(secondary.output().trim().is_empty());
            assert!(primary.transmitted.is_empty());
            assert_eq!(secondary.transmitted, vec![vec![12]]);
            assert!(secondary.focus.is_focused(window));
        });
    }

    #[gpui_test::test]
    fn clear_shortcut_removes_history_and_viewport_without_stealing_ctrl_k(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
        });
        let mut console = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(mock_console);
            entity.read(cx).focus.clone().focus(window, cx);
            console = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let console = console.unwrap();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
            console.update(cx, |console, _| {
                console
                    .buffer
                    .feed("mock-history\r\n".repeat(200).as_bytes());
                console.buffer.feed(b"mock-current\x1b[?1h");
                console.buffer.term.scroll_display(Scroll::Top);
                console.buffer.select(
                    CellPoint::new(Line(0), Column(0)),
                    CellPoint::new(Line(0), Column(4)),
                );
                assert!(console.buffer.term.history_size() > 0);
            });
        });
        cx.simulate_keystrokes(Platform::CURRENT.clear_key());
        cx.update(|window, cx| {
            let console = console.read(cx);
            assert!(console.output().trim().is_empty());
            assert_eq!(console.buffer.term.history_size(), 0);
            assert_eq!(console.buffer.term.grid().display_offset(), 0);
            assert!(console.buffer.selected_text().is_empty());
            assert!(console.buffer.term.mode().contains(TermMode::APP_CURSOR));
            assert_eq!(console.transmitted, vec![vec![12]]);
            assert!(console.focus.is_focused(window));
        });
        cx.simulate_keystrokes("ctrl-k");
        cx.update(|_, cx| {
            assert_eq!(console.read(cx).transmitted, vec![vec![12], vec![11]]);
        });
    }
}

impl Render for Console {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let p = Palette::new(self.appearance.theme);
        let search_bar = self.search_bar(cx);
        let suggestion_bar = self.suggestion_bar(cx);
        div()
            .id("native-console")
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .key_context("Console")
            .track_focus(&self.focus)
            .cursor_text()
            .when(self.link_hover, |surface| surface.cursor_pointer())
            .role(gpui_kit::accesskit::Role::TextInput)
            .aria_label(self.appearance.language.text("native_terminal"))
            .aria_value(self.buffer.visible_text())
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::clear_action))
            .on_action(cx.listener(|this, _: &Find, window, cx| this.open_search(window, cx)))
            .on_action(cx.listener(Self::send_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .children(search_bar)
            .children(self.agent_status_bar())
            .child(
                div()
                    .relative()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .debug_selector(|| "terminal-scrollbar".into())
                    .child(
                        canvas(
                            {
                                let entity = entity.clone();
                                move |bounds, window: &mut Window, cx: &mut App| {
                                    entity.update(cx, |this, cx| this.layout(bounds, window, cx))
                                }
                            },
                            move |bounds, frame: Frame, window: &mut Window, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.paint_selection_handlers(window, cx)
                                });
                                window.handle_input(
                                    &entity.read(cx).focus.clone(),
                                    ElementInputHandler::new(bounds, entity.clone()),
                                    cx,
                                );
                                for (bounds, color) in &frame.backgrounds {
                                    window.paint_quad(fill(*bounds, *color));
                                }
                                for (line, origin) in &frame.text {
                                    let _ = line.paint(
                                        *origin,
                                        frame.line_height,
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                                if let Some((bounds, color)) = frame.cursor {
                                    window.paint_quad(fill(bounds, color));
                                }
                            },
                        )
                        .size_full()
                        .min_h_0(),
                    )
                    .child(gpui_kit::component::scroll::Scrollbar::vertical(
                        &self.scrollbar,
                    ))
                    .children(suggestion_bar),
            )
            .children(self.status.as_ref().map(|status| {
                div()
                    .absolute()
                    .bottom(px(10.0))
                    .left(px(14.0))
                    .right(px(14.0))
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(Platform::CURRENT.hint(&status.text(self.appearance.language)))
            }))
    }
}

struct Frame {
    text: Vec<(Arc<ShapedLine>, Point<Pixels>)>,
    backgrounds: Vec<(Bounds<Pixels>, Hsla)>,
    cursor: Option<(Bounds<Pixels>, Hsla)>,
    line_height: Pixels,
}

impl Console {
    fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Frame {
        let old_geometry = (
            self.bounds,
            self.cell_width,
            self.cell_height,
            self.suggestions.viewport,
        );
        let p = Palette::new(self.appearance.theme);
        let font_size = px(self.appearance.code_size as f32);
        let font = Font {
            family: self.appearance.font_family.clone().into(),
            ..Default::default()
        };
        self.glyphs.configure(&font, font_size);
        self.cell_width = self.glyphs.cell_width(window);
        self.cell_height = px(self.appearance.code_size as f32 * 1.65);
        self.bounds = Bounds::new(
            bounds.origin + point(px(20.0), px(16.0)),
            size(
                (bounds.size.width - px(40.0)).max(px(2.0)),
                (bounds.size.height - px(32.0)).max(px(1.0)),
            ),
        );
        self.suggestions.viewport = bounds;
        let cols = (f32::from(self.bounds.size.width) / f32::from(self.cell_width))
            .floor()
            .max(2.0) as usize;
        let rows = (f32::from(self.bounds.size.height) / f32::from(self.cell_height))
            .floor()
            .max(1.0) as usize;
        if !self.replaying && self.buffer.size() != (cols, rows) {
            self.buffer.resize(cols, rows);
            if let Some(session) = &self.session
                && let Err(error) = session.resize(
                    cols,
                    rows,
                    f32::from(self.cell_width) as u16,
                    f32::from(self.cell_height) as u16,
                )
            {
                self.status = Some(Status::Resize(error));
            }
        }
        self.sync_scrollbar(bounds);
        // Overlay placement follows text metrics and pane bounds, without resizing the PTY.
        if self.suggestions.input.prefix().is_some()
            && old_geometry != (self.bounds, self.cell_width, self.cell_height, bounds)
        {
            cx.notify();
        }
        let content = self.buffer.term.renderable_content();
        let selection = content.selection;
        let offset = content.display_offset;
        let cursor = content.cursor;
        let mut frame = Frame {
            text: Vec::new(),
            backgrounds: Vec::new(),
            cursor: None,
            line_height: self.cell_height,
        };
        for indexed in content.display_iter {
            let cell = indexed.cell;
            let row = indexed.point.line.0 + offset as i32;
            if row < 0 || row >= rows as i32 {
                continue;
            }
            let origin = self.bounds.origin
                + point(
                    self.cell_width * indexed.point.column.0 as f32,
                    self.cell_height * row as f32,
                );
            let cell_bounds = Bounds::new(origin, size(self.cell_width, self.cell_height));
            let mut foreground = cell.fg;
            let mut background = cell.bg;
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if background != Color::Named(NamedColor::Background) {
                frame
                    .backgrounds
                    .push((cell_bounds, self.color(background)));
            }
            if selection
                .as_ref()
                .is_some_and(|selection| selection.contains(indexed.point))
            {
                frame
                    .backgrounds
                    .push((cell_bounds, p.accent.opacity(0.25)));
            }
            if cell.flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ) || cell.c == ' ' && cell.zerowidth().is_none()
            {
                continue;
            }
            let color = self
                .color(foreground)
                .opacity(if cell.flags.contains(Flags::DIM) {
                    0.65
                } else {
                    1.0
                });
            frame
                .text
                .push((self.glyphs.line(cell, color, window), origin));
        }
        if self.focus.is_focused(window)
            && (self.cursor_on || !self.buffer.term.cursor_style().blinking)
            && self.status.is_none()
            && self.buffer.term.mode().contains(TermMode::SHOW_CURSOR)
        {
            let row = cursor.point.line.0 + offset as i32;
            if row >= 0 && row < rows as i32 {
                let origin = self.bounds.origin
                    + point(
                        self.cell_width * cursor.point.column.0 as f32,
                        self.cell_height * row as f32,
                    );
                let cursor_height = font_size * 1.15;
                let cursor_origin =
                    origin + point(px(0.0), (self.cell_height - cursor_height) * 0.5);
                let cursor_color = self.color(Color::Named(NamedColor::Cursor));
                frame.cursor = match cursor.shape {
                    CursorShape::Hidden => None,
                    CursorShape::Beam => Some((
                        Bounds::new(cursor_origin, size(px(1.5), cursor_height)),
                        cursor_color,
                    )),
                    CursorShape::Underline => Some((
                        Bounds::new(
                            cursor_origin + point(px(0.0), cursor_height - px(2.0)),
                            size(self.cell_width, px(2.0)),
                        ),
                        cursor_color,
                    )),
                    _ => Some((
                        Bounds::new(cursor_origin, size(self.cell_width, cursor_height)),
                        cursor_color.opacity(0.45),
                    )),
                };
                if !self.composition.is_empty() {
                    let run = TextRun {
                        len: self.composition.len(),
                        font,
                        color: p.text,
                        background_color: Some(p.background),
                        underline: Some(UnderlineStyle {
                            color: Some(p.accent),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        strikethrough: None,
                    };
                    frame.text.push((
                        Arc::new(window.text_system().shape_line(
                            self.composition.clone().into(),
                            font_size,
                            &[run],
                            None,
                        )),
                        origin,
                    ));
                }
            }
        }
        let _ = cx;
        frame
    }
}

impl EntityInputHandler for Console {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *actual = Some(range.clone());
        Some(
            self.composition
                .encode_utf16()
                .skip(range.start)
                .take(range.end.saturating_sub(range.start))
                .collect::<Vec<_>>(),
        )
        .map(|text| String::from_utf16_lossy(&text))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.composition.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.composition.is_empty()).then(|| 0..self.composition.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.composition.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composition.clear();
        self.send(text.as_bytes(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composition = text.to_owned();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let cursor = self.buffer.term.grid().cursor.point;
        Some(Bounds::new(
            self.bounds.origin
                + point(
                    self.cell_width * cursor.column.0 as f32,
                    self.cell_height * cursor.line.0 as f32,
                ),
            size(self.cell_width, self.cell_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}
