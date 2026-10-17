//! Ordered, streaming replay retains the actual parser state, including split escape sequences.
use super::*;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
};

pub(super) struct Journal {
    file: File,
    pub length: u64,
}

impl Journal {
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Self {
            file: options.open(path)?,
            length: 0,
        })
    }

    pub fn append(&mut self, kind: u8, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_PACKET as usize {
            return Err(io::Error::other("Oversized terminal record"));
        }
        self.file.write_all(&[kind])?;
        self.file.write_all(&(bytes.len() as u32).to_le_bytes())?;
        self.file.write_all(bytes)?;
        self.length += 5 + bytes.len() as u64;
        Ok(())
    }

    pub fn resize(&mut self, size: PtySize) -> io::Result<()> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.cols.to_le_bytes());
        bytes.extend_from_slice(&size.rows.to_le_bytes());
        self.append(1, &bytes)
    }
}

pub(super) fn read(file: &mut File, offset: &mut u64) -> io::Result<Option<PtyEvent>> {
    // A writer may be between header and payload; retry from the record boundary without parsing a partial write.
    let length = file.metadata()?.len();
    if length < *offset + 5 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(*offset))?;
    let mut header = [0u8; 5];
    file.read_exact(&mut header)?;
    let count = u32::from_le_bytes(header[1..].try_into().unwrap()) as u64;
    if count > MAX_PACKET {
        return Err(io::Error::other("Oversized terminal replay record"));
    }
    if length < *offset + 5 + count {
        return Ok(None);
    }
    let mut bytes = vec![0; count as usize];
    file.read_exact(&mut bytes)?;
    *offset += 5 + count;
    Ok(Some(match header[0] {
        0 => PtyEvent::Data(bytes),
        1 if bytes.len() == 4 => PtyEvent::Resized {
            columns: u16::from_le_bytes(bytes[..2].try_into().unwrap()) as usize,
            rows: u16::from_le_bytes(bytes[2..].try_into().unwrap()) as usize,
        },
        2 => PtyEvent::Exited(String::from_utf8_lossy(&bytes).into_owned()),
        3 => PtyEvent::Error(String::from_utf8_lossy(&bytes).into_owned()),
        _ => return Err(io::Error::other("Invalid terminal replay record")),
    }))
}
