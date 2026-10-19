use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use tervia::state::File;

#[gpui_test::test]
fn an_automatic_result_from_before_explicit_io_completion_cannot_revert_the_buffer(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/explicit-io".into());
            this.state.projects[project].files.push(File {
                path: "mock.rs".into(),
                disk_path: "mock.rs".into(),
                language: "rust".into(),
                saved: "old disk".into(),
                draft: "new contents".into(),
            });
            this.init_project(project, window, cx);
            this.files_active = true;
            // An automatic read starts after explicit IO began but before it completed.
            this.monitors[project].generation = 20;
            this.monitors[project].syncing = Some(20);
            let old_open = this.file_reads.begin(
                project,
                "mock.rs".into(),
                Some("new contents".into()),
                false,
            );
            this.record_file_saved(project, 0, "new contents".into());
            assert_eq!(
                this.file_reads
                    .finish(project, "mock.rs".as_ref(), old_open, Some("new contents")),
                tervia::files::ReadOutcome::Stale
            );
            this.finish_disk_sync(
                project,
                20,
                vec![(0, "mock.rs".into(), Ok("old disk".into()))],
                window,
                cx,
            );
            let file = &this.state.projects[project].files[0];
            assert_eq!(file.saved, "new contents");
            assert_eq!(file.draft, "new contents");
            assert_eq!(
                this.editors[project][0].read(cx).value().as_ref(),
                "new contents"
            );
        })
    });
}

#[gpui_test::test]
fn a_first_directory_subscription_reconciles_its_gap_and_rejects_an_old_activation(
    cx: &mut TestAppContext,
) {
    use tervia::files::{Entry, EntryKind};
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/subscription".into());
            this.init_project(project, window, cx);
            this.files_active = true;
            let tree = &mut this.explorers[project].tree;
            let root = tree.begin(PathBuf::new(), false).unwrap();
            tree.finish(
                "".as_ref(),
                root,
                Ok(vec![Entry {
                    path: "src".into(),
                    kind: EntryKind::Directory,
                    ignored: false,
                }]),
            );
            tree.toggle("src".as_ref());
            let first = tree.begin("src".into(), false).unwrap();
            tree.finish("src".as_ref(), first, Ok(Vec::new()));
            assert!(!tree.loading());
            this.finish_directory_watch(project, "src".into(), 0, Ok(()), window, cx);
            assert!(
                this.explorers[project].tree.loading(),
                "A successful subscription must reread the pre-subscription listing"
            );
            let token = this.explorers[project]
                .tree
                .begin("src".into(), true)
                .unwrap();
            this.explorers[project].tree.finish(
                "src".as_ref(),
                token,
                Ok(vec![Entry {
                    path: "src/added-during-registration.rs".into(),
                    kind: EntryKind::File,
                    ignored: false,
                }]),
            );
            assert_eq!(this.explorers[project].tree.rows().len(), 2);
            this.monitor_activation(false, window, cx);
            this.finish_directory_watch(project, "src".into(), 0, Ok(()), window, cx);
            assert!(
                !this.explorers[project].tree.loading(),
                "An old registration must not restart work in the background"
            );
        })
    });
}

#[gpui_test::test]
fn precise_disk_sync_ignores_unrelated_files_and_merges_events_during_a_read(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("first.txt"), "synthetic first new").unwrap();
    std::fs::write(root.path().join("second.txt"), "synthetic second new").unwrap();
    std::fs::write(root.path().join("unrelated.txt"), "synthetic unrelated new").unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().to_path_buf());
            for path in ["first.txt", "second.txt", "unrelated.txt"] {
                this.state.projects[project].files.push(File {
                    path: path.into(),
                    disk_path: path.into(),
                    language: "text".into(),
                    saved: "synthetic old".into(),
                    draft: "synthetic old".into(),
                });
            }
            this.init_project(project, window, cx);
            this.files_active = true;
            this.sync_changed_files(project, BTreeSet::from(["first.txt".into()]), window, cx);
            this.sync_changed_files(project, BTreeSet::from(["second.txt".into()]), window, cx);
        })
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let this = view.read(cx);
        let files = &this.state.projects[0].files;
        assert_eq!(files[0].draft, "synthetic first new");
        assert_eq!(files[1].draft, "synthetic second new");
        assert_eq!(
            files[2].draft, "synthetic old",
            "Unrelated disk contents must not be read by a scoped event"
        );
        assert!(this.monitors[0].syncing.is_none());
        assert!(this.monitors[0].sync_paths.is_empty());
    });
}
