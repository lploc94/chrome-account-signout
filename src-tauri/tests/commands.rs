use chrome_account_signout_lib::workflow::{Binding, Engine, Loaded, Outcome, Runtime};
use signout_core::{Error, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[path = "../../crates/signout-core/tests/common/mod.rs"]
mod fixtures;
struct Fake {
    f: fixtures::Fixture,
    calls: Arc<AtomicUsize>,
}
impl Runtime for Fake {
    fn load(&self) -> Result<Loaded> {
        Ok(Loaded {
            root: self.f.root.clone(),
            profiles: self.f.inventory().load()?,
        })
    }
    fn run(&self, _: &Binding, _: &AtomicBool, progress: &dyn Fn() -> Result<()>) -> Outcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        progress().unwrap();
        Outcome::removed(true)
    }
}
fn engine() -> (Arc<Engine>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Engine::new(Arc::new(Fake {
            f: fixtures::Fixture::new(),
            calls: calls.clone(),
        })),
        calls,
    )
}
#[test]
fn forged_ids_and_primary_cannot_start() {
    let (e, calls) = engine();
    assert!(e.prepare("../../Web Data").is_err());
    let profiles = e.list().unwrap();
    let owner = profiles[0].accounts.iter().find(|a| a.is_primary).unwrap();
    assert!(e.prepare(&owner.id).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(e.confirm("forged").is_err());
}
#[test]
fn confirmation_is_single_use_and_not_a_path() {
    let (e, calls) = engine();
    let profiles = e.list().unwrap();
    let id = &profiles[0]
        .accounts
        .iter()
        .find(|a| !a.is_primary)
        .unwrap()
        .id;
    let pending = e.prepare(id).unwrap();
    let operation = e.confirm(&pending.token).unwrap();
    assert!(e.confirm(&pending.token).is_err());
    while e.active() {
        std::thread::yield_now()
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(e.status(&operation).unwrap().phase, "done");
    assert!(e.status("wrong").is_err());
    let wire = serde_json::to_string(&profiles).unwrap();
    assert!(!wire.contains("gaia"));
    assert!(!wire.contains("Web Data"));
}
#[test]
fn cancelled_or_refreshed_confirmation_does_nothing() {
    let (e, calls) = engine();
    let profiles = e.list().unwrap();
    let id = &profiles[0]
        .accounts
        .iter()
        .find(|a| !a.is_primary)
        .unwrap()
        .id;
    let p = e.prepare(id).unwrap();
    e.cancel(&p.token).unwrap();
    assert!(e.confirm(&p.token).is_err());
    let p = e.prepare(id).unwrap();
    e.list().unwrap();
    assert!(e.confirm(&p.token).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn invalid_runtime_errors_are_generic() {
    let e = Engine::new(Arc::new(Missing));
    assert_eq!(e.list().err(), Some(Error::MissingChrome));
}
struct Missing;
impl Runtime for Missing {
    fn load(&self) -> Result<Loaded> {
        Err(Error::MissingChrome)
    }
    fn run(&self, _: &Binding, _: &AtomicBool, _: &dyn Fn() -> Result<()>) -> Outcome {
        panic!("invalid request must not reach runtime")
    }
}
