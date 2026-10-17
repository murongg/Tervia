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
use gpui_kit::component::{progress::ProgressCircle, tooltip::Tooltip};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::{
    preferences::Preferences,
    updates::{self, Candidate, Installation, Installed, Schedule},
};

use super::{
    Tervia,
    controls::{icon, icon_button, label_button},
    theme::Palette,
};

mod options;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;

type Checker = Arc<dyn Fn() -> Result<Option<Candidate>, String> + Send + Sync>;

pub(super) enum State {
    Idle,
    Checking(Option<Candidate>),
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
            | Self::Checking(Some(candidate))
            | Self::Failed {
                candidate: Some(candidate),
                ..
            } => Some(candidate),
            _ => None,
        }
    }

    fn busy(&self) -> bool {
        matches!(self, Self::Checking(_) | Self::Installing)
    }
}

pub(super) struct Restart;

pub(super) struct Updater {
    pub(super) state: State,
    preferences: Entity<Preferences>,
    confirming: bool,
    progress: Option<Arc<(AtomicU64, AtomicU64)>>,
    checker: Checker,
    schedule: Schedule,
    automatic_enabled: bool,
    automatic_task: Option<Task<()>>,
    _subscription: Subscription,
}

impl EventEmitter<Restart> for Updater {}

impl Updater {
    pub(super) fn new(preferences: Entity<Preferences>, cx: &mut Context<Self>) -> Self {
        Self::with_checker(preferences, Arc::new(updates::check), cx)
    }

    fn with_checker(
        preferences: Entity<Preferences>,
        checker: Checker,
        cx: &mut Context<Self>,
    ) -> Self {
        let schedule = Schedule::new(preferences.read(cx).updates);
        let subscription = cx.observe(&preferences, |this, preferences, cx| {
            let settings = preferences.read(cx).updates;
            if settings != this.schedule.settings() {
                this.schedule.configure(settings);
                this.schedule_next(cx);
            }
            cx.notify();
        });
        Self {
            state: State::Idle,
            preferences,
            confirming: false,
            progress: None,
            checker,
            schedule,
            automatic_enabled: false,
            automatic_task: None,
            _subscription: subscription,
        }
    }

    pub(super) fn start_automatic(&mut self, cx: &mut Context<Self>) {
        self.automatic_enabled = true;
        self.schedule_next(cx);
    }

    fn schedule_next(&mut self, cx: &mut Context<Self>) {
        self.automatic_task = None;
        if !self.automatic_enabled
            || self.confirming
            || self.state.busy()
            || matches!(self.state, State::Installed(_))
        {
            return;
        }
        let Some(delay) = self.schedule.delay(cx.background_executor().now()) else {
            return;
        };
        self.automatic_task = Some(cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(delay).await;
            let _ = weak.update(cx, |this, cx| {
                this.automatic_task = None;
                this.check(cx);
            });
        }));
    }

    pub(super) fn check(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || self.confirming || matches!(self.state, State::Installed(_)) {
            return;
        }
        self.automatic_task = None;
        let previous = self.state.candidate().cloned();
        // Keep an already discovered update visible while polling or temporarily offline.
        self.state = State::Checking(previous.clone());
        let checker = self.checker.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { checker() })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.state = match result {
                    Ok(Some(candidate)) => State::Available(candidate),
                    Ok(None) => State::Latest,
                    Err(error) => State::Failed {
                        error,
                        candidate: previous,
                    },
                };
                this.schedule.record_check(cx.background_executor().now());
                this.schedule_next(cx);
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
        self.schedule_next(cx);
        cx.notify();
        let window = window.window_handle();
        cx.spawn(async move |weak, cx| {
            let choice = receiver.await;
            let _ = weak.update(cx, |this, cx| {
                this.confirming = false;
                if choice == Ok(1) {
                    this.install(candidate, window, cx);
                }
                this.schedule_next(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn install(&mut self, candidate: Candidate, window: AnyWindowHandle, cx: &mut Context<Self>) {
        let installation = match Installation::discover() {
            Ok(installation) => installation,
            Err(error) => {
                self.install_failed(error, candidate, window, cx);
                return;
            }
        };
        let progress = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        self.progress = Some(progress.clone());
        self.state = State::Installing;
        self.schedule_next(cx);
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
                match result {
                    Ok(installed) => this.state = State::Installed(installed),
                    Err(error) => this.install_failed(error, retry, window, cx),
                }
                this.schedule_next(cx);
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

    fn install_failed(
        &mut self,
        error: String,
        candidate: Candidate,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.state = State::Failed {
            error: error.clone(),
            candidate: Some(candidate),
        };
        cx.notify();
        let language = self.preferences.read(cx).language;
        // Toolbar installs need a visible error without requiring users to open settings.
        // Update shared state first so closing the initiating window cannot lose the result.
        let _ = window.update(cx, |_, window, cx| {
            window.prompt(
                PromptLevel::Warning,
                language.text("update_error"),
                Some(&error),
                &[language.text("close")],
                cx,
            )
        });
    }

    fn progress_status(&self) -> (Option<f32>, String) {
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
        let percentage =
            (total > 0).then(|| ((downloaded as f64 / total as f64) * 100.0).min(100.0) as f32);
        let progress = if let Some(percentage) = percentage {
            format!("{}%", percentage.floor() as u32)
        } else if downloaded > 0 {
            format!("{:.1} MiB", downloaded as f64 / 1_048_576.0)
        } else {
            String::new()
        };
        (percentage, progress)
    }

    fn progress_circle(&self, cx: &App) -> ProgressCircle {
        let preferences = self.preferences.read(cx);
        let (percentage, progress) = self.progress_status();
        ProgressCircle::new("workspace-update-progress-circle")
            .color(Palette::new(preferences.light_theme).accent)
            .w(px(18.0))
            .h(px(18.0))
            .value(percentage.unwrap_or(0.0))
            // The download reaching 100% still leaves verification and installation to finish.
            .loading(percentage.is_none_or(|value| value >= 100.0))
            .accessibility_label(
                preferences
                    .language
                    .format("update_installing", &[("progress", &progress)]),
            )
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
            State::Checking(None) => language.text("update_checking").into(),
            State::Checking(Some(candidate)) => {
                language.format("update_available", &[("version", candidate.version())])
            }
            State::Latest => language.text("update_latest").into(),
            State::Available(candidate) => {
                language.format("update_available", &[("version", candidate.version())])
            }
            State::Installing => {
                let (_, progress) = self.progress_status();
                language.format("update_installing", &[("progress", &progress)])
            }
            State::Installed(installed) => {
                language.format("update_installed", &[("version", installed.version())])
            }
            State::Failed { .. } => language.text("update_error").into(),
        };
        let installed = matches!(self.state, State::Installed(_));
        let install_disabled = self.confirming || self.state.busy();
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
                                .loading(matches!(self.state, State::Checking(_)))
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
                                                        .bg(if install_disabled {
                                                            p.raised
                                                        } else {
                                                            p.accent
                                                        })
                                                        .text_color(if install_disabled {
                                                            p.muted
                                                        } else {
                                                            p.on_accent
                                                        })
                                                        .h(px(32.0))
                                                        .px(px(12.0))
                                                        .disabled(install_disabled)
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
                                            .bg(p.accent)
                                            .text_color(p.on_accent)
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
            .child(self.options(cx))
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
            let (_, progress) = updater.progress_status();
            self.f("update_installing", &[("progress", &progress)])
        } else {
            let candidate = updater.state.candidate()?;
            format!(
                "{} · {}",
                self.f("update_available", &[("version", candidate.version())]),
                self.t("update_install")
            )
        };
        let p = self.p();
        let progress_label = label.clone();
        Some(
            div()
                .debug_selector(|| "workspace-update".into())
                .relative()
                .size(px(28.0))
                .when(installing, |container| {
                    container.child(
                        div()
                            .id("workspace-update-progress")
                            .debug_selector(|| "workspace-update-progress".into())
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(updater.progress_circle(cx))
                            .tooltip(move |window, cx| {
                                Tooltip::new(progress_label.clone()).build(window, cx)
                            }),
                    )
                })
                .when(!installing, |container| {
                    container.child(
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
                        .disabled(updater.state.busy() || updater.confirming)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if matches!(this.updater.read(cx).state, State::Installed(_)) {
                                this.restart_after_update(window, cx);
                            } else {
                                this.updater
                                    .update(cx, |updater, cx| updater.confirm_install(window, cx));
                            }
                        })),
                    )
                })
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
        self.flush_terminals(cx)
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
                this.capture_reading_positions(cx);
                if !this.flush_terminals(cx) {
                    return;
                }
                this.capture_terminal_positions(cx);
                // The replacement process may load immediately; ownership must be durable before relaunch.
                if let Some(store) = &mut this.workspace
                    && let Err(error) = store.save(&this.state)
                {
                    this.state.notice = format!("{}\n{error}", this.t("workspace_error"));
                    cx.notify();
                    return;
                }
                this.detach_terminals(cx);
                match installed.relaunch() {
                    Ok(()) => {
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
