use super::*;
use gpui_kit::base::{ElementExt as _, resize_handle};
use gpui_kit::component::resizable::resize_handle_appearance_without_line;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct State(Rc<RefCell<Layout>>);

impl State {
    pub(super) fn ends_with_header(&self) -> bool {
        let layout = self.0.borrow();
        layout.available > 0.0
            && layout
                .slots
                .len()
                .checked_sub(1)
                .is_some_and(|index| layout.header_at_end(index))
            && (layout.available - layout.slots.iter().map(|slot| slot.height).sum::<f32>()).abs()
                <= 1.0
    }
}

#[derive(Clone, Copy)]
pub(super) enum HeaderBorder {
    Top,
    Bottom(Hsla),
}

struct Slot {
    key: usize,
    folded: bool,
    height: f32,
}

#[derive(Default)]
struct Layout {
    slots: Vec<Slot>,
    saved: BTreeMap<usize, f32>,
    header: f32,
    available: f32,
    top: f32,
}

impl Layout {
    fn header_at_end(&self, index: usize) -> bool {
        // A subpixel body shares the header's border after device-pixel rounding.
        self.slots
            .get(index)
            .is_some_and(|slot| slot.height <= self.header + 1.0)
    }

    fn sync(&mut self, sections: &[(usize, bool)], header: f32) {
        if self.header == header
            && self.slots.len() == sections.len()
            && self
                .slots
                .iter()
                .zip(sections)
                .all(|(slot, (key, folded))| slot.key == *key && slot.folded == *folded)
        {
            return;
        }
        // Only record a section while it is expanded. Resizing its siblings while
        // it is folded must never replace the height we will restore with a header.
        for slot in &self.slots {
            if !slot.folded && !sections.contains(&(slot.key, false)) {
                self.saved.insert(slot.key, slot.height);
            }
        }
        self.header = header;
        let count = sections.iter().filter(|(_, folded)| !folded).count();
        let initial = header
            + (self.available - header * sections.len() as f32).max(0.0) / count.max(1) as f32;
        let mut changed = None;
        let slots = sections
            .iter()
            .map(|(key, folded)| {
                let previous = self.slots.iter().find(|slot| slot.key == *key);
                if previous.is_none_or(|slot| slot.folded != *folded) {
                    changed = Some(*key);
                }
                let height = if *folded {
                    header
                } else if let Some(previous) = previous.filter(|slot| !slot.folded) {
                    previous.height
                } else {
                    self.saved.get(key).copied().unwrap_or(initial)
                };
                Slot {
                    key: *key,
                    folded: *folded,
                    height,
                }
            })
            .collect();
        self.slots = slots;
        self.fit(changed);
    }

    fn minimum(&self) -> f32 {
        let count = self.slots.iter().filter(|slot| !slot.folded).count();
        self.header
            + ((self.available - self.header * self.slots.len() as f32).max(0.0)
                / count.max(1) as f32)
                .min(32.0)
    }

    fn fit(&mut self, restored: Option<usize>) {
        if self.available <= 0.0 {
            return;
        }
        let minimum = self.minimum();
        for slot in &mut self.slots {
            slot.height = if slot.folded {
                self.header
            } else {
                slot.height.max(minimum)
            };
        }
        let pivot = restored.and_then(|key| self.slots.iter().position(|slot| slot.key == key));
        let order: Vec<_> = match pivot {
            Some(pivot) => ((pivot + 1)..self.slots.len())
                .chain((0..pivot).rev())
                .chain(std::iter::once(pivot))
                .collect(),
            None => (0..self.slots.len()).rev().collect(),
        };
        let mut remaining = self.available - self.slots.iter().map(|slot| slot.height).sum::<f32>();
        // Prefer siblings before reducing a just-reopened section's remembered height.
        for index in order {
            let slot = &mut self.slots[index];
            if slot.folded {
                continue;
            }
            let change = remaining.max(minimum - slot.height);
            slot.height += change;
            remaining -= change;
            if remaining.abs() < 0.01 {
                break;
            }
        }
    }

    fn resize(&mut self, height: f32) -> bool {
        if (self.available - height).abs() < 0.01 {
            return false;
        }
        let headers = self.header * self.slots.len() as f32;
        let old_body = (self.available - headers).max(0.0);
        let body = (height - headers).max(0.0);
        let count = self.slots.iter().filter(|slot| !slot.folded).count();
        for slot in &mut self.slots {
            if !slot.folded {
                slot.height = self.header
                    + if old_body > 0.0 {
                        (slot.height - self.header).max(0.0) * body / old_body
                    } else {
                        body / count.max(1) as f32
                    };
            }
        }
        self.available = height;
        self.fit(None);
        true
    }

    fn neighbors(&self, boundary: usize) -> Option<(usize, usize)> {
        let upper = self.slots[..boundary]
            .iter()
            .rposition(|slot| !slot.folded)?;
        let lower = boundary
            + self.slots[boundary..]
                .iter()
                .position(|slot| !slot.folded)?;
        Some((upper, lower))
    }

    fn drag(&mut self, boundary: usize, heights: &[f32], delta: f32) {
        // A viewport change invalidates the drag's starting sizes and limits.
        if heights.len() != self.slots.len()
            || (heights.iter().sum::<f32>() - self.available).abs() > 0.1
        {
            return;
        }
        let Some((upper, lower)) = self.neighbors(boundary) else {
            return;
        };
        let minimum = self.minimum();
        let lower_limit = minimum - heights[upper];
        let upper_limit = heights[lower] - minimum;
        if lower_limit > upper_limit {
            return;
        }
        let delta = delta.clamp(lower_limit, upper_limit);
        self.slots[upper].height = heights[upper] + delta;
        self.slots[lower].height = heights[lower] - delta;
    }
}

#[derive(Clone)]
struct Drag {
    state: State,
    boundary: usize,
    heights: Vec<f32>,
    origin: f32,
}

impl Render for Drag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(super) fn panel(header: AnyElement, body: Option<Div>) -> Div {
    div()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(header)
        .children(body)
}

pub(super) fn stack(
    id: String,
    state: &State,
    header: f32,
    header_border: HeaderBorder,
    panels: Vec<(usize, bool, Div)>,
) -> Div {
    let sections: Vec<_> = panels
        .iter()
        .map(|(key, folded, _)| (*key, *folded))
        .collect();
    state.0.borrow_mut().sync(&sections, header);
    let layout = state.0.borrow();
    let measured = layout.available > 0.0;
    let heights: Vec<_> = layout.slots.iter().map(|slot| slot.height).collect();
    let children: Vec<_> = panels
        .into_iter()
        .enumerate()
        .map(|(index, (key, folded, panel))| {
            // Headers own their borders. Fill uncovered seams even when no resize is possible.
            let seam = match header_border {
                HeaderBorder::Bottom(color)
                    if index > 0
                        && if measured {
                            !layout.header_at_end(index - 1)
                        } else {
                            !sections[index - 1].1
                        } =>
                {
                    Some(color)
                }
                _ => None,
            };
            let drag = Drag {
                state: state.clone(),
                boundary: index,
                heights: heights.clone(),
                origin: layout.top + heights[..index].iter().sum::<f32>(),
            };
            div()
                .id(format!("{id}-panel-{key}"))
                .relative()
                .w_full()
                .flex_none()
                .when(measured || folded, |panel| panel.h(px(heights[index])))
                .when(!measured && !folded, |panel| {
                    panel.flex_1().min_h(px(header))
                })
                .child(panel)
                .when_some(seam, |panel, color| {
                    panel.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .w_full()
                            .h(px(1.0))
                            .bg(color),
                    )
                })
                .when(layout.neighbors(index).is_some(), |panel| {
                    panel.child(
                        resize_handle(("section-divider", key), Axis::Vertical)
                            .with_appearance(resize_handle_appearance_without_line())
                            .on_drag(drag, |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| (*drag).clone())
                            }),
                    )
                })
        })
        .collect();
    drop(layout);
    let moving = state.clone();
    let measuring = state.clone();
    // Section headers stay visible when an adjacent form needs to scroll.
    div()
        .flex_1()
        .min_h(px(header * sections.len() as f32))
        .child(
            div()
                .id(id)
                .relative()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .children(children)
                .child(div().flex_1().min_h_0())
                // The native handle stores its payload in Rc before starting GPUI's drag.
                .on_drag_move::<Rc<Drag>>(move |event, window, cx| {
                    let drag = event.drag(cx);
                    if Rc::ptr_eq(&moving.0, &drag.state.0) {
                        moving.0.borrow_mut().drag(
                            drag.boundary,
                            &drag.heights,
                            f32::from(event.event.position.y) - drag.origin,
                        );
                        window.refresh();
                    }
                })
                .on_prepaint(move |bounds, window, cx| {
                    let mut layout = measuring.0.borrow_mut();
                    layout.top = f32::from(bounds.top());
                    if layout.resize(f32::from(bounds.size.height)) {
                        window.defer(cx, |window, _| window.refresh());
                    }
                }),
        )
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn heights(layout: &Layout) -> Vec<f32> {
        layout.slots.iter().map(|slot| slot.height).collect()
    }

    #[test]
    fn collapsing_and_reopening_restores_neighbor_heights() {
        let mut layout = Layout::default();
        layout.sync(&[(1, false), (3, false), (2, false)], 34.0);
        layout.resize(600.0);
        layout.drag(1, &heights(&layout), 40.0);
        let before = heights(&layout);
        layout.sync(&[(1, true), (3, false), (2, false)], 34.0);
        assert_eq!(
            layout.slots[2].height, before[2],
            "The nearest expanded sibling receives the released space"
        );
        layout.sync(&[(1, false), (3, false), (2, false)], 34.0);
        assert_eq!(heights(&layout), before);
    }

    #[test]
    fn folded_height_survives_sibling_drags_and_container_resizing() {
        let mut layout = Layout::default();
        layout.sync(&[(1, false), (3, false), (2, false)], 34.0);
        layout.resize(600.0);
        layout.drag(1, &heights(&layout), 40.0);
        let remembered = layout.slots[0].height;
        layout.sync(&[(1, true), (3, false), (2, false)], 34.0);
        layout.drag(2, &heights(&layout), 30.0);
        layout.resize(900.0);
        layout.sync(&[(1, false), (3, false), (2, false)], 34.0);
        assert_eq!(layout.slots[0].height, remembered);
        assert_eq!(heights(&layout).iter().sum::<f32>(), 900.0);
    }

    #[test]
    fn dragging_skips_folded_sections_and_keeps_all_headers_readable() {
        let mut layout = Layout::default();
        layout.sync(&[(1, false), (3, true), (2, false)], 34.0);
        layout.resize(600.0);
        for delta in [10000.0, -10000.0] {
            layout.drag(1, &heights(&layout), delta);
            assert_eq!(layout.slots[1].height, 34.0);
            assert!(layout.slots[0].height >= 66.0);
            assert!(layout.slots[2].height >= 66.0);
            assert_eq!(heights(&layout).iter().sum::<f32>(), 600.0);
        }
        layout.resize(130.0);
        assert!(layout.slots.iter().all(|slot| slot.height >= 34.0));
        assert!((heights(&layout).iter().sum::<f32>() - 130.0).abs() < 0.01);
    }

    #[test]
    fn a_window_resize_invalidates_an_in_progress_drag() {
        let mut layout = Layout::default();
        layout.sync(&[(1, false), (3, false), (2, false)], 34.0);
        layout.resize(110.0);
        let start = heights(&layout);
        layout.resize(600.0);
        let resized = heights(&layout);
        layout.drag(1, &start, 20.0);
        assert_eq!(heights(&layout), resized);
    }
}
