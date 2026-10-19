use super::*;
use crate::ui::files::definitions::tests::mount;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

#[gpui_test::test]
fn keyboard_history_returns_to_positions_within_the_same_file(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "samefile", false);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    let (back, forward) = match Platform::CURRENT {
        Platform::Mac => ("ctrl--", "ctrl-shift--"),
        Platform::Windows => ("alt-left", "alt-right"),
        Platform::Linux => ("ctrl-alt--", "ctrl-shift--"),
    };
    cx.simulate_keystrokes(back);
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 0);
        assert_eq!(
            this.editors[0][0].read(cx).cursor_position(),
            Position::new(0, 0)
        );
        assert!(this.explorers[0].file_history.can_forward());
    });
    cx.simulate_keystrokes(forward);
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(
            this.editors[0][0].read(cx).cursor_position(),
            Position::new(1, 9)
        );
        assert!(this.explorers[0].file_history.can_back());
    });
}

#[gpui_test::test]
fn missing_history_targets_do_not_create_forward_entries(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    std::fs::remove_file(root.path().join("mock.ts")).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.state.projects[0].close_files(&[0]);
            this.navigate_file(Direction::Back, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 1);
        assert!(!this.explorers[0].file_history.can_forward());
        assert!(!this.explorers[0].file_history.can_back());
        assert!(!this.state.notice.is_empty());
        assert_eq!(
            this.editors[0][1].read(cx).value().as_str(),
            "\nexport function mock() {}\n"
        );
    });
}

#[gpui_test::test]
fn typing_during_a_history_read_keeps_the_current_draft_and_step(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.state.projects[0].close_files(&[0]);
            this.navigate_file(Direction::Back, window, cx);
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_value("synthetic later edit", window, cx)
            });
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 1);
        assert_eq!(
            this.editors[0][1].read(cx).value().as_str(),
            "synthetic later edit"
        );
        assert!(this.explorers[0].file_history.can_back());
        assert!(!this.explorers[0].file_history.can_forward());
    });
}

#[gpui_test::test]
fn invalidated_history_reads_cannot_focus_their_target(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.state.projects[0].close_files(&[0]);
            this.navigate_file(Direction::Back, window, cx);
            this.explorers[0]
                .file_history
                .relocate("unrelated".as_ref(), "renamed".as_ref());
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1);
        assert!(this.explorers[0].file_history.can_back());
        assert!(!this.explorers[0].file_history.can_forward());
    });
}

#[gpui_test::test]
fn reselecting_the_current_tab_cancels_a_pending_history_read(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.state.projects[0].close_files(&[0]);
            this.navigate_file(Direction::Back, window, cx);
            this.select_open_file(0, 1, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1);
        assert!(this.explorers[0].file_history.can_back());
        assert!(!this.explorers[0].file_history.can_forward());
    });
}

#[gpui_test::test]
fn file_history_shortcuts_work_when_the_workspace_has_focus(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |this, cx| this.focus.focus(window, cx)));
    cx.simulate_keystrokes(shortcuts(Platform::CURRENT).0);
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 0)
    });
}
