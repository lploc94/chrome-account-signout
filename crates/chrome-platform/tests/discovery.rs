use chrome_platform::{Installation, choose_installation};
use std::path::PathBuf;
fn install(name: &str) -> Installation {
    Installation {
        app: PathBuf::from(name),
        executable: PathBuf::from(format!("{name}/chrome")),
    }
}
#[test]
fn deduplicates_and_rejects_ambiguity() {
    let a = install("/external/Chrome");
    let b = install("/other/Chrome");
    assert_eq!(
        choose_installation(vec![a.clone(), a.clone()], &[])
            .unwrap()
            .executable,
        a.executable
    );
    assert!(choose_installation(vec![a, b], &[]).is_err());
}
#[test]
fn running_owner_preferred() {
    let a = install("/external/Chrome");
    let b = install("/other/Chrome");
    assert_eq!(
        choose_installation(
            vec![a.clone(), b.clone()],
            std::slice::from_ref(&b.executable)
        )
        .unwrap()
        .executable,
        b.executable
    );
    assert!(
        choose_installation(vec![a.clone(), b.clone()], &[a.executable, b.executable]).is_err()
    );
}
#[test]
fn no_installation_does_not_invent_fallback() {
    assert!(choose_installation(vec![], &[]).is_err());
}

#[test]
fn unknown_running_owner_refused() {
    let a = install("/verified");
    assert!(choose_installation(vec![a], &[PathBuf::from("/unverified/chrome")]).is_err());
}
