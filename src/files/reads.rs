use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    Apply,
    Stale,
    Edited,
}

struct Pending {
    token: usize,
    draft: Option<String>,
    reload: bool,
}
#[derive(Default)]
pub struct Reads {
    next: usize,
    pending: HashMap<(usize, PathBuf), Pending>,
    selections: HashMap<usize, usize>,
}

impl Reads {
    pub fn invalidate(&mut self, owner: usize, path: &Path) {
        self.pending.remove(&(owner, path.to_owned()));
    }
    pub fn invalidate_tree(&mut self, owner: usize, prefix: &Path) {
        self.pending
            .retain(|(project, path), _| *project != owner || !path.starts_with(prefix));
    }
    pub fn select(&mut self, owner: usize) -> usize {
        self.next += 1;
        self.selections.insert(owner, self.next);
        self.next
    }
    pub fn selection(&self, owner: usize) -> usize {
        self.selections.get(&owner).copied().unwrap_or_default()
    }
    pub fn selected(&self, owner: usize, token: usize) -> bool {
        self.selection(owner) == token
    }
    pub fn begin(
        &mut self,
        owner: usize,
        path: PathBuf,
        draft: Option<String>,
        reload: bool,
    ) -> usize {
        self.next += 1;
        self.pending.insert(
            (owner, path),
            Pending {
                token: self.next,
                draft,
                reload,
            },
        );
        self.next
    }
    pub fn finish(
        &mut self,
        owner: usize,
        path: &Path,
        token: usize,
        draft: Option<&str>,
    ) -> ReadOutcome {
        let key = (owner, path.to_owned());
        if self
            .pending
            .get(&key)
            .is_none_or(|pending| pending.token != token)
        {
            return ReadOutcome::Stale;
        }
        let pending = self.pending.remove(&key).unwrap();
        // The result was read before these newly typed edits; applying it would
        // replace user work even when the original reload was explicitly confirmed.
        if pending.draft.as_deref() != draft {
            return ReadOutcome::Edited;
        }
        ReadOutcome::Apply
    }
    pub fn has_reloads(&self) -> bool {
        self.pending.values().any(|pending| pending.reload)
    }
}
