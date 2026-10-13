use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Scans {
    pending: HashMap<PathBuf, usize>,
    next: usize,
}
impl Scans {
    pub fn begin(&mut self, root: PathBuf) -> Option<usize> {
        if self.pending.contains_key(&root) {
            return None;
        }
        self.next += 1;
        self.pending.insert(root, self.next);
        Some(self.next)
    }
    pub fn finish(&mut self, root: &Path, generation: usize) -> bool {
        if self.pending.get(root) != Some(&generation) {
            return false;
        }
        self.pending.remove(root);
        true
    }
    // An explicit directory open supersedes any earlier scan of the same workspace.
    pub fn invalidate(&mut self, root: &Path) {
        self.pending.remove(root);
    }
    pub fn running(&self, root: &Path) -> bool {
        self.pending.contains_key(root)
    }
}
