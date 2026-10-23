use super::Workspace;
use std::collections::{BTreeMap, BTreeSet};

// Persist names rather than positions so extending or reordering this catalog keeps saved choices.
pub const PROJECT_AVATARS: [&str; 24] = [
    "rocket",
    "mountain",
    "compass",
    "gem",
    "leaf",
    "orbit",
    "flame",
    "sun",
    "moon",
    "cloud",
    "tree-pine",
    "flower-2",
    "sprout",
    "waves",
    "cat",
    "dog",
    "bird",
    "fish",
    "anchor",
    "crown",
    "coffee",
    "lightbulb",
    "palette",
    "atom",
];

#[derive(Clone, Debug, Default)]
pub(crate) struct Catalog {
    pub order: Vec<usize>,
    pub favorites: BTreeSet<usize>,
    pub recent: Vec<usize>,
    pub closed: Vec<usize>,
    pub hidden: BTreeSet<usize>,
    pub groups: BTreeMap<usize, (Vec<usize>, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectMatch {
    pub owner: usize,
    pub index: usize,
    pub directory: bool,
}

impl Workspace {
    pub fn project_avatar(&self, index: usize) -> &'static str {
        let owner = self.project_owner(index).unwrap_or(index);
        self.projects[owner].avatar
    }

    pub fn set_project_avatar(&mut self, index: usize, avatar: &str) -> bool {
        let Some(&avatar) = PROJECT_AVATARS.iter().find(|name| **name == avatar) else {
            return false;
        };
        let Some(owner) = self.project_owner(index) else {
            return false;
        };
        if self.projects[owner].avatar == avatar {
            return false;
        }
        self.projects[owner].avatar = avatar;
        true
    }

    pub(super) fn record_project_target(&mut self, index: usize) {
        if let Some(owner) = self.project_owner(index)
            && let Some((members, target)) = self.catalog.groups.get_mut(&owner)
        {
            *target = index;
            if !members.contains(&index) {
                members.push(index);
            }
        }
    }

    pub fn project_available(&self, index: usize) -> bool {
        index < self.projects.len() && !self.closed_projects.contains(&index)
    }

    pub fn project_owner(&self, index: usize) -> Option<usize> {
        self.owner_in(index, &self.all_project_rows())
    }

    fn owner_in(&self, index: usize, rows: &[usize]) -> Option<usize> {
        if let Some(owner) = self.folder(index) {
            return Some(owner);
        }
        if let Some(owner) = self
            .catalog
            .groups
            .iter()
            .find_map(|(owner, (members, _))| {
                (self.project_available(*owner) && members.contains(&index)).then_some(*owner)
            })
        {
            return Some(owner);
        }
        rows.iter()
            .copied()
            .find(|owner| self.folder(index) == Some(*owner) || self.same_project(*owner, index))
    }

    pub fn project_members(&self, owner: usize) -> Vec<usize> {
        let rows = self.all_project_rows();
        (0..self.projects.len())
            .filter(|index| {
                self.project_available(*index) && self.owner_in(*index, &rows) == Some(owner)
            })
            .collect()
    }

    pub fn project_favorite(&self, owner: usize) -> bool {
        self.catalog.favorites.contains(&owner)
    }

    pub fn toggle_project_favorite(&mut self, owner: usize) -> bool {
        if !self.all_project_rows().contains(&owner) {
            return false;
        }
        if !self.catalog.favorites.remove(&owner) {
            self.catalog.favorites.insert(owner);
            true
        } else {
            false
        }
    }

    pub fn move_project(&mut self, from: usize, to: usize) -> bool {
        let rows = self.project_rows();
        if from == to
            || self.project_favorite(from) != self.project_favorite(to)
            || !rows.contains(&from)
            || !rows.contains(&to)
        {
            return false;
        }
        // Reorder identifiers, never the owner slots captured by async IO and editor callbacks.
        let mut order = self.catalog.order.clone();
        for index in self.all_project_rows() {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        let source = order.iter().position(|index| *index == from).unwrap();
        let target = order.iter().position(|index| *index == to).unwrap();
        order.remove(source);
        order.insert(target, from);
        self.catalog.order = order;
        true
    }

    pub fn previous_project(&self) -> Option<usize> {
        let current = self.project_owner(self.active_project);
        self.catalog
            .recent
            .iter()
            .copied()
            .find(|index| self.project_visible(*index) && self.project_owner(*index) != current)
    }

    pub fn project_closed(&self, owner: usize) -> bool {
        self.catalog.closed.contains(&owner)
    }

    pub fn closed_project_rows(&self) -> Vec<usize> {
        self.catalog
            .closed
            .iter()
            .copied()
            .filter(|index| self.project_available(*index))
            .collect()
    }

    pub fn close_project(&mut self, owner: usize) -> Option<Vec<usize>> {
        if !self.project_rows().contains(&owner) {
            return None;
        }
        let members = self.project_members(owner);
        let target = self.project_target(owner);
        self.catalog.groups.insert(owner, (members.clone(), target));
        self.catalog.hidden.extend(members.iter().copied());
        self.catalog.closed.retain(|index| *index != owner);
        self.catalog.closed.insert(0, owner);
        // Visibility is already hidden above; screen ownership decides whether navigation must change.
        if self.screen == super::Screen::Project && members.contains(&self.active_project) {
            let next = self
                .catalog
                .recent
                .iter()
                .copied()
                .find(|index| self.project_visible(*index))
                .or_else(|| {
                    self.project_rows()
                        .first()
                        .map(|index| self.project_target(*index))
                });
            if let Some(next) = next {
                self.switch_project(next);
            } else if !self.local_terminal.sessions.is_empty() {
                self.screen = super::Screen::Local;
            } else {
                self.screen = super::Screen::Welcome;
            }
        }
        Some(members)
    }

    pub fn reopen_project(&mut self, owner: usize) -> Option<usize> {
        if !self.project_closed(owner) || !self.project_available(owner) {
            return None;
        }
        let members = self.project_members(owner);
        for index in members {
            self.catalog.hidden.remove(&index);
        }
        self.catalog.closed.retain(|index| *index != owner);
        let target = self.project_target(owner);
        self.switch_project(target);
        Some(target)
    }

    pub fn search_projects(&self, query: &str, closed_only: bool) -> Vec<ProjectMatch> {
        let query = query.trim().replace('\\', "/").to_lowercase();
        let mut matches = Vec::new();
        let rows = self.all_project_rows();
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for index in 0..self.projects.len() {
            if self.project_available(index)
                && let Some(owner) = self.owner_in(index, &rows)
            {
                members.entry(owner).or_default().push(index);
            }
        }
        let mut visits = BTreeMap::new();
        for (rank, index) in self.catalog.recent.iter().enumerate() {
            if let Some(owner) = self.owner_in(*index, &rows) {
                visits.entry(owner).or_insert(rank);
            }
        }
        for owner in rows {
            let closed = self.project_closed(owner);
            if closed_only && !closed {
                continue;
            }
            let mut candidates = vec![ProjectMatch {
                owner,
                index: self.project_target(owner),
                directory: false,
            }];
            if !query.is_empty() {
                candidates.extend(
                    members
                        .get(&owner)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|index| *index != owner)
                        .map(|index| ProjectMatch {
                            owner,
                            index,
                            directory: true,
                        }),
                );
            }
            for found in candidates {
                let path = if found.directory {
                    &self.projects[found.index].root
                } else {
                    self.project_root(owner)
                };
                let path = path.to_string_lossy().replace('\\', "/").to_lowercase();
                let name = if found.directory {
                    self.projects[found.index].name.to_lowercase()
                } else {
                    self.project_label(owner).to_lowercase()
                };
                let score = if query.is_empty() || name.starts_with(&query) {
                    0
                } else if name.contains(&query) {
                    1
                } else if path.contains(&query) {
                    2
                } else {
                    let mut characters = path.chars();
                    if !query
                        .chars()
                        .all(|wanted| characters.any(|character| character == wanted))
                    {
                        continue;
                    }
                    3
                };
                let recent = if closed {
                    self.catalog.closed.iter().position(|index| *index == owner)
                } else {
                    visits.get(&owner).copied()
                }
                .unwrap_or(usize::MAX);
                matches.push((
                    (
                        closed,
                        score,
                        !closed && !self.project_favorite(owner),
                        recent,
                        found.directory,
                        path.len(),
                    ),
                    found,
                ));
            }
        }
        matches.sort_by_key(|(rank, _)| *rank);
        matches
            .into_iter()
            .take(500)
            .map(|(_, found)| found)
            .collect()
    }
}
