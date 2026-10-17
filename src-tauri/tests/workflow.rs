use chrome_account_signout_lib::workflow::*;
use signout_core::{Error, LockProvider, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[path = "../../crates/signout-core/tests/common/mod.rs"]
mod fixtures;
struct Browser {
    close_error: Option<Error>,
    open_error: bool,
    opened: AtomicBool,
}
impl BrowserControl for Browser {
    fn was_running(&self) -> Result<bool> {
        Ok(true)
    }
    fn close(&self, cancel: &AtomicBool) -> Result<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        };
        self.close_error.map_or(Ok(()), Err)
    }
    fn stopped(&self) -> Result<bool> {
        Ok(true)
    }
    fn reopen(&self, _: &str) -> Result<()> {
        self.opened.store(true, Ordering::Release);
        if self.open_error {
            Err(Error::Process)
        } else {
            Ok(())
        }
    }
}
fn browser() -> Browser {
    Browser {
        close_error: None,
        open_error: false,
        opened: AtomicBool::new(false),
    }
}
#[test]
fn committed_removal_survives_reopen_failure() {
    let f = fixtures::Fixture::new();
    let mut b = browser();
    b.open_error = true;
    let guard = chrome_platform::NativeLock::new(f.temp.path().join("operation.lock"))
        .acquire()
        .unwrap();
    let binding = Binding {
        root: f.root.clone(),
        selection: f.selection("target"),
    };
    let result = perform(
        &binding,
        &f.inventory(),
        guard.as_ref(),
        &b,
        &AtomicBool::new(false),
        &|| Ok(()),
        &|| Ok(()),
    );
    assert_eq!(result.phase, "done");
    assert!(result.message.contains("Account removed"));
    assert!(result.message.contains("Chrome could not reopen"));
    assert_eq!(f.rows().len(), 2);
}
#[test]
fn close_refusal_cancel_and_commit_failure_preserve_rows() {
    for mode in 0..3 {
        let f = fixtures::Fixture::new();
        let mut b = browser();
        if mode == 0 {
            b.close_error = Some(Error::Running)
        };
        let guard = chrome_platform::NativeLock::new(f.temp.path().join("operation.lock"))
            .acquire()
            .unwrap();
        let binding = Binding {
            root: f.root.clone(),
            selection: f.selection("target"),
        };
        let before = f.rows();
        let result = perform(
            &binding,
            &f.inventory(),
            guard.as_ref(),
            &b,
            &AtomicBool::new(mode == 1),
            &|| {
                if mode == 2 {
                    Err(Error::Storage)
                } else {
                    Ok(())
                }
            },
            &|| Ok(()),
        );
        assert_eq!(f.rows(), before);
        assert_eq!(b.opened.load(Ordering::Acquire), mode == 2);
        assert!(result.message.contains("No account was removed"));
    }
}
struct Blocking {
    f: fixtures::Fixture,
    mutating: bool,
    started: AtomicBool,
    release: AtomicBool,
    cancelled: AtomicBool,
}
impl Runtime for Blocking {
    fn load(&self) -> Result<Loaded> {
        Ok(Loaded {
            root: self.f.root.clone(),
            profiles: self.f.inventory().load()?,
        })
    }
    fn run(&self, _: &Binding, cancel: &AtomicBool, progress: &dyn Fn() -> Result<()>) -> Outcome {
        if self.mutating
            && let Err(e) = progress()
        {
            return Outcome::failed(e);
        }
        self.started.store(true, Ordering::Release);
        while !self.release.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire) {
                self.cancelled.store(true, Ordering::Release);
                return Outcome::failed(Error::Cancelled);
            }
            std::thread::yield_now()
        }
        Outcome::removed(false)
    }
}
#[test]
fn window_exit_cancels_waiting_but_defers_during_commit() {
    for mutating in [false, true] {
        let runtime = Arc::new(Blocking {
            f: fixtures::Fixture::new(),
            mutating,
            started: AtomicBool::new(false),
            release: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        });
        let engine = Engine::new(runtime.clone());
        let list = engine.list().unwrap();
        let id = &list[0].accounts.iter().find(|a| !a.is_primary).unwrap().id;
        let pending = engine.prepare(id).unwrap();
        let operation = engine.confirm(&pending.token).unwrap();
        while !runtime.started.load(Ordering::Acquire) {
            std::thread::yield_now()
        }
        assert!(!engine.request_exit());
        if mutating {
            assert_eq!(engine.status(&operation).unwrap().phase, "mutating");
            assert!(engine.cancel(&operation).is_err());
            runtime.release.store(true, Ordering::Release)
        }
        while engine.active() {
            std::thread::yield_now()
        }
        assert_eq!(runtime.cancelled.load(Ordering::Acquire), !mutating);
        assert!(engine.request_exit());
    }
}
#[test]
fn unknown_result_never_claims_no_change() {
    let o = Outcome::failed(Error::OutcomeUnknown);
    assert_eq!(o.phase, "unknown");
    assert!(!o.message.contains("No account was removed"));
}

#[test]
fn busy_sqlite_does_not_turn_close_into_success() {
    let f = fixtures::Fixture::new();
    let binding = Binding {
        root: f.root.clone(),
        selection: f.selection("target"),
    };
    let guard = chrome_platform::NativeLock::new(f.temp.path().join("operation.lock"))
        .acquire()
        .unwrap();
    let db = f.db();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let result = perform(
        &binding,
        &f.inventory(),
        guard.as_ref(),
        &browser(),
        &AtomicBool::new(false),
        &|| Ok(()),
        &|| Ok(()),
    );
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(result.phase, "error");
    assert_eq!(f.rows().len(), 3);
}

#[test]
fn cached_selection_closes_before_database_access_and_revalidates_after_close() {
    struct LockedBrowser {
        writer: rusqlite::Connection,
        fixture: fixtures::Fixture,
        closed: AtomicBool,
        change_identity: bool,
    }
    impl BrowserControl for LockedBrowser {
        fn was_running(&self) -> Result<bool> {
            Ok(true)
        }
        fn close(&self, _: &AtomicBool) -> Result<()> {
            self.writer.execute_batch("ROLLBACK").unwrap();
            if self.change_identity {
                self.fixture.metadata("target");
            }
            self.closed.store(true, Ordering::Release);
            Ok(())
        }
        fn stopped(&self) -> Result<bool> {
            Ok(self.closed.load(Ordering::Acquire))
        }
        fn reopen(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }
    for (selection_id, change_identity) in [("target", false), ("target", true), ("expired", false)]
    {
        let f = fixtures::Fixture::new();
        let before = f.full_rows();
        let writer = f.db();
        writer.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let inventory = f.inventory();
        let profile = inventory.load_metadata().unwrap().remove(0);
        let account = profile
            .accounts
            .iter()
            .find(|a| a.gaia_id == selection_id)
            .unwrap()
            .clone();
        let binding = Binding {
            root: f.root.clone(),
            selection: signout_core::Selection { profile, account },
        };
        let guard = chrome_platform::NativeLock::new(f.temp.path().join("operation.lock"))
            .acquire()
            .unwrap();
        let b = LockedBrowser {
            writer,
            fixture: f,
            closed: AtomicBool::new(false),
            change_identity,
        };
        let result = perform(
            &binding,
            &inventory,
            guard.as_ref(),
            &b,
            &AtomicBool::new(false),
            &|| Ok(()),
            &|| Ok(()),
        );
        assert!(b.closed.load(Ordering::Acquire));
        if change_identity {
            assert_eq!(result.phase, "error");
            assert_eq!(b.fixture.full_rows(), before);
        } else {
            assert_eq!(result.phase, "done");
            if selection_id == "expired" {
                assert!(result.message.contains("was already removed"));
                assert_eq!(b.fixture.full_rows(), before);
            } else {
                assert_eq!(
                    b.fixture.full_rows(),
                    before
                        .into_iter()
                        .filter(|r| r.0 != "AccountId-target")
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
