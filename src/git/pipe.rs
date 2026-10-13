use std::io::{self, Read};

pub(super) trait OutputPipe: Read {
    fn prepare(&self) -> io::Result<()>;
    fn read_ready(&mut self, bytes: &mut [u8]) -> io::Result<usize>;
}

#[cfg(unix)]
impl<T: Read + std::os::fd::AsRawFd> OutputPipe for T {
    fn prepare(&self) -> io::Result<()> {
        let fd = self.as_raw_fd();
        // These are exclusively owned child-output descriptors. Nonblocking
        // reads let Git exit independently of a hook retaining a write handle.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn read_ready(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.read(bytes)
    }
}

#[cfg(windows)]
impl<T: Read + std::os::windows::io::AsRawHandle> OutputPipe for T {
    fn prepare(&self) -> io::Result<()> {
        Ok(())
    }
    fn read_ready(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut available = 0;
        let success = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if success == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error()
                == Some(windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE as i32)
            {
                return Ok(0);
            }
            return Err(error);
        }
        if available == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(available as usize);
        self.read(&mut bytes[..count])
    }
}

#[derive(Default)]
pub(super) struct Capture {
    bytes: Vec<u8>,
    eof: bool,
    oversized: bool,
}
impl Capture {
    pub fn drain(&mut self, pipe: &mut impl OutputPipe) -> io::Result<()> {
        if self.eof {
            return Ok(());
        }
        let mut bytes = [0; 8192];
        // A noisy hook must not monopolize this loop and bypass the deadline.
        for _ in 0..64 {
            match pipe.read_ready(&mut bytes) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(count) => {
                    if self.bytes.len() + count <= 8 * 1024 * 1024 {
                        self.bytes.extend_from_slice(&bytes[..count]);
                    } else {
                        self.oversized = true;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.oversized {
            Err("Git output exceeds 8 MiB; narrow the comparison or history.".into())
        } else {
            Ok(self.bytes)
        }
    }
}
