use super::journal::Journal;
use super::*;
use crate::{
    pty::{PtySession, default_shell},
    terminal::TerminalBuffer,
};
use alacritty_terminal::{
    event::{Event, WindowSize},
    vte::ansi::Rgb,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct State {
    pty: Option<PtySession>,
    journal: Option<Journal>,
    buffer: TerminalBuffer,
    size: PtySize,
    attached: bool,
    colors: Vec<[u8; 3]>,
}

pub(super) fn packet(reader: &mut BufReader<TcpStream>) -> io::Result<Value> {
    let mut bytes = Vec::new();
    let count = reader.take(MAX_PACKET + 1).read_until(b'\n', &mut bytes)?;
    if count == 0 || count as u64 > MAX_PACKET || bytes.last() != Some(&b'\n') {
        return Err(io::Error::other("Invalid or oversized terminal packet"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub(super) fn reply(stream: &mut TcpStream, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")
}

pub(super) fn run(directory: &Path) -> io::Result<()> {
    runtime::private_directory(directory)?;
    let source = fs::read(directory.join("launch.json"))?;
    if source.len() > MAX_PACKET as usize {
        return Err(io::Error::other("Oversized terminal launch"));
    }
    let launch: Value = serde_json::from_slice(&source)?;
    if launch["version"].as_u64() != Some(VERSION) {
        return Err(io::Error::other("Incompatible terminal host"));
    }
    let token = field(&launch, "token")?.to_owned();
    let cwd: PathBuf = serde_json::from_value(launch["cwd"].clone())?;
    let size = PtySize {
        rows: launch["rows"]
            .as_u64()
            .filter(|n| (1..=500).contains(n))
            .ok_or_else(|| io::Error::other("Invalid terminal rows"))? as u16,
        cols: launch["cols"]
            .as_u64()
            .filter(|n| (2..=1000).contains(n))
            .ok_or_else(|| io::Error::other("Invalid terminal columns"))? as u16,
        pixel_width: 0,
        pixel_height: 0,
    };
    // Old launch files omit shell; reconnect always uses the existing host's own metadata.
    let shell = match launch.get("shell").filter(|shell| !shell.is_null()) {
        Some(shell) => serde_json::from_value::<PathBuf>(shell.clone())?.into_os_string(),
        None => default_shell(),
    };
    let (pty, events, tracker) = match launch.get("task").filter(|task| !task.is_null()) {
        Some(value) => {
            let task = crate::tasks::Task::from_json(value).map_err(io::Error::other)?;
            let command =
                crate::pty::task_command_for(&shell, &cwd, &task).map_err(io::Error::other)?;
            let (pty, events) = PtySession::spawn(command, size).map_err(io::Error::other)?;
            (pty, events, None)
        }
        None => PtySession::spawn_shell_with(&shell, &cwd, size).map_err(io::Error::other)?,
    };
    let mut journal = Journal::create(&directory.join("output"))?;
    journal.resize(size)?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let metadata = json!({
        "version": VERSION, "port": listener.local_addr()?.port(), "token": token,
        "capabilities": ["colors"],
        "pid": pty.process_id(), "shell": shell.to_string_lossy(),
        "integration": tracker.as_ref().map(Tracker::token),
    });
    let state = Arc::new(Mutex::new(State {
        pty: Some(pty),
        journal: Some(journal),
        buffer: TerminalBuffer::new(size.cols as usize, size.rows as usize),
        size,
        attached: false,
        colors: serde_json::from_value(launch["colors"].clone())?,
    }));
    runtime::write_private(&directory.join("endpoint.json"), &metadata)?;
    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = stop.clone();
    let accept_state = state.clone();
    let accept_meta = metadata.clone();
    let accepting = std::thread::spawn(move || {
        let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        while !accept_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if connections.load(Ordering::Relaxed) >= 16 {
                        continue;
                    }
                    connections.fetch_add(1, Ordering::Relaxed);
                    let state = accept_state.clone();
                    let metadata = accept_meta.clone();
                    let stop = accept_stop.clone();
                    let connections = connections.clone();
                    std::thread::spawn(move || {
                        serve(stream, &state, &metadata, &stop);
                        connections.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(_) => break,
            }
        }
    });
    let mut finished = false;
    while !stop.load(Ordering::Acquire) {
        match events.try_recv() {
            Ok(PtyEvent::Data(bytes)) => {
                let mut state = state.lock().unwrap();
                state.journal.as_mut().unwrap().append(0, &bytes)?;
                state.buffer.feed(&bytes);
                answer_queries(&mut state);
            }
            Ok(PtyEvent::Exited(status)) => {
                let mut state = state.lock().unwrap();
                state
                    .journal
                    .as_mut()
                    .unwrap()
                    .append(2, status.as_bytes())?;
                let mut ended = metadata.clone();
                ended["length"] = json!(state.journal.as_ref().unwrap().length);
                runtime::write_private(&directory.join("finished.json"), &ended)?;
                finished = true;
                break;
            }
            Ok(PtyEvent::Error(error)) => state
                .lock()
                .unwrap()
                .journal
                .as_mut()
                .unwrap()
                .append(3, error.as_bytes())?,
            Err(async_channel::TryRecvError::Closed) => break,
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    stop.store(true, Ordering::Release);
    let _ = accepting.join();
    let mut state = state.lock().unwrap();
    state.pty.take();
    state.journal.take();
    drop(state);
    if !finished {
        fs::remove_dir_all(directory)?;
    }
    Ok(())
}

fn serve(stream: TcpStream, state: &Arc<Mutex<State>>, metadata: &Value, stop: &AtomicBool) {
    // BSD/macOS accepted sockets inherit O_NONBLOCK; an idle control channel must wait for its next packet.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream);
    let mut lease = false;
    while !stop.load(Ordering::Acquire) {
        let Ok(request) = packet(&mut reader) else {
            break;
        };
        if request["version"].as_u64() != Some(VERSION) || request["token"] != metadata["token"] {
            let _ = reply(
                reader.get_mut(),
                &json!({"ok": false, "error": "Terminal authentication or protocol failed"}),
            );
            break;
        }
        let response = (|| -> Result<Value, String> {
            let mut state = state.lock().unwrap();
            match request["op"].as_str() {
                Some("attach") if !lease => {
                    if state.attached {
                        return Err("Terminal is already attached to another window".into());
                    }
                    state.attached = true;
                    lease = true;
                    if request["colors"]
                        .as_array()
                        .is_some_and(|colors| !colors.is_empty())
                    {
                        state.colors = read_colors(&request)?;
                    }
                }
                Some("close") => stop.store(true, Ordering::Release),
                Some("input") if lease => {
                    let bytes: Vec<u8> = serde_json::from_value(request["bytes"].clone())
                        .map_err(|e| e.to_string())?;
                    if bytes.len() > 65536 {
                        return Err("Terminal input exceeds 64 KiB".into());
                    }
                    state
                        .pty
                        .as_ref()
                        .ok_or("Terminal has ended")?
                        .write(&bytes)?;
                }
                Some("resize") if lease => {
                    let cols = request["cols"]
                        .as_u64()
                        .filter(|n| (2..=1000).contains(n))
                        .ok_or("Invalid columns")? as u16;
                    let rows = request["rows"]
                        .as_u64()
                        .filter(|n| (1..=500).contains(n))
                        .ok_or("Invalid rows")? as u16;
                    let cell_width = request["width"].as_u64().unwrap_or(0).min(65535) as u16;
                    let cell_height = request["height"].as_u64().unwrap_or(0).min(65535) as u16;
                    let size = PtySize {
                        cols,
                        rows,
                        pixel_width: cols.saturating_mul(cell_width),
                        pixel_height: rows.saturating_mul(cell_height),
                    };
                    if state.size != size {
                        state
                            .journal
                            .as_mut()
                            .ok_or("Terminal has ended")?
                            .resize(size)
                            .map_err(|e| e.to_string())?;
                        state.buffer.resize(cols as usize, rows as usize);
                        state.pty.as_ref().ok_or("Terminal has ended")?.resize(
                            cols as usize,
                            rows as usize,
                            cell_width,
                            cell_height,
                        )?;
                        state.size = size;
                    }
                }
                Some("status") if lease => {}
                Some("colors") if lease => {
                    state.colors = read_colors(&request)?;
                }
                Some("clear") if lease => {
                    let bytes = b"\x1b[2J\x1b[3J";
                    state
                        .journal
                        .as_mut()
                        .ok_or("Terminal has ended")?
                        .append(0, bytes)
                        .map_err(|e| e.to_string())?;
                    state.buffer.feed(bytes);
                }
                _ => return Err("Invalid terminal operation".into()),
            }
            let mut response = metadata.clone();
            response["ok"] = json!(true);
            response["length"] = json!(state.journal.as_ref().map_or(0, |journal| journal.length));
            response["agent"] = json!(
                state
                    .pty
                    .as_ref()
                    .and_then(PtySession::foreground_agent)
                    .map(|agent| agent.name())
            );
            Ok(response)
        })();
        let response = response.unwrap_or_else(|error| json!({"ok":false,"error":error}));
        if reply(reader.get_mut(), &response).is_err() {
            break;
        }
    }
    if lease {
        state.lock().unwrap().attached = false;
    }
}

fn read_colors(request: &Value) -> Result<Vec<[u8; 3]>, String> {
    let colors: Vec<[u8; 3]> =
        serde_json::from_value(request["colors"].clone()).map_err(|error| error.to_string())?;
    if colors.len() > 269 {
        return Err("Invalid terminal palette".into());
    }
    Ok(colors)
}

fn answer_queries(state: &mut State) {
    // The host answers device queries even while the GUI is absent. Replay must never answer them again.
    for event in state.buffer.events() {
        let reply = match event {
            Event::PtyWrite(reply) => Some(reply),
            Event::TextAreaSizeRequest(formatter) => Some(formatter(WindowSize {
                num_lines: state.size.rows,
                num_cols: state.size.cols,
                cell_width: state.size.pixel_width / state.size.cols,
                cell_height: state.size.pixel_height / state.size.rows,
            })),
            Event::ColorRequest(index, formatter) => {
                let color = state.buffer.term.colors()[index].unwrap_or_else(|| {
                    let channels = state.colors.get(index).copied().unwrap_or_default();
                    Rgb {
                        r: channels[0],
                        g: channels[1],
                        b: channels[2],
                    }
                });
                Some(formatter(color))
            }
            _ => None,
        };
        if let (Some(reply), Some(pty)) = (reply, state.pty.as_ref()) {
            let _ = pty.write(reply.as_bytes());
        }
    }
}
