use signout_core::{Error, Result};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub birth: u64,
    pub executable: PathBuf,
    pub args: Option<Vec<String>>,
}
#[derive(Default)]
pub struct ProcessState {
    pub main: HashSet<(u32, u64)>,
    pub relevant: HashSet<(u32, u64)>,
}
pub(crate) fn typed(p: &Process) -> Result<bool> {
    Ok(p.args
        .as_ref()
        .ok_or(Error::Process)?
        .iter()
        .any(|s| s == "--type" || s.starts_with("--type=")))
}
pub(crate) fn owns_root(
    p: &Process,
    root: &Path,
    normalize: &dyn Fn(&Path) -> Result<PathBuf>,
) -> Result<bool> {
    let args = p
        .args
        .as_ref()
        .filter(|a| !a.is_empty())
        .ok_or(Error::Process)?;
    let mut roots = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--user-data-dir" {
            roots.push(
                args.get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or(Error::Process)?
                    .as_str(),
            )
        } else if let Some(v) = arg.strip_prefix("--user-data-dir=") {
            roots.push(v)
        }
    }
    if roots.len() > 1 {
        return Err(Error::Process);
    }
    if let Some(s) = roots.first() {
        let p = Path::new(s);
        if !p.is_absolute() {
            return Err(Error::Process);
        };
        Ok(normalize(p)? == normalize(root)?)
    } else {
        Ok(true)
    }
}
pub fn classify(
    records: &[Process],
    executable: &Path,
    root: &Path,
    tracked: &HashSet<(u32, u64)>,
    normalize: &dyn Fn(&Path) -> Result<PathBuf>,
) -> Result<ProcessState> {
    // Native collectors supply only Chrome processes from every installation.
    // Helpers can use a separate macOS executable or the same Windows chrome.exe.
    let helper = |p: &Process| {
        p.executable.file_name().is_some_and(|n| {
            let name = n.to_string_lossy();
            name.contains("Google Chrome Helper") || name == "chrome_crashpad_handler"
        })
    };
    let mut ownership = HashMap::new();
    let mut out = ProcessState::default();
    let map: HashMap<_, _> = records.iter().map(|p| (p.pid, p)).collect();
    for p in records {
        if !typed(p)? && !helper(p) {
            let own = owns_root(p, root, normalize)?;
            if own && p.executable != executable {
                return Err(Error::AmbiguousChrome);
            }
            ownership.insert(p.pid, own);
            if own {
                out.main.insert((p.pid, p.birth));
                out.relevant.insert((p.pid, p.birth));
            }
        }
    }
    for p in records {
        if ownership.contains_key(&p.pid) {
            continue;
        }
        let identity = (p.pid, p.birth);
        if tracked.contains(&identity) {
            out.relevant.insert(identity);
            continue;
        }
        let mut parent = p.parent;
        let mut seen = HashSet::new();
        let mut found = false;
        let mut current_birth = p.birth;
        while parent > 0 && seen.insert(parent) {
            let Some(ancestor) = map.get(&parent) else {
                break;
            };
            if ancestor.birth > current_birth {
                break;
            };
            current_birth = ancestor.birth;
            if let Some(own) = ownership.get(&parent) {
                if *own {
                    out.relevant.insert(identity);
                }
                found = true;
                break;
            }
            parent = ancestor.parent;
        }
        if !found {
            // Crashpad intentionally reparents to the system launcher on macOS.
            // Its verified in-bundle executable and exact database root establish ownership.
            if p.executable
                .file_name()
                .is_some_and(|n| n == "chrome_crashpad_handler")
            {
                let args = p.args.as_ref().ok_or(Error::Process)?;
                let mut databases = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    if arg == "--database" {
                        databases.push(
                            args.get(i + 1)
                                .filter(|s| !s.starts_with("--"))
                                .ok_or(Error::Process)?
                                .as_str(),
                        )
                    } else if let Some(value) = arg.strip_prefix("--database=") {
                        databases.push(value)
                    }
                }
                if databases.len() != 1 || !Path::new(databases[0]).is_absolute() {
                    return Err(Error::Process);
                }
                let database = Path::new(databases[0]);
                if database.file_name() != Some(std::ffi::OsStr::new("Crashpad")) {
                    return Err(Error::Process);
                }
                if normalize(database.parent().ok_or(Error::Process)?)? == normalize(root)? {
                    out.relevant.insert(identity);
                }
            } else {
                return Err(Error::Process);
            }
        }
    }
    Ok(out)
}
