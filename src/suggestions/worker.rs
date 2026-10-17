use super::{Command, Shell, Store, Suggestion};
use std::{path::PathBuf, sync::mpsc};

type Result = std::result::Result<Option<Suggestion>, String>;
enum Request {
    Record {
        session: String,
        command: Command,
        reply: async_channel::Sender<Result>,
    },
    Complete {
        prefix: String,
        cwd: PathBuf,
        shell: Shell,
        source: Option<PathBuf>,
        reply: async_channel::Sender<std::result::Result<Vec<Suggestion>, String>>,
    },
}

#[derive(Clone)]
pub struct Worker {
    sender: mpsc::SyncSender<Request>,
}

impl Worker {
    pub fn new(path: Option<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Request>(64);
        std::thread::spawn(move || {
            let mut store = Store::new(path);
            // Record in PTY completion order even when UI tasks have been cancelled or superseded.
            for request in receiver {
                match request {
                    Request::Record {
                        session,
                        command,
                        reply,
                    } => {
                        let _ = reply.try_send(store.record(&session, &command));
                    }
                    Request::Complete {
                        prefix,
                        cwd,
                        shell,
                        source,
                        reply,
                    } => {
                        let _ = reply.try_send(store.complete_from(
                            &prefix,
                            &cwd,
                            shell,
                            source.as_deref(),
                        ));
                    }
                }
            }
        });
        Self { sender }
    }

    pub fn submit(
        &self,
        session: String,
        command: Command,
    ) -> std::result::Result<async_channel::Receiver<Result>, String> {
        let (sender, receiver) = async_channel::bounded(1);
        self.enqueue(Request::Record {
            session,
            command,
            reply: sender,
        })?;
        Ok(receiver)
    }

    pub fn complete(
        &self,
        prefix: String,
        cwd: PathBuf,
        shell: Shell,
    ) -> std::result::Result<
        async_channel::Receiver<std::result::Result<Vec<Suggestion>, String>>,
        String,
    > {
        self.complete_from(prefix, cwd, shell, None)
    }

    pub fn complete_from(
        &self,
        prefix: String,
        cwd: PathBuf,
        shell: Shell,
        source: Option<PathBuf>,
    ) -> std::result::Result<
        async_channel::Receiver<std::result::Result<Vec<Suggestion>, String>>,
        String,
    > {
        let (reply, receiver) = async_channel::bounded(1);
        self.enqueue(Request::Complete {
            prefix,
            cwd,
            shell,
            source,
            reply,
        })?;
        Ok(receiver)
    }

    fn enqueue(&self, request: Request) -> std::result::Result<(), String> {
        self.sender.try_send(request).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Command history queue is full".to_owned(),
            mpsc::TrySendError::Disconnected(_) => "Command history worker stopped".to_owned(),
        })
    }
}
