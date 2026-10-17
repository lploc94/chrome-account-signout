use chrome_platform::NativeLock;
use signout_core::LockProvider;
#[test]
fn same_file_lock_is_exclusive_and_reusable() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("operation.lock");
    let l = NativeLock::new(path.clone());
    let guard = l.acquire().unwrap();
    assert!(NativeLock::new(path.clone()).acquire().is_err());
    drop(guard);
    assert!(NativeLock::new(path).acquire().is_ok());
}
#[test]
fn lock_child_process() {
    let Ok(path) = std::env::var("SIGNOUT_TEST_LOCK") else {
        return;
    };
    assert!(NativeLock::new(path.into()).acquire().is_err());
}
#[test]
fn lock_excludes_another_process() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("operation.lock");
    let _guard = NativeLock::new(path.clone()).acquire().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child_process"])
        .env("SIGNOUT_TEST_LOCK", path)
        .status()
        .unwrap();
    assert!(status.success());
}
#[cfg(unix)]
#[test]
fn redirected_lock_rejected() {
    let t = tempfile::tempdir().unwrap();
    let original = t.path().join("real");
    std::fs::write(&original, []).unwrap();
    let link = t.path().join("operation.lock");
    std::os::unix::fs::symlink(original, &link).unwrap();
    assert!(NativeLock::new(link).acquire().is_err());
}
#[cfg(windows)]
#[test]
fn junction_inside_root_rejected() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    let target = root.join("actual");
    std::fs::create_dir_all(&target).unwrap();
    let link = root.join("Default");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        signout_core::safe_child(
            &root.canonicalize().unwrap(),
            std::path::Path::new("Default")
        )
        .is_err()
    );
    std::fs::remove_dir(link).unwrap();
}

#[path = "../../signout-core/tests/common/mod.rs"]
mod fixtures;
#[test]
fn borrowed_native_guard_allows_one_successful_removal() {
    let f = fixtures::Fixture::new();
    let guard = NativeLock::new(f.temp.path().join("operation.lock"))
        .acquire()
        .unwrap();
    assert_eq!(
        signout_core::remove(
            &f.inventory(),
            &f.selection("target"),
            guard.as_ref(),
            &|| Ok(true),
            &|| Ok(())
        )
        .unwrap(),
        signout_core::Removal::Removed
    );
    assert_eq!(f.rows().len(), 2);
}

#[cfg(windows)]
#[test]
fn redirected_lock_directory_rejected() {
    let t = tempfile::tempdir().unwrap();
    let target = t.path().join("real");
    std::fs::create_dir(&target).unwrap();
    let link = t.path().join("operation.lock");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(NativeLock::new(link.clone()).acquire().is_err());
    assert_eq!(std::fs::read_dir(target).unwrap().count(), 0);
    std::fs::remove_dir(link).unwrap();
}
#[cfg(windows)]
#[test]
fn canonical_windows_roots_handle_prefix_case_and_spaces() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Demo User");
    std::fs::create_dir(&root).unwrap();
    let canonical = root.canonicalize().unwrap();
    let uppercase = root.to_string_lossy().to_uppercase();
    assert_eq!(
        chrome_platform::canonical(std::path::Path::new(&uppercase)).unwrap(),
        canonical
    );
    assert_eq!(chrome_platform::canonical(&canonical).unwrap(), canonical);
}
