#![cfg(feature = "ui-tests")]

extern crate gpui_test as gpui;

use gpui_test::{
    AppContext, Context, IntoElement, ListAlignment, ListOffset, ListState, Render, ScrollDelta,
    ScrollWheelEvent, Styled, TestAppContext, Window, div, list, point, px, size,
};
use std::{cell::Cell, rc::Rc};

struct ListView {
    state: ListState,
    height: Rc<Cell<f32>>,
    rendered: Rc<Cell<usize>>,
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height.get();
        let rendered = self.rendered.clone();
        list(self.state.clone(), move |_, _, _| {
            rendered.set(rendered.get() + 1);
            div().h(px(height)).w_full().into_any_element()
        })
        .w_full()
        .h_full()
    }
}

#[gpui_test::test]
fn uniform_height_scrolls_on_first_layout(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let state = ListState::new(100, ListAlignment::Top, px(0.)).with_uniform_item_height(px(20.));
    let rendered = Rc::new(Cell::new(0));
    let view = cx.update(|_, cx| {
        cx.new(|_| ListView {
            state: state.clone(),
            height: Rc::new(Cell::new(20.)),
            rendered: rendered.clone(),
        })
    });

    cx.draw(point(px(0.), px(0.)), size(px(400.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    assert_eq!(state.max_offset_for_scrollbar().y, px(1800.));
    assert_eq!(rendered.get(), 10, "Only visible rows are rendered");
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(100.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
        ..Default::default()
    });
    assert_eq!(state.scroll_px_offset_for_scrollbar().y, px(-100.));
}

#[gpui_test::test]
fn uniform_height_survives_width_change_and_remeasures_rows(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let state = ListState::new(100, ListAlignment::Top, px(0.)).with_uniform_item_height(px(20.));
    let height = Rc::new(Cell::new(40.));
    let rendered = Rc::new(Cell::new(0));
    let view = cx.update(|_, cx| {
        cx.new(|_| ListView {
            state: state.clone(),
            height: height.clone(),
            rendered: rendered.clone(),
        })
    });
    cx.draw(point(px(0.), px(0.)), size(px(400.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    state.scroll_to(ListOffset {
        item_ix: 10,
        offset_in_item: px(0.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(400.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    height.set(20.);
    rendered.set(0);
    cx.draw(point(px(0.), px(0.)), size(px(200.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    assert_eq!(state.max_offset_for_scrollbar().y, px(1800.));
    assert_eq!(state.bounds_for_item(10).unwrap().size.height, px(20.));
    assert_eq!(rendered.get(), 10, "Resizing keeps rendering virtualized");
    assert_eq!(state.scroll_px_offset_for_scrollbar().y, px(-200.));
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(100.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
        ..Default::default()
    });
    assert_eq!(state.scroll_px_offset_for_scrollbar().y, px(-300.));
}
