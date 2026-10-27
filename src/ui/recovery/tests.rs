use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

#[gpui_test::test]
fn restored_files_page_loads_the_directory_without_open_documents(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("mock.txt"), "synthetic contents").unwrap();
    let mut store = Store::new(temporary.path().join("workspace.json"));
    let mut state = store.load().unwrap();
    state.open_directory(root);
    state.set_tab(Tab::Files);
    store.save(&state).unwrap();
    let state = store.load().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            Tervia::build(
                Preferences::default(),
                None,
                None,
                state,
                Some(store),
                window,
                cx,
            )
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    cx.read(|cx| {
        let this = view.read(cx);
        let project = this.state.active_project;
        assert_eq!(this.state.active_tab(), Tab::Files);
        assert!(this.state.projects[project].files.is_empty());
        let entries = this.explorers[project]
            .tree
            .entries(std::path::Path::new(""))
            .expect("Restoring the Files page must start its initial directory read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("mock.txt"));
        assert!(!this.explorers[project].tree.loading());
        assert!(this.monitors[project].enabled);
    });
}

#[gpui_test::test]
fn autosave_keeps_its_deadline_and_does_not_reschedule_itself_after_success(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    let mut state = store.load().unwrap();
    state.open_directory(temporary.path().to_path_buf());
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            Tervia::build(
                Preferences::default(),
                None,
                None,
                state,
                Some(store),
                window,
                cx,
            )
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.state.sidebar_expanded = true;
            this.schedule_workspace_save(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.state.sidebar_expanded = false;
            this.schedule_workspace_save(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(50));
    cx.run_until_parked();
    assert!(!Store::new(path).load().unwrap().sidebar_expanded);
    cx.read(|cx| assert!(view.read(cx).workspace_save.is_none()));
}
