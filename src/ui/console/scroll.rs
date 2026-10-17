use std::{cell::RefCell, rc::Rc};

use gpui_kit::component::scroll::ScrollbarHandle;

use super::*;

#[derive(Clone, Default)]
pub(super) struct TerminalScroll(Rc<RefCell<ScrollState>>);

#[derive(Default)]
struct ScrollState {
    bounds: Bounds<Pixels>,
    content: Size<Pixels>,
    offset: Point<Pixels>,
    requested: Option<Point<Pixels>>,
}

impl ScrollbarHandle for TerminalScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().bounds
    }

    fn content_size(&self) -> Size<Pixels> {
        self.0.borrow().content
    }

    fn offset(&self) -> Point<Pixels> {
        let state = self.0.borrow();
        state.requested.unwrap_or(state.offset)
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.0.borrow_mut().requested = Some(offset);
    }
}

impl Console {
    pub(super) fn sync_scrollbar(&mut self, bounds: Bounds<Pixels>) {
        let history = self.buffer.term.history_size();
        let mut scroll = self.scrollbar.0.borrow_mut();
        if let Some(requested) = scroll.requested.take() {
            // GPUI measures from the history top; Alacritty counts back from the live screen.
            let from_top = (-f32::from(requested.y) / f32::from(self.cell_height))
                .round()
                .clamp(0.0, history as f32) as usize;
            let target = history - from_top;
            let current = self.buffer.term.grid().display_offset();
            self.buffer
                .term
                .scroll_display(Scroll::Delta(target as i32 - current as i32));
        }
        scroll.bounds = bounds;
        scroll.content = size(
            bounds.size.width,
            bounds.size.height + self.cell_height * history as f32,
        );
        scroll.offset = point(
            px(0.0),
            -self.cell_height * (history - self.buffer.term.grid().display_offset()) as f32,
        );
    }

    pub(super) fn stop_selecting(&mut self) {
        self.selecting = false;
        self.selection_scroll.take();
    }

    pub(super) fn update_selection(&mut self, position: Point<Pixels>) {
        let (point, side) = self.selection_anchor(position);
        if let Some(selection) = self.buffer.term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    fn selection_scroll_delta(&self, position: Point<Pixels>) -> i32 {
        let distance = if position.y < self.bounds.top() {
            f32::from(self.bounds.top() - position.y)
        } else if position.y >= self.bounds.bottom() {
            -f32::from(position.y - self.bounds.bottom()) - 1.0
        } else {
            return 0;
        };
        let speed = (distance.abs() / f32::from(self.cell_height)).floor() as i32 + 1;
        speed.min(10) * if distance > 0.0 { 1 } else { -1 }
    }

    pub(super) fn start_selection_scroll(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection_scroll.is_some() || self.selection_scroll_delta(position) == 0 {
            return;
        }
        // Holding outside the pane must extend selection even when no new mouse moves arrive.
        self.selection_scroll = Some(cx.spawn_in(window, async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if weak
                    .update_in(cx, |this, window, cx| {
                        let position = window.mouse_position();
                        let delta = this.selection_scroll_delta(position);
                        if delta != 0 {
                            this.buffer.term.scroll_display(Scroll::Delta(delta));
                            // Recompute after scrolling: the same pointer now addresses a different history line.
                            this.update_selection(position);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(super) fn paint_selection_handlers(&self, window: &mut Window, cx: &Context<Self>) {
        let drag = cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            if this.selecting {
                this.mouse_move(event, window, cx);
                cx.stop_propagation();
            }
        });
        let release = cx.listener(|this, event: &MouseUpEvent, window, cx| {
            if event.button == MouseButton::Left
                && (this.selecting || this.link_pressed || this.mouse_reporting)
            {
                this.mouse_up(event, window, cx);
                cx.stop_propagation();
            }
        });
        // Element hover listeners stop firing outside the pane, but a selection owns the whole drag.
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                drag(event, window, cx);
            }
        });
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                release(event, window, cx);
            }
        });
    }
}
