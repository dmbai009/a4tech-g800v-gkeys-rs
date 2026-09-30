//! gkeys.exe — standalone G-keys for the A4Tech X7 G800V keyboard.
//!
//!     gkeys.exe               first run: install + start; next runs: uninstall / reinstall prompt
//!     gkeys.exe --run         the background remapper itself (what autostart launches)
//!     gkeys.exe --install     install / update without asking
//!     gkeys.exe --uninstall   stop, remove autostart, the Apps entry and the installed files
//!     --quiet                 no dialogs (for scripts)
//!     --console               with --run: print every G-key press to the terminal

#![windows_subsystem = "windows"]

mod install;
mod keys;
mod remap;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDNO, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNOCANCEL,
};

pub const APP_NAME: &str = "A4Tech G-keys";

static QUIET: OnceLock<bool> = OnceLock::new();
static LOG: OnceLock<Mutex<Option<Box<dyn Write + Send>>>> = OnceLock::new();

/// Null-terminated UTF-16 for Win32 calls
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn log(msg: &str) {
    let Some(lock) = LOG.get() else { return };
    if let Some(out) = lock.lock().unwrap().as_mut() {
        let _ = writeln!(out, "{msg}");
        let _ = out.flush();
    }
}

/// A windowed exe has no console: print to stdout if it was redirected to us,
/// otherwise attach to the terminal we were started from.
fn open_log() -> Option<Box<dyn Write + Send>> {
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !handle.is_null() && handle as isize != -1 {
        return Some(Box::new(std::io::stdout()));
    }
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    let console = OpenOptions::new().write(true).open("CONOUT$").ok()?;
    Some(Box::new(console))
}

fn quiet() -> bool {
    *QUIET.get().unwrap_or(&false)
}

pub fn message(text: &str, error: bool) {
    if quiet() {
        return;
    }
    let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
    let (text, title) = (wide(text), wide(APP_NAME));
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | icon) };
}

pub enum Answer {
    Yes,
    No,
    Cancel,
}

pub fn ask(text: &str) -> Answer {
    if quiet() {
        return Answer::Cancel;
    }
    let (text, title) = (wide(text), wide(APP_NAME));
    let flags = MB_YESNOCANCEL | MB_ICONQUESTION;
    match unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) } {
        IDYES => Answer::Yes,
        IDNO => Answer::No,
        _ => Answer::Cancel,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    QUIET.set(has("--quiet")).ok();
    LOG.set(Mutex::new(if has("--console") { open_log() } else { None })).ok();

    if has("--run") {
        remap::run();
        return;
    }
    let result = if has("--uninstall") {
        install::uninstall_with_message();
        Ok(())
    } else if has("--install") {
        install::install()
    } else {
        install::interactive()
    };
    // No console in the exe, so errors are surfaced in a dialog
    if let Err(e) = result {
        message(&format!("Error: {e}"), true);
        std::process::exit(1);
    }
}
