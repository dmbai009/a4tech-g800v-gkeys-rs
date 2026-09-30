//! Per-user install and uninstall: %LOCALAPPDATA%, HKCU Run and the Settings → Apps entry.
//! No admin rights are needed.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs, io};

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use crate::remap::stop_running_instance;
use crate::{message, APP_NAME};

// Kept identical to the Python version, so this exe updates and uninstalls an older install too
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\A4TechGKeys";
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn install_dir() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    base.join("Programs").join(APP_NAME)
}

pub fn installed_exe() -> PathBuf {
    install_dir().join("gkeys.exe")
}

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

pub fn is_installed() -> bool {
    hkcu().open_subkey(UNINSTALL_KEY).is_ok()
}

fn same_file(a: &PathBuf, b: &PathBuf) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn start_background() -> io::Result<()> {
    // The working directory is the install folder: a background process keeps its working
    // directory busy, so inheriting ours (e.g. Downloads) would lock that folder until reboot
    Command::new(installed_exe())
        .arg("--run")
        .current_dir(install_dir())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

pub fn install() -> io::Result<()> {
    stop_running_instance(Duration::from_secs(5));
    fs::create_dir_all(install_dir())?;
    let me = env::current_exe()?;
    let target = installed_exe();
    if !same_file(&me, &target) {
        // The stopped instance may keep the old exe locked for a moment
        let mut attempt = 0;
        loop {
            match fs::copy(&me, &target) {
                Ok(_) => break,
                Err(_) if attempt < 20 => {
                    attempt += 1;
                    sleep(Duration::from_millis(250));
                }
                Err(e) => return Err(e),
            }
        }
    }

    let exe = target.display().to_string();
    let (run, _) = hkcu().create_subkey(RUN_KEY)?;
    run.set_value(APP_NAME, &format!("\"{exe}\" --run"))?;

    let (apps, _) = hkcu().create_subkey(UNINSTALL_KEY)?;
    let size_kb = (fs::metadata(&target)?.len() / 1024) as u32;
    apps.set_value("DisplayName", &APP_NAME)?;
    apps.set_value("DisplayIcon", &exe)?;
    apps.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    apps.set_value("Publisher", &"dmbai009")?;
    apps.set_value("InstallLocation", &install_dir().display().to_string())?;
    apps.set_value("UninstallString", &format!("\"{exe}\" --uninstall"))?;
    apps.set_value("NoModify", &1u32)?;
    apps.set_value("NoRepair", &1u32)?;
    apps.set_value("EstimatedSize", &size_kb)?;

    start_background()
}

/// Returns true if the background process stopped
pub fn uninstall() -> bool {
    let stopped = stop_running_instance(Duration::from_secs(5));
    if let Ok(run) = hkcu().open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE) {
        let _ = run.delete_value(APP_NAME);
    }
    let _ = hkcu().delete_subkey(UNINSTALL_KEY);

    // The running exe can't delete itself: a hidden cmd retries for ~15 s after we exit.
    // CREATE_NO_WINDOW gives it a hidden console (with DETACHED_PROCESS cmd fails outright);
    // it runs from %TEMP%, since a process can't remove its own working directory.
    let dir = install_dir();
    if dir.is_dir() {
        let d = dir.display();
        let _ = Command::new("cmd")
            .raw_arg(format!(
                "/c for /l %i in (1,1,15) do (ping -n 2 127.0.0.1 >nul & rmdir /s /q \"{d}\" 2>nul & if not exist \"{d}\" exit)"
            ))
            .current_dir(env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    stopped
}

pub fn uninstall_with_message() {
    if uninstall() {
        message(&format!("{APP_NAME} is uninstalled."), false);
    } else {
        message(
            &format!(
                "{APP_NAME} is uninstalled, but the running instance didn't stop.\n\
                 End gkeys.exe in Task Manager or sign out and back in."
            ),
            true,
        );
    }
}

pub fn interactive() -> io::Result<()> {
    if !is_installed() {
        install()?;
        message(
            &format!(
                "{APP_NAME} is installed and running.\n\n\
                 G1-G7 → F13-F19, G9-G13 → F20-F24, G14-G16 → Kana / Convert / NonConvert.\n\
                 Remove any bindings from the G-keys in the A4Tech app.\n\n\
                 It starts automatically with Windows. To remove it, run this file again \
                 or uninstall \"{APP_NAME}\" in Settings → Apps."
            ),
            false,
        );
        return Ok(());
    }
    match crate::ask(&format!(
        "{APP_NAME} is already installed.\n\n\
         Yes — uninstall\nNo — reinstall / update to this version\nCancel — do nothing"
    )) {
        crate::Answer::Yes => uninstall_with_message(),
        crate::Answer::No => {
            install()?;
            message(&format!("{APP_NAME} is reinstalled and running."), false);
        }
        crate::Answer::Cancel => {}
    }
    Ok(())
}
