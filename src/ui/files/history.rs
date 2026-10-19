use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
};
use tervia::state::ReadingPosition;

const LIMIT: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct Location {
    pub path: PathBuf,
    pub position: ReadingPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Direction {
    Back,
    Forward,
}

#[derive(Clone, Debug)]
pub(in crate::ui) struct Step {
    pub target: Location,
    current: Location,
    direction: Direction,
    generation: Arc<()>,
}

#[derive(Default)]
pub(in crate::ui) struct History {
    back: VecDeque<Location>,
    forward: VecDeque<Location>,
    pending: Option<Step>,
}

impl History {
    pub fn record(&mut self, from: Location, to: Location) {
        self.cancel();
        if from == to {
            return;
        }
        Self::push(&mut self.back, from);
        self.forward.clear();
    }

    pub fn begin(&mut self, direction: Direction, current: Location) -> Option<Step> {
        if self.pending.is_some() {
            return None;
        }
        let target = self.stack(direction).back()?.clone();
        let step = Step {
            target,
            current,
            direction,
            generation: Arc::new(()),
        };
        self.pending = Some(step.clone());
        Some(step)
    }

    pub fn finish(&mut self, step: &Step) -> bool {
        if !self.is_current(step) {
            return false;
        }
        // The caller restores the destination before committing. Failed or
        // superseded reads must never move a stack entry to the other direction.
        let pending = self.pending.take().unwrap();
        self.stack_mut(pending.direction).pop_back();
        let opposite = match pending.direction {
            Direction::Back => Direction::Forward,
            Direction::Forward => Direction::Back,
        };
        Self::push(self.stack_mut(opposite), pending.current);
        true
    }

    pub fn reject(&mut self, step: &Step) -> bool {
        if !self.is_current(step) {
            return false;
        }
        let pending = self.pending.take().unwrap();
        self.stack_mut(pending.direction).pop_back();
        true
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn abandon(&mut self, step: &Step) -> bool {
        if !self.is_current(step) {
            return false;
        }
        self.cancel();
        true
    }

    pub fn can_back(&self) -> bool {
        self.pending.is_none() && !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        self.pending.is_none() && !self.forward.is_empty()
    }

    pub fn relocate(&mut self, from: &Path, to: &Path) {
        self.cancel();
        for location in self.back.iter_mut().chain(self.forward.iter_mut()) {
            if let Ok(suffix) = location.path.strip_prefix(from) {
                // An empty suffix must not turn a regular file into `file/`.
                location.path = if suffix.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(suffix)
                };
            }
        }
    }

    pub fn is_current(&self, step: &Step) -> bool {
        self.pending
            .as_ref()
            // Each transaction owns a distinct token, including across projects.
            .is_some_and(|pending| Arc::ptr_eq(&pending.generation, &step.generation))
    }

    fn stack(&self, direction: Direction) -> &VecDeque<Location> {
        match direction {
            Direction::Back => &self.back,
            Direction::Forward => &self.forward,
        }
    }

    fn stack_mut(&mut self, direction: Direction) -> &mut VecDeque<Location> {
        match direction {
            Direction::Back => &mut self.back,
            Direction::Forward => &mut self.forward,
        }
    }

    fn push(stack: &mut VecDeque<Location>, location: Location) {
        if stack.back() == Some(&location) {
            return;
        }
        stack.push_back(location);
        if stack.len() > LIMIT {
            stack.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use tervia::state::ReadingPosition;

    fn location(path: &str, line: u32) -> Location {
        Location {
            path: path.into(),
            position: ReadingPosition {
                line,
                column: line + 1,
                scroll_x: line as f32 + 0.25,
                scroll_y: line as f32 * 24.0,
                source: true,
            },
        }
    }

    #[test]
    fn round_trips_preserve_the_live_caret_and_scroll() {
        let mut history = History::default();
        let origin = location("mock.ts", 1);
        history.record(origin.clone(), location("target.ts", 2));
        let live_destination = location("target.ts", 9);
        let back = history
            .begin(Direction::Back, live_destination.clone())
            .unwrap();
        assert_eq!(back.target, origin);
        assert!(!history.can_back());
        assert!(!history.can_forward());
        assert!(history.begin(Direction::Back, origin.clone()).is_none());
        assert!(history.begin(Direction::Forward, origin).is_none());
        assert!(history.finish(&back));
        let live_origin = location("mock.ts", 5);
        let forward = history
            .begin(Direction::Forward, live_origin.clone())
            .unwrap();
        assert_eq!(forward.target, live_destination);
        assert!(history.finish(&forward));
        let back = history.begin(Direction::Back, live_destination).unwrap();
        assert_eq!(back.target, live_origin);
    }

    #[test]
    fn a_successful_new_jump_truncates_the_forward_branch() {
        let mut history = History::default();
        let first = location("mock.ts", 0);
        let second = location("target.ts", 1);
        let third = location("third.ts", 2);
        history.record(first.clone(), second.clone());
        history.record(second.clone(), third.clone());
        let back = history.begin(Direction::Back, third).unwrap();
        assert!(history.finish(&back));
        assert!(history.can_forward());
        let branch = location("branch.ts", 3);
        history.record(second.clone(), branch.clone());
        assert!(!history.can_forward());
        let back = history.begin(Direction::Back, branch).unwrap();
        assert_eq!(back.target, second);
        assert!(history.finish(&back));
        let back = history.begin(Direction::Back, second).unwrap();
        assert_eq!(back.target, first);
    }

    #[test]
    fn cancel_and_new_jumps_invalidate_pending_steps_without_committing_them() {
        let mut history = History::default();
        let origin = location("mock.ts", 0);
        let current = location("target.ts", 1);
        history.record(origin.clone(), current.clone());
        let stale = history.begin(Direction::Back, current.clone()).unwrap();
        history.cancel();
        let replacement = history.begin(Direction::Back, current.clone()).unwrap();
        assert_eq!(replacement.target, origin);
        assert!(!history.finish(&stale));
        assert!(!history.reject(&stale));
        assert!(!history.can_back());
        let next = location("next.ts", 2);
        history.record(current.clone(), next.clone());
        assert!(!history.finish(&replacement));
        assert!(!history.can_forward());
        let back = history.begin(Direction::Back, next).unwrap();
        assert_eq!(back.target, current);
    }

    #[test]
    fn rejecting_a_missing_target_keeps_the_opposite_stack_and_older_targets() {
        let mut history = History::default();
        let locations = [
            location("mock.ts", 0),
            location("missing.ts", 1),
            location("third.ts", 2),
            location("fourth.ts", 3),
        ];
        for pair in locations.windows(2) {
            history.record(pair[0].clone(), pair[1].clone());
        }
        let back = history
            .begin(Direction::Back, locations[3].clone())
            .unwrap();
        assert!(history.finish(&back));
        let missing = history
            .begin(Direction::Back, locations[2].clone())
            .unwrap();
        assert_eq!(missing.target, locations[1]);
        assert!(history.reject(&missing));
        assert!(!history.finish(&missing));
        assert!(history.can_back());
        assert!(history.can_forward());
        let forward = history
            .begin(Direction::Forward, locations[2].clone())
            .unwrap();
        assert_eq!(forward.target, locations[3]);
        assert!(history.finish(&forward));
        let back = history
            .begin(Direction::Back, locations[3].clone())
            .unwrap();
        assert_eq!(back.target, locations[2]);
        assert!(history.finish(&back));
        let back = history
            .begin(Direction::Back, locations[2].clone())
            .unwrap();
        assert_eq!(back.target, locations[0]);
    }

    #[test]
    fn each_stack_retains_only_the_latest_128_locations() {
        let mut history = History::default();
        for line in 0..256 {
            history.record(location("mock.ts", line), location("mock.ts", line + 1));
        }
        let mut current = location("mock.ts", 256);
        for expected in (128..256).rev() {
            let back = history.begin(Direction::Back, current).unwrap();
            assert_eq!(back.target, location("mock.ts", expected));
            current = back.target.clone();
            assert!(history.finish(&back));
        }
        assert!(!history.can_back());
        for expected in 129..257 {
            let forward = history.begin(Direction::Forward, current).unwrap();
            assert_eq!(forward.target, location("mock.ts", expected));
            current = forward.target.clone();
            assert!(history.finish(&forward));
        }
        assert!(!history.can_forward());
    }

    #[test]
    fn project_histories_cannot_commit_each_others_steps() {
        let mut first = History::default();
        let mut second = History::default();
        let origin = location("mock.ts", 0);
        let current = location("target.ts", 1);
        first.record(origin.clone(), current.clone());
        second.record(origin, current.clone());
        let first_step = first.begin(Direction::Back, current.clone()).unwrap();
        let second_step = second.begin(Direction::Back, current).unwrap();
        assert!(!second.finish(&first_step));
        assert!(!first.reject(&second_step));
        assert!(first.finish(&first_step));
        assert!(second.finish(&second_step));
    }

    #[test]
    fn renames_relocate_path_prefixes_and_invalidate_pending_steps() {
        let mut history = History::default();
        let origin = location("src/mock.ts", 0);
        let current = location("src/target.ts", 1);
        history.record(origin, current.clone());
        let stale = history.begin(Direction::Back, current).unwrap();
        history.relocate("src".as_ref(), "renamed".as_ref());
        assert!(!history.finish(&stale));
        let current = location("renamed/target.ts", 1);
        let back = history.begin(Direction::Back, current.clone()).unwrap();
        assert_eq!(back.target, location("renamed/mock.ts", 0));
        assert!(history.finish(&back));
        history.relocate("src".as_ref(), "unrelated".as_ref());
        let forward = history.begin(Direction::Forward, back.target).unwrap();
        assert_eq!(forward.target, current);
    }

    #[test]
    fn file_renames_keep_a_file_path_and_leave_sibling_prefixes_unchanged() {
        let mut history = History::default();
        let origin = location("src/mock.ts", 0);
        let current = location("src2/target.ts", 1);
        history.record(origin, current.clone());
        history.relocate("src/mock.ts".as_ref(), "renamed.ts".as_ref());
        let back = history.begin(Direction::Back, current).unwrap();
        assert_eq!(back.target.path.as_os_str(), "renamed.ts");
        assert!(history.finish(&back));
        history.relocate("src".as_ref(), "renamed".as_ref());
        let forward = history.begin(Direction::Forward, back.target).unwrap();
        assert_eq!(forward.target.path.as_os_str(), "src2/target.ts");
    }

    #[test]
    fn no_op_jumps_do_not_create_history_or_discard_forward_locations() {
        let mut history = History::default();
        let origin = location("mock.ts", 0);
        let current = location("target.ts", 1);
        history.record(origin.clone(), origin.clone());
        assert!(!history.can_back());
        history.record(origin.clone(), current.clone());
        let back = history.begin(Direction::Back, current).unwrap();
        assert!(history.finish(&back));
        history.record(origin.clone(), origin.clone());
        assert!(!history.can_back());
        assert!(history.can_forward());
        let stale = history.begin(Direction::Forward, origin.clone()).unwrap();
        history.record(origin.clone(), origin);
        assert!(!history.finish(&stale));
        assert!(history.can_forward());
    }

    #[test]
    fn abandoned_reads_preserve_history_and_cannot_cancel_a_newer_step() {
        let mut history = History::default();
        let origin = location("mock.ts", 0);
        let current = location("target.ts", 1);
        history.record(origin, current.clone());
        let stale = history.begin(Direction::Back, current.clone()).unwrap();
        assert!(history.abandon(&stale));
        let latest = history.begin(Direction::Back, current).unwrap();
        assert!(!history.abandon(&stale));
        assert!(history.finish(&latest));
        assert!(history.can_forward());
    }
}
