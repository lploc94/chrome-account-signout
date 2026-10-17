mod common;
use common::*;
use signout_core::{Removal, remove};
#[test]
fn exact_row_and_metadata_preserved() {
    let f = Fixture::new();
    let mut rows = f.rows();
    let prefs = std::fs::read(f.root.join("Default/Preferences")).unwrap();
    assert_eq!(
        remove(
            &f.inventory(),
            &f.selection("target"),
            &Guard,
            &|| Ok(true),
            &|| Ok(())
        )
        .unwrap(),
        Removal::Removed
    );
    rows.retain(|(k, _)| k != "AccountId-target");
    assert_eq!(rows, f.rows());
    assert_eq!(
        prefs,
        std::fs::read(f.root.join("Default/Preferences")).unwrap()
    );
}
#[test]
fn primary_running_and_changed_identity_refused() {
    let f = Fixture::new();
    let rows = f.rows();
    assert!(
        remove(
            &f.inventory(),
            &f.selection("primary"),
            &Guard,
            &|| Ok(true),
            &|| Ok(())
        )
        .is_err()
    );
    let s = f.selection("target");
    assert!(remove(&f.inventory(), &s, &Guard, &|| Ok(false), &|| Ok(())).is_err());
    f.metadata("target");
    assert!(remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).is_err());
    assert_eq!(rows, f.rows());
}
#[test]
fn rollback_and_revalidation() {
    for change in [false, true] {
        let f = Fixture::new();
        let rows = f.rows();
        let s = f.selection("target");
        assert!(
            remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| {
                if change {
                    f.metadata("target");
                    Ok(())
                } else {
                    Err(signout_core::Error::Storage)
                }
            })
            .is_err()
        );
        assert_eq!(rows, f.rows());
    }
}
#[test]
fn repeat_is_already_removed() {
    let f = Fixture::new();
    let s = f.selection("target");
    assert_eq!(
        remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).unwrap(),
        Removal::Removed
    );
    assert_eq!(
        remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).unwrap(),
        Removal::AlreadyRemoved
    );
}
#[test]
fn exclusive_writer_refused() {
    let f = Fixture::new();
    let s = f.selection("target");
    let db = f.db();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).is_err());
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(f.rows().len(), 3);
}
#[test]
fn wal_snapshot() {
    let f = Fixture::new();
    let db = f.db();
    db.execute_batch(
        "PRAGMA journal_mode=WAL; INSERT INTO token_service VALUES('unrelated',X'99',NULL,0)",
    )
    .unwrap();
    let s = f.selection("target");
    assert_eq!(s.profile.accounts.len(), 3);
    assert_eq!(
        remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).unwrap(),
        Removal::Removed
    );
    assert_eq!(f.rows().len(), 3);
}

#[test]
fn sql_shaped_identity_is_a_literal_key() {
    let f = Fixture::new();
    let id = "odd' OR 1=1 --";
    let mut prefs: serde_json::Value =
        serde_json::from_slice(&std::fs::read(f.root.join("Default/Preferences")).unwrap())
            .unwrap();
    prefs["account_info"][1]["gaia"] = id.into();
    prefs["account_info"][1]["account_id"] = id.into();
    f.write("Default/Preferences", prefs);
    f.db()
        .execute(
            "UPDATE token_service SET service=?1 WHERE service='AccountId-target'",
            [format!("AccountId-{id}")],
        )
        .unwrap();
    let s = f.selection(id);
    assert_eq!(
        remove(&f.inventory(), &s, &Guard, &|| Ok(true), &|| Ok(())).unwrap(),
        Removal::Removed
    );
    assert_eq!(f.rows().len(), 2);
    assert!(f.rows().iter().any(|(key, _)| key == "AccountId-primary"));
}
