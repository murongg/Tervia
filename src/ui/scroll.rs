use gpui_kit::base::StyledExt as _;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle};
use gpui_kit::*;

pub(super) fn overlay<H: ScrollbarHandle + Clone>(handle: &H, axis: ScrollbarAxis) -> Div {
    div()
        .absolute()
        .inset_0()
        .debug_selector(|| "scrollbar-overlay".into())
        .child(Scrollbar::new(handle).axis(axis).viewport_from_layout())
}

pub(super) trait ListScrollbars {
    fn scrollbar(self, handle: &UniformListScrollHandle) -> Div;
}

impl ListScrollbars for UniformList {
    fn scrollbar(mut self, handle: &UniformListScrollHandle) -> Div {
        let style = self.style();
        // The wrapper takes the list's place in its parent's layout; virtualization stays on the list.
        let layout = StyleRefinement {
            size: style.size.clone(),
            min_size: style.min_size.clone(),
            max_size: style.max_size.clone(),
            flex_grow: style.flex_grow,
            flex_shrink: style.flex_shrink,
            flex_basis: style.flex_basis,
            align_self: style.align_self,
            ..Default::default()
        };
        div()
            .refine_style(&layout)
            .relative()
            .child(self.size_full().flex_none().track_scroll(handle))
            .child(overlay(handle, ScrollbarAxis::Vertical))
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::{cell::Cell, rc::Rc};

    struct ListFixture {
        handle: UniformListScrollHandle,
        rendered: Rc<Cell<usize>>,
    }

    impl Render for ListFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.rendered.clone();
            div()
                .w(px(240.0))
                .h(px(200.0))
                .flex()
                .flex_col()
                .child(div().h(px(40.0)).flex_shrink_0().child("Synthetic header"))
                .child(
                    uniform_list("synthetic-list", 500, move |range, _, _| {
                        rendered.set(rendered.get() + range.len());
                        range
                            .map(|row| div().h(px(24.0)).child(format!("Synthetic row {row}")))
                            .collect()
                    })
                    .flex_1()
                    .min_h_0()
                    .scrollbar(&self.handle),
                )
        }
    }

    #[gpui_test::test]
    fn virtual_list_scrollbar_respects_layout_and_drags_without_rendering_all_rows(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::preferences::apply(&tervia::preferences::Preferences::default(), cx);
        });
        let handle = UniformListScrollHandle::new();
        let rendered = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view(|_, _| ListFixture {
            handle: handle.clone(),
            rendered: rendered.clone(),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx
            .debug_bounds("scrollbar-overlay")
            .expect("List scrollbar");
        assert_eq!(
            bounds.top(),
            px(40.0),
            "The scrollbar must exclude the fixed header"
        );
        assert_eq!(bounds.size.height, px(160.0));
        assert!(rendered.get() < 40, "Wrapping must preserve virtualization");
        cx.simulate_event(ScrollWheelEvent {
            position: bounds.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-24.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let top = point(bounds.right() - px(8.0), bounds.top() + px(24.0));
        let bottom = point(top.x, bounds.bottom() - px(24.0));
        cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(bottom, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::default());
        assert!(handle.offset().y < px(0.0));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(bottom, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(top, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(top, MouseButton::Left, Modifiers::default());
        assert_eq!(handle.offset().y, px(0.0));
    }
}
