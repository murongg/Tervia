use crate::ui::Tervia;
use core::prelude::v1::test;
use gpui_kit::*;
use gpui_test::TestAppContext;
use tervia::tasks::{Catalog, Task};

#[gpui_test::test]
fn new_task_sources_use_the_existing_search_launch_and_rerun_flow(cx: &mut TestAppContext) {
    let (view, cx, root) = workspace(cx);
    std::fs::write(root.path().join("Makefile"), "mock-make:\n\t@echo mock\n").unwrap();
    std::fs::write(root.path().join("justfile"), "mock-just:\n  echo mock\n").unwrap();
    std::fs::write(
        root.path().join("Taskfile.yml"),
        "version: '3'\ntasks:\n  mock-task: echo mock\n",
    )
    .unwrap();
    cx.update(|window, cx| view.update(cx, |this, cx| this.show_project_tasks(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.tasks.catalog.tasks.len(), 3);
            for (query, program) in [
                ("Makefile", "make"),
                ("justfile", "just"),
                ("Taskfile", "task"),
            ] {
                this.tasks
                    .input
                    .update(cx, |input, cx| input.set_value(query, window, cx));
                let rows = this.task_rows(cx);
                assert_eq!(rows.len(), 1);
                assert_eq!(this.tasks.catalog.tasks[rows[0]].program, program);
            }
            this.confirm_project_task(window, cx);
            assert_eq!(
                this.state.terminals().sessions[1]
                    .task
                    .as_ref()
                    .unwrap()
                    .args,
                ["--taskfile", "Taskfile.yml", "mock-task"]
            );
            this.ensure_console(1, window, cx)
                .update(cx, |console, cx| {
                    console.finish_process("Success".into(), cx)
                });
            this.rerun_project_task(1, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), 3);
            assert_eq!(
                this.state.terminals().sessions[1].task,
                this.state.terminals().sessions[2].task
            );
        })
    });
}

#[gpui_test::test]
fn task_picker_with_warnings_fits_the_minimum_window_in_both_themes(cx: &mut TestAppContext) {
    let (view, cx, _root) = workspace(cx);
    cx.simulate_resize(size(px(880.0), px(560.0)));
    for light in [false, true] {
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.appearance.theme = light.into();
                this.show_project_tasks(window, cx);
                this.install_tasks(
                    this.state.active_project,
                    this.tasks.request,
                    Catalog {
                        tasks: (0..20)
                            .map(|index| mock_task(&format!("mock-task-{index}")))
                            .collect(),
                        warnings: vec!["Synthetic manifest warning. ".repeat(20)],
                    },
                    cx,
                );
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let picker = cx.debug_bounds("project-tasks").unwrap();
        assert!(picker.bottom() <= px(544.0), "{picker:?}");
        let hint = cx.debug_bounds("task-picker-hint").unwrap();
        assert!(hint.bottom() <= picker.bottom());
    }
}

#[gpui_test::test]
fn clicking_a_task_row_starts_the_clicked_task(cx: &mut TestAppContext) {
    let (view, cx, root) = workspace(cx);
    std::fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"check":"echo mock-check","fail":"exit 7"}}"#,
    )
    .unwrap();
    cx.update(|window, cx| view.update(cx, |this, cx| this.show_project_tasks(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("project-task-1").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = view.read(cx);
        assert!(!this.tasks.open);
        assert_eq!(this.state.terminals().sessions.len(), 2);
        assert_eq!(
            this.state.terminals().sessions[1]
                .task
                .as_ref()
                .unwrap()
                .label,
            "fail"
        );
    });
}

fn mock_task(label: &str) -> Task {
    Task {
        label: label.into(),
        source: "package.json".into(),
        program: "npm".into(),
        args: vec!["run".into(), label.into()],
    }
}

fn workspace(
    cx: &mut TestAppContext,
) -> (
    Entity<Tervia>,
    &mut gpui_test::VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::console::init(cx);
        crate::ui::menus::bind_menu_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            this.config = Some(tervia::config::Config::new(root.path().join("config.toml")));
            let project = this.state.open_directory(root.path().to_owned());
            this.init_project(project, window, cx);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    (view.unwrap(), cx, root)
}

#[gpui_test::test]
fn picker_filters_and_launches_a_task_in_its_own_terminal(cx: &mut TestAppContext) {
    let (view, cx, _root) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.show_project_tasks(window, cx);
            let project = this.state.active_project;
            let request = this.tasks.request;
            assert!(this.install_tasks(
                project,
                request,
                Catalog {
                    tasks: vec![mock_task("dev"), mock_task("mock:test")],
                    warnings: vec![]
                },
                cx
            ));
            this.tasks
                .input
                .update(cx, |input, cx| input.set_value("mock:test", window, cx));
            assert_eq!(this.task_rows(cx), [1]);
            this.confirm_project_task(window, cx);
            assert!(!this.tasks.open);
            assert_eq!(this.state.terminals().sessions.len(), 2);
            assert_eq!(
                this.state.terminals().sessions[1].task,
                Some(mock_task("mock:test"))
            );
            assert!(this.state.terminals().sessions[0].task.is_none());
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("task-run-summary-1").is_some());
}

#[gpui_test::test]
fn keyboard_selection_discovers_scripts_and_enter_starts_exactly_one_task(cx: &mut TestAppContext) {
    let (view, cx, root) = workspace(cx);
    std::fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"dev":"echo mock-dev","test":"echo mock-test"}}"#,
    )
    .unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| this.show_project_tasks(window, cx));
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-task-1").is_some());
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.terminals().sessions.len(), 2);
            assert_eq!(
                this.state.terminals().sessions[1]
                    .task
                    .as_ref()
                    .unwrap()
                    .label,
                "test"
            );
            assert!(
                this.ensure_console(1, window, cx)
                    .read(cx)
                    .focus
                    .is_focused(window)
            );
        })
    });
}

#[gpui_test::test]
fn rerun_waits_for_exit_and_retains_the_previous_output_and_session(cx: &mut TestAppContext) {
    let (view, cx, _root) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.run_project_task(mock_task("mock:test"), window, cx);
            let first = this.ensure_console(1, window, cx);
            this.rerun_project_task(1, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), 2);
            first.update(cx, |console, cx| {
                console.finish_process("Exited with code 7".into(), cx)
            });
            assert!(first.read(cx).can_rerun_task());
            this.rerun_project_task(1, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), 3);
            assert_eq!(
                this.state.terminals().sessions[1].task,
                this.state.terminals().sessions[2].task
            );
            assert_eq!(
                this.terminals[this.state.active_project][1]
                    .as_ref()
                    .unwrap()
                    .entity_id(),
                first.entity_id()
            );
        })
    });
}

#[gpui_test::test]
fn late_discovery_cannot_replace_a_new_request_or_launch_in_another_directory(
    cx: &mut TestAppContext,
) {
    let (view, cx, _root) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.show_project_tasks(window, cx);
            let project = this.state.active_project;
            let old = this.tasks.request;
            this.show_project_tasks(window, cx);
            let catalog = Catalog {
                tasks: vec![mock_task("dev")],
                warnings: vec![],
            };
            assert!(!this.install_tasks(project, old, catalog.clone(), cx));
            let request = this.tasks.request;
            assert!(this.install_tasks(project, request, catalog.clone(), cx));
            this.open_local_terminal(window, cx);
            assert!(!this.install_tasks(project, request, catalog, cx));
            this.confirm_project_task(window, cx);
            assert!(this.state.local_terminal.sessions[0].task.is_none());
            assert_eq!(this.state.projects[project].terminal.sessions.len(), 1);
        })
    });
}

#[gpui_test::test]
fn completed_task_tabs_are_retained_for_replay_but_missing_hosts_are_discarded(
    cx: &mut TestAppContext,
) {
    let (view, cx, root) = workspace(cx);
    let sessions = root.path().join("sessions");
    std::fs::create_dir_all(sessions.join("a".repeat(32))).unwrap();
    std::fs::write(sessions.join("a".repeat(32)).join("finished.json"), "{}").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.run_project_task(mock_task("done"), window, cx);
            this.state.terminals_mut().sessions[1].connection = Some("a".repeat(32));
            this.run_project_task(mock_task("missing"), window, cx);
            this.state.terminals_mut().sessions[2].connection = Some("b".repeat(32));
            assert!(crate::ui::sessions::discard_exited(
                &mut this.state,
                &sessions
            ));
            assert_eq!(this.state.terminals().sessions.len(), 2);
            assert_eq!(
                this.state.terminals().sessions[1].task,
                Some(mock_task("done"))
            );
        })
    });
}
