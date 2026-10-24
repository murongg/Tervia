use super::{
    Tervia,
    controls::{icon, icon_button, label_button, list_row, toolbar},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dialog::{DialogAction, DialogClose, DialogEntrance, DialogFooter},
    input::{Input, InputState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::ssh::{Host, Store};

#[cfg(all(test, feature = "ui-tests"))]
mod tests;
mod views;

pub(super) struct Manager {
    hosts: Vec<Host>,
    store: Option<Store>,
    search: Entity<InputState>,
    active_host: Option<usize>,
    form: Option<Form>,
    error: Option<String>,
}

#[derive(Clone)]
struct Form {
    index: Option<usize>,
    inputs: Vec<Entity<InputState>>,
}

impl Manager {
    pub fn new(
        path: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Tervia>,
    ) -> Self {
        let mut store = path.map(Store::new);
        let result = store.as_mut().map_or(Ok(Vec::new()), Store::load);
        let (hosts, error) = match result {
            Ok(hosts) => (hosts, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Self {
            hosts,
            store,
            error,
            search: cx.new(|cx| InputState::new(window, cx)),
            active_host: None,
            form: None,
        }
    }
    fn selected(&self) -> Option<&Host> {
        self.active_host.and_then(|i| self.hosts.get(i))
    }
    fn commit(&mut self, hosts: Vec<Host>) -> Result<(), String> {
        self.store
            .as_mut()
            .ok_or("SSH configuration is unavailable")?
            .save(&hosts)?;
        self.hosts = hosts;
        Ok(())
    }
}

impl Tervia {
    pub(super) fn subscribe_ssh(&mut self, cx: &mut Context<Self>) {
        self.subscriptions
            .push(cx.observe(&self.ssh.search, |_, _, cx| cx.notify()));
    }

    fn edit_ssh_host(&mut self, index: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let host = index.and_then(|i| self.ssh.hosts.get(i));
        let values = host
            .map(|h| {
                vec![
                    h.name.clone(),
                    h.address.clone(),
                    h.user.clone(),
                    h.port.map(|p| p.to_string()).unwrap_or_default(),
                    h.identity
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ]
            })
            .unwrap_or_else(|| vec![String::new(); 5]);
        let language = self.appearance.language;
        let placeholders = [
            "ssh_name_example",
            "ssh_address_example",
            "ssh_inherit",
            "ssh_inherit",
            "ssh_identity_example",
        ];
        let inputs = values
            .into_iter()
            .zip(placeholders)
            .map(|(value, key)| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value)
                        .placeholder(language.text(key))
                })
            })
            .collect::<Vec<_>>();
        self.ssh.form = Some(Form { index, inputs });
        self.ssh.error = None;
        let weak = cx.weak_entity();
        let initial_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            let Some(view) = weak.upgrade() else {
                return dialog;
            };
            let this = view.read(cx);
            let Some(form) = this.ssh.form.clone() else {
                return dialog;
            };
            let p = this.p();
            let language = this.appearance.language;
            let error = this.ssh.error.clone();
            let confirm = weak.clone();
            let closed = weak.clone();
            if initial_focus.replace(false) {
                form.inputs[0].update(cx, |input, cx| input.focus(window, cx));
            }
            let chooser = weak.clone();
            let field = |index: usize, key: &'static str| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.secondary)
                            .child(language.text(key)),
                    )
                    .child(Input::new(&form.inputs[index]).aria_label(language.text(key)))
            };
            dialog
                .w(px(520.0))
                .h(px(480.0))
                // Keep focus and hit targets at their final positions from the first frame.
                .entrance(DialogEntrance::None)
                .p(px(24.0))
                .bg(p.sidebar)
                .border_color(p.border)
                .rounded(px(10.0))
                .margin_top(((window.viewport_size().height - px(480.0)) / 2.0).max(px(16.0)))
                .title(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(9.0))
                                .child(icon(IconName::Server, p.accent).size(px(20.0)))
                                .child(
                                    div()
                                        .text_size(px(18.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(language.text(if form.index.is_some() {
                                            "ssh_edit"
                                        } else {
                                            "ssh_add"
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(p.secondary)
                                .child(language.text("ssh_form_hint")),
                        ),
                )
                .overlay_closable(false)
                .child(
                    div()
                        .debug_selector(|| "ssh-host-dialog".into())
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        .child(field(0, "ssh_name"))
                        .child(field(1, "ssh_address"))
                        .child(
                            div()
                                .pt(px(16.0))
                                .border_t_1()
                                .border_color(p.border)
                                .flex()
                                .flex_col()
                                .gap(px(14.0))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(16.0))
                                        .child(div().flex_1().min_w_0().child(field(2, "ssh_user")))
                                        .child(
                                            div()
                                                .w(px(120.0))
                                                .flex_shrink_0()
                                                .child(field(3, "ssh_port")),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(p.secondary)
                                                .child(language.text("ssh_identity")),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div().flex_1().min_w_0().child(
                                                        Input::new(&form.inputs[4]).aria_label(
                                                            language.text("ssh_identity"),
                                                        ),
                                                    ),
                                                )
                                                .child(
                                                    Button::new("ssh-choose-identity")
                                                        .ghost()
                                                        .icon(icon(
                                                            IconName::FolderOpen,
                                                            p.secondary,
                                                        ))
                                                        .label(language.text("ssh_browse"))
                                                        .on_click(move |_, window, cx| {
                                                            let _ =
                                                                chooser.update(cx, |this, cx| {
                                                                    this.choose_ssh_identity(
                                                                        window, cx,
                                                                    )
                                                                });
                                                        }),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(p.secondary)
                                        .child(language.text("ssh_config_hint")),
                                ),
                        )
                        .when_some(error, |body, error| {
                            body.child(div().text_size(px(12.0)).text_color(p.red).child(error))
                        }),
                )
                .footer(
                    DialogFooter::new()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(80.0))
                                .child(DialogClose::new().trigger(|button| {
                                    button.w_full().ghost().label(language.text("ssh_cancel"))
                                })),
                        )
                        .child(
                            div().w(px(112.0)).child(
                                DialogAction::new().child(
                                    Button::new("ssh-host-save")
                                        .w_full()
                                        .primary()
                                        .label(language.text(if form.index.is_some() {
                                            "save"
                                        } else {
                                            "ssh_add"
                                        }))
                                        .debug_selector(|| "ssh-host-save".into()),
                                ),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    confirm
                        .update(cx, |this, cx| this.save_ssh_host(cx))
                        .unwrap_or(false)
                })
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |this, cx| {
                        this.ssh.form = None;
                        this.ssh.error = None;
                        cx.notify();
                    });
                })
        });
        cx.notify();
    }

    fn choose_ssh_identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.ssh.form.as_ref().map(|form| form.inputs[4].clone()) else {
            return;
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("ssh_choose_identity").into()),
        });
        cx.spawn_in(window, async move |weak, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let _ = weak.update_in(cx, |this, window, cx| {
                    // A file chooser may outlive the dialog that opened it.
                    if this
                        .ssh
                        .form
                        .as_ref()
                        .is_some_and(|form| form.inputs[4].entity_id() == input.entity_id())
                    {
                        input.update(cx, |input, cx| {
                            input.set_value(path.to_string_lossy().into_owned(), window, cx)
                        });
                    }
                });
            }
        })
        .detach();
    }

    fn save_ssh_host(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(form) = &self.ssh.form else {
            return false;
        };
        let editing = form.index;
        let values = form
            .inputs
            .iter()
            .map(|i| i.read(cx).value().trim().to_owned())
            .collect::<Vec<_>>();
        let result = (|| {
            let port = if values[3].is_empty() {
                None
            } else {
                Some(
                    values[3]
                        .parse::<u16>()
                        .map_err(|_| "Port must be between 1 and 65535")?,
                )
            };
            let identity = if values[4].is_empty() {
                None
            } else {
                Some(if let Some(rest) = values[4].strip_prefix("~/") {
                    dirs::home_dir()
                        .ok_or("Cannot find your home directory")?
                        .join(rest)
                } else {
                    std::path::PathBuf::from(&values[4])
                })
            };
            let host = Host {
                name: values[0].clone(),
                address: values[1].clone(),
                user: values[2].clone(),
                port,
                identity,
            };
            host.validate()?;
            let mut hosts = self.ssh.hosts.clone();
            let index = editing.unwrap_or(hosts.len());
            let previous = hosts.get(index).cloned();
            if let Some(slot) = hosts.get_mut(index) {
                *slot = host;
            } else {
                hosts.push(host);
            }
            self.ssh.commit(hosts)?;
            if let Some(previous) = previous {
                let mut renamed = previous.clone();
                renamed.name = self.ssh.hosts[index].name.clone();
                // Only a label change may keep using a PTY connected with the original SSH options.
                if renamed == self.ssh.hosts[index] {
                    for session in self
                        .state
                        .projects
                        .iter_mut()
                        .flat_map(|project| &mut project.terminal.sessions)
                        .chain(&mut self.state.local_terminal.sessions)
                    {
                        if session.ssh.as_ref() == Some(&previous) {
                            session.ssh = Some(renamed.clone());
                        }
                    }
                }
            }
            Ok::<_, String>(index)
        })();
        match result {
            Ok(index) => {
                self.ssh.active_host = Some(index);
                self.ssh.error = None;
                cx.notify();
                true
            }
            Err(e) => {
                self.ssh.error = Some(e);
                cx.notify();
                false
            }
        }
    }

    fn import_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            use std::io::Read;
            let path = dirs::home_dir()
                .ok_or("Cannot find your home directory")?
                .join(".ssh/config");
            let mut source = String::new();
            std::fs::File::open(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?
                .take(1024 * 1024 + 1)
                .read_to_string(&mut source)
                .map_err(|e| e.to_string())?;
            let mut hosts = self.ssh.hosts.clone();
            for host in tervia::ssh::aliases(&source)? {
                if !hosts.iter().any(|h| h.name == host.name) {
                    hosts.push(host);
                }
            }
            self.ssh.commit(hosts)
        })();
        self.ssh.error = result.err();
        cx.notify();
    }

    fn remove_ssh_host(&mut self, host: Host, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ssh.hosts.contains(&host) {
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            self.t("ssh_remove"),
            Some(&format!("{}\n{}", host.name, host.address)),
            &[self.t("ssh_cancel"), self.t("ssh_remove")],
            cx,
        );
        cx.spawn_in(window, async move |weak, cx| {
            if answer.await != Ok(1) {
                return;
            }
            let _ = weak.update_in(cx, |this, _, cx| {
                // Keep the displayed host when deleting another row shifts its index.
                let selected = this.ssh.selected().cloned();
                let mut hosts = this.ssh.hosts.clone();
                hosts.retain(|h| h != &host);
                match this.ssh.commit(hosts) {
                    Ok(()) => {
                        this.ssh.active_host = selected.and_then(|host| {
                            this.ssh
                                .hosts
                                .iter()
                                .position(|candidate| candidate == &host)
                        });
                        this.ssh.error = None;
                    }
                    Err(e) => this.ssh.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::ui) fn selected_ssh_session(&self) -> Option<usize> {
        let host = self.ssh.selected()?;
        self.state
            .terminals()
            .sessions
            .iter()
            .rposition(|s| s.ssh.as_ref() == Some(host))
    }

    fn select_ssh_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.ssh.hosts.len() {
            return;
        }
        self.ssh.active_host = Some(index);
        self.ssh.error = None;
        if let Some(session) = self.selected_ssh_session() {
            self.state.select_session(session);
            let console = self.ensure_console(session, window, cx);
            console.read(cx).focus.clone().focus(window, cx);
            if console.read(cx).can_restart() {
                self.connect_ssh_terminal(window, cx);
            }
            cx.notify();
        } else {
            self.connect_ssh_terminal(window, cx);
        }
    }

    fn connect_ssh_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.ssh.selected().cloned() else {
            return;
        };
        if self.state.terminals().sessions.len() >= 64 {
            self.ssh.error = Some(self.t("terminal_limit").into());
            cx.notify();
            return;
        }
        self.state.new_session();
        let session = self.state.terminals().active_session;
        self.state.terminals_mut().sessions[session].ssh = Some(host);
        self.ensure_console(session, window, cx);
        cx.notify();
    }
}
