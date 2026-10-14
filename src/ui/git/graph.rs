use super::*;
use tervia::git::graph::{Graph, Line, VisibleRow};

pub(super) fn width(graph: &Graph) -> f32 {
    (graph.columns.max(3) * 18 + 24) as f32
}

pub(super) fn color(index: usize, light: bool) -> Hsla {
    let colors = if light {
        [0x2e67a3, 0x7652aa, 0x936519, 0x4e7083, 0x925d77]
    } else {
        [0x98bee8, 0xb79ce8, 0xd4b178, 0x9cc7d3, 0xd7a7bf]
    };
    rgb(colors[index % colors.len()]).into()
}

pub(super) fn cell(
    graph: &Graph,
    visible: VisibleRow,
    selected: bool,
    light: bool,
) -> impl IntoElement {
    let row = graph.rows[visible.index].clone();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for line in &row.above {
                paint_line(
                    line,
                    if visible.above { 0.0 } else { 8.0 },
                    16.0,
                    !visible.above,
                    bounds,
                    light,
                    window,
                );
            }
            for line in &row.below {
                paint_line(
                    line,
                    16.0,
                    if visible.below { 32.0 } else { 24.0 },
                    !visible.below,
                    bounds,
                    light,
                    window,
                );
            }
            let radius = if selected { 4.5 } else { 3.0 };
            let center = bounds.origin + point(px(16.0 + row.column as f32 * 18.0), px(16.0));
            window.paint_quad(quad(
                Bounds::new(
                    center - point(px(radius), px(radius)),
                    size(px(radius * 2.0), px(radius * 2.0)),
                ),
                px(radius),
                color(row.color, light),
                px(0.0),
                transparent_black(),
                BorderStyle::default(),
            ));
        },
    )
    .w(px(width(graph)))
    .h(px(32.0))
    .flex_shrink_0()
}

fn paint_line(
    line: &Line,
    top: f32,
    bottom: f32,
    interrupted: bool,
    bounds: Bounds<Pixels>,
    light: bool,
    window: &mut Window,
) {
    let from = bounds.origin + point(px(16.0 + line.from as f32 * 18.0), px(top));
    let to = bounds.origin + point(px(16.0 + line.to as f32 * 18.0), px(bottom));
    let mut path = PathBuilder::stroke(px(1.75));
    if interrupted {
        // Leave a physical gap between filtered rows, so matching columns cannot imply a false edge.
        path = path.dash_array(&[px(2.0), px(2.0)]);
    }
    path.move_to(from);
    if line.from == line.to {
        path.line_to(to);
    } else {
        let middle = px((top + bottom) / 2.0) + bounds.origin.y;
        path.cubic_bezier_to(to, point(from.x, middle), point(to.x, middle));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color(line.color, light));
    }
}
