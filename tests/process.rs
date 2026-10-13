#![cfg(feature = "pty")]

use tervia::{
    agent::Agent,
    process::{WindowsArgs, descendants, linux_group},
};

#[test]
fn ownership_and_linux_group_filtering_do_not_include_other_processes() {
    assert_eq!(
        descendants(10, &[(10, 1), (11, 10), (12, 11), (99, 98), (10, 12)]),
        vec![10, 11, 12]
    );
    assert_eq!(linux_group("44 (mock worker (name)) S 1 23 4 5"), Some(23));
    assert_eq!(linux_group("malformed synthetic stat"), None);
}

#[test]
fn windows_launcher_paths_and_quoting_are_parsed_without_later_prompt_arguments() {
    let data: Vec<u16> = r#""C:\Program Files\nodejs\node.exe" "C:\mock\node_modules\@google\gemini-cli\dist\index.js" "synthetic prompt""#.encode_utf16().collect();
    assert_eq!(
        Agent::from_argv("C:\\mock\\node.exe", WindowsArgs::new(&data)),
        Some(Agent::Gemini)
    );
    let data: Vec<u16> = r#"python.exe -m hermes_cli.main "synthetic prompt""#
        .encode_utf16()
        .collect();
    assert_eq!(
        Agent::from_argv("C:\\mock\\python.exe", WindowsArgs::new(&data)),
        Some(Agent::Hermes)
    );
}
