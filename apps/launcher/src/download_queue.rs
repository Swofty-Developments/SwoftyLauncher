//! Scheduling for Addon Browser install jobs.
//!
//! The scheduler runs on the UI thread. It does not perform downloads or guarantee
//! installation safety: callers must also coordinate requirements discovered after
//! a job starts with the core installer.

use std::collections::{BTreeSet, VecDeque};

const MIN_LIMIT: usize = 1;
const MAX_LIMIT: usize = 10;
pub(crate) const DEFAULT_LIMIT: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Queued,
    Active,
    Completed,
    Failed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Job<T> {
    pub(crate) id: String,
    pub(crate) request: T,
    /// Root addon and all requirements known before starting.
    pub(crate) claims: BTreeSet<String>,
    pub(crate) state: State,
}

pub(crate) struct Queue<T> {
    jobs: VecDeque<Job<T>>,
    limit: usize,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self {
            jobs: VecDeque::new(),
            limit: DEFAULT_LIMIT,
        }
    }
}

impl<T> Queue<T> {
    pub(crate) fn limit(&self) -> usize {
        self.limit
    }

    pub(crate) fn set_limit(&mut self, limit: usize) {
        self.limit = limit.clamp(MIN_LIMIT, MAX_LIMIT);
    }

    pub(crate) fn jobs(&self) -> &VecDeque<Job<T>> {
        &self.jobs
    }

    pub(crate) fn pending(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(j.state, State::Queued | State::Active))
            .count()
    }

    /// Returns false when this addon is already queued or running.
    pub(crate) fn enqueue(&mut self, id: String, request: T, mut claims: BTreeSet<String>) -> bool {
        if self
            .jobs
            .iter()
            .any(|j| j.id == id && matches!(j.state, State::Queued | State::Active))
        {
            return false;
        }
        claims.insert(id.clone());
        self.jobs.push_back(Job {
            id,
            request,
            claims,
            state: State::Queued,
        });
        true
    }

    /// Start the earliest eligible job, allowing unrelated jobs past blocked ones.
    /// Returns its index, which stays valid until a job is removed from the queue.
    pub(crate) fn start_next(&mut self) -> Option<usize> {
        if self.jobs.iter().filter(|j| j.state == State::Active).count() >= self.limit {
            return None;
        }
        let index = self.jobs.iter().position(|candidate| {
            candidate.state == State::Queued
                && !self.jobs.iter().any(|active| {
                    active.state == State::Active && !active.claims.is_disjoint(&candidate.claims)
                })
        })?;
        self.jobs[index].state = State::Active;
        Some(index)
    }

    pub(crate) fn finish(&mut self, id: &str, result: Result<(), String>) -> bool {
        let Some(job) = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.state == State::Active)
        else {
            return false;
        };
        job.state = match result {
            Ok(()) => State::Completed,
            Err(error) => State::Failed(error),
        };
        true
    }

    pub(crate) fn remove_queued(&mut self, id: &str) -> bool {
        let Some(index) = self
            .jobs
            .iter()
            .position(|j| j.id == id && j.state == State::Queued)
        else {
            return false;
        };
        self.jobs.remove(index);
        true
    }

    /// Retry the newest failed attempt, unless another request for the addon is pending.
    pub(crate) fn retry(&mut self, id: &str) -> bool {
        if self
            .jobs
            .iter()
            .any(|j| j.id == id && matches!(j.state, State::Queued | State::Active))
        {
            return false;
        }
        let Some(index) = self
            .jobs
            .iter()
            .rposition(|j| j.id == id && matches!(j.state, State::Failed(_)))
        else {
            return false;
        };
        let mut job = self.jobs.remove(index).expect("job index came from the queue");
        job.state = State::Queued;
        self.jobs.push_back(job);
        true
    }

    pub(crate) fn clear_finished(&mut self) {
        self.jobs
            .retain(|j| matches!(j.state, State::Queued | State::Active));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(q: &mut Queue<()>, id: &str, claims: &[&str]) -> bool {
        q.enqueue(id.into(), (), claims.iter().map(|s| (*s).into()).collect())
    }

    fn start(q: &mut Queue<()>) -> Option<String> {
        q.start_next().map(|i| q.jobs()[i].id.clone())
    }

    #[test]
    fn fifo_respects_limit_and_skips_blocked_jobs() {
        let mut q = Queue::default();
        add(&mut q, "a", &["shared"]);
        add(&mut q, "b", &["shared"]);
        add(&mut q, "c", &[]);
        assert_eq!(start(&mut q).as_deref(), Some("a"));
        assert_eq!(start(&mut q).as_deref(), Some("c"));
        assert_eq!(start(&mut q), None);
        assert!(q.finish("a", Ok(())));
        assert_eq!(start(&mut q).as_deref(), Some("b"));
    }

    #[test]
    fn changing_limit_does_not_interrupt_running_jobs() {
        let mut q = Queue::default();
        for id in ["a", "b", "c"] {
            add(&mut q, id, &[]);
        }
        assert_eq!(q.limit(), 2);
        start(&mut q);
        start(&mut q);
        q.set_limit(1);
        assert_eq!(start(&mut q), None);
        q.finish("a", Ok(()));
        assert_eq!(start(&mut q), None);
        q.set_limit(10);
        assert_eq!(start(&mut q).as_deref(), Some("c"));
        q.set_limit(100);
        assert_eq!(q.limit(), 10);
        q.set_limit(0);
        assert_eq!(q.limit(), 1);
    }

    #[test]
    fn deduplicate_retry_remove_and_clear() {
        let mut q = Queue::default();
        assert!(add(&mut q, "a", &[]));
        assert!(!add(&mut q, "a", &[]));
        assert!(add(&mut q, "b", &[]));
        assert_eq!(q.pending(), 2);
        assert!(q.remove_queued("b"));
        start(&mut q);
        assert!(!q.remove_queued("a"));
        assert!(q.finish("a", Err("failed".into())));
        assert!(q.retry("a"));
        assert_eq!(q.jobs()[0].state, State::Queued);
        assert_eq!(q.jobs()[0].request, ());
        start(&mut q);
        assert!(q.finish("a", Ok(())));
        q.clear_finished();
        assert!(q.jobs().is_empty());
    }

    #[test]
    fn root_ids_are_automatically_claimed() {
        let mut q = Queue::default();
        add(&mut q, "dependency", &[]);
        add(&mut q, "root", &["dependency"]);
        assert_eq!(start(&mut q).as_deref(), Some("dependency"));
        assert_eq!(start(&mut q), None);
    }
}
