use tervia::platform::Platform;

#[test]
fn platforms_have_native_fonts_shells_and_clear_shortcuts() {
    assert_eq!(Platform::Mac.mono_font(), "Menlo");
    assert_eq!(Platform::Windows.mono_font(), "Consolas");
    assert_eq!(Platform::Linux.mono_font(), "DejaVu Sans Mono");
    assert_eq!(Platform::Mac.default_shell(), "/bin/zsh");
    assert_eq!(Platform::Windows.default_shell(), "powershell.exe");
    assert_eq!(Platform::Linux.default_shell(), "/bin/bash");
    assert_eq!(Platform::Windows.hint("Clear · ⌘K"), "Clear · Ctrl+Shift+K");
    assert_eq!(Platform::Linux.hint("New · ⌘T"), "New · Ctrl+Shift+T");
}

#[test]
fn shell_arguments_keep_interactive_profiles_enabled() {
    assert_eq!(Platform::shell_args("/mock/bin/zsh"), &["-l", "-i"]);
    assert_eq!(
        Platform::shell_args("C:\\Windows\\PowerShell.EXE"),
        &["-NoLogo", "-NoExit"]
    );
    assert_eq!(Platform::shell_args("C:\\mock\\cmd.exe"), &["/K"]);
}
