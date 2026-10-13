use tervia::files::{ReadOutcome, Reads};

#[test]
fn removing_a_directory_invalidates_pending_reads_only_within_that_owner_and_prefix() {
    let mut reads = Reads::default();
    let deleted = reads.begin(7, "linked/mock.txt".into(), None, false);
    let sibling = reads.begin(7, "other/mock.txt".into(), None, false);
    let other_owner = reads.begin(8, "linked/mock.txt".into(), None, false);
    reads.invalidate_tree(7, "linked".as_ref());
    assert_eq!(
        reads.finish(7, "linked/mock.txt".as_ref(), deleted, None),
        ReadOutcome::Stale
    );
    assert_eq!(
        reads.finish(7, "other/mock.txt".as_ref(), sibling, None),
        ReadOutcome::Apply
    );
    assert_eq!(
        reads.finish(8, "linked/mock.txt".as_ref(), other_owner, None),
        ReadOutcome::Apply
    );
}

#[test]
fn typing_while_reload_is_pending_preserves_the_new_draft() {
    let mut reads = Reads::default();
    let path = std::path::Path::new("mock.rs");
    let request = reads.begin(0, path.into(), Some("synthetic original".into()), true);
    assert!(reads.has_reloads());
    assert_eq!(
        reads.finish(0, path, request, Some("synthetic newly typed")),
        ReadOutcome::Edited
    );
    assert!(!reads.has_reloads());
}

#[test]
fn late_reads_cannot_replace_a_newer_read_or_release_its_reload_guard() {
    let mut reads = Reads::default();
    let path = std::path::Path::new("mock.rs");
    let old = reads.begin(0, path.into(), Some("synthetic".into()), true);
    let new = reads.begin(0, path.into(), Some("synthetic".into()), true);
    assert_eq!(
        reads.finish(0, path, old, Some("synthetic")),
        ReadOutcome::Stale
    );
    assert!(reads.has_reloads());
    assert_eq!(
        reads.finish(0, path, new, Some("synthetic")),
        ReadOutcome::Apply
    );
    assert!(!reads.has_reloads());
}

#[test]
fn slower_open_results_cannot_steal_the_latest_file_selection() {
    let mut reads = Reads::default();
    let first = reads.select(0);
    let second = reads.select(0);
    assert!(!reads.selected(0, first));
    assert!(reads.selected(0, second));
    let other_project = reads.select(1);
    assert!(reads.selected(0, second));
    assert!(reads.selected(1, other_project));
}

#[test]
fn a_completed_save_invalidates_reads_of_its_previous_disk_version() {
    let mut reads = Reads::default();
    let path = std::path::Path::new("mock.rs");
    let token = reads.begin(0, path.into(), Some("Synthetic draft".into()), false);
    reads.invalidate(0, path);
    assert_eq!(
        reads.finish(0, path, token, Some("Synthetic draft")),
        ReadOutcome::Stale
    );
}
