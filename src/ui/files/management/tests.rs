use super::*;
use crate::ui::Tervia;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use std::{fs, path::PathBuf};
use tervia::state::{File, Tab};

#[test]
fn form_destinations_are_normalized_before_updating_document_paths() {
    for edit in [
        Edit::Rename("mock.rs".into()),
        Edit::Move("mock.rs".into()),
        Edit::Copy("mock.rs".into()),
    ] {
        let operation = edit.operation("renamed.rs/").unwrap();
        let to = match operation {
            Operation::Move { to, .. } | Operation::Copy { to, .. } => to,
            _ => unreachable!(),
        };
        assert_eq!(to.to_string_lossy(), "renamed.rs");
    }
}

struct Surface(Entity<Tervia>);
impl Render for Surface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |this, cx| this.files(window, cx))
    }
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (Entity<Tervia>, &'a mut gpui_test::VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::ui::menus::bind_menu_keys);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory(root.to_path_buf());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this.load_directory(project, PathBuf::new(), true, window, cx);
            this
        });
        view = Some(entity.clone());
        let surface = cx.new(|_| Surface(entity));
        gpui_kit::base::Root::new(surface, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view.unwrap(), cx)
}

#[gpui_test::test]
fn file_context_menu_supports_keyboard_rename_and_escape(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.rs"), "synthetic source").unwrap();
    let (view, cx) = mount(cx, root.path());
    let position = cx.debug_bounds("explorer-file-mock.rs").unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("file-entry-form").is_some());
    cx.simulate_input("renamed.rs");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(root.path().join("renamed.rs").exists());
    assert!(!root.path().join("mock.rs").exists());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_entry(0, Edit::File(PathBuf::new()), window, cx)
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_input("cancelled.rs");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!root.path().join("cancelled.rs").exists());
    view.update(cx, |this, _| {
        assert!(this.explorers[0].management.edit.is_none())
    });
}

#[gpui_test::test]
fn case_only_rename_keeps_the_open_editor_and_unsaved_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Mock.rs"), "synthetic source").unwrap();
    let (view, cx) = mount(cx, root.path());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_real_file(0, "Mock.rs".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.focus(window, cx);
                editor.select_all(window, cx);
            });
        })
    });
    cx.simulate_input("synthetic unsaved draft");
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.run_file_operation(
                0,
                Operation::Move {
                    from: "Mock.rs".into(),
                    to: "mock.rs".into(),
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        let project = &this.state.projects[0];
        assert_eq!(project.files.len(), 1);
        assert_eq!(project.open_files, [0]);
        assert_eq!(project.files[0].path, "mock.rs");
        assert_eq!(project.files[0].draft, "synthetic unsaved draft");
        assert_eq!(
            this.editors[0][0].read(cx).value(),
            "synthetic unsaved draft"
        );
    });
    assert_eq!(
        fs::read_to_string(root.path().join("mock.rs")).unwrap(),
        "synthetic source"
    );
}

#[gpui_test::test]
fn failed_creation_keeps_the_form_and_existing_contents(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.rs"), "synthetic source").unwrap();
    let (view, cx) = mount(cx, root.path());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_entry(0, Edit::File(PathBuf::new()), window, cx)
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_input("mock.rs");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(root.path().join("mock.rs")).unwrap(),
        "synthetic source"
    );
    view.update(cx, |this, _| {
        assert!(this.explorers[0].management.edit.is_some());
        assert!(!this.explorers[0].management.error.is_empty());
        assert!(!this.saving_file);
    });
}

#[gpui_test::test]
fn trash_cancel_and_new_drafts_during_confirmation_leave_disk_untouched(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.rs"), "synthetic source").unwrap();
    let (view, cx) = mount(cx, root.path());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_real_file(0, "mock.rs".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.trash_entry(0, "mock.rs".into(), window, cx)
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_cancel"));
    cx.run_until_parked();
    assert!(root.path().join("mock.rs").exists());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.trash_entry(0, "mock.rs".into(), window, cx);
            this.state.projects[0].files[0].draft = "synthetic new draft".into();
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("files_trash"));
    cx.run_until_parked();
    assert!(root.path().join("mock.rs").exists());
    view.update(cx, |this, _| {
        assert_eq!(this.state.projects[0].files[0].draft, "synthetic new draft");
        assert_eq!(this.state.notice, "files_manage_dirty");
        assert!(!this.file_closing);
    });
}

#[gpui_test::test]
fn moving_files_updates_each_open_directory_and_the_editor_language(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("scope")).unwrap();
    fs::write(root.path().join("scope/mock.rs"), "synthetic source").unwrap();
    let (view, cx) = mount(cx, root.path());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_real_file(0, "scope/mock.rs".into(), window, cx);
            let owner = this.state.open_directory(root.path().join("scope"));
            this.init_project(owner, window, cx);
            this.open_real_file(owner, "mock.rs".into(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.state.switch_project(0);
            this.state.projects[0].files[0].draft = "synthetic root draft".into();
            this.state.projects[1].files[0].draft = "synthetic directory draft".into();
            this.run_file_operation(
                0,
                Operation::Move {
                    from: "scope/mock.rs".into(),
                    to: "scope/renamed.py".into(),
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        for (owner, path, draft) in [
            (0, "scope/renamed.py", "synthetic root draft"),
            (1, "renamed.py", "synthetic directory draft"),
        ] {
            assert_eq!(this.state.projects[owner].files[0].path, path);
            assert_eq!(this.state.projects[owner].files[0].draft, draft);
            assert_eq!(this.editors[owner][0].read(cx).language_name(), "python");
        }
    });
    assert!(root.path().join("scope/renamed.py").exists());
}

#[gpui_test::test]
fn creating_a_file_never_steals_a_later_project_selection(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let (view, cx) = mount(cx, root.path());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.run_file_operation(0, Operation::CreateFile("mock.rs".into()), window, cx);
            let owner = this.state.open_directory(other.path().to_path_buf());
            this.init_project(owner, window, cx);
            this.state.set_tab(Tab::Git);
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.active_project, 1);
        assert_eq!(this.state.project().tab, Tab::Git);
        assert!(this.state.projects[0].open_files.is_empty());
    });
    assert!(root.path().join("mock.rs").exists());
}

#[gpui_test::test]
fn file_operations_preserve_unrelated_reads_and_external_notices(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory(root.path().to_path_buf());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    let mut token = 0;
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            token = this.file_reads.begin(0, "unrelated.rs".into(), None, false);
            this.monitors[0]
                .external
                .insert("unrelated.rs".into(), "files_disk_changed");
            this.run_file_operation(0, Operation::CreateDirectory("mock".into()), window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(
            this.file_reads
                .finish(0, "unrelated.rs".as_ref(), token, None),
            tervia::files::ReadOutcome::Apply
        );
        assert!(
            this.monitors[0]
                .external
                .contains_key(Path::new("unrelated.rs"))
        );
    });
}

#[gpui_test::test]
fn file_creation_form_creates_a_file_and_cancel_does_not_write(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory(root.path().to_path_buf());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_entry(0, Edit::File(PathBuf::new()), window, cx);
            this.explorers[0]
                .management
                .input
                .update(cx, |input, cx| input.set_value("mock.rs", window, cx));
            this.submit_entry(0, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(root.path().join("mock.rs").is_file());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_entry(0, Edit::Directory(PathBuf::new()), window, cx);
            this.cancel_entry(0, window, cx);
            assert!(this.explorers[0].management.edit.is_none());
        })
    });
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[gpui_test::test]
fn moving_a_folder_updates_other_directory_documents_and_delete_rejects_dirty_drafts(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("mock")).unwrap();
    fs::write(root.path().join("mock/example.rs"), "synthetic saved").unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory(root.path().to_path_buf());
            this.state.projects[project].files.push(File {
                path: "mock/example.rs".into(),
                disk_path: "mock/example.rs".into(),
                language: "rust".into(),
                saved: "synthetic saved".into(),
                draft: "synthetic draft".into(),
            });
            this.state.projects[project].open_file(0);
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_entry(0, Edit::Rename("mock".into()), window, cx);
            this.explorers[0]
                .management
                .input
                .update(cx, |input, cx| input.set_value("renamed", window, cx));
            this.submit_entry(0, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.state.projects[0].files[0].disk_path,
                PathBuf::from("renamed/example.rs")
            );
            assert_eq!(this.state.projects[0].files[0].draft, "synthetic draft");
            this.trash_entry(0, "renamed".into(), window, cx);
            assert!(!this.saving_file);
            assert_eq!(this.state.notice, "files_manage_dirty");
        })
    });
    assert!(!cx.has_pending_prompt());
    assert!(root.path().join("renamed/example.rs").exists());
}
