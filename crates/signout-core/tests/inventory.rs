mod common;
use common::*;
#[test]
fn only_metadata_accounts_with_tokens() {
    let f = Fixture::new();
    let p = f.inventory().load().unwrap().remove(0);
    assert_eq!(p.accounts.len(), 3);
    assert!(
        p.accounts
            .iter()
            .find(|a| a.gaia_id == "primary")
            .unwrap()
            .is_primary
    );
}
#[test]
fn malformed_identity_and_primary_fail_closed() {
    for mode in 0..4 {
        let f = Fixture::new();
        let path = f.root.join("Default/Preferences");
        let mut v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match mode {
            0 => v["account_info"][1]["account_id"] = "wrong".into(),
            1 => v["account_info"][1]["gaia"] = "bad\0id".into(),
            2 => {
                let dup = v["account_info"][1].clone();
                v["account_info"].as_array_mut().unwrap().push(dup);
            }
            _ => v["account_info"][0]["email"] = "wrong@example.test".into(),
        };
        f.write("Default/Preferences", v);
        let p = f.inventory().load().unwrap().remove(0);
        assert!(p.unavailable_reason.is_some());
        assert!(p.accounts.is_empty());
    }
}
#[test]
fn unsupported_schema_or_trigger_unavailable() {
    for sql in [
        "ALTER TABLE token_service ADD COLUMN unknown BLOB",
        "CREATE TRIGGER unwanted AFTER DELETE ON token_service BEGIN DELETE FROM token_service; END",
    ] {
        let f = Fixture::new();
        f.db().execute_batch(sql).unwrap();
        assert!(
            f.inventory().load().unwrap()[0]
                .unavailable_reason
                .is_some()
        );
    }
}
#[test]
fn traversal_refused() {
    let f = Fixture::new();
    assert!(f.inventory().safe_file("../Default", "Web Data").is_err());
    assert!(
        f.inventory()
            .safe_file("Default/child", "Web Data")
            .is_err()
    );
}
#[cfg(unix)]
#[test]
fn redirected_database_even_inside_root_refused() {
    let f = Fixture::new();
    let db = f.root.join("Default/Web Data");
    let other = f.root.join("Default/Other");
    std::fs::rename(&db, &other).unwrap();
    std::os::unix::fs::symlink(&other, &db).unwrap();
    assert!(
        f.inventory().load().unwrap()[0]
            .unavailable_reason
            .is_some()
    );
}
#[cfg(unix)]
#[test]
fn relocated_root_supported() {
    let f = Fixture::new();
    let link = f.temp.path().join("relocated");
    std::os::unix::fs::symlink(&f.root, &link).unwrap();
    assert_eq!(
        signout_core::Inventory::new(&link).unwrap().load().unwrap()[0]
            .accounts
            .len(),
        3
    );
}

#[test]
fn intermediate_folder_spelling_cannot_alias_another_selection() {
    let f = Fixture::new();
    assert!(f.inventory().safe_file("dEfAuLt", "Web Data").is_err());
}
