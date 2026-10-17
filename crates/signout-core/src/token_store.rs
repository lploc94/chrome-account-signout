use crate::{Error, Removal, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::{collections::HashSet, path::Path, time::Duration};
pub(crate) struct TokenStore {
    db: Connection,
}
impl TokenStore {
    pub fn open(path: &Path, writable: bool) -> Result<Self> {
        let access = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let db = Connection::open_with_flags(
            path,
            access | OpenFlags::SQLITE_OPEN_FULL_MUTEX | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|_| Error::Storage)?;
        db.busy_timeout(Duration::from_millis(300))
            .map_err(|_| Error::Storage)?;
        db.execute_batch("PRAGMA trusted_schema=OFF")
            .map_err(|_| Error::Storage)?;
        let store = Self { db };
        store.validate_schema()?;
        Ok(store)
    }
    fn validate_schema(&self) -> Result<()> {
        let mut q = self
            .db
            .prepare("PRAGMA table_xinfo(token_service)")
            .map_err(|_| Error::Schema)?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })
            .map_err(|_| Error::Schema)?;
        let mut columns = HashSet::new();
        let mut key = false;
        for r in rows {
            let (name, typ, not_null, pk, hidden) = r.map_err(|_| Error::Schema)?;
            if hidden != 0 {
                return Err(Error::Schema);
            };
            if name == "service" {
                key = not_null == 1
                    && pk == 1
                    && matches!(typ.to_ascii_uppercase().as_str(), "TEXT" | "VARCHAR")
            };
            columns.insert(name);
        }
        let required = ["service", "encrypted_token"];
        let allowed = [
            "service",
            "encrypted_token",
            "binding_key",
            "mtls_token_binding",
        ];
        if !key
            || !required.iter().all(|k| columns.contains(*k))
            || columns.iter().any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(Error::Schema);
        }
        let forbidden:i64=self.db.query_row("SELECT count(*) FROM sqlite_master WHERE (type='trigger' AND tbl_name='token_service') OR (name='token_service' AND (type!='table' OR upper(sql) LIKE '%VIRTUAL TABLE%'))",[],|r|r.get(0)).map_err(|_|Error::Schema)?;
        if forbidden != 0 {
            return Err(Error::Schema);
        };
        Ok(())
    }
    pub fn services(&self) -> Result<HashSet<String>> {
        let mut q = self
            .db
            .prepare("SELECT service FROM token_service")
            .map_err(|_| Error::Storage)?;
        q.query_map([], |r| r.get(0))
            .map_err(|_| Error::Storage)?
            .map(|r| r.map_err(|_| Error::Storage))
            .collect()
    }
    fn contains(&self, key: &str) -> Result<bool> {
        self.db
            .query_row("SELECT 1 FROM token_service WHERE service=?1", [key], |r| {
                r.get::<_, i64>(0)
            })
            .optional()
            .map(|r| r.is_some())
            .map_err(|_| Error::Storage)
    }
    pub fn remove(
        &self,
        key: &str,
        validate: &dyn Fn() -> Result<()>,
        before_commit: &dyn Fn() -> Result<()>,
    ) -> Result<Removal> {
        self.db
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| Error::Storage)?;
        let result = (|| {
            self.validate_schema()?;
            validate()?;
            let count = self
                .db
                .execute("DELETE FROM token_service WHERE service=?1", [key])
                .map_err(|_| Error::Storage)?;
            if count > 1 {
                return Err(Error::Schema);
            };
            before_commit()?;
            validate()?;
            if self.db.execute_batch("COMMIT").is_err()
                && !(self.db.is_autocommit() && !self.contains(key)?)
            {
                return Err(Error::Storage);
            }
            Ok(if count == 0 {
                Removal::AlreadyRemoved
            } else {
                Removal::Removed
            })
        })();
        if result.is_err() && !self.db.is_autocommit() {
            let _ = self.db.execute_batch("ROLLBACK");
        }
        result
    }
}
