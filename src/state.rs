use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Terminal,
    Git,
    Files,
}

#[derive(Clone, Copy, Debug)]
pub enum FileClose {
    One(usize),
    Others(usize),
    Right(usize),
    All,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct File {
    pub path: String,
    pub disk_path: PathBuf,
    pub language: String,
    pub saved: String,
    pub draft: String,
}
impl File {
    pub fn is_image(&self) -> bool {
        self.language == "image"
    }
    pub fn dirty(&self) -> bool {
        self.saved != self.draft
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub monogram: String,
    pub path: String,
    pub root: PathBuf,
    pub repository: Option<PathBuf>,
    pub branch: String,
    pub tab: Tab,
    pub sessions: Vec<Session>,
    pub active_session: usize,
    pub focused_session: usize,
    pub split_terminal: bool,
    pub dock: Dock,
    pub positions: std::collections::BTreeMap<PathBuf, ReadingPosition>,
    pub restoring: std::collections::BTreeSet<PathBuf>,
    pub files: Vec<File>,
    pub open_files: Vec<usize>,
    pub active_file: usize,
    pub preview_file: Option<usize>,
    pub closed_files: Vec<usize>,
}

impl Project {
    pub fn current_file(&self) -> Option<&File> {
        self.open_files
            .contains(&self.active_file)
            .then(|| self.files.get(self.active_file))
            .flatten()
    }
    pub fn open_file(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        if !self.open_files.contains(&index) {
            self.open_files.push(index);
        }
        self.closed_files.retain(|file| *file != index);
        self.active_file = index;
        self.pin_file(index);
    }

    pub fn pin_file(&mut self, index: usize) {
        if self.preview_file == Some(index) {
            self.preview_file = None;
        }
    }

    pub fn preview_file(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        if self.open_files.contains(&index) {
            self.active_file = index;
            return;
        }
        let position = self.preview_file.take().and_then(|previous| {
            // A draft is never discarded by browsing, even before its input event pins the tab.
            if self.files[previous].dirty() {
                return None;
            }
            let position = self.open_files.iter().position(|file| *file == previous)?;
            self.open_files.remove(position);
            Some(position)
        });
        self.open_files
            .insert(position.unwrap_or(self.open_files.len()), index);
        self.preview_file = Some(index);
        self.active_file = index;
        self.closed_files.retain(|file| *file != index);
    }

    pub fn file_targets(&self, scope: FileClose) -> Vec<usize> {
        match scope {
            FileClose::All => self.open_files.clone(),
            FileClose::One(index) => self
                .open_files
                .iter()
                .copied()
                .filter(|file| *file == index)
                .collect(),
            FileClose::Others(index) if self.open_files.contains(&index) => self
                .open_files
                .iter()
                .copied()
                .filter(|file| *file != index)
                .collect(),
            FileClose::Right(index) => self
                .open_files
                .iter()
                .position(|file| *file == index)
                .map(|position| self.open_files[position + 1..].to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    pub fn close_files(&mut self, targets: &[usize]) -> bool {
        let position = self
            .open_files
            .iter()
            .position(|file| *file == self.active_file)
            .unwrap_or_default();
        let removed: Vec<_> = self
            .open_files
            .iter()
            .copied()
            .filter(|file| targets.contains(file))
            .collect();
        if removed.is_empty() {
            return false;
        }
        for file in &removed {
            self.closed_files.retain(|closed| closed != file);
            self.closed_files.push(*file);
        }
        if self.closed_files.len() > 100 {
            self.closed_files.drain(..self.closed_files.len() - 100);
        }
        // Document indices stay stable for editors and in-flight reads; only tab membership changes.
        self.open_files.retain(|file| !removed.contains(file));
        if self
            .preview_file
            .is_some_and(|file| removed.contains(&file))
        {
            self.preview_file = None;
        }
        if !self.open_files.contains(&self.active_file) {
            self.active_file = self
                .open_files
                .get(position.min(self.open_files.len().saturating_sub(1)))
                .copied()
                .unwrap_or_default();
        }
        true
    }

    pub fn move_file(&mut self, source: usize, target: usize) -> bool {
        let Some(from) = self.open_files.iter().position(|file| *file == source) else {
            return false;
        };
        let Some(to) = self.open_files.iter().position(|file| *file == target) else {
            return false;
        };
        if from == to {
            return false;
        }
        let file = self.open_files.remove(from);
        self.open_files.insert(to, file);
        true
    }

    pub fn cycle_file(&mut self, backwards: bool) {
        if self.open_files.is_empty() {
            return;
        }
        let current = self
            .open_files
            .iter()
            .position(|file| *file == self.active_file)
            .unwrap_or_default();
        let next = if backwards {
            (current + self.open_files.len() - 1) % self.open_files.len()
        } else {
            (current + 1) % self.open_files.len()
        };
        self.active_file = self.open_files[next];
    }

    pub fn reopen_file(&mut self) -> Option<usize> {
        while let Some(file) = self.closed_files.pop() {
            if file < self.files.len() {
                self.open_file(file);
                return Some(file);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Dock {
    pub visible: bool,
    pub size: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReadingPosition {
    pub line: u32,
    pub column: u32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub source: bool,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            visible: false,
            size: 220,
        }
    }
}

#[derive(Clone, Debug)]
struct RepositoryGroup {
    root: PathBuf,
    active: usize,
}

#[derive(Clone, Debug)]
struct FolderGroup {
    directories: std::collections::BTreeSet<usize>,
    pinned: std::collections::BTreeSet<usize>,
    children: Vec<usize>,
    active: usize,
    expanded: bool,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub projects: Vec<Project>,
    closed_projects: std::collections::BTreeSet<usize>,
    repositories: std::collections::BTreeMap<PathBuf, RepositoryGroup>,
    folders: std::collections::BTreeMap<usize, FolderGroup>,
    pub active_project: usize,
    pub sidebar_expanded: bool,
    pub notice: String,
    pub palette_open: bool,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        let mut workspace = Self::empty();
        workspace.open_directory(root);
        workspace
    }

    pub fn empty() -> Self {
        Self {
            projects: Vec::new(),
            closed_projects: Default::default(),
            repositories: Default::default(),
            folders: Default::default(),
            active_project: 0,
            sidebar_expanded: false,
            notice: String::new(),
            palette_open: false,
        }
    }

    pub fn project(&self) -> &Project {
        &self.projects[self.active_project]
    }

    pub fn project_mut(&mut self) -> &mut Project {
        &mut self.projects[self.active_project]
    }

    pub fn switch_project(&mut self, index: usize) {
        if self.project_visible(index) {
            if let Some(parent) = self.folder(index)
                && let Some(group) = self.folders.get_mut(&parent)
            {
                group.active = index;
            }
            self.active_project = index;
            if let Some(identity) = &self.projects[index].repository
                && let Some(group) = self.repositories.get_mut(identity)
            {
                group.active = index;
            }
            self.notice.clear();
        }
    }

    pub fn project_visible(&self, index: usize) -> bool {
        index < self.projects.len() && !self.closed_projects.contains(&index)
    }

    pub fn group_folder(&mut self, root: usize, children: &[usize]) {
        if self.attach_directories(root, children) {
            let children = children
                .iter()
                .copied()
                .filter(|index| self.project_visible(*index))
                .collect();
            self.folders.get_mut(&root).unwrap().children = children;
        }
    }

    /// A navigated directory belongs to its opened folder even when Git identity is shared.
    pub fn attach_directory(&mut self, root: usize, directory: usize) {
        if self.project_visible(root)
            && self.project_visible(directory)
            && self.projects[directory]
                .root
                .starts_with(&self.projects[root].root)
        {
            self.attach_directories(root, &[directory]);
            if let Some(group) = self.folders.get_mut(&root) {
                // Only explicit navigation persists; scan results refresh independently.
                group.pinned.insert(directory);
            }
        }
    }

    pub fn working_directories(&self, root: usize) -> Vec<usize> {
        let mut directories = vec![root];
        for index in self
            .folder_children(root)
            .into_iter()
            .chain(self.saved_directories(root))
        {
            if !directories.contains(&index) {
                directories.push(index);
            }
        }
        directories
    }

    pub(crate) fn saved_directories(&self, root: usize) -> Vec<usize> {
        let mut directories = vec![root];
        if let Some(group) = self.folders.get(&root) {
            directories.extend(
                group
                    .pinned
                    .iter()
                    .copied()
                    .filter(|index| *index != root && self.project_visible(*index)),
            );
        }
        let active = self.project_target(root);
        if self.project_visible(active) && !directories.contains(&active) {
            directories.push(active);
        }
        directories
    }

    fn attach_directories(&mut self, root: usize, children: &[usize]) -> bool {
        if !self.project_visible(root)
            || self.folder(root).is_some_and(|owner| {
                owner != root
                    && !self.projects[owner]
                        .root
                        .starts_with(&self.projects[root].root)
            })
        {
            return false;
        }
        let children: Vec<_> = children
            .iter()
            .copied()
            .filter(|index| self.project_visible(*index))
            .collect();
        let mut cached = std::collections::BTreeSet::new();
        let mut pinned = std::collections::BTreeSet::new();
        for child in &children {
            if *child != root
                && self.projects[*child]
                    .root
                    .starts_with(&self.projects[root].root)
                && let Some(previous) = self.folders.remove(child)
            {
                cached.extend(previous.directories);
                pinned.extend(previous.pinned);
            }
        }
        let group = self.folders.entry(root).or_insert_with(|| FolderGroup {
            directories: std::collections::BTreeSet::from([root]),
            pinned: std::collections::BTreeSet::from([root]),
            children: Vec::new(),
            active: root,
            expanded: true,
        });
        // Cached directory owners stay attached after rescans so drafts never reappear as new projects.
        group.directories.extend(cached);
        group.pinned.extend(pinned);
        group.directories.extend(children.iter().copied());
        if self.folder(self.active_project) == Some(root)
            && let Some(group) = self.folders.get_mut(&root)
        {
            group.active = self.active_project;
        }
        true
    }

    pub fn folder(&self, index: usize) -> Option<usize> {
        // Retired folder worktrees must not hide the remaining checkout directories.
        self.folders
            .iter()
            .find_map(|(root, group)| {
                (self.project_visible(*root) && group.directories.contains(&index)).then_some(*root)
            })
            .or_else(|| {
                self.folders.iter().find_map(|(root, group)| {
                    (self.project_visible(*root)
                        && group
                            .directories
                            .iter()
                            .any(|member| self.same_project(*member, index)))
                    .then_some(*root)
                })
            })
    }

    pub fn folder_children(&self, root: usize) -> Vec<usize> {
        let mut children = Vec::new();
        if let Some(group) = self.folders.get(&root) {
            for index in &group.children {
                if self.project_visible(*index)
                    && !self.same_project(root, *index)
                    && !children
                        .iter()
                        .any(|child| self.same_project(*child, *index))
                {
                    children.push(*index);
                }
            }
        }
        children
    }

    pub fn folder_expanded(&self, root: usize) -> bool {
        self.folders.get(&root).is_none_or(|group| group.expanded)
    }

    pub fn toggle_folder(&mut self, root: usize) {
        if let Some(group) = self.folders.get_mut(&root) {
            group.expanded = !group.expanded;
        }
    }

    pub fn associate_repository(&mut self, index: usize, identity: PathBuf, root: Option<PathBuf>) {
        let Some(project) = self.projects.get_mut(index) else {
            return;
        };
        project.repository = Some(identity.clone());
        let group = self
            .repositories
            .entry(identity)
            .or_insert_with(|| RepositoryGroup {
                root: root.clone().unwrap_or_else(|| project.root.clone()),
                active: index,
            });
        if let Some(root) = root {
            group.root = root;
        }
        if index == self.active_project {
            group.active = index;
        }
    }

    pub fn project_rows(&self) -> Vec<usize> {
        let mut repositories = std::collections::BTreeSet::new();
        self.projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                if !self.project_visible(index) {
                    return None;
                }
                if self.folder(index).is_some_and(|root| root != index) {
                    return None;
                }
                if let Some(identity) = &project.repository
                    && !repositories.insert(identity)
                {
                    return None;
                }
                Some(index)
            })
            .collect()
    }

    pub fn same_project(&self, first: usize, second: usize) -> bool {
        first == second
            || self
                .projects
                .get(first)
                .and_then(|project| project.repository.as_ref())
                .zip(
                    self.projects
                        .get(second)
                        .and_then(|project| project.repository.as_ref()),
                )
                .is_some_and(|(first, second)| first == second)
    }

    pub fn project_target(&self, index: usize) -> usize {
        if let Some(group) = self.folders.get(&index)
            && self.project_visible(group.active)
        {
            return group.active;
        }
        self.projects
            .get(index)
            .and_then(|project| project.repository.as_ref())
            .and_then(|identity| self.repositories.get(identity))
            .filter(|group| {
                self.project_visible(group.active) && self.same_project(index, group.active)
            })
            .map(|group| group.active)
            .unwrap_or(index)
    }

    pub fn project_label(&self, index: usize) -> String {
        let project = &self.projects[index];
        self.project_root(index)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| project.name.clone())
    }

    pub fn project_root(&self, index: usize) -> &PathBuf {
        let project = &self.projects[index];
        if self.folders.contains_key(&index) {
            return &project.root;
        }
        project
            .repository
            .as_ref()
            .and_then(|identity| self.repositories.get(identity))
            .map(|group| &group.root)
            .unwrap_or(&project.root)
    }

    pub fn retire_project(&mut self, index: usize) {
        if index >= self.projects.len() || index == self.active_project {
            return;
        }
        // Keep owner IDs stable for in-flight IO; removed worktrees leave reusable hidden slots.
        self.closed_projects.insert(index);
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.project_mut().tab = tab;
        self.notice.clear();
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_expanded = !self.sidebar_expanded;
    }

    pub fn dismiss_palette(&mut self) -> bool {
        let was_open = self.palette_open;
        self.palette_open = false;
        was_open
    }

    pub fn toggle_split(&mut self) {
        self.project_mut().split_terminal = !self.project().split_terminal;
    }

    pub fn new_session(&mut self) {
        let project = self.project_mut();
        project.sessions.push(Session {
            name: format!("shell {}", project.sessions.len() + 1),
        });
        project.active_session = project.sessions.len() - 1;
        project.focused_session = project.active_session;
    }

    pub fn select_session(&mut self, index: usize) {
        if index < self.project().sessions.len() {
            self.project_mut().active_session = index;
            self.project_mut().focused_session = index;
        }
    }

    pub fn focus_session(&mut self, index: usize) {
        if index < self.project().sessions.len() {
            self.project_mut().focused_session = index;
        }
    }

    pub fn command_session(&self) -> usize {
        self.project()
            .focused_session
            .min(self.project().sessions.len() - 1)
    }

    pub fn close_session(&mut self, index: usize) {
        let project = self.project_mut();
        // A project always keeps one terminal surface, even when its last tab is closed.
        if project.sessions.len() > 1 && index < project.sessions.len() {
            project.sessions.remove(index);
            project.active_session = project
                .active_session
                .saturating_sub(usize::from(index <= project.active_session));
            project.active_session = project.active_session.min(project.sessions.len() - 1);
            project.focused_session = project
                .focused_session
                .saturating_sub(usize::from(index <= project.focused_session))
                .min(project.sessions.len() - 1);
            if project.sessions.len() == 1 {
                project.split_terminal = false;
            }
        }
    }

    pub fn open_file(&mut self, index: usize) {
        self.project_mut().open_file(index);
    }

    pub fn open_directory(&mut self, root: std::path::PathBuf) -> usize {
        let index = self.register_directory(root);
        self.switch_project(index);
        index
    }

    pub fn register_directory(&mut self, root: std::path::PathBuf) -> usize {
        if let Some(index) = self
            .projects
            .iter()
            .position(|project| project.root == root)
        {
            self.closed_projects.remove(&index);
            return index;
        }
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy()
            .into_owned();
        let monogram = name
            .chars()
            .next()
            .unwrap_or('T')
            .to_uppercase()
            .to_string();
        let index = self.projects.len();
        self.projects.push(Project {
            name,
            monogram,
            path: root.to_string_lossy().into_owned(),
            root,
            repository: None,
            branch: "—".into(),
            tab: Tab::Git,
            sessions: vec![Session {
                name: "shell".into(),
            }],
            active_session: 0,
            focused_session: 0,
            split_terminal: false,
            dock: Dock::default(),
            positions: Default::default(),
            restoring: Default::default(),
            files: Vec::new(),
            open_files: Vec::new(),
            active_file: 0,
            preview_file: None,
            closed_files: Vec::new(),
        });
        index
    }
}
