use std::{ops::Range, path::PathBuf, time::Duration};

use alacritty_terminal::{
    event::{Event, WindowSize},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point as CellPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags},
    vte::ansi::{ClearMode, Color, CursorShape, Handler, NamedColor, Rgb},
};
use gpui_kit::*;
use portable_pty::PtySize;
use tervia::{
    agent::Agent,
    locale::Language,
    platform::Platform,
    preferences::Preferences,
    pty::{PtyEvent, PtySession, shell_command},
    terminal::{TerminalBuffer, key_bytes, mouse_bytes, paste_bytes},
};

use super::theme::Palette;

gpui_kit::actions!(console, [CopySelection, Paste, Escape, SelectAll, Clear]);

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

pub fn init(cx: &mut App) {
    cx.bind_keys([
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
}

pub struct Console {
    pub focus: FocusHandle,
    pub cwd: PathBuf,
    pub agent: Option<Agent>,
    buffer: TerminalBuffer,
    session: Option<PtySession>,
    appearance: Preferences,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    selecting: bool,
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
    Exited(String),
    Connection(String),
    Spawn(String),
    Input(String),
    Resize(String),
}

impl Status {
    fn text(&self, language: Language) -> String {
        match self {
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
    pub fn new(
        cwd: PathBuf,
        preferences: Entity<Preferences>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = preferences.read(cx).clone();
        let mut console = Self {
            focus: cx.focus_handle(),
            cwd: cwd.clone(),
            agent: None,
            buffer: TerminalBuffer::new(80, 24),
            session: None,
            appearance,
            bounds: Bounds::default(),
            cell_width: px(8.0),
            cell_height: px(23.0),
            selecting: false,
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
                this.appearance = preferences.read(cx).clone();
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
                        let agent = this.session.as_ref().and_then(PtySession::foreground_agent);
                        if this.agent != agent {
                            this.agent = agent;
                            cx.emit(IdentityChanged);
                            cx.notify();
                        }
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
            match PtySession::spawn(
                shell_command(&cwd),
                PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                },
            ) {
                Ok((session, receiver)) => {
                    console.session = Some(session);
                    console._reader = Some(cx.spawn(async move |weak, cx| {
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
                                                this.buffer.feed(&bytes);
                                                this.handle_events(cx);
                                            }
                                            PtyEvent::Exited(status) => {
                                                this.status = Some(Status::Exited(status));
                                                this.session.take();
                                            }
                                            PtyEvent::Error(error) => {
                                                this.status = Some(Status::Connection(error));
                                            }
                                        }
                                    }
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
                Err(error) => console.status = Some(Status::Spawn(error)),
            }
        }
        console
    }

    pub fn shutdown(&mut self) {
        self.session.take();
        self._reader.take();
    }

    fn focus_changed(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.buffer.term.is_focused = focused;
        if focused {
            cx.emit(Focused);
        }
        if self.buffer.term.mode().contains(TermMode::FOCUS_IN_OUT)
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
    }

    fn handle_events(&mut self, cx: &mut Context<Self>) {
        for event in self.buffer.events() {
            match event {
                Event::PtyWrite(reply) => {
                    if let Some(session) = &self.session {
                        let _ = session.write(reply.as_bytes());
                    }
                }
                Event::ClipboardStore(_, text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text))
                }
                // OSC 52 queries must not disclose the host clipboard to an arbitrary shell program.
                Event::ClipboardLoad(_, _) => {}
                Event::ColorRequest(index, formatter) => {
                    let color = self.index_color(index);
                    if let Some(session) = &self.session {
                        let _ = session.write(formatter(color).as_bytes());
                    }
                }
                Event::TextAreaSizeRequest(formatter) => {
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
        self.send(b"\x1b", cx);
    }

    fn send_key(&mut self, key: &SendKey, _: &mut Window, cx: &mut Context<Self>) {
        if !self.composition.is_empty() {
            cx.propagate();
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

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.modifiers.platform || !self.composition.is_empty() {
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

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        if self.buffer.term.mode().intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
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
        self.buffer.term.selection = Some(Selection::new(
            ty,
            self.grid_point(event.position),
            Side::Left,
        ));
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let point = self.grid_point(event.position);
            if let Some(selection) = self.buffer.term.selection.as_mut() {
                selection.update(point, Side::Right);
            }
            cx.notify();
        } else if event.pressed_button == Some(MouseButton::Left)
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
        if !self.selecting
            && self.buffer.term.mode().intersects(TermMode::MOUSE_MODE)
            && !event.modifiers.shift
        {
            let (column, row) = self.viewport_point(event.position);
            self.send(
                &mouse_bytes(0, column, row, true, *self.buffer.term.mode()),
                cx,
            );
        }
        self.selecting = false;
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

    fn index_color(&self, index: usize) -> Rgb {
        if let Some(color) = self.buffer.term.colors()[index] {
            return color;
        }
        let p = Palette::new(self.appearance.light_theme);
        let color = match index {
            256 => p.text.to_rgb(),
            257 => p.background.to_rgb(),
            258 => p.text.to_rgb(),
            0..=15 => {
                let palette = if self.appearance.light_theme {
                    [
                        0x262a28, 0x92343e, 0x205f37, 0x805600, 0x31578d, 0x783d87, 0x1f626d,
                        0x485662, 0x495763, 0x9a3747, 0x235e48, 0x875b00, 0x365b9d, 0x7a368c,
                        0x1a6977, 0x262a28,
                    ]
                } else {
                    [
                        0x303341, 0xe69b9f, 0x99cfa6, 0xe9cd97, 0x9fbfe5, 0xcbb0e8, 0x96ccd1,
                        0xdde1e8, 0x929cad, 0xf1a9ad, 0xa8d9b5, 0xf1d7a9, 0xb4d0f1, 0xd7bff0,
                        0xabe0e2, 0xf4f5f7,
                    ]
                };
                rgb(palette[index])
            }
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

    fn mock_console(cx: &mut Context<Console>) -> Console {
        Console {
            focus: cx.focus_handle(),
            cwd: "/mock-home".into(),
            agent: None,
            buffer: TerminalBuffer::new(80, 24),
            session: None,
            appearance: Preferences {
                language: Language::English,
                ..Preferences::default()
            },
            bounds: Bounds::default(),
            cell_width: px(8.0),
            cell_height: px(23.0),
            selecting: false,
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
            assert_eq!(view.state.project().active_session, 0);
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
        let p = Palette::new(self.appearance.light_theme);
        div()
            .id("native-console")
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .key_context("Console")
            .track_focus(&self.focus)
            .cursor_text()
            .role(gpui_kit::accesskit::Role::TextInput)
            .aria_label(self.appearance.language.text("native_terminal"))
            .aria_value(self.buffer.visible_text())
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::clear_action))
            .on_action(cx.listener(Self::send_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(
                canvas(
                    {
                        let entity = entity.clone();
                        move |bounds, window: &mut Window, cx: &mut App| {
                            entity.update(cx, |this, cx| this.layout(bounds, window, cx))
                        }
                    },
                    move |bounds, frame: Frame, window: &mut Window, cx: &mut App| {
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
                .size_full(),
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
    text: Vec<(ShapedLine, Point<Pixels>)>,
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
        let p = Palette::new(self.appearance.light_theme);
        let font_size = px(self.appearance.code_size as f32);
        let font = Font {
            family: self.appearance.font_family.clone().into(),
            ..Default::default()
        };
        let run = TextRun {
            len: 1,
            font: font.clone(),
            color: p.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        self.cell_width = window
            .text_system()
            .shape_line("M".into(), font_size, &[run], None)
            .width
            .max(px(1.0));
        self.cell_height = px(self.appearance.code_size as f32 * 1.65);
        self.bounds = Bounds::new(
            bounds.origin + point(px(20.0), px(16.0)),
            size(
                (bounds.size.width - px(40.0)).max(px(2.0)),
                (bounds.size.height - px(32.0)).max(px(1.0)),
            ),
        );
        let cols = (f32::from(self.bounds.size.width) / f32::from(self.cell_width))
            .floor()
            .max(2.0) as usize;
        let rows = (f32::from(self.bounds.size.height) / f32::from(self.cell_height))
            .floor()
            .max(1.0) as usize;
        if self.buffer.size() != (cols, rows) {
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
            let mut text = cell.c.to_string();
            if let Some(combining) = cell.zerowidth() {
                text.extend(combining);
            }
            let color = self
                .color(foreground)
                .opacity(if cell.flags.contains(Flags::DIM) {
                    0.65
                } else {
                    1.0
                });
            let run = TextRun {
                len: text.len(),
                font: Font {
                    weight: if cell.flags.contains(Flags::BOLD) {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    },
                    style: if cell.flags.contains(Flags::ITALIC) {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    },
                    ..font.clone()
                },
                color,
                background_color: None,
                underline: cell
                    .flags
                    .intersects(Flags::ALL_UNDERLINES)
                    .then_some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                strikethrough: None,
            };
            frame.text.push((
                window
                    .text_system()
                    .shape_line(text.into(), font_size, &[run], None),
                origin,
            ));
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
                        window.text_system().shape_line(
                            self.composition.clone().into(),
                            font_size,
                            &[run],
                            None,
                        ),
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
