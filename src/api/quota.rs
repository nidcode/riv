//! Client-side per-minute quota tracking so batches can be shrunk before the server says 429.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    GetSession,
    ListSessions,
    GetSchedule,
    ReserveSessions,
    CancelReservation,
    AssociateFavorites,
    DisassociateFavorite,
    CreatePersonalTime,
    UpdatePersonalTime,
    DeletePersonalTime,
}

impl Op {
    /// Documented per-minute quota (sessions for the two bulk operations).
    pub fn limit(self) -> u32 {
        match self {
            Op::GetSession | Op::ListSessions => 120,
            Op::GetSchedule => 60,
            _ => 30,
        }
    }
}

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Quota {
    used: Mutex<HashMap<Op, VecDeque<(Instant, u32)>>>,
}

impl Quota {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, op: Op, f: impl FnOnce(&mut VecDeque<(Instant, u32)>) -> R) -> R {
        let mut guard = self.used.lock().unwrap_or_else(|p| p.into_inner());
        let q = guard.entry(op).or_default();
        let now = Instant::now();
        while q.front().is_some_and(|(t, _)| now.duration_since(*t) >= WINDOW) {
            q.pop_front();
        }
        f(q)
    }

    pub fn remaining(&self, op: Op) -> u32 {
        self.with(op, |q| op.limit().saturating_sub(q.iter().map(|(_, n)| n).sum()))
    }

    pub fn record(&self, op: Op, n: u32) {
        self.with(op, |q| q.push_back((Instant::now(), n)));
    }

    /// Time until at least one unit frees up (None if some is already available).
    pub fn wait_hint(&self, op: Op) -> Option<Duration> {
        self.with(op, |q| {
            let used: u32 = q.iter().map(|(_, n)| n).sum();
            if used < op.limit() {
                return None;
            }
            q.front().map(|(t, _)| WINDOW.saturating_sub(Instant::now().duration_since(*t)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_decreases() {
        let q = Quota::new();
        assert_eq!(q.remaining(Op::ReserveSessions), 30);
        q.record(Op::ReserveSessions, 12);
        assert_eq!(q.remaining(Op::ReserveSessions), 18);
        q.record(Op::ReserveSessions, 18);
        assert_eq!(q.remaining(Op::ReserveSessions), 0);
        assert!(q.wait_hint(Op::ReserveSessions).is_some());
        assert_eq!(q.remaining(Op::GetSchedule), 60);
    }
}
