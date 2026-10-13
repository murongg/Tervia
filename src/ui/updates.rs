use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use gpui_kit::assets::IconName;
use gpui_kit::base::Disableable;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::{
    preferences::Preferences,
    updates::{self, Candidate, Installation, Installed},
};

use super::{
    Tervia,
    controls::{icon, icon_button, label_button},
    theme::Palette,
};

pub(super) enum State {
    Idle,
    Checking,
    Latest,
    Available(Candidate),
    Installing,
    Installed(Installed),
    Failed {
        error: String,
        candidate: Option<Candidate>,
    },
}

impl State {
    #[cfg(all(test, feature = "ui-tests"))]
    pub(super) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn candidate(&self) -> Option<&Candidate> {
        match self {
            Self::Available(candidate)
            | Self::Failed {
                candidate: Some(candidate),
                ..
            } => Some(candidate),
            _ => None,
        }
    }

    fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

pub(super) struct Restart;

pub(super) struct Updater {
    pub(super) state: State,
    preferences: Entity<Preferences>,
    confirming: bool,
    progress: Option<Arc<(AtomicU64, AtomicU64)>>,
    _subscription: Subscription,
}

impl EventEmitter<Restart> for Updater {}

impl Updater {
    pub(super) fn new(preferences: Entity<Preferences>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&preferences, |_, _, cx| cx.notify());
        Self {
            state: State::Idle,
            preferences,
            confirming: false,
            progress: None,
            _subscription: subscription,
        }
    }

    pub(super) fn check(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || self.confirming || matches!(self.state, State::Installed(_)) {
            return;
        }
        self.state = State::Checking;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async { updates::check() })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.state = match result {
                    Ok(Some(candidate)) => State::Available(candidate),
                    Ok(None) => State::Latest,
                    Err(error) => State::Failed {
                        error,
                        candidate: None,
                    },
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn confirm_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirming || self.state.busy() {
            return;
        }
        let Some(candidate) = self.state.candidate().cloned() else {
            return;
        };
        let language = self.preferences.read(cx).language;
        let receiver = window.prompt(
            PromptLevel::Warning,
            language.text("update_install"),
            Some(language.text("update_confirm")),
            &[language.text("git_cancel"), language.text("update_install")],
            cx,
        );
        self.confirming = true;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let choice = receiver.await;
            let _ = weak.update(cx, |this, cx| {
                this.confirming = false;
                if choice == Ok(1) {
                    this.install(candidate, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install(&mut self, candidate: Candidate, cx: &mut Context<Self>) {
        let installation = match Installation::discover() {
            Ok(installation) => installation,
            Err(error) => {
                self.state = State::Failed {
                    error,
                    candidate: Some(candidate),
                };
                return;
            }
        };
        let progress = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        self.progress = Some(progress.clone());
        self.state = State::Installing;
        cx.spawn(async move |weak, cx| {
            let retry = candidate.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    updates::install(candidate, &installation, move |downloaded, total| {
                        progress.0.store(downloaded, Ordering::Relaxed);
                        progress.1.store(total.unwrap_or(0), Ordering::Relaxed);
                    })
                })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.progress = None;
                this.state = match result {
                    Ok(installed) => State::Installed(installed),
                    Err(error) => State::Failed {
                        error,
                        candidate: Some(retry),
                    },
                };
                cx.notify();
            });
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if !weak
                    .update(cx, |this, cx| {
                        let installing = matches!(this.state, State::Installing);
                        if installing {
                            cx.notify();
                        }
                        installing
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
}

impl Render for Updater {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preferences = self.preferences.read(cx);
        let language = preferences.language;
        let p = Palette::new(preferences.light_theme);
        let candidate = self.state.candidate().cloned();
        let status = match &self.state {
            State::Idle => String::new(),
            State::Checking => language.text("update_checking").into(),
            State::Latest => language.text("update_latest").into(),
            State::Available(candidate) => {
                language.format("update_available", &[("version", candidate.version())])
            }
            State::Installing => {
                let (downloaded, total) = self
                    .progress
                    .as_ref()
                    .map(|progress| {
                        (
                            progress.0.load(Ordering::Relaxed),
                            progress.1.load(Ordering::Relaxed),
                        )
                    })
                    .unwrap_or_default();
                let progress = if total > 0 {
                    format!(
                        "{}%",
                        downloaded
                            .saturating_mul(100)
                            .checked_div(total)
                            .unwrap_or(0)
                            .min(100)
                    )
                } else if downloaded > 0 {
                    format!("{:.1} MiB", downloaded as f64 / 1_048_576.0)
                } else {
                    String::new()
                };
                language.format("update_installing", &[("progress", &progress)])
            }
            State::Installed(installed) => {
                language.format("update_installed", &[("version", installed.version())])
            }
            State::Failed { .. } => language.text("update_error").into(),
        };
        let installed = matches!(self.state, State::Installed(_));
        let (status_icon, status_color) = match self.state {
            State::Latest | State::Installed(_) => (IconName::Check, p.green),
            State::Failed { .. } => (IconName::Info, p.red),
            _ => (IconName::Download, p.accent),
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(24.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text("updates")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .w_full()
                    .child(
                        img(Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            include_bytes!("../../assets/tervia.svg").to_vec(),
                        )))
                        .size(px(48.0))
                        .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Tervia"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "updates-version".into())
                                    .text_size(px(12.0))
                                    .text_color(p.secondary)
                                    .child(language.format(
                                        "update_version",
                                        &[("version", updates::VERSION)],
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "updates-check".into())
                            .flex_shrink_0()
                            .child(
                                label_button(
                                    "updates-check-button",
                                    language.text("update_check"),
                                    p,
                                )
                                .outline()
                                .h(px(32.0))
                                .px(px(12.0))
                                .loading(matches!(self.state, State::Checking))
                                .disabled(self.state.busy() || self.confirming || installed)
                                .on_click(cx.listener(|this, _, _, cx| this.check(cx))),
                            ),
                    ),
            )
            .child(div().h(px(1.0)).bg(p.border))
            .when(!matches!(self.state, State::Idle), |panel| {
                panel.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(12.0))
                        .child(div().mt(px(2.0)).child(icon(status_icon, status_color)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .debug_selector(|| "updates-status".into())
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(status),
                                )
                                .when(matches!(self.state, State::Failed { .. }), |body| {
                                    let State::Failed { error, .. } = &self.state else {
                                        unreachable!()
                                    };
                                    body.child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(p.secondary)
                                            .child(error.clone()),
                                    )
                                })
                                .when_some(candidate, |body, candidate| {
                                    let url = candidate.notes_url();
                                    body.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(16.0))
                                            .child(
                                                div()
                                                    .debug_selector(|| "updates-install".into())
                                                    .child(
                                                        label_button(
                                                            "updates-install-button",
                                                            language.text("update_install"),
                                                            p,
                                                        )
                                                        .primary()
                                                        .h(px(32.0))
                                                        .px(px(12.0))
                                                        .disabled(self.confirming)
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.confirm_install(window, cx)
                                                            },
                                                        )),
                                                    ),
                                            )
                                            .child(
                                                label_button(
                                                    "updates-notes",
                                                    language.text("update_notes"),
                                                    p,
                                                )
                                                .on_click(move |_, _, cx| cx.open_url(&url)),
                                            ),
                                    )
                                })
                                .when(installed, |body| {
                                    body.child(
                                        div().debug_selector(|| "updates-restart".into()).child(
                                            label_button(
                                                "updates-restart-button",
                                                language.text("update_restart"),
                                                p,
                                            )
                                            .primary()
                                            .h(px(32.0))
                                            .px(px(12.0))
                                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Restart))),
                                        ),
                                    )
                                }),
                        ),
                )
            })
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.secondary)
                    .child(language.text("update_source")),
            )
    }
}

impl Tervia {
    pub(super) fn workspace_update(&self, cx: &mut Context<Self>) -> Option<Div> {
        let updater = self.updater.read(cx);
        let installed = matches!(updater.state, State::Installed(_));
        let installing = matches!(updater.state, State::Installing);
        let label = if installed {
            self.t("update_restart").to_owned()
        } else if installing {
            self.t("update_wait").to_owned()
        } else {
            let candidate = updater.state.candidate()?;
            format!(
                "{} · {}",
                self.f("update_available", &[("version", candidate.version())]),
                self.t("update_install")
            )
        };
        let p = self.p();
        Some(
            div()
                .debug_selector(|| "workspace-update".into())
                .relative()
                .size(px(28.0))
                .child(
                    icon_button(
                        "workspace-update-button",
                        if installed {
                            IconName::RefreshCw
                        } else {
                            IconName::Download
                        },
                        label.clone(),
                        p,
                        cx,
                    )
                    .icon(icon(
                        if installed {
                            IconName::RefreshCw
                        } else {
                            IconName::Download
                        },
                        p.accent,
                    ))
                    .accessibility_label(label)
                    .loading(installing)
                    .disabled(installing || updater.confirming)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if matches!(this.updater.read(cx).state, State::Installed(_)) {
                            this.restart_after_update(window, cx);
                        } else {
                            this.updater
                                .update(cx, |updater, cx| updater.confirm_install(window, cx));
                        }
                    })),
                )
                .when(!installing, |button| {
                    button.child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .right(px(2.0))
                            .size(px(5.0))
                            .rounded_full()
                            .bg(p.accent),
                    )
                }),
        )
    }

    pub(crate) fn try_quit(&mut self, cx: &mut Context<Self>) -> bool {
        if matches!(self.updater.read(cx).state, State::Installing) {
            // Normal quit/close must not interrupt the bundle's transactional replacement.
            self.state.notice = self.t("update_wait").into();
            cx.notify();
            return false;
        }
        true
    }

    pub(super) fn update_restart_allowed(&self) -> bool {
        !self.git_mutating
            && !self.saving_file
            && !self.file_closing
            && !self.file_reads.has_reloads()
            && !self
                .state
                .projects
                .iter()
                .any(|project| project.files.iter().any(|file| file.dirty()))
    }

    pub(super) fn restart_after_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_restarting {
            return;
        }
        let State::Installed(installed) = &self.updater.read(cx).state else {
            return;
        };
        let installed = installed.clone();
        if !self.update_restart_allowed() {
            drop(window.prompt(
                PromptLevel::Warning,
                self.t("update_restart"),
                Some(self.t("update_restart_blocked")),
                &[self.t("close")],
                cx,
            ));
            return;
        }
        let receiver = window.prompt(
            PromptLevel::Warning,
            self.t("update_restart"),
            Some(self.t("update_restart_confirm")),
            &[self.t("git_cancel"), self.t("update_restart")],
            cx,
        );
        self.update_restarting = true;
        cx.spawn_in(window, async move |weak, cx| {
            let choice = receiver.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.update_restarting = false;
                if choice != Ok(1) {
                    return;
                }
                // A settings-window confirmation can stay open while the workspace is edited.
                // Recheck every owner and pending write after confirmation, before ending any PTY.
                if !this.update_restart_allowed() {
                    drop(window.prompt(
                        PromptLevel::Warning,
                        this.t("update_restart"),
                        Some(this.t("update_restart_blocked")),
                        &[this.t("close")],
                        cx,
                    ));
                    return;
                }
                match installed.relaunch() {
                    Ok(()) => {
                        this.shutdown_terminals(cx);
                        cx.quit();
                    }
                    Err(error) => {
                        this.state.notice = format!("{}\n{error}", this.t("update_error"));
                        drop(window.prompt(
                            PromptLevel::Warning,
                            this.t("update_error"),
                            Some(&error),
                            &[this.t("close")],
                            cx,
                        ));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}
