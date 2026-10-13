use super::super::Tervia;
use core::prelude::v1::test;
use gpui_kit::*;
use gpui_test::TestAppContext;
use tervia::state::{File, Tab};

#[gpui_test::test]
fn edits_typed_during_save_and_close_keep_the_tabs_and_their_project(cx: &mut TestAppContext) {
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
            let owner = this.state.open_directory("/synthetic/tab-owner".into());
            this.state.projects[owner].files.push(File {
                path: "mock.txt".into(),
                disk_path: "mock.txt".into(),
                language: "plaintext".into(),
                saved: "old".into(),
                draft: "newly typed".into(),
            });
            this.state.projects[owner].open_file(0);
            this.init_project(owner, window, cx);
            let other = this.state.open_directory("/synthetic/other".into());
            this.init_project(other, window, cx);
            this.finish_tab_saves(
                owner,
                vec![0],
                vec![(0, "approved draft".into(), Ok(()))],
                window,
                cx,
            );
            assert_eq!(this.state.projects[owner].open_files, vec![0]);
            assert_eq!(this.state.projects[owner].files[0].saved, "approved draft");
            assert_eq!(this.state.projects[owner].files[0].draft, "newly typed");
            assert_eq!(this.state.active_project, other);
            assert_eq!(this.state.project().tab, Tab::Git);
        })
    });
}

#[gpui_test::test]
fn a_failed_batch_save_keeps_all_requested_tabs_open(cx: &mut TestAppContext) {
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
            let owner = this.state.open_directory("/synthetic/tab-save".into());
            this.state.projects[owner].files = (0..2)
                .map(|index| File {
                    path: format!("mock-{index}.txt"),
                    disk_path: format!("mock-{index}.txt").into(),
                    language: "plaintext".into(),
                    saved: "old".into(),
                    draft: "new".into(),
                })
                .collect();
            this.state.projects[owner].open_file(0);
            this.state.projects[owner].open_file(1);
            this.init_project(owner, window, cx);
            this.finish_tab_saves(
                owner,
                vec![0, 1],
                vec![
                    (0, "new".into(), Ok(())),
                    (1, "new".into(), Err("Synthetic external edit".into())),
                ],
                window,
                cx,
            );
            assert_eq!(this.state.project().open_files, vec![0, 1]);
            assert_eq!(this.state.project().files[1].draft, "new");
            assert_eq!(this.state.notice, "Synthetic external edit");
        })
    });
}

#[gpui_test::test]
fn middle_click_on_the_tab_title_closes_it_without_reopening_on_mouse_up(cx: &mut TestAppContext) {
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
            let owner = this.state.open_directory("/synthetic/middle-click".into());
            this.state.projects[owner].files.push(File {
                path: "mock.txt".into(),
                disk_path: "mock.txt".into(),
                language: "plaintext".into(),
                saved: "Synthetic".into(),
                draft: "Synthetic".into(),
            });
            this.state.projects[owner].open_file(0);
            this.state.set_tab(Tab::Files);
            this.init_project(owner, window, cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("file-tab-0").unwrap();
    let position = point(bounds.origin.x + px(25.0), bounds.center().y);
    cx.simulate_mouse_move(position, None, Default::default());
    cx.simulate_mouse_down(position, MouseButton::Middle, Default::default());
    cx.simulate_mouse_up(position, MouseButton::Middle, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.state.project().open_files.is_empty());
            assert!(this.state.project().current_file().is_none());
        })
    });
}

#[gpui_test::test]
fn dragging_a_tab_title_reorders_only_its_project_tabs(cx: &mut TestAppContext) {
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
            let owner = this.state.open_directory("/synthetic/drag-tabs".into());
            this.state.projects[owner].files = (0..2)
                .map(|index| File {
                    path: format!("mock-{index}.txt"),
                    disk_path: format!("mock-{index}.txt").into(),
                    language: "plaintext".into(),
                    saved: "Synthetic".into(),
                    draft: "Synthetic".into(),
                })
                .collect();
            this.state.projects[owner].open_file(0);
            this.state.projects[owner].open_file(1);
            this.state.set_tab(Tab::Files);
            this.init_project(owner, window, cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first = cx.debug_bounds("file-tab-0").unwrap();
    let last = cx.debug_bounds("file-tab-1").unwrap();
    let from = point(first.origin.x + px(25.0), first.center().y);
    let to = point(last.origin.x + px(25.0), last.center().y);
    cx.simulate_mouse_move(from, None, Default::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        from + point(px(15.0), px(0.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(to, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.state.project().open_files, vec![1, 0])
        })
    });
}
