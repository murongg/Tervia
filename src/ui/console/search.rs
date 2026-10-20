use super::super::controls::icon_button;
use super::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};

pub(super) struct Search {
    pub input: Entity<InputState>,
    pub open: bool,
    matcher: tervia::terminal::Search,
    matches: Vec<alacritty_terminal::term::search::Match>,
    active: usize,
}

impl Console {
    pub(in crate::ui) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_none() {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(self.appearance.language.text("terminal_search"))
            });
            self._subscriptions.push(cx.subscribe_in(
                &input,
                window,
                |this, input, event, _, cx| match event {
                    InputEvent::Change => {
                        this.search.as_mut().unwrap().matcher =
                            tervia::terminal::Search::new(input.read(cx).value().as_ref());
                        this.refresh_search(cx);
                    }
                    InputEvent::PressEnter { shift, .. } => this.navigate_match(*shift, cx),
                    _ => {}
                },
            ));
            self.search = Some(Search {
                input,
                open: true,
                matcher: tervia::terminal::Search::new(""),
                matches: Vec::new(),
                active: 0,
            });
        }
        self.search.as_mut().unwrap().open = true;
        self.refresh_search(cx);
        self.search
            .as_ref()
            .unwrap()
            .input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        if !search.open {
            return;
        }
        let matches = search.matcher.find(&self.buffer);
        if matches != search.matches {
            search.matches = matches;
            search.active = search.active.min(search.matches.len().saturating_sub(1));
            if let Some(found) = search.matches.get(search.active) {
                self.buffer.reveal_match(found);
            } else {
                self.buffer.term.selection = None;
            }
        }
        cx.notify();
    }

    fn navigate_match(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        if search.matches.is_empty() {
            return;
        }
        search.active = if backwards {
            (search.active + search.matches.len() - 1) % search.matches.len()
        } else {
            (search.active + 1) % search.matches.len()
        };
        self.buffer.reveal_match(&search.matches[search.active]);
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &mut self.search {
            search.open = false;
        }
        self.buffer.term.selection = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let search = self.search.as_ref().filter(|search| search.open)?;
        let p = Palette::new(self.appearance.theme);
        let language = self.appearance.language;
        let count = search.matches.len();
        let position = if count == 0 { 0 } else { search.active + 1 };
        Some(
            div()
                .debug_selector(|| "terminal-search".into())
                .key_context("ConsoleSearch")
                .h(px(40.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(12.0))
                .gap(px(5.0))
                .border_b_1()
                .border_color(p.border)
                .on_action(cx.listener(|this, _: &CloseSearch, window, cx| {
                    this.close_search(window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &NextMatch, _, cx| {
                    this.navigate_match(false, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &PreviousMatch, _, cx| {
                    this.navigate_match(true, cx);
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&search.input).h(px(28.0)).w_full()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.secondary)
                        .text_size(px(11.0))
                        .child(format!(
                            "{position}/{count}{}",
                            if count == 5000 { "+" } else { "" }
                        )),
                )
                .child(
                    icon_button(
                        "terminal-search-previous",
                        gpui_kit::assets::IconName::ArrowUp,
                        language.text("search_previous"),
                        p,
                        cx,
                    )
                    .accessibility_label(language.text("search_previous"))
                    .on_click(cx.listener(|this, _, _, cx| this.navigate_match(true, cx))),
                )
                .child(
                    icon_button(
                        "terminal-search-next",
                        gpui_kit::assets::IconName::ArrowDown,
                        language.text("search_next"),
                        p,
                        cx,
                    )
                    .accessibility_label(language.text("search_next"))
                    .on_click(cx.listener(|this, _, _, cx| this.navigate_match(false, cx))),
                )
                .child(
                    icon_button(
                        "terminal-search-close",
                        gpui_kit::assets::IconName::X,
                        language.text("close"),
                        p,
                        cx,
                    )
                    .accessibility_label(language.text("close"))
                    .on_click(cx.listener(|this, _, window, cx| this.close_search(window, cx))),
                )
                .into_any_element(),
        )
    }
}
