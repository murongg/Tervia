use super::*;
use crate::ui::controls::{icon, icon_button};
use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement as _;
use tervia::{git::ErrorKind, locale::Language};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    Probe,
    Action,
}

struct Failure {
    error: tervia::git::Error,
    details: String,
    origin: Origin,
}

#[derive(Default)]
pub(in crate::ui) struct ErrorState {
    failure: Option<Failure>,
    expanded: bool,
}

impl ErrorState {
    pub fn is_empty(&self) -> bool {
        self.failure.is_none()
    }

    pub fn clear(&mut self) {
        self.failure = None;
        self.expanded = false;
    }

    pub fn action(&mut self, error: impl Into<tervia::git::Error>) {
        self.record(error.into(), Origin::Action);
    }

    pub fn probe(&mut self, error: tervia::git::Error) {
        // Passive failures must not replace the diagnostic from a failed user action.
        if !self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.origin == Origin::Action)
        {
            self.record(error, Origin::Probe);
        }
    }

    pub fn recovered(&mut self) {
        if self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.origin == Origin::Probe)
        {
            self.clear();
        }
    }

    pub(super) fn set(&mut self, error: tervia::git::Error, origin: Origin) {
        match origin {
            Origin::Action => self.action(error),
            Origin::Probe => self.probe(error),
        }
    }

    fn record(&mut self, error: tervia::git::Error, origin: Origin) {
        let details = error.details();
        if !self.failure.as_ref().is_some_and(|previous| {
            previous.origin == origin
                && previous.error.kind() == error.kind()
                && previous.details == details
        }) {
            self.expanded = false;
        }
        self.failure = Some(Failure {
            error,
            details,
            origin,
        });
    }

    pub fn details(&self) -> Option<&str> {
        self.failure
            .as_ref()
            .map(|failure| failure.details.as_str())
    }
}

fn summary_key(kind: ErrorKind) -> Option<&'static str> {
    Some(match kind {
        ErrorKind::MissingExecutable => "git_error_missing",
        ErrorKind::Start => "git_error_start",
        ErrorKind::Io => "git_error_io",
        ErrorKind::Timeout => "git_error_timeout",
        ErrorKind::Command => "git_error_command",
        ErrorKind::Message => return None,
    })
}

fn summary(error: &tervia::git::Error, language: Language) -> String {
    let mut text = match summary_key(error.kind()) {
        None => error.message().to_owned(),
        Some(key) if error.kind() == ErrorKind::Command && !error.message().is_empty() => {
            // Command errors can carry actionable guidance such as the output capture limit.
            format!("{} {}", language.text(key), error.message())
        }
        Some(key) => language.text(key).to_owned(),
    };
    // Partial-success guidance must survive every error kind without exposing raw causes.
    if let Some(guidance) = error.guidance() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(guidance);
    }
    text
}

impl Tervia {
    pub(super) fn git_error(&self, project: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.git_p();
        let state = &self.git_states[project].error;
        let failure = state
            .failure
            .as_ref()
            .expect("Error presentation requires a failure");
        let key = if state.expanded {
            "git_hide_details"
        } else {
            "git_show_details"
        };
        div()
            .id("git-error")
            .debug_selector(|| "git-error".into())
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(8.0))
            .bg(p.red.opacity(0.08))
            .border_b_1()
            .border_color(p.red.opacity(0.25))
            .text_size(px(12.0))
            .text_color(p.red)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(icon(IconName::TriangleAlert, p.red))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(summary(&failure.error, self.appearance.language)),
                    )
                    .child(
                        label_button("git-toggle-error-details", self.t(key), p)
                            .debug_selector(|| "git-toggle-error-details".into())
                            .accessibility_label(self.t(key))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let error = &mut this.git_states[project].error;
                                error.expanded = !error.expanded;
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_button(
                            "git-copy-error",
                            IconName::Copy,
                            self.t("git_copy_error"),
                            p,
                            cx,
                        )
                        .debug_selector(|| "git-copy-error".into())
                        .accessibility_label(self.t("git_copy_error"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(details) = this.git_states[project].error.details() {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    details.to_owned(),
                                ));
                            }
                        })),
                    ),
            )
            .when(state.expanded, |body| {
                body.child(
                    div()
                        .id("git-error-details")
                        .debug_selector(|| "git-error-details".into())
                        .max_h(px(160.0))
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .child(failure.details.clone()),
                )
            })
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn missing_executable_kind_uses_translated_install_guidance() {
        let key = summary_key(ErrorKind::MissingExecutable).unwrap();
        assert_eq!(
            Language::English.text(key),
            "Git is unavailable. Install Git and make it available on PATH."
        );
        for language in Language::ALL
            .into_iter()
            .filter(|language| *language != Language::System)
        {
            assert_ne!(language.text(key), key);
            if language != Language::English {
                assert_ne!(language.text(key), Language::English.text(key));
            }
        }
        assert!(
            Language::English
                .text(summary_key(ErrorKind::Timeout).unwrap())
                .contains("partially completed")
        );
        assert_eq!(
            summary(
                &tervia::git::Error::from("Synthetic conflict guidance"),
                Language::English
            ),
            "Synthetic conflict guidance"
        );
        assert_eq!(summary_key(ErrorKind::Start), Some("git_error_start"));
        assert_eq!(summary_key(ErrorKind::Io), Some("git_error_io"));
        assert_eq!(summary_key(ErrorKind::Command), Some("git_error_command"));
    }

    #[gpui_test::test]
    fn collapsed_stash_cleanup_io_error_keeps_the_successful_apply_warning(
        cx: &mut gpui_test::TestAppContext,
    ) {
        use std::{fs, process::Command};
        cx.update(gpui_kit::init);
        let root = tempfile::tempdir().unwrap();
        let repo = Repository::initialize(root.path()).unwrap();
        for (key, value) in [
            ("user.name", "Synthetic Developer"),
            ("user.email", "synthetic@example.test"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
            ("core.hooksPath", ".synthetic-hooks"),
            ("core.autocrlf", "false"),
        ] {
            assert!(
                Command::new("git")
                    .current_dir(root.path())
                    .args(["config", "--local", key, value])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let path = root.path().join("mock.txt");
        fs::write(&path, "synthetic base\n").unwrap();
        repo.execute(Operation::StageAll).unwrap();
        repo.execute(Operation::Commit {
            message: "Synthetic initial commit".into(),
            amend: false,
        })
        .unwrap();
        fs::write(&path, "synthetic stashed change\n").unwrap();
        repo.execute(Operation::StashPush("Synthetic stash".into()))
            .unwrap();
        let stash = repo.snapshot(10).unwrap().stashes[0].clone();
        assert_eq!(fs::read_to_string(&path).unwrap(), "synthetic base\n");
        let lock = root.path().join(".git/refs/stash.lock");
        fs::write(&lock, "synthetic held lock\n").unwrap();
        let error = repo
            .execute(Operation::StashPop {
                reference: stash.reference,
                expected: stash.hash.clone(),
            })
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Io);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "synthetic stashed change\n",
            "Stash contents were successfully applied before cleanup failed"
        );
        let snapshot = repo.snapshot(10).unwrap();
        assert!(
            snapshot
                .stashes
                .iter()
                .any(|entry| entry.hash == stash.hash),
            "The stash remains available after cleanup failure"
        );
        assert_eq!(fs::read_to_string(lock).unwrap(), "synthetic held lock\n");
        let warning = "Stash applied; automatic removal did not complete";
        assert!(error.details().contains(warning));
        assert_eq!(error.guidance(), Some(warning));
        let cause = error.message().to_owned();
        let collapsed = summary(&error, Language::English);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut this = Tervia::new(window, cx);
                let project = this
                    .state
                    .open_directory(root.path().canonicalize().unwrap());
                this.init_project(project, window, cx);
                this.state.set_tab(Tab::Git);
                let ui = &mut this.git_states[project];
                ui.repository = Some(repo);
                ui.snapshot = Some(Arc::new(snapshot));
                ui.error.action(error);
                this
            });
            gpui_kit::base::Root::new(entity, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("git-error").is_some());
        assert!(
            cx.debug_bounds("git-error-details").is_none(),
            "The warning must remain visible without expanding diagnostic details"
        );
        assert!(
            collapsed.contains(warning),
            "Collapsed summary hides the successful apply: {collapsed}"
        );
        assert!(
            !collapsed.contains(&cause),
            "Raw cleanup causes remain in details"
        );
    }

    #[test]
    fn collapsed_io_summary_keeps_raw_causes_private_and_legacy_messages_visible() {
        let raw = tervia::git::Error::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "synthetic metadata path",
        ));
        assert_eq!(
            summary(&raw, Language::English),
            Language::English.text("git_error_io")
        );
        let limit = "Git output exceeds 8 MiB; narrow the comparison or history.";
        assert_eq!(
            summary(&tervia::git::Error::from(limit), Language::English),
            limit
        );
        assert_eq!(
            summary(
                &tervia::git::Error::from("Synthetic conflict resolution guidance"),
                Language::English
            ),
            "Synthetic conflict resolution guidance"
        );
    }

    #[test]
    fn unchanged_polling_keeps_details_open_and_a_new_error_collapses_them() {
        let mut state = ErrorState::default();
        state.probe("synthetic probe failure".into());
        state.expanded = true;
        state.probe("synthetic probe failure".into());
        assert!(state.expanded);
        state.probe("different synthetic probe failure".into());
        assert!(!state.expanded);
        state.expanded = true;
        state.action("synthetic action failure");
        assert!(!state.expanded);
        state.expanded = true;
        state.probe("synthetic passive failure".into());
        state.recovered();
        assert_eq!(state.details(), Some("synthetic action failure"));
        assert!(state.expanded);
    }
}
