use super::{Installation, Process, canonical, choose_installation, policy};
use serde::Deserialize;
use signout_core::{Error, Result};
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HLOCAL, HWND, LPARAM, WPARAM},
        Security::WinTrust::*,
        Storage::FileSystem::*,
        System::{
            Registry::*,
            Threading::{
                GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{Shell::CommandLineToArgvW, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, w},
};
fn wide(p: &std::ffi::OsStr) -> Vec<u16> {
    p.encode_wide().chain(Some(0)).collect()
}
fn local() -> Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(Error::Path)
}
pub fn standard_root() -> Result<PathBuf> {
    Ok(local()?.join("Google/Chrome/User Data"))
}
pub fn lock_path() -> Result<PathBuf> {
    Ok(local()?.join("ChromeAccountSignout/operation.lock"))
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
struct Row {
    ProcessId: u32,
    ParentProcessId: u32,
    ExecutablePath: Option<String>,
    CommandLine: Option<String>,
    CreationDate: Option<String>,
}
fn argv(command: &str) -> Result<Vec<String>> {
    unsafe {
        let command = wide(std::ffi::OsStr::new(command));
        let mut count = 0;
        let data = CommandLineToArgvW(PCWSTR(command.as_ptr()), &mut count);
        if data.is_null() || count <= 0 {
            return Err(Error::Process);
        }
        let result = std::slice::from_raw_parts(data, count as usize)
            .iter()
            .map(|p| p.to_string().map_err(|_| Error::Process))
            .collect();
        let _ = windows::Win32::Foundation::LocalFree(Some(HLOCAL(data.cast())));
        result
    }
}
fn birth_from_wmi(raw: &str) -> Result<u64> {
    // wmi 0.18's chrono %f parses CIM microseconds as nanoseconds. Use it only
    // for calendar/timezone conversion, then decode the six-digit fraction explicitly.
    if !raw.is_ascii()
        || raw.len() != 25
        || raw.as_bytes()[14] != b'.'
        || !matches!(raw.as_bytes()[21], b'+' | b'-')
        || !raw.as_bytes()[15..21].iter().all(u8::is_ascii_digit)
    {
        return Err(Error::Process);
    }
    let offset: u32 = raw[22..].parse().map_err(|_| Error::Process)?;
    if offset >= 1440 {
        return Err(Error::Process);
    }
    let parsed: wmi::WMIDateTime = raw.parse().map_err(|_| Error::Process)?;
    let seconds: u64 = parsed
        .0
        .timestamp()
        .try_into()
        .map_err(|_| Error::Process)?;
    let fraction: u64 = raw[15..21].parse().map_err(|_| Error::Process)?;
    seconds
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(fraction))
        .ok_or(Error::Process)
}
pub fn processes() -> Result<Vec<Process>> {
    let conn = wmi::WMIConnection::new().map_err(|_| Error::Process)?;
    let rows:Vec<Row>=conn.raw_query("SELECT ProcessId,ParentProcessId,ExecutablePath,CommandLine,CreationDate FROM Win32_Process WHERE Name='chrome.exe'").map_err(|_|Error::Process)?;
    let mut result = Vec::new();
    for r in rows {
        let pid = r.ProcessId;
        let record: Result<Process> = (|| {
            Ok(Process {
                pid,
                parent: r.ParentProcessId,
                birth: birth_from_wmi(&r.CreationDate.ok_or(Error::Process)?)?,
                executable: canonical(Path::new(&r.ExecutablePath.ok_or(Error::Process)?))?,
                args: Some(argv(&r.CommandLine.ok_or(Error::Process)?)?),
            })
        })();
        if !alive(pid) {
            continue;
        }
        result.push(record?);
    }
    Ok(result)
}
fn alive(pid: u32) -> bool {
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(e) => return e.code() != windows::core::HRESULT::from_win32(87),
        };
        let mut code = 0;
        let status = GetExitCodeProcess(handle, &mut code);
        let _ = CloseHandle(handle);
        status.is_err() || code == 259
    }
}

pub fn is_main(p: &Process) -> bool {
    p.executable
        .file_name()
        .is_some_and(|s| s.eq_ignore_ascii_case("chrome.exe"))
        && policy::typed(p) == Ok(false)
}
fn registered(root: HKEY, view: REG_ROUTINE_FLAGS) -> Option<PathBuf> {
    unsafe {
        let mut size = 0;
        let flags = RRF_RT_REG_SZ | view;
        if RegGetValueW(
            root,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe"),
            PCWSTR::null(),
            flags,
            None,
            None,
            Some(&mut size),
        )
        .is_err()
        {
            return None;
        }
        let mut data = vec![0u16; size as usize / 2];
        if RegGetValueW(
            root,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe"),
            PCWSTR::null(),
            flags,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .is_err()
        {
            return None;
        }
        let end = data.iter().position(|&c| c == 0)?;
        let value = String::from_utf16(&data[..end]).ok()?;
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(&value);
        Some(PathBuf::from(value))
    }
}
fn verified(path: &Path) -> Result<Installation> {
    unsafe {
        let exe = canonical(path)?;
        if !exe.is_file()
            || !exe
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("chrome.exe"))
        {
            return Err(Error::MissingChrome);
        }
        let name = wide(exe.as_os_str());
        // Trusted embedded Authenticode signature, offline-only verification: no runtime network access.
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(name.as_ptr()),
            ..Default::default()
        };
        let mut trust = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        };
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let valid = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        ) == 0;
        trust.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        );
        if !valid {
            return Err(Error::MissingChrome);
        }
        let size = GetFileVersionInfoSizeW(PCWSTR(name.as_ptr()), None);
        if size == 0 {
            return Err(Error::MissingChrome);
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(name.as_ptr()), None, size, data.as_mut_ptr().cast())
            .map_err(|_| Error::MissingChrome)?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0;
        if !VerQueryValueW(
            data.as_ptr().cast(),
            w!("\\VarFileInfo\\Translation"),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return Err(Error::MissingChrome);
        }
        let trans = std::slice::from_raw_parts(ptr.cast::<u16>(), len as usize / 2);
        let mut product_ok = false;
        for pair in trans.as_chunks::<2>().0 {
            let key = format!(
                "\\StringFileInfo\\{:04x}{:04x}\\ProductName",
                pair[0], pair[1]
            );
            let key = wide(std::ffi::OsStr::new(&key));
            let mut value = std::ptr::null_mut();
            let mut count = 0;
            if VerQueryValueW(
                data.as_ptr().cast(),
                PCWSTR(key.as_ptr()),
                &mut value,
                &mut count,
            )
            .as_bool()
                && count > 0
            {
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(
                    value.cast::<u16>(),
                    count as usize - 1,
                ));
                if text == "Google Chrome" {
                    let original_key = format!(
                        "\\StringFileInfo\\{:04x}{:04x}\\OriginalFilename",
                        pair[0], pair[1]
                    );
                    let original_key = wide(std::ffi::OsStr::new(&original_key));
                    let mut original = std::ptr::null_mut();
                    let mut original_len = 0;
                    if VerQueryValueW(
                        data.as_ptr().cast(),
                        PCWSTR(original_key.as_ptr()),
                        &mut original,
                        &mut original_len,
                    )
                    .as_bool()
                        && original_len > 0
                    {
                        let name = String::from_utf16_lossy(std::slice::from_raw_parts(
                            original.cast::<u16>(),
                            original_len as usize - 1,
                        ));
                        product_ok = name.eq_ignore_ascii_case("chrome.exe");
                    }
                }
            }
        }
        if !product_ok {
            return Err(Error::MissingChrome);
        }
        Ok(Installation {
            app: exe.clone(),
            executable: exe,
        })
    }
}
pub fn discover(root: &Path) -> Result<Installation> {
    let records = processes()?;
    let mut paths = Vec::new();
    for key in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [RRF_SUBKEY_WOW6464KEY, RRF_SUBKEY_WOW6432KEY] {
            if let Some(p) = registered(key, view) {
                paths.push(p)
            }
        }
    }
    for variable in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(p) = std::env::var_os(variable) {
            paths.push(PathBuf::from(p).join("Google/Chrome/Application/chrome.exe"))
        }
    }
    paths.extend(
        records
            .iter()
            .filter(|p| is_main(p))
            .map(|p| p.executable.clone()),
    );
    paths.sort();
    paths.dedup();
    let candidates: Vec<_> = paths.iter().filter_map(|p| verified(p).ok()).collect();
    let mut owners = Vec::new();
    for p in &records {
        if is_main(p) && policy::owns_root(p, root, &canonical)? {
            owners.push(p.executable.clone())
        }
    }
    choose_installation(candidates, &owners)
}
struct CloseContext {
    pid: u32,
    windows: Vec<HWND>,
    class_prefix: String,
}
unsafe extern "system" fn close_window(hwnd: HWND, param: LPARAM) -> BOOL {
    unsafe {
        let context = &mut *(param.0 as *mut CloseContext);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut class = [0u16; 256];
        let count = GetClassNameW(hwnd, &mut class);
        let class = String::from_utf16_lossy(&class[..count.max(0) as usize]);
        if pid == context.pid
            && class.starts_with(&context.class_prefix)
            && IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).unwrap_or_default().is_invalid()
        {
            context.windows.push(hwnd);
        }
        BOOL(1)
    }
}
pub fn close(identity: (u32, u64), installation: &Installation) -> Result<bool> {
    if !processes()?.iter().any(|p| {
        p.pid == identity.0
            && p.birth == identity.1
            && p.executable == installation.executable
            && is_main(p)
    }) {
        return Err(Error::Process);
    }
    // Hold the process handle across window enumeration so its PID cannot be recycled.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, identity.0)
            .map_err(|_| Error::Process)?;
        let mut created = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let checked = GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user);
        let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let matched = ticks
            .checked_div(10)
            .and_then(|v| v.checked_sub(11_644_473_600_000_000))
            == Some(identity.1);
        let result = if checked.is_ok() && matched {
            post_close(identity.0)
        } else {
            Err(Error::Process)
        };
        let _ = CloseHandle(handle);
        result
    }
}
fn post_close(pid: u32) -> Result<bool> {
    post_close_class(pid, "Chrome_WidgetWin_")
}
fn post_close_class(pid: u32, class_prefix: &str) -> Result<bool> {
    let mut c = CloseContext {
        pid,
        windows: Vec::new(),
        class_prefix: class_prefix.into(),
    };
    unsafe {
        EnumWindows(
            Some(close_window),
            LPARAM((&mut c as *mut CloseContext) as isize),
        )
        .map_err(|_| Error::Process)?
    };
    Ok(post_all(&c.windows, &|hwnd| unsafe {
        PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok()
    }))
}
fn launch_path(root: &Path) -> Result<PathBuf> {
    let s = root.to_str().ok_or(Error::Path)?;
    if let Some(rest) = s.strip_prefix("\\\\?\\UNC\\") {
        Ok(PathBuf::from(format!("\\\\{rest}")))
    } else if let Some(rest) = s.strip_prefix("\\\\?\\") {
        Ok(PathBuf::from(rest))
    } else {
        Ok(root.into())
    }
}
pub fn reopen(installation: &Installation, root: &Path, folder: &str) -> Result<()> {
    let root = launch_path(root)?;
    Command::new(&installation.executable)
        .arg(format!("--user-data-dir={}", root.display()))
        .arg(format!("--profile-directory={folder}"))
        .spawn()
        .map_err(|_| Error::Process)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_launch_path_and_windows_quoting() {
        assert_eq!(
            launch_path(Path::new(r"\\?\C:\Users\Demo User\Chrome")).unwrap(),
            PathBuf::from(r"C:\Users\Demo User\Chrome")
        );
        assert_eq!(
            argv(r#""C:\Program Files\Chrome\chrome.exe" --user-data-dir="C:\Demo User\Chrome""#)
                .unwrap()[1],
            r"--user-data-dir=C:\Demo User\Chrome"
        );
    }
}

#[cfg(test)]
mod native_windows {
    use super::*;
    #[test]
    fn close_all_visible_windows_and_leave_hidden_internal_window() {
        unsafe {
            use windows::Win32::{Foundation::HANDLE, System::StationsAndDesktops::*};
            let station = GetProcessWindowStation().unwrap();
            let mut flags = USEROBJECTFLAGS::default();
            GetUserObjectInformationW(
                HANDLE(station.0),
                UOI_FLAGS,
                Some((&mut flags as *mut USEROBJECTFLAGS).cast()),
                std::mem::size_of_val(&flags) as u32,
                None,
            )
            .unwrap();
            if flags.dwFlags & WSF_VISIBLE as u32 == 0 {
                assert_ne!(
                    std::env::var("SIGNOUT_REQUIRE_DESKTOP").as_deref(),
                    Ok("1"),
                    "Interactive desktop required for native HWND qualification"
                );
                eprintln!(
                    "Native HWND smoke not qualified in this service desktop; run with SIGNOUT_REQUIRE_DESKTOP=1 in logged-in desktop."
                );
                return;
            }
            let title = w!("Signout isolated test window");
            let first = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                title,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let second = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                title,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let hidden = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                title,
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            if !IsWindowVisible(first).as_bool() {
                DestroyWindow(first).unwrap();
                DestroyWindow(second).unwrap();
                DestroyWindow(hidden).unwrap();
                assert_ne!(
                    std::env::var("SIGNOUT_REQUIRE_DESKTOP").as_deref(),
                    Ok("1"),
                    "Visible desktop required"
                );
                eprintln!(
                    "Native HWNDs not visible in this service desktop; interactive qualification required."
                );
                return;
            }
            assert!(
                post_close_class(std::process::id(), "Static").unwrap()
                    || post_close_class(std::process::id(), "STATIC").unwrap()
            );
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            assert!(!IsWindow(Some(first)).as_bool());
            assert!(!IsWindow(Some(second)).as_bool());
            assert!(IsWindow(Some(hidden)).as_bool());
            DestroyWindow(hidden).unwrap();
            assert!(!post_close_class(std::process::id(), "STATIC").unwrap());
        }
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;
    #[test]
    fn cim_fraction_and_malformed_dates() {
        assert_eq!(
            birth_from_wmi("20261017120000.640984+420").unwrap() % 1_000_000,
            640984
        );
        for raw in [
            "invalid",
            "20261017120000.640984+9999",
            "20261017120000.é40984+420",
        ] {
            assert!(birth_from_wmi(raw).is_err());
        }
    }
    #[test]
    fn wmi_creation_time_matches_kernel_process_identity() {
        unsafe {
            let pid = std::process::id();
            let conn = wmi::WMIConnection::new().unwrap();
            let mut rows:Vec<Row>=conn.raw_query(format!("SELECT ProcessId,ParentProcessId,ExecutablePath,CommandLine,CreationDate FROM Win32_Process WHERE ProcessId={pid}")).unwrap();
            let birth: u64 = birth_from_wmi(&rows.remove(0).CreationDate.unwrap()).unwrap();
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).unwrap();
            let mut created = FILETIME::default();
            let mut exited = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user).unwrap();
            CloseHandle(h).unwrap();
            let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
            assert_eq!(birth, ticks / 10 - 11_644_473_600_000_000);
        }
    }
}

fn post_all(windows: &[HWND], post: &dyn Fn(HWND) -> bool) -> bool {
    !windows.is_empty() && windows.iter().all(|&h| post(h))
}

#[cfg(test)]
mod post_tests {
    use super::*;
    #[test]
    fn native_post_failure_is_not_success() {
        let invalid = HWND(0x12345678usize as *mut _);
        assert!(!post_all(&[invalid], &|h| unsafe {
            PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok()
        }));
        assert!(!post_all(&[], &|_| true));
    }
}

#[cfg(test)]
mod process_exit_tests {
    use super::*;
    #[test]
    fn exited_process_is_not_an_access_failure() {
        assert!(alive(std::process::id()));
        let mut child = Command::new("cmd").args(["/C", "exit"]).spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!alive(pid));
    }
}
