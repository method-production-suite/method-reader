//! Turns on a book: `desktop/ui/book-queue.js`. One queue per library EPUB;
//! readers run side by side, a writer runs alone, arrival order is kept,
//! and a failure never stops the queue.
//!
//! The JavaScript held promises; here the queue is a plain state machine.
//! `ask` registers a turn and says whether it may start now; `finish` ends
//! one and returns every turn that may start because of it. The runtime
//! (src/runtime.rs) starts the engine call for each id it is handed. What a
//! page owes before any turn (tune.js's settle, `beforeEveryTurn`) is the
//! caller's to flush before it asks.

use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// Every turn a page asks for, and what it does to the book.
pub const TURNS: &[(&str, Access)] = &[
    ("save", Access::Write),
    ("convert", Access::Write),
    ("kindle", Access::Read),
    ("send", Access::Read),
    ("copy", Access::Read),
    ("check", Access::Read),
];

pub fn access_of(label: &str) -> Option<Access> {
    TURNS.iter().find(|(l, _)| *l == label).map(|(_, a)| *a)
}

pub type TurnId = u64;

#[derive(Clone, Debug)]
struct Turn {
    id: TurnId,
    label: &'static str,
    access: Access,
}

#[derive(Default, Debug)]
struct Queue {
    running: Vec<Turn>,
    waiting: VecDeque<Turn>,
}

#[derive(Default, Debug)]
pub struct BookQueue {
    queues: HashMap<String, Queue>,
    books: HashMap<TurnId, String>,
    next: TurnId,
}

/// What `ask` said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asked {
    pub id: TurnId,
    pub started: bool,
}

impl BookQueue {
    /// Ask for a turn on `book`. An unknown label is refused, never guessed
    /// at.
    pub fn ask(&mut self, book: &str, label: &str) -> Result<Asked, String> {
        let (label, access) = TURNS
            .iter()
            .find(|(l, _)| *l == label)
            .copied()
            .ok_or_else(|| format!("book-queue has no turn called \"{label}\""))?;
        self.next += 1;
        let id = self.next;
        self.books.insert(id, book.to_string());
        let queue = self.queues.entry(book.to_string()).or_default();
        queue.waiting.push_back(Turn { id, label, access });
        let started = Self::pump(queue);
        Ok(Asked {
            id,
            started: started.contains(&id),
        })
    }

    /// A turn ended, however it ended. Returns the turns that start now.
    pub fn finish(&mut self, id: TurnId) -> Vec<TurnId> {
        let Some(book) = self.books.remove(&id) else {
            return vec![];
        };
        let Some(queue) = self.queues.get_mut(&book) else {
            return vec![];
        };
        queue.running.retain(|t| t.id != id);
        let started = Self::pump(queue);
        if queue.running.is_empty() && queue.waiting.is_empty() {
            self.queues.remove(&book);
        }
        started
    }

    /// The labels running on `book` now, oldest first.
    pub fn holders(&self, book: &str) -> Vec<&'static str> {
        self.queues
            .get(book)
            .map(|q| q.running.iter().map(|t| t.label).collect())
            .unwrap_or_default()
    }

    /// Whether any book has anything running or waiting.
    pub fn is_idle(&self) -> bool {
        self.queues.is_empty()
    }

    fn pump(queue: &mut Queue) -> Vec<TurnId> {
        let mut started = Vec::new();
        while let Some(next) = queue.waiting.front() {
            let blocked = match next.access {
                Access::Write => !queue.running.is_empty(),
                Access::Read => queue.running.iter().any(|t| t.access == Access::Write),
            };
            if blocked {
                break;
            }
            let turn = queue.waiting.pop_front().expect("front exists");
            started.push(turn.id);
            queue.running.push(turn);
        }
        started
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_says_whether_it_reads_the_book_or_writes_it() {
        assert_eq!(access_of("save"), Some(Access::Write));
        assert_eq!(access_of("convert"), Some(Access::Write));
        for l in ["kindle", "send", "copy", "check"] {
            assert_eq!(access_of(l), Some(Access::Read));
        }
        let mut q = BookQueue::default();
        assert!(q.ask("/b.epub", "nonsense").is_err());
    }

    #[test]
    fn with_nothing_queued_a_turn_starts_at_once_and_two_readers_run_together() {
        let mut q = BookQueue::default();
        assert!(q.ask("/b", "send").unwrap().started);
        assert!(q.ask("/b", "copy").unwrap().started);
        assert_eq!(q.holders("/b"), vec!["send", "copy"]);
    }

    #[test]
    fn a_writer_waits_for_every_reader_and_runs_alone() {
        let mut q = BookQueue::default();
        let r1 = q.ask("/b", "kindle").unwrap();
        let r2 = q.ask("/b", "send").unwrap();
        let w = q.ask("/b", "save").unwrap();
        assert!(!w.started);
        assert!(q.finish(r1.id).is_empty());
        assert_eq!(q.finish(r2.id), vec![w.id]);
        assert_eq!(q.holders("/b"), vec!["save"]);
        let r3 = q.ask("/b", "check").unwrap();
        assert!(!r3.started, "nothing starts beside a writer");
        assert_eq!(q.finish(w.id), vec![r3.id]);
    }

    #[test]
    fn a_reader_behind_a_waiting_writer_waits_for_it() {
        let mut q = BookQueue::default();
        let r1 = q.ask("/b", "kindle").unwrap();
        let w = q.ask("/b", "save").unwrap();
        let r2 = q.ask("/b", "send").unwrap();
        assert!(!r2.started, "the change lands before the book is read");
        assert_eq!(q.finish(r1.id), vec![w.id]);
        assert_eq!(q.finish(w.id), vec![r2.id]);
    }

    #[test]
    fn another_book_does_not_wait() {
        let mut q = BookQueue::default();
        let _hung = q.ask("/a", "save").unwrap();
        assert!(q.ask("/b", "save").unwrap().started);
        assert!(q.holders("/c").is_empty());
    }

    #[test]
    fn an_empty_book_leaves_no_entry() {
        let mut q = BookQueue::default();
        let t = q.ask("/a", "copy").unwrap();
        assert!(!q.is_idle());
        q.finish(t.id);
        assert!(q.is_idle());
        assert!(q.finish(t.id).is_empty(), "finishing twice does nothing");
    }
}
