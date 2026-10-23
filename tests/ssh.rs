use serde_json::json;
use tervia::ssh::{Host, Store, aliases};

fn host() -> Host {
    Host::from_json(&json!({"name":"Mock server", "address":"mock.example.test", "user":"mock-user", "port":2222, "identity":"/mock keys/id_ed25519"})).unwrap()
}

#[test]
fn ssh_arguments_preserve_paths_and_do_not_use_a_shell() {
    let host = host();
    let terminal = host.arguments().unwrap();
    assert_eq!(terminal.last().unwrap(), "mock.example.test");
    assert!(
        terminal
            .windows(2)
            .any(|v| v == ["-i", "/mock keys/id_ed25519"])
    );
    assert!(terminal.windows(2).any(|v| v == ["-p", "2222"]));
    assert!(terminal.windows(2).any(|v| v == ["-l", "mock-user"]));
    assert_eq!(Host::from_json(&host.to_json()).unwrap(), host);
}

#[test]
fn host_validation_rejects_options_control_characters_and_invalid_ports() {
    for address in [
        "-oProxyCommand=mock",
        "mock;command",
        "mock host",
        "user@host",
        "mock\nhost",
        "",
    ] {
        let mut value = host().to_json();
        value["address"] = json!(address);
        assert!(Host::from_json(&value).is_err(), "{address:?}");
    }
    for port in [json!(0), json!(65536), json!("22")] {
        let mut value = host().to_json();
        value["port"] = port;
        assert!(Host::from_json(&value).is_err());
    }
    for address in [
        "mock-alias",
        "2001:db8::1",
        "[2001:db8::2]",
        "mock.example.test",
    ] {
        let mut value = host().to_json();
        value["address"] = json!(address);
        assert!(Host::from_json(&value).is_ok());
    }
}

#[test]
fn config_discovery_lists_literal_aliases_without_resolving_or_executing_config() {
    let hosts = aliases("# mock config\nHost mock-alpha mock-beta *.example.test !mock-denied\n HostName example.test\n User mock-user\nHost=\"mock-gamma\"\nMatch exec \"touch /never-execute\"\nHost mock-alpha\n").unwrap();
    assert_eq!(
        hosts.iter().map(|h| h.address.as_str()).collect::<Vec<_>>(),
        ["mock-alpha", "mock-beta", "mock-gamma"]
    );
    assert!(hosts.iter().all(|h| h.user.is_empty() && h.port.is_none()));
}

#[test]
fn host_store_round_trips_and_refuses_external_edits_or_corrupt_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    let mut store = Store::new(path.clone());
    assert!(store.load().unwrap().is_empty());
    store.save(&[host()]).unwrap();
    assert_eq!(Store::new(path.clone()).load().unwrap(), [host()]);
    std::fs::write(&path, "{\"version\":1,\"hosts\":[]}").unwrap();
    assert!(store.save(&[]).is_err());
    assert!(Store::new(path.clone()).load().unwrap().is_empty());
    std::fs::write(&path, "invalid mock json").unwrap();
    let mut broken = Store::new(path.clone());
    assert!(broken.load().is_err());
    assert!(broken.save(&[]).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "invalid mock json");
}

#[test]
fn host_store_never_writes_a_file_it_cannot_load_again() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    let hosts = (0..256)
        .map(|i| {
            let mut h = host();
            h.name = format!("Mock host {i}");
            h.identity = Some(format!("/mock/{}", "x".repeat(5000)).into());
            h
        })
        .collect::<Vec<_>>();
    assert!(store.save(&hosts).is_err());
    assert!(!path.exists());
}

#[cfg(feature = "pty")]
#[test]
fn ssh_terminal_launch_uses_openssh_and_the_terminal_environment() {
    let command =
        tervia::pty::ssh_command_for(std::path::Path::new("/mock project"), &host()).unwrap();
    assert_eq!(command.get_argv()[0], "ssh");
    assert_eq!(command.get_argv().last().unwrap(), "mock.example.test");
    assert_eq!(command.get_env("TERM").unwrap(), "xterm-256color");
}

#[test]
fn ssh_module_and_terminal_identity_survive_workspace_recovery() {
    use tervia::state::{Tab, Workspace};
    let root = tempfile::tempdir().unwrap();
    let mut state = Workspace::new(root.path().to_owned());
    state.set_tab(Tab::Ssh);
    let session = &mut state.project_mut().terminal.sessions[0];
    session.ssh = Some(host());
    session.connection = Some("0123456789abcdef0123456789abcdef".into());
    let path = root.path().join("workspace.json");
    let mut store = tervia::workspace::Store::new(path);
    store.load().unwrap();
    store.save(&state).unwrap();
    let restored = store.load().unwrap();
    assert_eq!(restored.project().tab, Tab::Ssh);
    assert_eq!(restored.project().terminal.sessions[0].ssh, Some(host()));
}
