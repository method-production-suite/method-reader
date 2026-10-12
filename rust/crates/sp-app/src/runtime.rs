//! Engine calls off the UI thread. The window never blocks on the engine:
//! a request is handed to a worker, and its progress lines and its answer
//! come back as events the next frame reads. In tests the runtime runs
//! each request at once, on the caller's thread, and still delivers its
//! events on the next poll, so the order a screen sees is the order the
//! window would see.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::Value;

use crate::engine::{Engine, Reply, Request};

pub type JobId = u64;

#[derive(Debug)]
pub enum Event {
    Progress { job: JobId, line: Value },
    Done { job: JobId, reply: Reply },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A worker thread per call; the frame is woken when it answers.
    Threads,
    /// Run at once on the caller's thread (tests).
    Inline,
}

pub struct Runtime {
    engine: Arc<dyn Engine>,
    mode: Mode,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    next: JobId,
    repaint: Option<egui::Context>,
}

impl Runtime {
    pub fn new(engine: Arc<dyn Engine>, mode: Mode) -> Self {
        let (tx, rx) = channel();
        Self {
            engine,
            mode,
            tx,
            rx,
            next: 0,
            repaint: None,
        }
    }

    /// Wake this context when a worker answers.
    pub fn set_repaint(&mut self, ctx: egui::Context) {
        self.repaint = Some(ctx);
    }

    pub fn spawn(&mut self, request: Request) -> JobId {
        self.next += 1;
        let job = self.next;
        let engine = Arc::clone(&self.engine);
        let tx = self.tx.clone();
        let repaint = self.repaint.clone();
        let work = move || {
            let progress_tx = tx.clone();
            let progress_repaint = repaint.clone();
            let reply = engine.run(&request, &mut |line| {
                let _ = progress_tx.send(Event::Progress { job, line });
                if let Some(ctx) = &progress_repaint {
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Event::Done { job, reply });
            if let Some(ctx) = &repaint {
                ctx.request_repaint();
            }
        };
        match self.mode {
            Mode::Inline => work(),
            Mode::Threads => {
                std::thread::spawn(work);
            }
        }
        job
    }

    /// Every event that has arrived since the last poll.
    pub fn poll(&mut self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}
