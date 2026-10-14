use tervia::git::{Commit, graph::Graph};

fn commit(hash: &str, parents: &str) -> Commit {
    Commit {
        hash: hash.into(),
        parents: parents.into(),
        author: "Synthetic Developer".into(),
        when: "2026-01-01".into(),
        message: format!("Synthetic {hash}"),
        refs: String::new(),
    }
}

#[test]
fn a_linear_history_connects_across_rows_and_stops_at_the_root() {
    let graph = Graph::new(&[
        commit("mock-tip", "mock-middle"),
        commit("mock-middle", "mock-root"),
        commit("mock-root", ""),
    ]);
    assert_eq!(graph.columns, 1);
    assert!(graph.rows[0].above.is_empty());
    assert!(graph.rows[2].below.is_empty());
    for row in &graph.rows {
        assert_eq!(row.column, 0);
        assert_eq!(row.color, graph.rows[0].color);
    }
    for pair in graph.rows.windows(2) {
        assert_eq!(pair[0].below[0].to, pair[1].above[0].from);
        assert_eq!(pair[0].below[0].color, pair[1].above[0].color);
    }
}

#[test]
fn a_merge_keeps_its_feature_lane_alive_across_interleaved_main_commits() {
    let graph = Graph::new(&[
        commit("mock-merge", "mock-main mock-feature-tip"),
        commit("mock-main", "mock-main-parent"),
        commit("mock-feature-tip", "mock-feature-parent"),
        commit("mock-main-parent", "mock-root"),
        commit("mock-feature-parent", "mock-root"),
        commit("mock-root", ""),
    ]);
    assert_eq!(graph.columns, 2);
    assert_eq!(graph.rows[0].below.len(), 2);
    let feature = &graph.rows[2];
    assert_ne!(feature.column, graph.rows[1].column);
    assert_ne!(feature.color, graph.rows[1].color);
    assert_eq!(feature.column, graph.rows[4].column);
    assert_eq!(feature.color, graph.rows[4].color);
    let through = graph.rows[3]
        .below
        .iter()
        .find(|line| line.color == feature.color)
        .unwrap();
    assert_eq!((through.from, through.to), (feature.column, feature.column));
    assert!(
        graph.rows[4]
            .below
            .iter()
            .any(|line| { line.from == graph.rows[4].column && line.to == graph.rows[5].column })
    );
    assert!(graph.rows[5].below.is_empty());
}

#[test]
fn octopus_merges_keep_every_parent_and_do_not_create_duplicate_target_lanes() {
    let graph = Graph::new(&[
        commit("mock-merge", "mock-main mock-a mock-b"),
        commit("mock-a", "mock-root"),
        commit("mock-b", "mock-root"),
        commit("mock-main", "mock-root"),
        commit("mock-root", ""),
    ]);
    assert_eq!(graph.rows[0].below.len(), 3);
    assert_eq!(graph.columns, 3);
    assert_eq!(graph.rows[4].above.len(), 1);
    assert!(graph.rows[4].below.is_empty());
}

#[test]
fn pagination_preserves_connections_to_unloaded_parents_and_existing_lane_colors() {
    let commits = [
        commit("mock-merge", "mock-main mock-feature"),
        commit("mock-main", "mock-root"),
        commit("mock-feature", "mock-root"),
        commit("mock-root", ""),
    ];
    let page = Graph::new(&commits[..2]);
    let full = Graph::new(&commits);
    assert_eq!(page.rows, full.rows[..2]);
    assert_eq!(page.rows[1].below.len(), 2);
}

#[test]
fn filtered_rows_disconnect_at_hidden_commits_instead_of_inventing_edges() {
    let graph = Graph::new(&[
        commit("mock-a", "mock-b"),
        commit("mock-b", "mock-c"),
        commit("mock-c", "mock-d"),
        commit("mock-d", "mock-unloaded"),
    ]);
    let visible = graph.visible(&[0, 2, 3]);
    assert!(visible[0].above);
    assert!(!visible[0].below);
    assert!(!visible[1].above);
    assert!(visible[1].below);
    assert!(visible[2].above);
    assert!(
        visible[2].below,
        "Unloaded ancestry still continues below the page"
    );
    let isolated = graph.visible(&[2]);
    assert!(!isolated[0].above);
    assert!(!isolated[0].below);
}

#[test]
fn disconnected_roots_reuse_free_columns_without_connecting_to_each_other() {
    let graph = Graph::new(&[commit("mock-root-a", ""), commit("mock-root-b", "")]);
    assert_eq!(graph.columns, 1);
    assert!(
        graph
            .rows
            .iter()
            .all(|row| row.above.is_empty() && row.below.is_empty())
    );
    assert_ne!(graph.rows[0].color, graph.rows[1].color);
    let empty = Graph::new(&[]);
    assert!(empty.rows.is_empty());
    assert!(empty.visible(&[]).is_empty());
}

#[test]
fn nested_merges_preserve_the_main_lane_and_every_edge_reaches_its_actual_parent() {
    let commits = [
        commit("mock-tip", "mock-main mock-feature"),
        commit("mock-main", "mock-main-next"),
        commit("mock-feature", "mock-feature-next"),
        commit("mock-main-next", "mock-merge-fix"),
        commit("mock-feature-next", "mock-feature-base"),
        commit("mock-merge-fix", "mock-main-base mock-fix"),
        commit("mock-feature-base", "mock-root"),
        commit("mock-main-base", "mock-root"),
        commit("mock-fix", "mock-root"),
        commit("mock-root", ""),
    ];
    let graph = Graph::new(&commits);
    for index in [0, 1, 3, 5, 7, 9] {
        assert_eq!(
            graph.rows[index].column, 0,
            "The first-parent lane stays in place"
        );
        assert_eq!(graph.rows[index].color, graph.rows[0].color);
    }
    for (index, commit) in commits.iter().enumerate() {
        let row = &graph.rows[index];
        let reached: std::collections::BTreeSet<_> = row
            .below
            .iter()
            .filter(|line| line.from == row.column)
            .map(|line| {
                let mut column = line.to;
                for (row, parent) in graph.rows.iter().zip(&commits).skip(index + 1) {
                    if row.column == column {
                        return parent.hash.as_str();
                    }
                    column = row
                        .below
                        .iter()
                        .find(|line| line.from == column)
                        .unwrap()
                        .to;
                }
                panic!("A loaded parent cannot disappear from its path");
            })
            .collect();
        assert_eq!(reached, commit.parents.split_whitespace().collect());
    }
}
