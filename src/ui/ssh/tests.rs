use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use tervia::state::Tab;

fn host() -> Host {
    Host::from_json(&serde_json::json!({"name":"Mock host","address":"mock.example.test"})).unwrap()
}

fn workspace(cx: &mut TestAppContext) -> (Entity<Tervia>, &mut gpui_test::VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let index = view.state.open_directory("/synthetic/ssh-ui".into());
            view.init_project(index, window, cx);
            view.state.set_tab(Tab::Ssh);
            view.ssh.hosts.push(host());
            view
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    (view.unwrap(), cx)
}

#[gpui_test::test]
fn selecting_hosts_keeps_ssh_visible_and_reuses_their_owned_terminal(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.select_ssh_host(0, window, cx);
            let first = this.state.terminals().active_session;
            assert_eq!(this.state.active_tab(), Tab::Ssh);
            assert_eq!(this.state.terminals().sessions[first].ssh, Some(host()));
            let count = this.state.terminals().sessions.len();
            this.select_ssh_host(0, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), count);
            assert_eq!(this.state.terminals().active_session, first);
            let mut other = host();
            other.name = "Mock beta".into();
            other.address = "beta.example.test".into();
            this.ssh.hosts.push(other.clone());
            this.select_ssh_host(1, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), count + 1);
            assert_eq!(
                this.state.terminals().sessions[this.state.terminals().active_session].ssh,
                Some(other)
            );
            this.select_ssh_host(0, window, cx);
            assert_eq!(this.state.terminals().active_session, first);
            assert!(this.state.terminals().sessions[0].ssh.is_none());
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("ssh-terminal").is_some());
}

#[gpui_test::test]
fn adding_a_host_opens_a_modal_and_validation_keeps_it_open(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.simulate_resize(size(px(880.0), px(560.0)));
    for light in [false, true] {
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.appearance.theme = light.into();
                this.edit_ssh_host(None, window, cx);
                assert!(window.has_active_dialog(cx));
                assert!(!this.save_ssh_host(cx));
                assert!(this.ssh.form.is_some());
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let files = cx.debug_bounds("module-files").unwrap();
        let ssh = cx.debug_bounds("module-ssh").unwrap();
        assert!(files.right() <= ssh.left());
        let dialog = cx.debug_bounds("ssh-host-dialog").unwrap();
        assert!(dialog.right() <= px(880.0));
        let popup = cx.debug_bounds("dialog-0").unwrap();
        assert!(
            popup.top() >= px(38.0) && popup.bottom() <= px(522.0),
            "{popup:?}"
        );
        let save = cx.debug_bounds("ssh-host-save").unwrap();
        assert!(save.bottom() <= px(544.0));
        cx.update(|window, cx| window.close_dialog(cx));
        cx.run_until_parked();
    }
}

#[gpui_test::test]
fn terminal_limit_cannot_repurpose_a_local_session_as_ssh(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            while this.state.terminals().sessions.len() < 64 {
                this.state.new_session();
            }
            this.select_ssh_host(0, window, cx);
            assert!(
                this.state
                    .terminals()
                    .sessions
                    .iter()
                    .all(|s| s.ssh.is_none())
            );
        })
    });
}

#[gpui_test::test]
fn explicitly_closing_an_ssh_project_does_not_relaunch_its_remote_connection(
    cx: &mut TestAppContext,
) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let session = &mut this.state.project_mut().terminal.sessions[0];
            session.ssh = Some(host());
            session.connection = Some("0123456789abcdef0123456789abcdef".into());
            this.request_project_close(this.state.active_project, window, cx);
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_confirm"));
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.state.projects[0].terminal.sessions[0].ssh.is_none())
        })
    });
}

#[gpui_test::test]
fn popup_save_persists_the_host_and_closes_the_dialog(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let mut store = Store::new(path.clone());
            store.load().unwrap();
            this.ssh.store = Some(store);
            this.edit_ssh_host(None, window, cx);
            let form = this.ssh.form.as_ref().unwrap();
            for (input, value) in form.inputs.iter().zip([
                "Mock added",
                "added.example.test",
                "mock-user",
                "2222",
                "",
            ]) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx.debug_bounds("ssh-host-save").unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert!(!window.has_active_dialog(cx));
            assert_eq!(this.ssh.hosts.len(), 2);
            assert!(this.ssh.form.is_none());
        })
    });
    assert_eq!(
        Store::new(path).load().unwrap()[1].address,
        "added.example.test"
    );
}

#[gpui_test::test]
fn ssh_output_paths_never_resolve_against_local_project_files(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    let selection = cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.select_ssh_host(0, window, cx);
            let selection = this.file_reads.selection(0);
            let console = this.ensure_console(this.state.terminals().active_session, window, cx);
            console.update(cx, |_, cx| {
                cx.emit(tervia::terminal::FileLink {
                    path: "mock.rs".into(),
                    line: 1,
                    column: 1,
                })
            });
            selection
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.file_reads.selected(0, selection))
        })
    });
}

#[gpui_test::test]
fn identity_chooser_updates_only_the_form_that_opened_it(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.edit_ssh_host(None, window, cx);
            this.choose_ssh_identity(window, cx);
        })
    });
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![std::path::PathBuf::from("/mock keys/id_ed25519")])
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.ssh.form.as_ref().unwrap().inputs[4].read(cx).value(),
                "/mock keys/id_ed25519"
            );
            this.choose_ssh_identity(window, cx);
            window.close_dialog(cx);
            this.edit_ssh_host(None, window, cx);
        })
    });
    cx.simulate_path_prompt_response(|_| Some(vec![std::path::PathBuf::from("/mock keys/stale")]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert!(
                this.ssh.form.as_ref().unwrap().inputs[4]
                    .read(cx)
                    .value()
                    .is_empty()
            )
        })
    });
}

fn beta() -> Host {
    Host::from_json(&serde_json::json!({"name":"Mock beta","address":"beta.example.test"})).unwrap()
}

fn host_menu(cx: &mut gpui_test::VisualTestContext, selector: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds(selector).unwrap();
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
}

#[gpui_test::test]
fn host_context_menu_connects_the_clicked_host_only_after_confirmation(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.ssh.hosts.push(beta());
            this.select_ssh_host(0, window, cx);
        })
    });
    host_menu(cx, "ssh-host-1");
    view.update(cx, |this, _| {
        assert_eq!(this.ssh.active_host, Some(0));
        assert_eq!(this.state.terminals().sessions.len(), 2);
    });
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.ssh.active_host, Some(1));
        assert_eq!(this.state.active_tab(), Tab::Ssh);
        assert_eq!(this.state.terminals().sessions.len(), 3);
        assert_eq!(this.state.terminals().sessions[2].ssh, Some(beta()));
    });
}

#[gpui_test::test]
fn host_context_menu_edits_the_filtered_host_without_connecting_it(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.ssh.hosts.push(beta());
            this.select_ssh_host(0, window, cx);
            this.ssh
                .search
                .update(cx, |input, cx| input.set_value("beta", window, cx));
        })
    });
    host_menu(cx, "ssh-host-1");
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert!(window.has_active_dialog(cx));
            let form = this.ssh.form.as_ref().unwrap();
            assert_eq!(form.index, Some(1));
            assert_eq!(form.inputs[0].read(cx).value(), "Mock beta");
            assert_eq!(form.inputs[1].read(cx).value(), "beta.example.test");
            assert_eq!(this.ssh.active_host, Some(0));
            assert_eq!(this.state.terminals().sessions.len(), 2);
        })
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update(cx, |this, _| assert!(this.ssh.form.is_none()));
}

#[gpui_test::test]
fn host_context_menu_delete_preserves_the_other_host_and_its_terminals(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.ssh.hosts.push(beta());
            let mut store = Store::new(path.clone());
            store.load().unwrap();
            store.save(&this.ssh.hosts).unwrap();
            this.ssh.store = Some(store);
            this.select_ssh_host(0, window, cx);
            this.select_ssh_host(1, window, cx);
        })
    });
    host_menu(cx, "ssh-host-0");
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("ssh_cancel"));
    cx.run_until_parked();
    view.update(cx, |this, _| assert_eq!(this.ssh.hosts.len(), 2));
    host_menu(cx, "ssh-host-0");
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("ssh_remove"));
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.ssh.hosts, [beta()]);
        assert_eq!(this.ssh.active_host, Some(0));
        assert_eq!(this.state.terminals().sessions.len(), 3);
        assert_eq!(this.state.terminals().sessions[1].ssh, Some(host()));
        assert_eq!(this.state.terminals().sessions[2].ssh, Some(beta()));
    });
    assert_eq!(Store::new(path).load().unwrap(), [beta()]);
}

#[gpui_test::test]
fn renaming_a_host_keeps_its_existing_sessions_in_each_project(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let mut store = Store::new(path.clone());
            store.load().unwrap();
            this.ssh.hosts.push(beta());
            store.save(&this.ssh.hosts).unwrap();
            this.ssh.store = Some(store);
            this.select_ssh_host(0, window, cx);
            let original = this.ensure_console(1, window, cx);
            let other = this.state.open_directory("/synthetic/ssh-secondary".into());
            this.init_project(other, window, cx);
            this.select_ssh_host(0, window, cx);
            this.select_ssh_host(1, window, cx);
            this.state.switch_project(0);

            this.edit_ssh_host(Some(0), window, cx);
            this.ssh.form.as_ref().unwrap().inputs[0]
                .update(cx, |input, cx| input.set_value("Mock renamed", window, cx));
            assert!(this.save_ssh_host(cx));
            let mut renamed = host();
            renamed.name = "Mock renamed".into();
            assert_eq!(
                this.state.projects[0].terminal.sessions[1].ssh,
                Some(renamed.clone())
            );
            assert_eq!(
                this.state.projects[other].terminal.sessions[1].ssh,
                Some(renamed)
            );
            assert_eq!(
                this.state.projects[other].terminal.sessions[2].ssh,
                Some(beta())
            );
            this.select_ssh_host(0, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), 2);
            assert_eq!(this.state.terminals().active_session, 1);
            assert_eq!(
                this.ensure_console(1, window, cx).entity_id(),
                original.entity_id()
            );
        })
    });
    assert_eq!(Store::new(path).load().unwrap()[0].name, "Mock renamed");
}

#[gpui_test::test]
fn editing_connection_options_keeps_old_sessions_bound_to_their_original_host(
    cx: &mut TestAppContext,
) {
    let (view, cx) = workspace(cx);
    let root = tempfile::tempdir().unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let mut store = Store::new(root.path().join("ssh.json"));
            store.load().unwrap();
            store.save(&this.ssh.hosts).unwrap();
            this.ssh.store = Some(store);
            this.select_ssh_host(0, window, cx);
            for (field, value) in [
                (1, "changed.example.test"),
                (2, "mock-operator"),
                (3, "2222"),
                (4, "/mock keys/id_ed25519"),
            ] {
                let previous = this.ssh.hosts[0].clone();
                let session = this.state.terminals().active_session;
                let count = this.state.terminals().sessions.len();
                this.edit_ssh_host(Some(0), window, cx);
                let form = this.ssh.form.as_ref().unwrap();
                form.inputs[0].update(cx, |input, cx| input.set_value("Mock renamed", window, cx));
                form.inputs[field].update(cx, |input, cx| input.set_value(value, window, cx));
                assert!(this.save_ssh_host(cx));
                assert_eq!(this.state.terminals().sessions[session].ssh, Some(previous));
                assert!(this.selected_ssh_session().is_none());
                this.select_ssh_host(0, window, cx);
                assert_eq!(this.state.terminals().sessions.len(), count + 1);
                window.close_dialog(cx);
            }
        })
    });
}

#[gpui_test::test]
fn a_failed_profile_save_cannot_rename_live_sessions(cx: &mut TestAppContext) {
    let (view, cx) = workspace(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let mut store = Store::new(path.clone());
            store.load().unwrap();
            store.save(&this.ssh.hosts).unwrap();
            this.ssh.store = Some(store);
            this.select_ssh_host(0, window, cx);
            this.edit_ssh_host(Some(0), window, cx);
            this.ssh.form.as_ref().unwrap().inputs[0]
                .update(cx, |input, cx| input.set_value("Mock renamed", window, cx));
            std::fs::write(&path, "mock external edit").unwrap();
            assert!(!this.save_ssh_host(cx));
            assert_eq!(this.ssh.hosts[0], host());
            assert_eq!(this.state.terminals().sessions[1].ssh, Some(host()));
        })
    });
}

#[gpui_test::test]
fn host_context_menu_reconnects_an_exited_session_and_retains_the_old_console(
    cx: &mut TestAppContext,
) {
    let (view, cx) = workspace(cx);
    let previous = cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.select_ssh_host(0, window, cx);
            let console = this.ensure_console(1, window, cx);
            console.update(cx, |console, cx| {
                console.finish_process("Mock SSH exit".into(), cx)
            });
            console
        })
    });
    host_menu(cx, "ssh-host-0");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.active_tab(), Tab::Ssh);
            assert_eq!(this.state.terminals().sessions.len(), 3);
            assert_eq!(this.state.terminals().active_session, 2);
            assert_eq!(
                this.consoles()[1].as_ref().unwrap().entity_id(),
                previous.entity_id()
            );
            assert_ne!(
                this.ensure_console(2, window, cx).entity_id(),
                previous.entity_id()
            );
            assert_eq!(this.state.terminals().sessions[1].ssh, Some(host()));
            assert_eq!(this.state.terminals().sessions[2].ssh, Some(host()));
            this.select_ssh_host(0, window, cx);
            assert_eq!(this.state.terminals().sessions.len(), 3);
            assert_eq!(this.state.terminals().active_session, 2);
        })
    });
}
