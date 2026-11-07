//! Which program runs in a local tab, on Windows. ConPTY has no
//! foreground process group to ask (Unix: `tcgetpgrp`); what a shell runs
//! are its child processes, so the newest of those is the program -- as
//! the job a Unix shell put in the foreground would be.

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

/// Console hosts and WSL's helper aren't programs anyone ran.
const HOSTS: &[&str] = &["conhost", "openconsole", "wslhost"];

/// A process as the snapshot lists it.
#[derive(Clone, Debug)]
pub(super) struct Process {
    pub pid: u32,
    pub parent: u32,
    /// The exe's name without `.exe`.
    pub name: String,
}

/// The program the shell `shell_pid` runs right now, by name (`nvim`,
/// `python`); `None` while it runs nothing.
pub fn running_program(shell_pid: u32) -> Option<String> {
    pick(&snapshot()?, shell_pid, created)
}

/// The newest child of `shell` that isn't a console host. Children from
/// before their parent can't be its own: theirs was an earlier process
/// with the same (reused) ID. A child named like the shell is the shell
/// itself behind a launcher (Git's `bin\bash.exe` starts
/// `usr\bin\bash.exe`), so its children count instead. `created` says
/// when a process started (100 ns ticks; `None` for one Windows won't
/// tell about) -- asked only for the few that matter.
pub(super) fn pick(processes: &[Process], shell: u32, created: impl Fn(u32) -> Option<u64>) -> Option<String> {
    let name = &processes.iter().find(|process| process.pid == shell)?.name;
    let (mut parent, mut since) = (shell, created(shell)?);
    loop {
        let (started, child) = processes
            .iter()
            .filter(|process| process.parent == parent && process.pid != parent)
            .filter(|process| !HOSTS.iter().any(|host| process.name.eq_ignore_ascii_case(host)))
            .filter_map(|process| Some((created(process.pid).filter(|&started| started >= since)?, process)))
            .max_by_key(|(started, _)| *started)?;
        if !child.name.eq_ignore_ascii_case(name) {
            return Some(child.name.clone());
        }
        (parent, since) = (child.pid, started);
    }
}

/// Every process, with its parent and name.
fn snapshot() -> Option<Vec<Process>> {
    // SAFETY: a snapshot handle, closed below; the entry is filled by the calls.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut processes = Vec::new();
    let mut more = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
        let name = match exe.len().checked_sub(4) {
            Some(cut) if exe.get(cut..).is_some_and(|ext| ext.eq_ignore_ascii_case(".exe")) => exe[..cut].to_string(),
            _ => exe,
        };
        processes.push(Process { pid: entry.th32ProcessID, parent: entry.th32ParentProcessID, name });
        more = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snap) };
    Some(processes)
}

/// When process `pid` started, in 100 ns ticks.
fn created(pid: u32) -> Option<u64> {
    // SAFETY: the handle is checked and closed; the times are plain structs.
    let process: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
    let [creation, exit, kernel, user] = &mut times;
    let ok = unsafe { GetProcessTimes(process, creation, exit, kernel, user) } != 0;
    unsafe { CloseHandle(process) };
    ok.then(|| u64::from(times[0].dwHighDateTime) << 32 | u64::from(times[0].dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The processes, and `pick` over them with the given start times.
    fn run(table: &[(u32, u32, &str, Option<u64>)], shell: u32) -> Option<String> {
        let processes: Vec<_> =
            table.iter().map(|&(pid, parent, name, _)| Process { pid, parent, name: name.to_string() }).collect();
        pick(&processes, shell, |pid| table.iter().find(|row| row.0 == pid)?.3)
    }

    #[test]
    fn the_newest_child_is_the_program() {
        let table = [
            (10, 1, "pwsh", Some(100)),
            (11, 10, "conhost", Some(300)),
            (12, 10, "git", Some(150)),
            (13, 10, "less", Some(200)),
            (14, 13, "grandchild", Some(400)),
            (15, 99, "other", Some(500)),
        ];
        assert_eq!(run(&table, 10).as_deref(), Some("less"));
        // Console hosts and grandchildren don't count; nothing left = shell idle.
        assert_eq!(run(&table[..2], 10), None);
    }

    #[test]
    fn children_from_before_the_shell_belong_to_a_reused_id() {
        let table = [(10, 1, "cmd", Some(100)), (20, 10, "stale", Some(50)), (21, 10, "unknown", None)];
        assert_eq!(run(&table, 10), None);
    }

    #[test]
    fn a_launcher_counts_as_the_shell() {
        // Git Bash: bin\bash.exe starts usr\bin\bash.exe, which runs vim.
        let table = [(10, 1, "bash", Some(100)), (11, 10, "bash", Some(110)), (12, 11, "vim", Some(200))];
        assert_eq!(run(&table, 10).as_deref(), Some("vim"));
        assert_eq!(run(&table[..2], 10), None, "the real shell, idle");
    }

    /// For real: this test process is the "shell", a `cmd` it starts the
    /// program.
    #[test]
    fn a_child_process_is_found() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "pause"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let name = running_program(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(name.as_deref(), Some("cmd"));
    }
}
