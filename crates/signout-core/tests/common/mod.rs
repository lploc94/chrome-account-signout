#![allow(dead_code)]
use rusqlite::Connection;
use serde_json::json;
use signout_core::{Inventory, Selection};
use std::path::PathBuf;
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub root: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("User Data");
        std::fs::create_dir_all(root.join("Default")).unwrap();
        let f = Self { temp, root };
        f.metadata("primary");
        f.db().execute_batch("CREATE TABLE token_service(service VARCHAR PRIMARY KEY NOT NULL, encrypted_token BLOB, binding_key BLOB, mtls_token_binding INTEGER); INSERT INTO token_service VALUES('AccountId-primary',X'010203',X'04',1),('AccountId-target',X'050607',NULL,0),('AccountId-target-neighbor',X'0809',X'10',1);").unwrap();
        f
    }
    pub fn metadata(&self, primary: &str) {
        self.write("Local State",json!({"profile":{"info_cache":{"Default":{"name":"Test profile","gaia_id":primary,"user_name":"owner@example.test"}}}}));
        self.write(
            "Default/Preferences",
            json!({"account_info":[
   {"gaia":"primary","account_id":"primary","email":"owner@example.test","full_name":"Owner"},
   {"gaia":"target","account_id":"target","email":"friend@example.test","full_name":"Friend"},
   {"gaia":"target-neighbor","account_id":"target-neighbor","email":"other@example.test"},
   {"gaia":"expired","account_id":"expired","email":"expired@example.test"}]}),
        );
    }
    pub fn write(&self, path: &str, v: serde_json::Value) {
        std::fs::write(self.root.join(path), serde_json::to_vec(&v).unwrap()).unwrap();
    }
    pub fn db(&self) -> Connection {
        Connection::open(self.root.join("Default/Web Data")).unwrap()
    }
    pub fn inventory(&self) -> Inventory {
        Inventory::new(&self.root).unwrap()
    }
    pub fn selection(&self, id: &str) -> Selection {
        let profile = self.inventory().load().unwrap().remove(0);
        let account = profile
            .accounts
            .iter()
            .find(|a| a.gaia_id == id)
            .unwrap()
            .clone();
        Selection { profile, account }
    }
    pub fn rows(&self) -> Vec<(String, Vec<u8>)> {
        self.db()
            .prepare("SELECT service,encrypted_token FROM token_service ORDER BY service")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
}
pub struct Guard;
impl signout_core::HeldOperation for Guard {}
