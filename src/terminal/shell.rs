//! A private, bounded OSC channel keeps shell lifecycle metadata out of the visible grid.
use super::TerminalBuffer;
use crate::suggestions::{Command, Shell};
use std::path::PathBuf;

pub enum Event {
    Output(Vec<u8>),
    Started,
    Completed(Command),
    Prompt(PathBuf),
    History(Option<PathBuf>),
}

struct Capture {
    input: String,
    cwd: PathBuf,
    output: TerminalBuffer,
}

pub struct Tracker {
    token: String,
    prefix: Vec<u8>,
    shell: Shell,
    pending: Vec<u8>,
    passthrough: bool,
    escaped: bool,
    capture: Option<Capture>,
}

impl Tracker {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn shell(&self) -> Shell {
        self.shell
    }

    pub fn new(token: String, shell: &str) -> Self {
        Self {
            prefix: format!("\x1b]777;tervia;{token};").into_bytes(),
            token,
            shell: Shell::from_executable(shell),
            pending: Vec::new(),
            passthrough: false,
            escaped: false,
            capture: None,
        }
    }

    fn output(&mut self, bytes: Vec<u8>, events: &mut Vec<Event>) {
        if bytes.is_empty() {
            return;
        }
        if let Some(capture) = &mut self.capture {
            capture.output.feed(&bytes);
            capture.output.events();
        }
        events.push(Event::Output(bytes));
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = vec![];
        let mut output = Vec::new();
        for &byte in bytes {
            if self.passthrough {
                output.push(byte);
                if byte == 7 || (self.escaped && byte == b'\\') {
                    self.passthrough = false;
                }
                self.escaped = byte == 27;
            } else if self.pending.is_empty() {
                if byte == 27 {
                    self.pending.push(byte);
                } else {
                    output.push(byte);
                }
            } else if self.pending.len() == 1 && byte != b']' {
                output.extend(std::mem::take(&mut self.pending));
                if byte == 27 {
                    self.pending.push(byte);
                } else {
                    output.push(byte);
                }
            } else {
                self.pending.push(byte);
                if byte == 7 || self.pending.ends_with(b"\x1b\\") {
                    let frame = std::mem::take(&mut self.pending);
                    if frame.starts_with(&self.prefix) {
                        self.output(std::mem::take(&mut output), &mut events);
                        if !self.frame(&frame, &mut events) {
                            output.extend(frame);
                        }
                    } else {
                        output.extend(frame);
                    }
                } else if self.pending.len() > 65536 {
                    output.extend(std::mem::take(&mut self.pending));
                    self.passthrough = true;
                    self.escaped = byte == 27;
                }
            }
        }
        self.output(output, &mut events);
        events
    }

    fn frame(&mut self, bytes: &[u8], events: &mut Vec<Event>) -> bool {
        let end = bytes.len() - if bytes.ends_with(&[7]) { 1 } else { 2 };
        let Ok(body) = std::str::from_utf8(&bytes[self.prefix.len()..end]) else {
            return false;
        };
        let parts: Vec<_> = body.split(';').collect();
        match parts.as_slice() {
            ["H", path] => {
                let Some(path) = unhex(path).filter(|s| !s.contains('\0')) else {
                    return false;
                };
                let path = if path.is_empty() {
                    None
                } else {
                    let path = PathBuf::from(path);
                    if !path.is_absolute() {
                        return false;
                    }
                    Some(path)
                };
                events.push(Event::History(path));
            }
            ["C", cwd, input] => {
                let Some(cwd) = unhex(cwd)
                    .map(PathBuf::from)
                    .filter(|cwd| cwd.is_absolute())
                else {
                    return false;
                };
                let Some(input) = unhex(input).filter(|s| s.len() <= 4096 && !s.contains('\0'))
                else {
                    return false;
                };
                self.capture = Some(Capture {
                    input,
                    cwd,
                    output: TerminalBuffer::with_history(240, 24, 256),
                });
                events.push(Event::Started);
            }
            ["D", code] => {
                let Ok(exit_code) = code.parse::<i32>() else {
                    return false;
                };
                if let Some(capture) = self.capture.take() {
                    let text = capture.output.text();
                    let start = text
                        .char_indices()
                        .rev()
                        .nth(32767)
                        .map_or(0, |(index, _)| index);
                    events.push(Event::Completed(Command {
                        input: capture.input,
                        output: text[start..].trim_end().into(),
                        cwd: capture.cwd,
                        exit_code,
                        shell: self.shell,
                    }));
                }
            }
            ["P", cwd] => {
                let Some(cwd) = unhex(cwd)
                    .map(PathBuf::from)
                    .filter(|cwd| cwd.is_absolute())
                else {
                    return false;
                };
                self.capture = None;
                events.push(Event::Prompt(cwd));
            }
            _ => return false,
        }
        true
    }
}

fn unhex(value: &str) -> Option<String> {
    if value.len() > 32768 || !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<_>> = value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some((high * 16 + low) as u8)
        })
        .collect();
    String::from_utf8(bytes?).ok()
}
