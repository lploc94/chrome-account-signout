use super::{Installation, Process, canonical, choose_installation, policy};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSBundle, NSString, NSURL};
use signout_core::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
pub fn standard_root() -> Result<PathBuf> {
    Ok(home()?.join("Library/Application Support/Google/Chrome"))
}
pub fn lock_path() -> Result<PathBuf> {
    Ok(home()?.join("Library/Caches/ChromeAccountSignout/operation.lock"))
}
fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(Error::Path)
}
fn url_path(url: &NSURL) -> Result<PathBuf> {
    url.path()
        .map(|s| PathBuf::from(s.to_string()))
        .ok_or(Error::Path)
}
fn verify(url: &NSURL) -> Result<Installation> {
    let bundle = NSBundle::bundleWithURL(url).ok_or(Error::MissingChrome)?;
    if bundle.bundleIdentifier().map(|s| s.to_string()).as_deref() != Some("com.google.Chrome") {
        return Err(Error::MissingChrome);
    }
    let exe_url = bundle.executableURL().ok_or(Error::MissingChrome)?;
    let executable = canonical(&url_path(&exe_url)?)?;
    if !executable.is_file() {
        return Err(Error::MissingChrome);
    }
    Ok(Installation {
        app: canonical(&url_path(url)?)?,
        executable,
    })
}
pub fn discover(root: &Path) -> Result<Installation> {
    objc2::rc::autoreleasepool(|_| {
        let id = NSString::from_str("com.google.Chrome");
        let mut candidates = Vec::new();
        if let Some(url) = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&id)
            && let Ok(c) = verify(&url)
        {
            candidates.push(c)
        }
        for app in NSRunningApplication::runningApplicationsWithBundleIdentifier(&id) {
            if let Some(url) = app.bundleURL()
                && let Ok(c) = verify(&url)
            {
                candidates.push(c)
            }
        }
        let records = processes()?;
        let mut owners = Vec::new();
        for p in &records {
            if is_main(p) && policy::owns_root(p, root, &canonical)? {
                owners.push(p.executable.clone())
            }
        }
        choose_installation(candidates, &owners)
    })
}
pub fn is_main(p: &Process) -> bool {
    p.executable
        .file_name()
        .is_some_and(|s| s == "Google Chrome")
        && policy::typed(p) == Ok(false)
}
fn birth(pid: i32) -> Option<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            std::mem::size_of_val(&info) as i32,
        )
    };
    (n as usize == std::mem::size_of_val(&info))
        .then_some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}
fn arguments(pid: i32) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = 0;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || size < 4
    {
        return None;
    }
    let mut data = vec![0u8; size];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            data.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    };
    data.truncate(size);
    let count = i32::from_ne_bytes(data.get(..4)?.try_into().ok()?);
    if count <= 0 {
        return None;
    }
    let mut cursor = 4;
    while *data.get(cursor)? != 0 {
        cursor += 1
    }
    while *data.get(cursor)? == 0 {
        cursor += 1
    }
    let mut result = Vec::new();
    for _ in 0..count {
        let start = cursor;
        while *data.get(cursor)? != 0 {
            cursor += 1
        }
        result.push(std::str::from_utf8(data.get(start..cursor)?).ok()?.into());
        cursor += 1;
    }
    Some(result)
}
pub fn processes() -> Result<Vec<Process>> {
    let output = Command::new("/bin/ps")
        .args(["-axww", "-o", "pid=,ppid=,comm="])
        .output()
        .map_err(|_| Error::Process)?;
    if !output.status.success() {
        return Err(Error::Process);
    }
    let text = String::from_utf8(output.stdout).map_err(|_| Error::Process)?;
    let mut records = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let Some((pid, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((parent, exe)) = rest.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        let exe = exe.trim();
        if !(exe.ends_with("/Google Chrome")
            || exe.contains("/Google Chrome Helper")
            || exe.ends_with("/chrome_crashpad_handler"))
        {
            continue;
        }
        let pid: i32 = pid.parse().map_err(|_| Error::Process)?;
        let parent = parent.parse().map_err(|_| Error::Process)?;
        // Process exit during enumeration is harmless; unreadable live identity fails closed.
        let Some(birth) = birth(pid) else {
            if !alive(pid) {
                continue;
            } else {
                return Err(Error::Process);
            }
        };
        let exe = match canonical(Path::new(exe)) {
            Ok(p) => p,
            Err(_) if !alive(pid) => continue,
            Err(e) => return Err(e),
        };
        if exe
            .file_name()
            .is_some_and(|n| n == "chrome_crashpad_handler")
            && !exe
                .ancestors()
                .filter(|p| p.extension().is_some_and(|e| e == "app"))
                .any(|p| {
                    let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
                    verify(&url).is_ok()
                })
        {
            continue;
        }
        let args = arguments(pid);
        if !alive(pid) {
            continue;
        }
        if args.is_none() || self::birth(pid) != Some(birth) {
            return Err(Error::Process);
        }
        records.push(Process {
            pid: pid as u32,
            parent,
            birth,
            executable: exe,
            args,
        });
    }
    Ok(records)
}
pub fn close(identity: (u32, u64), installation: &Installation) -> Result<bool> {
    objc2::rc::autoreleasepool(|_| {
        if birth(identity.0 as i32) != Some(identity.1) {
            return Err(Error::Process);
        }
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(identity.0 as i32)
            .ok_or(Error::Process)?;
        let actual = app.executableURL().ok_or(Error::Process)?;
        if canonical(&url_path(&actual)?)? != installation.executable {
            return Err(Error::Process);
        }
        Ok(app.terminate())
    })
}
pub fn reopen(installation: &Installation, root: &Path, folder: &str) -> Result<()> {
    let status = Command::new("/usr/bin/open")
        .args(["-n", "-a"])
        .arg(&installation.app)
        .arg("--args")
        .arg(format!("--user-data-dir={}", root.display()))
        .arg(format!("--profile-directory={folder}"))
        .status()
        .map_err(|_| Error::Process)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Process)
    }
}

fn alive(pid: i32) -> bool {
    unsafe {
        libc::kill(pid, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exited_process_is_distinct_from_unreadable_live_process() {
        assert!(alive(std::process::id() as i32));
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!alive(pid as i32));
        assert!(birth(pid as i32).is_none());
    }
}
