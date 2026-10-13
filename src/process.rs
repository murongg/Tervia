use crate::agent::Agent;
use std::collections::{BTreeSet, VecDeque};

pub fn descendants(root: u32, rows: &[(u32, u32)]) -> Vec<u32> {
    let mut queue = VecDeque::from([root]);
    let mut visited = BTreeSet::new();
    let mut result = Vec::new();
    while let Some(pid) = queue.pop_front() {
        if !visited.insert(pid) {
            continue;
        }
        result.push(pid);
        queue.extend(
            rows.iter()
                .filter(|(_, parent)| *parent == pid)
                .map(|(child, _)| *child),
        );
    }
    result
}

pub fn linux_group(stat: &str) -> Option<u32> {
    stat.get(stat.rfind(')')? + 1..)?
        .split_whitespace()
        .nth(2)?
        .parse()
        .ok()
}

pub struct WindowsArgs<'a> {
    rest: &'a [u16],
}
impl<'a> WindowsArgs<'a> {
    pub fn new(data: &'a [u16]) -> Self {
        Self { rest: data }
    }
}
impl Iterator for WindowsArgs<'_> {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        while self.rest.first().is_some_and(|c| matches!(*c, 9 | 32)) {
            self.rest = &self.rest[1..]
        }
        if self.rest.is_empty() {
            return None;
        }
        let mut result = Vec::new();
        let mut quoted = false;
        let mut index = 0;
        while index < self.rest.len() {
            let character = self.rest[index];
            if !quoted && matches!(character, 9 | 32) {
                break;
            }
            if character == 92 {
                let start = index;
                while self.rest.get(index) == Some(&92) {
                    index += 1
                }
                let count = index - start;
                if self.rest.get(index) == Some(&34) {
                    result.extend(std::iter::repeat_n(92, count / 2));
                    if count % 2 == 1 {
                        result.push(34)
                    } else {
                        quoted = !quoted
                    }
                    index += 1;
                } else {
                    result.extend(std::iter::repeat_n(92, count))
                }
                continue;
            }
            if character == 34 {
                if quoted && self.rest.get(index + 1) == Some(&34) {
                    result.push(34);
                    index += 1
                } else {
                    quoted = !quoted
                }
            } else {
                result.push(character)
            }
            index += 1;
        }
        self.rest = &self.rest[index..];
        Some(String::from_utf16_lossy(&result))
    }
}

pub fn foreground(root: u32, group: Option<u32>) -> Option<Agent> {
    #[cfg(target_os = "macos")]
    {
        let _ = root;
        mac::foreground(group?)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = root;
        linux::foreground(group?)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = group;
        windows::foreground(root)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (root, group);
        None
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    pub fn foreground(group: u32) -> Option<Agent> {
        const PROC_PGRP_ONLY: u32 = 2;
        let mut pids = [0i32; 256];
        let bytes = unsafe {
            libc::proc_listpids(
                PROC_PGRP_ONLY,
                group,
                pids.as_mut_ptr().cast(),
                std::mem::size_of_val(&pids) as i32,
            )
        };
        let count = (bytes.max(0) as usize / 4).min(pids.len());
        std::iter::once(group as i32)
            .chain(pids[..count].iter().copied())
            .filter(|pid| *pid > 1)
            .find_map(agent_for_pid)
    }
    fn agent_for_pid(pid: i32) -> Option<Agent> {
        let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        if unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) } <= 0 {
            return None;
        }
        let executable = std::ffi::CStr::from_bytes_until_nul(&path)
            .ok()?
            .to_string_lossy();
        if let Some(agent) = Agent::from_executable(&executable) {
            return Some(agent);
        }
        if !Agent::is_launcher(&executable) {
            return None;
        }
        let mut query = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut length = 0usize;
        if unsafe {
            libc::sysctl(
                query.as_mut_ptr(),
                3,
                std::ptr::null_mut(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        let mut bytes = vec![0u8; length.min(1024 * 1024)];
        length = bytes.len();
        if unsafe {
            libc::sysctl(
                query.as_mut_ptr(),
                3,
                bytes.as_mut_ptr().cast(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        bytes.truncate(length);
        let count = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        if count <= 0 {
            return None;
        }
        let data = bytes.get(4..)?;
        let executable_end = data.iter().position(|byte| *byte == 0)?;
        let mut data = &data[executable_end + 1..];
        while data.first() == Some(&0) {
            data = &data[1..]
        }
        // Decode only the launcher prefix; the classifier stops before prompts and environment.
        Agent::from_argv(
            &executable,
            data.split(|byte| *byte == 0)
                .take(count as usize)
                .map(String::from_utf8_lossy),
        )
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{fs, path::PathBuf};
    pub fn foreground(group: u32) -> Option<Agent> {
        let entries = fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
                .is_none()
            {
                continue;
            }
            let path = entry.path();
            let Some(stat) = fs::read_to_string(path.join("stat")).ok() else {
                continue;
            };
            if linux_group(&stat) != Some(group) {
                continue;
            }
            if let Some(agent) = agent_for_path(path) {
                return Some(agent);
            }
        }
        None
    }
    fn agent_for_path(path: PathBuf) -> Option<Agent> {
        let executable = fs::read_link(path.join("exe"))
            .ok()?
            .to_string_lossy()
            .trim_end_matches(" (deleted)")
            .to_owned();
        if let Some(agent) = Agent::from_executable(&executable) {
            return Some(agent);
        }
        if !Agent::is_launcher(&executable) {
            return None;
        }
        let data = fs::read(path.join("cmdline")).ok()?;
        Agent::from_argv(
            &executable,
            data.split(|byte| *byte == 0).map(String::from_utf8_lossy),
        )
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::{ffi::c_void, mem, sync::OnceLock};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
    };
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub fn foreground(root: u32) -> Option<Agent> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let snapshot = Handle(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut rows = Vec::new();
        let mut valid = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while valid {
            rows.push((entry.th32ProcessID, entry.th32ParentProcessID));
            valid = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        // ConPTY has no Unix foreground-group API. Restrict inspection to this
        // shell's descendants; never search unrelated terminal sessions.
        descendants(root, &rows).into_iter().find_map(agent_for_pid)
    }
    type Query = unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum: u16,
        buffer: *const u16,
    }
    fn query() -> Option<Query> {
        static QUERY: OnceLock<Option<Query>> = OnceLock::new();
        *QUERY.get_or_init(|| unsafe {
            let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
            let module = GetModuleHandleW(name.as_ptr());
            if module.is_null() {
                return None;
            }
            let address = GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast())?;
            Some(mem::transmute::<unsafe extern "system" fn() -> isize, Query>(address))
        })
    }
    fn agent_for_pid(pid: u32) -> Option<Agent> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let handle = Handle(handle);
        let mut path = [0u16; 32768];
        let mut length = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle.0, 0, path.as_mut_ptr(), &mut length) } == 0 {
            return None;
        }
        let executable = String::from_utf16_lossy(&path[..length as usize]);
        if let Some(agent) = Agent::from_executable(&executable) {
            return Some(agent);
        }
        if !Agent::is_launcher(&executable) {
            return None;
        }
        let query = query()?;
        let mut needed = 0u32;
        unsafe {
            query(handle.0, 60, std::ptr::null_mut(), 0, &mut needed);
        }
        if needed < mem::size_of::<UnicodeString>() as u32 || needed > 1024 * 1024 {
            return None;
        }
        let mut bytes = vec![0usize; (needed as usize).div_ceil(mem::size_of::<usize>())];
        if unsafe { query(handle.0, 60, bytes.as_mut_ptr().cast(), needed, &mut needed) } < 0 {
            return None;
        }
        let value = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<UnicodeString>()) };
        let start = value.buffer as usize;
        let base = bytes.as_ptr() as usize;
        let end = base.checked_add(mem::size_of_val(bytes.as_slice()))?;
        if start < base
            || start.checked_add(value.length as usize)? > end
            || value.length % 2 != 0
            || !start.is_multiple_of(mem::align_of::<u16>())
        {
            return None;
        }
        let data = unsafe { std::slice::from_raw_parts(value.buffer, value.length as usize / 2) };
        Agent::from_argv(&executable, WindowsArgs::new(data))
    }
}
