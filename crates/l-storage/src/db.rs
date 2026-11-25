//! Database connection owned by a dedicated thread.
//!
//! The UI thread never touches SQLite directly: all access goes through
//! `Db::call`, which dispatches a closure to the DB thread and blocks the
//! *caller* on a oneshot. Callers on the UI thread should use the async
//! wrappers in the app layer or accept the short hop.

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::migrations;

/// Handle to the database thread.
pub struct Db {
    tx: Sender<Msg>,
    _thread: JoinHandle<()>,
}

type Reply = Box<dyn Any + Send>;
type Task = Box<dyn FnOnce(&mut Connection) -> Reply + Send>;

enum Msg {
    Run(Task, std::sync::mpsc::Sender<Result<Reply, String>>),
    Shutdown,
}

impl Db {
    /// Open (or create) the database at `path`, run migrations, spawn thread.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        // Open once on this thread to run migrations, then move the
        // connection to the worker.
        let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        migrations::run(&mut conn).map_err(|e| e.clone())?;

        let (tx, rx) = channel::<Msg>();
        let thread = std::thread::Builder::new()
            .name("l-db".into())
            .spawn(move || {
                let mut conn = conn;
                while let Ok(msg) = rx.recv() {
                    match msg {
                        Msg::Run(task, reply) => {
                            // Catch panics so a bad query can't kill the thread.
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    task(&mut conn)
                                }));
                            match result {
                                Ok(v) => {
                                    let _ = reply.send(Ok(v));
                                }
                                Err(_) => {
                                    let _ = reply.send(Err("database task panicked".into()));
                                }
                            }
                        }
                        Msg::Shutdown => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            tx,
            _thread: thread,
        })
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, String> {
        Self::open(PathBuf::from(":memory:"))
    }

    /// Run `f` on the DB thread and return its result.
    pub fn call<R, F>(&self, f: F) -> Result<R, String>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> R + Send + 'static,
    {
        let (rtx, rrx) = channel();
        let task: Task = Box::new(move |conn| Box::new(f(conn)) as Reply);
        self.tx
            .send(Msg::Run(task, rtx))
            .map_err(|_| "database thread gone".to_string())?;
        let reply = rrx
            .recv()
            .map_err(|_| "database thread gone".to_string())??;
        reply
            .downcast::<R>()
            .map(|b| *b)
            .map_err(|_| "type mismatch in db reply".to_string())
    }

    /// Shutdown the worker thread.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        self.shutdown();
    }
}
