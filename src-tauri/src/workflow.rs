use serde::Serialize;
use signout_core::{Error, Inventory, LockProvider, Profile, Removal, Result, Selection};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

pub struct Loaded {
    pub root: PathBuf,
    pub profiles: Vec<Profile>,
}
#[derive(Clone)]
pub struct Binding {
    pub root: PathBuf,
    pub selection: Selection,
}
pub trait Runtime: Send + Sync {
    fn load(&self) -> Result<Loaded>;
    fn run(
        &self,
        binding: &Binding,
        cancel: &AtomicBool,
        mutating: &dyn Fn() -> Result<()>,
    ) -> Outcome;
}
#[derive(Clone, Serialize)]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub email: String,
    pub is_primary: bool,
}
#[derive(Clone, Serialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub accounts: Vec<AccountView>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct Pending {
    pub token: String,
    pub email: String,
}
#[derive(Clone, Serialize)]
pub struct Status {
    pub id: String,
    pub phase: String,
    pub message: String,
}
pub struct Outcome {
    pub phase: &'static str,
    pub message: String,
}
impl Outcome {
    pub fn removed(reopened: bool) -> Self {
        Self {
            phase: "done",
            message: if reopened {
                "Đã gỡ tài khoản trên máy này. Kiểm tra Chrome để xác nhận."
            } else {
                "Đã gỡ tài khoản trên máy này. Chưa mở lại được Chrome; hãy tự mở và kiểm tra."
            }
            .into(),
        }
    }
    pub fn failed(error: Error) -> Self {
        match error {
            Error::OutcomeUnknown => Self {
                phase: "unknown",
                message: error.to_string(),
            },
            Error::Cancelled => Self {
                phase: "done",
                message: error.to_string(),
            },
            _ => Self {
                phase: "error",
                message: format!("Chưa gỡ tài khoản. {error}"),
            },
        }
    }
}
struct Prepared {
    token: String,
    binding: Binding,
}
struct Operation {
    status: Status,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
struct State {
    cache: HashMap<String, Binding>,
    pending: Option<Prepared>,
    operation: Option<Operation>,
    loading: bool,
}
pub struct Engine {
    state: Mutex<State>,
    runtime: Arc<dyn Runtime>,
}
fn active(s: &State) -> bool {
    s.operation
        .as_ref()
        .is_some_and(|o| matches!(o.status.phase.as_str(), "waiting" | "mutating"))
}
impl Engine {
    pub fn new(runtime: Arc<dyn Runtime>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            runtime,
        })
    }
    pub fn list(&self) -> Result<Vec<ProfileView>> {
        {
            let mut s = self.state.lock().map_err(|_| Error::Storage)?;
            if active(&s) || s.loading {
                return Err(Error::Busy);
            }
            s.loading = true;
            s.pending = None;
            s.cache.clear();
        }
        let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.runtime.load()))
            .unwrap_or(Err(Error::Storage));
        let mut s = self.state.lock().map_err(|_| Error::Storage)?;
        s.loading = false;
        let loaded = loaded?;
        let mut result = Vec::new();
        for profile in loaded.profiles {
            let mut accounts = Vec::new();
            for account in &profile.accounts {
                let id = Uuid::new_v4().to_string();
                s.cache.insert(
                    id.clone(),
                    Binding {
                        root: loaded.root.clone(),
                        selection: Selection {
                            profile: profile.clone(),
                            account: account.clone(),
                        },
                    },
                );
                accounts.push(AccountView {
                    id,
                    name: account.name.clone(),
                    email: account.email.clone(),
                    is_primary: account.is_primary,
                });
            }
            result.push(ProfileView {
                id: Uuid::new_v4().to_string(),
                name: profile.name,
                accounts,
                error: profile.unavailable_reason,
            });
        }
        Ok(result)
    }
    pub fn prepare(&self, id: &str) -> Result<Pending> {
        let mut s = self.state.lock().map_err(|_| Error::Storage)?;
        if active(&s) || s.loading {
            return Err(Error::Busy);
        }
        let binding = s.cache.get(id).cloned().ok_or(Error::Identity)?;
        if binding.selection.account.is_primary
            || binding.selection.profile.unavailable_reason.is_some()
        {
            return Err(Error::Identity);
        }
        let token = Uuid::new_v4().to_string();
        let email = binding.selection.account.email.clone();
        s.pending = Some(Prepared {
            token: token.clone(),
            binding,
        });
        Ok(Pending { token, email })
    }
    pub fn confirm(self: &Arc<Self>, token: &str) -> Result<String> {
        let (id, binding, cancel) = {
            let mut s = self.state.lock().map_err(|_| Error::Storage)?;
            if active(&s) || s.loading {
                return Err(Error::Busy);
            }
            if s.pending.as_ref().is_none_or(|p| p.token != token) {
                return Err(Error::Identity);
            }
            let p = s.pending.take().ok_or(Error::Identity)?;
            let id = Uuid::new_v4().to_string();
            let cancel = Arc::new(AtomicBool::new(false));
            s.operation = Some(Operation {
                status: Status {
                    id: id.clone(),
                    phase: "waiting".into(),
                    message: "Đang chờ Chrome đóng…".into(),
                },
                cancel: cancel.clone(),
            });
            (id, p.binding, cancel)
        };
        let engine = self.clone();
        let op_id = id.clone();
        if std::thread::Builder::new()
            .name("chrome-signout".into())
            .spawn(move || {
                let progress = || {
                    let mut s = engine.state.lock().map_err(|_| Error::Storage)?;
                    let op = s
                        .operation
                        .as_mut()
                        .filter(|o| o.status.id == op_id)
                        .ok_or(Error::Identity)?;
                    if op.cancel.load(Ordering::Acquire) {
                        return Err(Error::Cancelled);
                    };
                    op.status.phase = "mutating".into();
                    op.status.message = "Đang gỡ tài khoản và mở lại Chrome…".into();
                    Ok(())
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    engine.runtime.run(&binding, &cancel, &progress)
                }))
                .unwrap_or_else(|_| Outcome::failed(Error::OutcomeUnknown));
                if let Ok(mut s) = engine.state.lock()
                    && let Some(op) = s.operation.as_mut().filter(|o| o.status.id == op_id)
                {
                    op.status.phase = result.phase.into();
                    op.status.message = result.message;
                }
            })
            .is_err()
        {
            if let Ok(mut s) = self.state.lock()
                && let Some(op) = s.operation.as_mut()
            {
                op.status.phase = "error".into();
                op.status.message = Error::Storage.to_string();
            }
            return Err(Error::Storage);
        }
        Ok(id)
    }
    pub fn status(&self, id: &str) -> Result<Status> {
        self.state
            .lock()
            .map_err(|_| Error::Storage)?
            .operation
            .as_ref()
            .filter(|o| o.status.id == id)
            .map(|o| o.status.clone())
            .ok_or(Error::Identity)
    }
    pub fn cancel(&self, id: &str) -> Result<()> {
        let mut s = self.state.lock().map_err(|_| Error::Storage)?;
        if s.pending.as_ref().is_some_and(|p| p.token == id) {
            s.pending = None;
            return Ok(());
        };
        let op = s
            .operation
            .as_mut()
            .filter(|o| o.status.id == id)
            .ok_or(Error::Identity)?;
        if op.status.phase != "waiting" {
            return Err(Error::Busy);
        };
        op.cancel.store(true, Ordering::Release);
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.state.lock().map(|s| active(&s)).unwrap_or(true)
    }
    /// Normal window close/app exit may request cancellation only before mutation.
    pub fn request_exit(&self) -> bool {
        let Ok(s) = self.state.lock() else {
            return false;
        };
        if !active(&s) {
            return true;
        };
        if let Some(op) = &s.operation
            && op.status.phase == "waiting"
        {
            op.cancel.store(true, Ordering::Release)
        }
        false
    }
}

pub trait BrowserControl {
    fn was_running(&self) -> Result<bool>;
    fn close(&self, cancel: &AtomicBool) -> Result<()>;
    fn stopped(&self) -> Result<bool>;
    fn reopen(&self, folder: &str) -> Result<()>;
}
struct NativeBrowser(Mutex<chrome_platform::Chrome>);
impl BrowserControl for NativeBrowser {
    fn was_running(&self) -> Result<bool> {
        Ok(!self
            .0
            .lock()
            .map_err(|_| Error::Process)?
            .snapshot()?
            .main
            .is_empty())
    }
    fn close(&self, cancel: &AtomicBool) -> Result<()> {
        self.0.lock().map_err(|_| Error::Process)?.close(cancel)
    }
    fn stopped(&self) -> Result<bool> {
        self.0.lock().map_err(|_| Error::Process)?.stopped()
    }
    fn reopen(&self, folder: &str) -> Result<()> {
        self.0.lock().map_err(|_| Error::Process)?.reopen(folder)
    }
}
/// Trusted Rust integration seam; never exposed as an IPC command or a root override.
pub fn perform(
    binding: &Binding,
    inventory: &Inventory,
    guard: &dyn signout_core::HeldOperation,
    browser: &dyn BrowserControl,
    cancel: &AtomicBool,
    mutating: &dyn Fn() -> Result<()>,
    validate_root: &dyn Fn() -> Result<()>,
) -> Outcome {
    let result = (|| {
        validate_root()?;
        let current = inventory.metadata(&binding.selection.profile.folder)?;
        if current.primary_gaia_id != binding.selection.profile.primary_gaia_id
            || !current
                .accounts
                .iter()
                .any(|a| a == &binding.selection.account && !a.is_primary)
        {
            return Err(Error::Identity);
        }
        let was_running = browser.was_running()?;
        let operation = (|| {
            browser.close(cancel)?;
            mutating()?;
            let stopped = || {
                validate_root()?;
                browser.stopped()
            };
            signout_core::remove(inventory, &binding.selection, guard, &stopped, &|| Ok(()))
        })();
        match operation {
            Ok(removal) => {
                let reopened = browser.reopen(&binding.selection.profile.folder).is_ok();
                let mut outcome = Outcome::removed(reopened);
                if removal == Removal::AlreadyRemoved {
                    outcome.message = format!(
                        "Tài khoản đã được gỡ trước đó. {}",
                        if reopened {
                            "Kiểm tra Chrome để xác nhận."
                        } else {
                            "Hãy tự mở Chrome và kiểm tra."
                        }
                    );
                }
                Ok(outcome)
            }
            Err(e) => {
                let mut outcome = Outcome::failed(e);
                if was_running && browser.reopen(&binding.selection.profile.folder).is_err() {
                    outcome.message.push_str(" Hãy tự mở lại Chrome.");
                }
                Ok(outcome)
            }
        }
    })();
    result.unwrap_or_else(Outcome::failed)
}
pub struct NativeRuntime;
impl Runtime for NativeRuntime {
    fn load(&self) -> Result<Loaded> {
        let root = chrome_platform::canonical(&chrome_platform::standard_root()?)?;
        chrome_platform::discover(&root)?;
        Ok(Loaded {
            profiles: Inventory::new(&root)?.load_metadata()?,
            root,
        })
    }
    fn run(
        &self,
        binding: &Binding,
        cancel: &AtomicBool,
        mutating: &dyn Fn() -> Result<()>,
    ) -> Outcome {
        let result = (|| {
            let guard =
                chrome_platform::NativeLock::new(chrome_platform::lock_path()?).acquire()?;
            let validate_root = || {
                if chrome_platform::canonical(&chrome_platform::standard_root()?)? == binding.root {
                    Ok(())
                } else {
                    Err(Error::Identity)
                }
            };
            validate_root()?;
            let inventory = Inventory::new(&binding.root)?;
            let browser = NativeBrowser(Mutex::new(chrome_platform::Chrome::new(
                binding.root.clone(),
            )?));
            Ok(perform(
                binding,
                &inventory,
                guard.as_ref(),
                &browser,
                cancel,
                mutating,
                &validate_root,
            ))
        })();
        result.unwrap_or_else(Outcome::failed)
    }
}
