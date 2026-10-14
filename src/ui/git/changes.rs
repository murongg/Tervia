use super::super::controls::icon;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
};

// Keep section IDs stable; display order places untracked files beside worktree edits.
const CHANGE_GROUPS: [u8; 4] = [0, 1, 3, 2];

fn group_files(files: &[FileStatus], group: u8) -> impl Iterator<Item = (usize, &FileStatus)> {
    files
        .iter()
        .enumerate()
        .filter(move |(_, file)| match group {
            0 => file.conflicted,
            1 => !file.conflicted && !matches!(file.worktree, '.' | '?'),
            2 => !file.conflicted && file.index != '.',
            3 => !file.conflicted && file.worktree == '?',
            _ => false,
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChangeRow {
    Group {
        group: u8,
        count: usize,
    },
    Folder {
        group: u8,
        path: PathBuf,
        depth: usize,
        count: usize,
    },
    File {
        group: u8,
        index: usize,
        depth: usize,
    },
    Empty,
}

#[derive(Default)]
struct Folder {
    folders: BTreeMap<OsString, Folder>,
    files: Vec<usize>,
    count: usize,
}
impl Folder {
    fn append(
        &self,
        group: u8,
        path: &std::path::Path,
        depth: usize,
        collapsed: &BTreeSet<(u8, PathBuf)>,
        rows: &mut Vec<ChangeRow>,
    ) {
        for (name, folder) in &self.folders {
            let path = path.join(name);
            rows.push(ChangeRow::Folder {
                group,
                path: path.clone(),
                depth,
                count: folder.count,
            });
            if !collapsed.contains(&(group, path.clone())) {
                folder.append(group, &path, depth + 1, collapsed, rows);
            }
        }
        rows.extend(self.files.iter().map(|index| ChangeRow::File {
            group,
            index: *index,
            depth,
        }));
    }
}

pub(super) fn change_rows(
    files: &[FileStatus],
    collapsed: &BTreeSet<u8>,
    folders: &BTreeSet<(u8, PathBuf)>,
) -> Vec<ChangeRow> {
    let mut rows = Vec::new();
    for group in CHANGE_GROUPS {
        let mut root = Folder::default();
        for (index, file) in group_files(files, group) {
            root.count += 1;
            let mut folder = &mut root;
            for part in file
                .path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .components()
            {
                folder = folder.folders.entry(part.as_os_str().into()).or_default();
                folder.count += 1;
            }
            folder.files.push(index);
        }
        if group == 0 && root.count == 0 {
            continue;
        }
        rows.push(ChangeRow::Group {
            group,
            count: root.count,
        });
        if collapsed.contains(&group) {
            continue;
        }
        if root.count == 0 {
            rows.push(ChangeRow::Empty);
        } else {
            root.append(group, std::path::Path::new(""), 0, folders, &mut rows);
        }
    }
    rows
}

impl GitUi {
    pub(super) fn reveal_change(&mut self, target: &Detail) {
        let (group, path) = match target {
            Detail::Conflict(path) => (0, path),
            Detail::File(file, side) => (
                if *side == Side::Index {
                    2
                } else if file.worktree == '?' {
                    // Untracked files use a worktree Diff but own a separate list section.
                    3
                } else {
                    1
                },
                &file.path,
            ),
            _ => return,
        };
        self.collapsed_changes.remove(&group);
        self.collapsed_folders
            .retain(|(owner, folder)| *owner != group || !path.starts_with(folder));
        self.rebuild_change_rows();
        if let Some(row) = self.change_rows.iter().position(|row| match row {
            ChangeRow::File {
                group: owner,
                index,
                ..
            } => {
                *owner == group
                    && self
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.status.files[*index].path == *path)
            }
            _ => false,
        }) {
            self.changes_scroll
                .scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    fn rebuild_change_rows(&mut self) {
        if let Some(snapshot) = &self.snapshot {
            self.change_rows = change_rows(
                &snapshot.status.files,
                &self.collapsed_changes,
                &self.collapsed_folders,
            )
            .into();
        }
    }
}

impl Tervia {
    pub(super) fn git_changes(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let captured = snapshot.clone();
        let rows = ui.change_rows.clone();
        let list = uniform_list(
            ("git-change-rows", project),
            rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| this.git_change_row(project, &captured, &rows[index], cx))
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .track_scroll(&ui.changes_scroll);
        let message = ui.message.read(cx).value().to_string();
        let has_message = !message.trim().is_empty();
        let staged = snapshot
            .status
            .files
            .iter()
            .filter(|file| !file.conflicted && file.index != '.')
            .count();
        let can_commit = !ui.busy
            && !self.git_mutating
            && !self.saving_file
            && !self.file_reads.has_reloads()
            && !message.trim().is_empty()
            && staged > 0
            && !snapshot.status.files.iter().any(|file| file.conflicted);
        let form = div()
            .id("git-commit-form")
            .debug_selector(|| "git-commit-form".into())
            .flex_shrink_0()
            .p(px(12.0))
            .border_t_1()
            .border_color(p.border)
            .bg(p.sidebar)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(icon(IconName::GitCommitHorizontal, p.accent))
                    .child(self.f("git_commit_to", &[("branch", &snapshot.status.branch)]))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.muted)
                            .child(staged.to_string()),
                    ),
            )
            .child(
                Textarea::new(&ui.message)
                    .aria_label(self.t("commit_message"))
                    .disabled(self.git_mutating)
                    .h(px(88.0)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        self.git_action(
                            "real-amend",
                            "git_amend",
                            Operation::Commit {
                                message: message.clone(),
                                amend: true,
                            },
                            cx,
                        )
                        .text_size(px(11.0))
                        .disabled(
                            ui.busy
                                || self.git_mutating
                                || message.trim().is_empty()
                                || snapshot.status.unborn
                                || snapshot.status.operation.is_some(),
                        ),
                    )
                    .child(div().flex_1()),
            )
            .child(
                self.git_action(
                    "real-commit",
                    "git_commit",
                    Operation::Commit {
                        message,
                        amend: false,
                    },
                    cx,
                )
                .primary()
                .bg(if can_commit { p.accent } else { p.raised })
                .text_color(if can_commit { p.on_accent } else { p.muted })
                .h(px(32.0))
                .w_full()
                .disabled(!can_commit),
            );
        let footer = div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(p.border)
            .child(
                label_button("git-toggle-commit", self.t("git_commit_toggle"), p)
                    .debug_selector(|| "git-toggle-commit".into())
                    .icon(icon(
                        if ui.review.commit_expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        p.secondary,
                    ))
                    .h(px(36.0))
                    .w_full()
                    .rounded(px(0.0))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.muted)
                            .child(staged.to_string()),
                    )
                    .when(has_message, |button| {
                        button.child(div().size(px(5.0)).rounded_full().bg(p.accent))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let review = &mut this.git_states[project].review;
                        review.commit_expanded = !review.commit_expanded;
                        cx.notify();
                    })),
            )
            .when(ui.review.commit_expanded, |footer| footer.child(form));
        div()
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .child(self.git_panel_title("git_working_copy"))
            .child(list)
            .child(footer)
    }

    fn git_change_row(
        &self,
        project: usize,
        snapshot: &Snapshot,
        row: &ChangeRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[project];
        let p = self.git_p();
        match row {
            ChangeRow::Group { group, count } => {
                let group = *group;
                let key = match group {
                    0 => "git_conflicts",
                    1 => "unstaged",
                    3 => "untracked",
                    _ => "staged",
                };
                let staging = group != 2;
                let folded = ui.collapsed_changes.contains(&group);
                div()
                    .id(("git-change-section", group as usize))
                    .w_full()
                    .h(px(34.0))
                    .hover(|style| style.bg(p.raised))
                    .flex()
                    .items_center()
                    .bg(p.toolbar)
                    .border_b_1()
                    .border_color(p.border)
                    .pr(px(5.0))
                    .child(
                        label_button(("git-change-group", group as usize), "", p)
                            .text()
                            .flex_1()
                            .h_full()
                            .rounded(px(0.0))
                            .accessibility_label(format!(
                                "{} {}",
                                self.t(if folded { "git_expand" } else { "git_collapse" }),
                                self.t(key)
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(px(11.0))
                                    .child(icon(
                                        if folded {
                                            IconName::ChevronRight
                                        } else {
                                            IconName::ChevronDown
                                        },
                                        p.muted,
                                    ))
                                    .child(
                                        div()
                                            .text_color(if group == 0 {
                                                p.red
                                            } else {
                                                p.secondary
                                            })
                                            .child(self.t(key)),
                                    )
                                    .child(div().text_color(p.muted).child(count.to_string())),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let ui = &mut this.git_states[project];
                                if !ui.collapsed_changes.remove(&group) {
                                    ui.collapsed_changes.insert(group);
                                }
                                ui.rebuild_change_rows();
                                cx.notify();
                            })),
                    )
                    .when(group > 0, |header| {
                        header.child(
                            self.git_action_for(
                                project,
                                ("git-all", group as usize),
                                if staging { "stage_all" } else { "unstage_all" },
                                if staging {
                                    Operation::StageFiles(
                                        group_files(&snapshot.status.files, group)
                                            .map(|(_, file)| file.path.clone())
                                            .collect(),
                                    )
                                } else {
                                    Operation::UnstageAll
                                },
                                cx,
                            )
                            .label("")
                            .icon(icon(
                                if staging {
                                    IconName::Plus
                                } else {
                                    IconName::Minus
                                },
                                p.secondary,
                            ))
                            .tooltip(self.t(if staging { "stage_all" } else { "unstage_all" }))
                            .accessibility_label(self.t(if staging {
                                "stage_all"
                            } else {
                                "unstage_all"
                            }))
                            .w(px(24.0))
                            .h(px(24.0))
                            .text_size(px(11.0))
                            .disabled(
                                ui.busy
                                    || self.git_mutating
                                    || *count == 0
                                    || snapshot.status.files.iter().any(|file| file.conflicted),
                            ),
                        )
                    })
                    .into_any_element()
            }
            ChangeRow::Folder {
                group,
                path,
                depth,
                count,
            } => {
                let key = (*group, path.clone());
                let folded = ui.collapsed_folders.contains(&key);
                label_button(
                    format!("git-folder-{}-{}", group, path.to_string_lossy()),
                    "",
                    p,
                )
                .w_full()
                .h(px(34.0))
                .rounded(px(0.0))
                .accessibility_label(path.to_string_lossy().into_owned())
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .pl(px(6.0 + *depth as f32 * 12.0))
                        .text_size(px(12.0))
                        .child(icon(
                            if folded {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            },
                            p.muted,
                        ))
                        .child(icon(IconName::Folder, p.muted))
                        .child(
                            div().flex_1().min_w_0().text_ellipsis().child(
                                path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(p.muted)
                                .child(count.to_string()),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let ui = &mut this.git_states[project];
                    if !ui.collapsed_folders.remove(&key) {
                        ui.collapsed_folders.insert(key.clone());
                    }
                    ui.rebuild_change_rows();
                    cx.notify();
                }))
                .into_any_element()
            }
            ChangeRow::Empty => div()
                .h(px(34.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .text_size(px(12.0))
                .text_color(p.muted)
                .child(self.t("git_no_files"))
                .into_any_element(),
            ChangeRow::File {
                group,
                index,
                depth,
            } => {
                #[cfg(all(test, feature = "ui-tests"))]
                ui.rendered_rows.set(ui.rendered_rows.get() + 1);
                let file = &snapshot.status.files[*index];
                let group = *group;
                let row_id = index * CHANGE_GROUPS.len() + group as usize;
                let side = if group == 2 {
                    Side::Index
                } else {
                    Side::Worktree
                };
                let selection = if group == 0 {
                    Detail::Conflict(file.path.clone())
                } else {
                    Detail::File(file.clone(), side)
                };
                let selected = matches!(&ui.selected, Some(Detail::File(selected, selected_side)) if selected.path == file.path && *selected_side == side)
                    || matches!(&ui.selected, Some(Detail::Conflict(path)) if path == &file.path);
                let (action, action_key, glyph) = match group {
                    0 => (
                        Operation::Resolve(file.path.clone()),
                        "git_resolve",
                        IconName::Check,
                    ),
                    1 | 3 => (Operation::Stage(file.path.clone()), "stage", IconName::Plus),
                    _ => (
                        Operation::Unstage(file.path.clone()),
                        "unstage",
                        IconName::Minus,
                    ),
                };
                let status = if side == Side::Index {
                    file.index
                } else {
                    file.worktree
                };
                let color = if group == 0 || status == 'D' {
                    p.red
                } else if status == 'A' || status == '?' {
                    p.green
                } else {
                    p.accent
                };
                let path = file.path.to_string_lossy().into_owned();
                div()
                    .w_full()
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .pr(px(5.0))
                    .bg(if selected { p.selected } else { p.background })
                    .child(
                        label_button(("git-file", row_id), "", p)
                            .h_full()
                            .rounded(px(0.0))
                            .flex_1()
                            .min_w_0()
                            .accessibility_label(path.clone())
                            .tooltip(path.clone())
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .pl(px(*depth as f32 * 12.0))
                                    .child(
                                        div()
                                            .w(px(16.0))
                                            .flex_shrink_0()
                                            .text_center()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_size(px(11.0))
                                            .text_color(color)
                                            .child(status.to_string()),
                                    )
                                    .child(icon(IconName::FileText, p.muted))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(px(12.0))
                                            .text_color(p.text)
                                            .text_ellipsis()
                                            .child(
                                                file.path
                                                    .file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                                    .into_owned(),
                                            ),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.git_detail(project, selection.clone(), window, cx)
                            })),
                    )
                    .child(
                        self.git_action_for(project, ("git-stage", row_id), action_key, action, cx)
                            .label("")
                            .icon(icon(glyph, p.secondary))
                            .w(px(26.0))
                            .h(px(26.0))
                            .tooltip(self.t(action_key))
                            .accessibility_label(format!("{} {path}", self.t(action_key))),
                    )
                    .into_any_element()
            }
        }
    }
}
