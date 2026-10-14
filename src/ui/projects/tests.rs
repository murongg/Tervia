use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use tervia::files::{Entry, EntryKind};

#[gpui_test::test]
fn ordinary_folders_follow_the_real_hierarchy_and_do_not_share_repository_selection(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    std::fs::create_dir_all(directory.join("mock-src/mock-nested")).unwrap();
    std::fs::create_dir(directory.join("mock-docs")).unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(directory.clone());
            view.init_project(parent, window, cx);
            view.state.group_folder(parent, &[]);
            view.state.sidebar_expanded = true;
            view.state.set_tab(tervia::state::Tab::Files);
            view.load_navigation(parent, PathBuf::new(), false, window, cx);
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let src = cx
        .debug_bounds("directory-0-mock-src")
        .expect("Ordinary directory is visible without Git");
    assert!(
        cx.debug_bounds("directory-0-mock-src/mock-nested")
            .is_none()
    );
    let fold = cx.debug_bounds("directory-fold-0-mock-src").unwrap();
    cx.simulate_click(fold.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let nested = cx.debug_bounds("directory-0-mock-src/mock-nested").unwrap();
    assert!(
        nested.left() > src.left(),
        "Hierarchy uses actual parent indentation"
    );
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 0));
    cx.simulate_click(nested.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(
            view.state.project().root,
            directory.join("mock-src/mock-nested")
        );
        assert_eq!(view.state.project_rows(), vec![0]);
        assert_eq!(view.state.project().tab, tervia::state::Tab::Terminal);
        view.state
            .associate_repository(0, directory.join(".git"), None);
        view.state
            .associate_repository(view.state.active_project, directory.join(".git"), None);
        view.state.set_tab(tervia::state::Tab::Files);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let parent = cx.debug_bounds("project-root-0").unwrap();
    cx.simulate_click(parent.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 0));
}

#[gpui_test::test]
fn opening_a_plain_directory_starts_in_its_terminal_before_git_scan(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            view.open_directory(directory.clone(), window, cx);
            assert_eq!(view.state.project().root, directory);
            assert_eq!(view.state.project().tab, tervia::state::Tab::Terminal);
            assert_eq!(view.state.project_rows(), vec![0]);
            assert!(view.scanning_project(0));
            view
        });
        gpui_kit::base::Root::new(view, window, cx)
    });
}

#[gpui_test::test]
fn directory_navigation_virtualizes_large_plain_folder_lists(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory("/synthetic/mock-large".into());
            view.init_project(parent, window, cx);
            view.state.group_folder(parent, &[]);
            view.state.set_tab(tervia::state::Tab::Files);
            view.state.sidebar_expanded = true;
            view.navigation
                .roots
                .insert(parent, tree::DirectoryTree::new(cx));
            let tree = &mut view.navigation.roots.get_mut(&parent).unwrap().tree;
            let token = tree.begin(PathBuf::new(), false).unwrap();
            tree.finish(
                Path::new(""),
                token,
                Ok((0..10_000)
                    .map(|index| Entry {
                        path: format!("mock-folder-{index:05}").into(),
                        kind: EntryKind::Directory,
                    })
                    .collect()),
            );
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    entity.unwrap().update(cx, |view, _| {
        assert!(
            view.navigation.rendered_rows.get() < 100,
            "Only visible directory rows should render"
        );
        assert_eq!(
            view.state.projects.len(),
            1,
            "Browsing does not create thousands of working contexts"
        );
    });
}

#[gpui_test::test]
fn opening_an_existing_plain_child_keeps_one_tree_and_later_module_selection_cancels_navigation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let child_path = root_path.join("mock-child");
    std::fs::create_dir(&child_path).unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(root_path.clone());
            view.init_project(parent, window, cx);
            view.state.group_folder(parent, &[]);
            view.state.set_tab(tervia::state::Tab::Files);
            let child = view.state.register_directory(child_path.clone());
            view.init_project(child, window, cx);
            view.state.projects[child].tab = tervia::state::Tab::Files;
            view.open_directory(child_path.clone(), window, cx);
            assert_eq!(view.state.project_rows(), vec![parent]);
            assert_eq!(view.state.active_project, child);
            view.activate_directory(parent, PathBuf::new(), window, cx);
            view.change_tab(tervia::state::Tab::Files, window, cx);
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    entity.unwrap().update(cx, |view, _| {
        assert_eq!(view.state.project().root, child_path)
    });
}

#[gpui_test::test]
fn restoring_a_deep_plain_directory_reveals_its_ancestors_and_keeps_root_folding(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let relative = PathBuf::from("mock-a/mock-b/mock-c/mock-d/mock-e");
    std::fs::create_dir_all(root_path.join(&relative)).unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(root_path.clone());
            view.init_project(parent, window, cx);
            view.state.group_folder(parent, &[]);
            let child = view.state.register_directory(root_path.join(&relative));
            view.init_project(child, window, cx);
            view.state.attach_directory(parent, child);
            view.state.switch_project(child);
            view.state.set_tab(tervia::state::Tab::Files);
            view.state.toggle_folder(parent);
            view.restore_navigation(parent, window, cx);
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    entity.unwrap().update(cx, |view, _| {
        let rows = view.navigation.rows(0, true);
        assert!(
            rows.iter()
                .any(|row| matches!(row, Node::Directory {path,..} if *path == relative))
        );
        assert!(!view.state.folder_expanded(0));
        assert_eq!(view.state.project().root, root_path.join(relative));
    });
}

#[gpui_test::test]
fn an_external_linked_worktree_keeps_its_folder_highlight_when_the_tree_is_folded(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory("/synthetic/mock-main".into());
            view.init_project(parent, window, cx);
            view.state.group_folder(parent, &[]);
            let linked = view
                .state
                .register_directory("/synthetic/mock-linked".into());
            view.init_project(linked, window, cx);
            for index in [parent, linked] {
                view.state
                    .associate_repository(index, "/synthetic/mock-main/.git".into(), None);
            }
            view.state.switch_project(linked);
            view.state.set_tab(tervia::state::Tab::Files);
            view.state.sidebar_expanded = true;
            view.state.toggle_folder(parent);
            assert_eq!(view.state.folder(linked), Some(parent));
            view
        });
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("project-row-0").unwrap();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.bounds == bounds.scale(window.scale_factor())
                    && !quad.background.is_transparent()),
            "The active external checkout must still identify its workspace root"
        );
    });
}
