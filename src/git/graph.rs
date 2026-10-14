use super::Commit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub from: usize,
    pub to: usize,
    pub color: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub column: usize,
    pub color: usize,
    pub above: Vec<Line>,
    pub below: Vec<Line>,
}

#[derive(Clone, Copy, Debug)]
pub struct VisibleRow {
    pub index: usize,
    pub above: bool,
    pub below: bool,
}

#[derive(Default)]
pub struct Graph {
    pub rows: Vec<Row>,
    pub columns: usize,
}

struct Lane<'a> {
    target: &'a str,
    color: usize,
}

impl Graph {
    /// Commits must be ordered with children before parents, as in Git's --date-order.
    pub fn new(commits: &[Commit]) -> Self {
        let mut graph = Self::default();
        let mut lanes: Vec<Option<Lane<'_>>> = Vec::new();
        let mut next_color = 0;
        for commit in commits {
            let above = lanes
                .iter()
                .enumerate()
                .filter_map(|(column, lane)| {
                    lane.as_ref().map(|lane| Line {
                        from: column,
                        to: column,
                        color: lane.color,
                    })
                })
                .collect();
            let column = lanes
                .iter()
                .position(|lane| lane.as_ref().is_some_and(|lane| lane.target == commit.hash))
                .unwrap_or_else(|| vacant(&lanes));
            let color = lanes.get_mut(column).and_then(Option::take).map_or_else(
                || {
                    let color = next_color;
                    next_color += 1;
                    color
                },
                |lane| lane.color,
            );
            let mut below: Vec<_> = lanes
                .iter()
                .enumerate()
                .filter_map(|(column, lane)| {
                    lane.as_ref().map(|lane| Line {
                        from: column,
                        to: column,
                        color: lane.color,
                    })
                })
                .collect();
            for (parent_index, parent) in commit.parents.split_whitespace().enumerate() {
                let mut target = lanes
                    .iter()
                    .position(|lane| lane.as_ref().is_some_and(|lane| lane.target == parent));
                if let Some(existing) = target
                    && parent_index == 0
                    && color < lanes[existing].as_ref().unwrap().color
                {
                    // Older lane IDs win shared ancestry: an interleaved side branch must
                    // not pull the first-parent mainline into its column or recolor it.
                    let lane = lanes[existing].take().unwrap();
                    lanes[column] = Some(Lane {
                        target: lane.target,
                        color,
                    });
                    for line in &mut below {
                        if line.from == existing {
                            line.to = column;
                        }
                    }
                    target = Some(column);
                }
                let target = target.unwrap_or_else(|| {
                    let target = if parent_index == 0 {
                        column
                    } else {
                        vacant(&lanes)
                    };
                    let color = if parent_index == 0 {
                        color
                    } else {
                        let color = next_color;
                        next_color += 1;
                        color
                    };
                    while lanes.len() <= target {
                        lanes.push(None);
                    }
                    lanes[target] = Some(Lane {
                        target: parent,
                        color,
                    });
                    target
                });
                let line = Line {
                    from: column,
                    to: target,
                    color: lanes[target].as_ref().unwrap().color,
                };
                if !below.contains(&line) {
                    below.push(line);
                }
            }
            // Keep unknown parent targets active: a page boundary is not a root commit.
            graph.columns = graph.columns.max(lanes.len()).max(column + 1);
            graph.rows.push(Row {
                column,
                color,
                above,
                below,
            });
        }
        graph
    }

    /// Filtering may remove ancestors. Only consecutive source rows can share a boundary.
    pub fn visible(&self, indices: &[usize]) -> Vec<VisibleRow> {
        indices
            .iter()
            .enumerate()
            .map(|(position, &index)| VisibleRow {
                index,
                above: position
                    .checked_sub(1)
                    .map_or(index == 0, |previous| indices[previous] + 1 == index),
                below: indices
                    .get(position + 1)
                    .map_or(index + 1 == self.rows.len(), |&next| index + 1 == next),
            })
            .collect()
    }
}

fn vacant(lanes: &[Option<Lane<'_>>]) -> usize {
    lanes
        .iter()
        .position(Option::is_none)
        .unwrap_or(lanes.len())
}
