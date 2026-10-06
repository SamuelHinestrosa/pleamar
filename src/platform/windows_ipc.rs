//! Local command pipes. A scene owns its name until its last handle closes.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, ERROR_PIPE_CONNECTED, ERROR_NO_DATA};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::*;
use windows::core::PCWSTR;

#[path = "windows_ipc_security.rs"]
mod security;

const LIMIT: usize = 65536;
const DEFAULT_WAIT: Duration = Duration::from_secs(2);

fn prefix() -> Result<String, String> {
    // A second logon of the same account must have its own scene names.
    let identity = format!("{}|{}", super::config_dir().display(), std::env::var("PLEAMAR_SOCKET_DIR").unwrap_or_default());
    let hash = identity.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let hash = security::Logon::current()?.hash(hash);
    Ok(format!("pleamar-{hash:016x}-"))
}

fn pipe_path(scene: &str) -> Result<String, String> {
    if scene.is_empty() || scene.len() > 120 || scene.chars().any(|c| c.is_control() || "\\/:".contains(c)) {
        return Err("invalid scene name for a command pipe".into());
    }
    Ok(format!(r"\\.\pipe\{}{scene}", prefix()?))
}

pub(super) fn read_line(file: &mut File, until: Instant) -> Result<String, String> {
    let mut data = Vec::new();
    loop {
        let mut available = 0;
        unsafe { PeekNamedPipe(HANDLE(file.as_raw_handle()), None, 0, None, Some(&mut available), None) }.map_err(|e| e.to_string())?;
        if available > 0 {
            let mut buf = [0u8; 4096];
            let n = file.read(&mut buf[..(available as usize).min(4096)]).map_err(|e| e.to_string())?;
            data.extend_from_slice(&buf[..n]);
            if data.len() > LIMIT { return Err("command is too long".into()); }
            if let Some(end) = data.iter().position(|b| *b == b'\n') {
                return String::from_utf8(data[..end].to_vec()).map_err(|e| e.to_string());
            }
        }
        if Instant::now() >= until { return Err("command pipe timed out".into()); }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn bind(scene: &str) -> Result<File, String> {
    let path = pipe_path(scene)?;
    bind_path(&path)
}

pub(super) fn bind_path(path: &str) -> Result<File, String> {
    let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let mut security = security::Security::new()?;
    let attributes = security.attributes();
    let handle = unsafe {
        CreateNamedPipeW(PCWSTR(wide.as_ptr()), PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1, LIMIT as u32, LIMIT as u32, 2000, Some(&attributes))
    };
    if handle.is_invalid() { return Err(format!("{path}: {}", std::io::Error::last_os_error())); }
    Ok(unsafe { File::from_raw_handle(handle.0) })
}

pub(super) fn accept_ready(pipe: &File) -> bool {
    let handle = HANDLE(pipe.as_raw_handle());
    let status = unsafe { ConnectNamedPipe(handle, None) };
    // With PIPE_NOWAIT, only ERROR_PIPE_CONNECTED means a client is ready.
    // Success and ERROR_PIPE_LISTENING merely advertise the listening endpoint.
    if status.as_ref().err().is_some_and(|e| e.code() == ERROR_PIPE_CONNECTED.to_hresult()) { return true; }
    if status.as_ref().err().is_some_and(|e| e.code() == ERROR_NO_DATA.to_hresult()) {
        // A client can time out before we accept it. Without resetting this
        // state, every future client gets ERROR_PIPE_BUSY indefinitely.
        unsafe { let _ = DisconnectNamedPipe(handle); }
    }
    false
}

fn wait_for_client(pipe: &File) -> Result<(), String> {
    let handle = HANDLE(pipe.as_raw_handle());
    // Only accepting is unbounded. Reads and writes keep their nonblocking
    // handle mode and the existing exchange deadline after a client arrives.
    unsafe { SetNamedPipeHandleState(handle, Some(&PIPE_WAIT), None, None) }.map_err(|e| e.to_string())?;
    loop {
        match unsafe { ConnectNamedPipe(handle, None) } {
            Ok(()) => break,
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => break,
            Err(e) if e.code() == ERROR_NO_DATA.to_hresult() => {
                unsafe { DisconnectNamedPipe(handle) }.map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    unsafe { SetNamedPipeHandleState(handle, Some(&PIPE_NOWAIT), None, None) }.map_err(|e| e.to_string())
}

fn serve(mut pipe: File, receive: Box<dyn Fn(String) -> Option<String> + Send>) -> Result<(), String> {
    loop {
        wait_for_client(&pipe)?;
        let handle = HANDLE(pipe.as_raw_handle());
        let mut quitting = false;
        if let Ok(line) = read_line(&mut pipe, Instant::now() + DEFAULT_WAIT) {
            // A quit must be acknowledged before the UI thread exits.
            quitting = line.split_whitespace().next() == Some("quit");
            let answer = if quitting { String::new() } else { receive(line.clone()).unwrap_or_default() };
            let answer = serde_json::to_string(&answer).unwrap();
            if answer.len() < LIMIT { let _ = writeln!(pipe, "{answer}"); }
            // Its acknowledgement prevents DisconnectNamedPipe discarding the reply.
            let _ = read_line(&mut pipe, Instant::now() + DEFAULT_WAIT);
            if quitting { receive(line); }
        }
        unsafe { DisconnectNamedPipe(handle) }.map_err(|e| e.to_string())?;
        if quitting { return Ok(()); }
    }
}

pub fn listen_for_commands(scene: &str, receive: Box<dyn Fn(String) -> Option<String> + Send>) {
    let pipe = match bind(scene) {
        Ok(p) => p,
        Err(e) => { eprintln!("orders · {e}"); return; }
    };
    std::thread::spawn(move || {
        if let Err(e) = serve(pipe, receive) { eprintln!("orders · {e}"); }
    });
}

pub fn ask(scene: &str, command: &str, wait: Duration) -> Result<String, String> {
    ask_path(&pipe_path(scene)?, command, wait)
}

pub(super) fn ask_path(path: &str, command: &str, wait: Duration) -> Result<String, String> {
    if command.len() >= LIMIT || command.contains(['\n', '\r']) { return Err("send one command line at a time (less than 64 KiB)".into()); }
    let until = Instant::now() + wait;
    let mut pipe = loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => break file,
            Err(e) if Instant::now() >= until => return Err(format!("{path}: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    writeln!(pipe, "{command}").map_err(|e| e.to_string())?;
    let reply = read_line(&mut pipe, until)?;
    let _ = writeln!(pipe, "ack");
    serde_json::from_str(&reply).map_err(|e| e.to_string())
}

pub fn running_scenes() -> Vec<String> {
    let prefix = match prefix() {
        Ok(prefix) => prefix,
        Err(e) => { eprintln!("orders · {e}"); return Vec::new(); }
    };
    let Ok(entries) = std::fs::read_dir(r"\\.\pipe\") else { return Vec::new() };
    let mut names: Vec<String> = entries.filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_string_lossy().strip_prefix(&prefix).map(str::to_owned)).collect();
    names.sort();
    names.dedup();
    names
}

pub fn send(scene: Option<&str>, command: &str) -> Result<(), String> {
    let name = match scene {
        Some(name) => name.to_owned(),
        None => {
            let names = running_scenes();
            match names.as_slice() {
                [name] => name.clone(),
                [] => return Err("there is no scene running".into()),
                _ => return Err(format!("there are several scenes running; say which: {}", names.join(", "))),
            }
        }
    };
    let answer = ask(&name, command, DEFAULT_WAIT)?;
    if !answer.is_empty() { println!("{answer}"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_pipe_roundtrip_and_duplicate_owner() {
        let name = format!("pipe test ñ {}", std::process::id());
        listen_for_commands(&name, Box::new(|line| Some(if line == "multiline" { "first\nsecond 🚀".into() } else { format!("reply {line}") })));
        assert!(bind(&name).is_err(), "a second scene must not steal the first pipe");
        assert!(running_scenes().contains(&name));
        assert_eq!(ask(&name, "text greeting héllo 世界", DEFAULT_WAIT).unwrap(), "reply text greeting héllo 世界");
        assert_eq!(ask(&name, "get greeting", DEFAULT_WAIT).unwrap(), "reply get greeting");
        assert_eq!(ask(&name, "multiline", DEFAULT_WAIT).unwrap(), "first\nsecond 🚀");
    }
    #[test]
    fn caller_timeout_bounds_connection_and_reply() {
        let name = format!("slow pipe {}", std::process::id());
        listen_for_commands(&name, Box::new(|_| { std::thread::sleep(Duration::from_millis(150)); Some("late".into()) }));
        let start = Instant::now();
        assert!(ask(&name, "probe report", Duration::from_millis(25)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(ask(&format!("missing {}", std::process::id()), "probe", Duration::from_millis(25)).is_err());
    }
    #[test]
    fn abandoned_connection_cannot_block_the_next_command() {
        let name = format!("abandoned pipe {}", std::process::id());
        let path = pipe_path(&name).unwrap();
        let mut pipe = bind(&name).unwrap();
        assert!(!accept_ready(&pipe));
        let abandoned = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        drop(abandoned);
        let stale = unsafe { ConnectNamedPipe(HANDLE(pipe.as_raw_handle()), None) }.unwrap_err();
        assert_eq!(stale.code(), ERROR_NO_DATA.to_hresult());
        assert!(!accept_ready(&pipe));
        let client = std::thread::spawn(move || ask_path(&path, "after abandoned client", Duration::from_secs(5)));
        let until = Instant::now() + Duration::from_secs(5);
        while !accept_ready(&pipe) {
            assert!(Instant::now() < until, "stale connection still owns the command endpoint");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(read_line(&mut pipe, until).unwrap(), "after abandoned client");
        writeln!(pipe, "\"recovered\"").unwrap();
        let _ = read_line(&mut pipe, until);
        assert_eq!(client.join().unwrap().unwrap(), "recovered");
    }
    #[test]
    fn commands_resume_after_idle_and_quit_releases_endpoint() {
        #[link(name="kernel32")]
        unsafe extern "system" { fn QueryThreadCycleTime(thread: HANDLE, cycles: *mut u64) -> i32; }
        let name = format!("idle commands {}", std::process::id());
        let path = pipe_path(&name).unwrap();
        let pipe = bind(&name).unwrap();
        // A stale connection must also recover when switching to blocking accept.
        assert!(!accept_ready(&pipe));
        drop(OpenOptions::new().read(true).write(true).open(&path).unwrap());
        let (quit, observed) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || serve(pipe, Box::new(move |line| {
            if line == "quit" { quit.send(()).unwrap(); }
            Some(format!("reply {line}"))
        })));
        for cycle in 0..2 {
            std::thread::sleep(Duration::from_millis(100));
            let cycles = || {
                let mut count = 0;
                assert_ne!(unsafe { QueryThreadCycleTime(HANDLE(thread.as_raw_handle()), &mut count) }, 0);
                count
            };
            let before = cycles();
            std::thread::sleep(Duration::from_millis(350));
            println!("idle command cycle {cycle}: {} CPU cycles during 350 ms", cycles() - before);
            assert_eq!(ask(&name, "hello 世界", DEFAULT_WAIT).unwrap(), "reply hello 世界");
            // An accepted client that sends no request must not hold the endpoint.
            let until = Instant::now() + DEFAULT_WAIT;
            let abandoned = loop {
                match OpenOptions::new().read(true).write(true).open(&path) {
                    Ok(pipe) => break pipe,
                    Err(e) if Instant::now() >= until => panic!("could not connect idle client: {e}"),
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            };
            drop(abandoned);
            assert_eq!(ask(&name, "after disconnect", DEFAULT_WAIT).unwrap(), "reply after disconnect");
        }
        assert_eq!(ask(&name, "quit", DEFAULT_WAIT).unwrap(), "");
        observed.recv_timeout(DEFAULT_WAIT).unwrap();
        let until = Instant::now() + DEFAULT_WAIT;
        while !thread.is_finished() {
            assert!(Instant::now() < until, "command listener did not stop after quit");
            std::thread::sleep(Duration::from_millis(5));
        }
        thread.join().unwrap().unwrap();
        assert!(bind(&name).is_ok(), "quit must release exclusive ownership of the pipe");
    }
    #[test]
    fn rejects_pipe_path_injection() {
        for name in ["", "../scene", "x\\y", "C:scene", "line\n"] { assert!(pipe_path(name).is_err()); }
        assert!(pipe_path("scene ñ").is_ok());
    }
}
