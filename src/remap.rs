//! The background remapper: reads the keyboard's vendor HID report and sends the mapped keys.
//!
//! One instance per user session (named mutex). Another process — the installer — can ask it
//! to exit through a named event. The names are the same as in the Python version, so either
//! version can stop the other when updating.

use std::ptr::null;
use std::thread::sleep;
use std::time::{Duration, Instant};

use hidapi::HidApi;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, OpenMutexW, ResetEvent, SetEvent, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};

use crate::keys::{parse_report, KeyEvent, Remapper};
use crate::log;
use crate::wide;

const VID: u16 = 0x09DA; // A4Tech; the G800V tested here is 09DA:90C0
const VENDOR_PAGE: u16 = 0xFFA0;
const VENDOR_USAGE: u16 = 0xA5;

const MUTEX_NAME: &str = "Local\\A4TechGKeys";
const STOP_EVENT_NAME: &str = "Local\\A4TechGKeysStop";
const SYNCHRONIZE: u32 = 0x0010_0000;
const EVENT_MODIFY_STATE: u32 = 0x0002;

fn send(events: &[KeyEvent]) {
    if events.is_empty() {
        return;
    }
    let inputs: Vec<INPUT> = events
        .iter()
        .map(|e| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: e.key.vk,
                    wScan: e.key.scan,
                    dwFlags: if e.up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        })
        .collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

fn apply(remapper: &mut Remapper, state: Option<u16>) {
    let mut events = Vec::new();
    let changes = match state {
        Some(state) => remapper.update(state, &mut events),
        None => remapper.release_all(&mut events),
    };
    for c in changes {
        let keys = crate::keys::mapping(c.g)
            .map(|keys| keys.iter().map(|k| k.name).collect::<Vec<_>>().join("+"))
            .unwrap_or_else(|| "(unmapped)".into());
        log(&format!("G{} {} -> {}", c.g, if c.pressed { "down" } else { "up" }, keys));
    }
    send(&events);
}

struct StopEvent(HANDLE);

impl StopEvent {
    fn requested(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }

    /// Sleeps, but wakes up at once when a stop is requested
    fn wait(&self, ms: u32) {
        unsafe { WaitForSingleObject(self.0, ms) };
    }
}

/// Runs until another process signals the stop event. Returns immediately if already running.
pub fn run() {
    let mutex_name = wide(MUTEX_NAME);
    unsafe {
        // The handle stays open for the whole process lifetime: that is what marks us as running
        CreateMutexW(null(), 0, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log("already running");
            return;
        }
    }
    let event_name = wide(STOP_EVENT_NAME);
    let stop = StopEvent(unsafe { CreateEventW(null(), 1, 0, event_name.as_ptr()) });
    unsafe { ResetEvent(stop.0) };

    let mut api = loop {
        match HidApi::new() {
            Ok(api) => break api,
            Err(e) => {
                log(&format!("HID is unavailable ({e}), retrying..."));
                stop.wait(3000);
                if stop.requested() {
                    return;
                }
            }
        }
    };

    let mut remapper = Remapper::default();
    let mut buf = [0u8; 64];
    while !stop.requested() {
        // Opening can fail while the keyboard is being re-plugged: never give up, just retry
        let device = api.refresh_devices().ok().and_then(|_| {
            api.device_list()
                .find(|d| d.vendor_id() == VID && d.usage_page() == VENDOR_PAGE && d.usage() == VENDOR_USAGE)
                .and_then(|d| api.open_path(d.path()).ok())
        });
        let Some(device) = device else {
            log("keyboard not found, waiting...");
            stop.wait(1000);
            continue;
        };
        log("listening for G-keys");
        while !stop.requested() {
            match device.read_timeout(&mut buf, 500) {
                Ok(0) => {}
                Ok(n) => {
                    if let Some(state) = parse_report(&buf[..n]) {
                        apply(&mut remapper, Some(state));
                    }
                }
                Err(e) => {
                    log(&format!("keyboard disconnected ({e}), reconnecting..."));
                    break;
                }
            }
        }
        // Release everything still held so nothing gets stuck
        apply(&mut remapper, None);
        if !stop.requested() {
            stop.wait(1000);
        }
    }
    log("stopped");
}

pub fn is_running() -> bool {
    let name = wide(MUTEX_NAME);
    unsafe {
        let m = OpenMutexW(SYNCHRONIZE, 0, name.as_ptr());
        if m.is_null() {
            return false;
        }
        CloseHandle(m);
        true
    }
}

/// Asks a running instance to exit and waits for it. Returns true if none is left.
pub fn stop_running_instance(timeout: Duration) -> bool {
    let name = wide(STOP_EVENT_NAME);
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
    if !event.is_null() {
        unsafe { SetEvent(event) };
    }
    let deadline = Instant::now() + timeout;
    while is_running() && Instant::now() < deadline {
        sleep(Duration::from_millis(100));
    }
    if !event.is_null() {
        unsafe { CloseHandle(event) };
    }
    !is_running()
}
