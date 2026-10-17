use super::*;
use crate::{agent::Agent, pty::PtyEvent};
use std::{
    fs::File,
    io::BufReader,
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub struct Connection {
    sender: Option<mpsc::SyncSender<Command>>,
    stop: Arc<AtomicBool>,
    identity: Arc<Mutex<Option<Agent>>>,
    pid: Option<u32>,
    target: PathBuf,
    supports_colors: bool,
}

struct Command {
    value: Value,
    completion: Option<mpsc::SyncSender<Result<(), String>>>,
}

impl Connection {
    pub fn flush(&self) -> Result<(), String> {
        let Some(sender) = &self.sender else {
            return Ok(());
        };
        if self.target.join("finished.json").exists() {
            return Ok(());
        }
        let (done, reply) = mpsc::sync_channel(1);
        match sender.try_send(Command {
            value: json!({"op":"status"}),
            completion: Some(done),
        }) {
            Ok(()) => reply
                .recv_timeout(Duration::from_secs(3))
                .map_err(|error| format!("Waiting for terminal input to be preserved: {error}"))?,
            Err(mpsc::TrySendError::Disconnected(_)) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                Err("Terminal input is still being sent; wait before quitting".into())
            }
        }
    }
    pub fn foreground_agent(&self) -> Option<Agent> {
        *self.identity.lock().unwrap()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.pid
    }
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        // Packet bounds apply per chunk, while large bracketed pastes retain byte order.
        for bytes in bytes.chunks(65536) {
            self.send(json!({"op":"input","bytes":bytes}))?;
        }
        Ok(())
    }
    pub fn resize(
        &self,
        columns: usize,
        rows: usize,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        self.send(json!({"op":"resize","cols":columns,"rows":rows,"width":width,"height":height}))
    }
    pub fn terminate(&self) -> Result<(), String> {
        terminate(&self.target).map_err(|error| error.to_string())
    }
    pub fn clear(&self) -> Result<(), String> {
        self.send(json!({"op":"clear"}))
    }

    pub fn colors(&self, colors: Vec<[u8; 3]>) -> Result<(), String> {
        // New interfaces can reconnect to hosts from an earlier app version without requiring optional operations.
        if !self.supports_colors {
            return Ok(());
        }
        self.send(json!({"op":"colors","colors":colors}))
    }
    fn send(&self, value: Value) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or("Terminal has ended")?
            .try_send(Command {
                value,
                completion: None,
            })
            .map_err(|error| error.to_string())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.sender.take();
    }
}

fn metadata(directory: &Path) -> io::Result<Value> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("Invalid terminal directory"));
    }
    let bytes = fs::read(directory.join("endpoint.json"))?;
    if bytes.len() > MAX_PACKET as usize {
        return Err(io::Error::other("Oversized terminal endpoint"));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["version"].as_u64() != Some(VERSION) {
        return Err(io::Error::other("Incompatible terminal protocol"));
    }
    Ok(value)
}

fn socket(metadata: &Value) -> io::Result<BufReader<TcpStream>> {
    let port = metadata["port"]
        .as_u64()
        .filter(|port| *port > 0 && *port <= 65535)
        .ok_or_else(|| io::Error::other("Invalid terminal port"))? as u16;
    let stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(2),
    )?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(BufReader::new(stream))
}

fn request(
    stream: &mut BufReader<TcpStream>,
    metadata: &Value,
    mut value: Value,
) -> io::Result<Value> {
    value["version"] = json!(VERSION);
    value["token"] = metadata["token"].clone();
    host::reply(stream.get_mut(), &value)?;
    let response = host::packet(stream)?;
    if response["ok"] != true {
        return Err(io::Error::other(
            response["error"]
                .as_str()
                .unwrap_or("Terminal request failed"),
        ));
    }
    Ok(response)
}

pub(super) fn terminate(directory: &Path) -> io::Result<()> {
    if directory.join("finished.json").is_file() {
        fs::remove_dir_all(directory)?;
    } else {
        let meta = metadata(directory)?;
        request(&mut socket(&meta)?, &meta, json!({"op":"close"}))?;
    }
    #[cfg(target_os = "macos")]
    super::macos::cleanup(directory);
    Ok(())
}

pub(super) fn connect(directory: &Path, colors: &[[u8; 3]]) -> io::Result<Opened> {
    let meta = metadata(directory)?;
    let finished = directory.join("finished.json").is_file();
    let mut stream = if finished { None } else { Some(socket(&meta)?) };
    let response = if let Some(stream) = &mut stream {
        // A just-dropped client may still be releasing its lease on the host thread.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match request(stream, &meta, json!({"op":"attach","colors":colors})) {
                Ok(value) => break value,
                Err(error)
                    if error.to_string().contains("already attached")
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error),
            }
        }
    } else {
        serde_json::from_slice(&fs::read(directory.join("finished.json"))?)?
    };
    let replay_length = response["length"]
        .as_u64()
        .ok_or_else(|| io::Error::other("Invalid terminal replay boundary"))?;
    let pid = meta["pid"].as_u64().map(|pid| pid as u32);
    let tracker = meta["integration"]
        .as_str()
        .map(|token| Tracker::new(token.to_owned(), meta["shell"].as_str().unwrap_or("")));
    let stop = Arc::new(AtomicBool::new(false));
    let identity = Arc::new(Mutex::new(
        response["agent"].as_str().and_then(Agent::from_executable),
    ));
    let (events, receiver) = async_channel::bounded(64);
    let (sender, commands) = mpsc::sync_channel::<Command>(1024);
    if let Some(mut stream) = stream.take() {
        let identity = identity.clone();
        let stop = stop.clone();
        let events = events.clone();
        let meta = meta.clone();
        std::thread::spawn(move || {
            // Drain accepted input/resize requests before releasing the lease, even if the GUI has detached.
            loop {
                let command = match commands.recv_timeout(Duration::from_millis(400)) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) => Command {
                        value: json!({"op":"status"}),
                        completion: None,
                    },
                    Err(_) => break,
                };
                let result = request(&mut stream, &meta, command.value);
                if let Some(completion) = command.completion {
                    let _ =
                        completion.send(result.as_ref().map(|_| ()).map_err(ToString::to_string));
                }
                match result {
                    Ok(response) => {
                        *identity.lock().unwrap() =
                            response["agent"].as_str().and_then(Agent::from_executable)
                    }
                    Err(error) => {
                        if !stop.load(Ordering::Acquire) {
                            deliver(&events, PtyEvent::Error(error.to_string()), &stop);
                        }
                        break;
                    }
                }
            }
        });
    }
    let mut file = File::open(directory.join("output"))?;
    let reader_stop = stop.clone();
    std::thread::spawn(move || {
        let mut offset = 0;
        let mut replaying = true;
        while !reader_stop.load(Ordering::Acquire) {
            if replaying && offset >= replay_length {
                if !deliver(&events, PtyEvent::ReplayComplete, &reader_stop) {
                    break;
                }
                replaying = false;
            }
            match journal::read(&mut file, &mut offset) {
                Ok(Some(event)) => {
                    let ended = matches!(event, PtyEvent::Exited(_));
                    if !deliver(&events, event, &reader_stop) || ended {
                        break;
                    }
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(8)),
                Err(error) => {
                    deliver(&events, PtyEvent::Error(error.to_string()), &reader_stop);
                    break;
                }
            }
        }
    });
    Ok((
        Connection {
            sender: (!finished).then_some(sender),
            stop,
            identity,
            pid,
            target: directory.to_owned(),
            supports_colors: response["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "colors")),
        },
        receiver,
        tracker,
    ))
}

fn deliver(
    sender: &async_channel::Sender<PtyEvent>,
    mut event: PtyEvent,
    stop: &AtomicBool,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(async_channel::TrySendError::Full(value)) => {
                event = value;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return false,
        }
    }
    false
}
