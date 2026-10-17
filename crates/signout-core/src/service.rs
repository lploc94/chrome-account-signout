use crate::{Error, Inventory, Removal, Result, Selection, token_store::TokenStore};
/// An owned live exclusive lock. Only trusted native code implements this interface;
/// there is no IPC allowing callers to provide a guard or a database path.
pub trait HeldOperation: Send {}
pub trait LockProvider: Send + Sync {
    fn acquire(&self) -> Result<Box<dyn HeldOperation>>;
}
pub fn remove(
    inventory: &Inventory,
    selection: &Selection,
    _guard: &dyn HeldOperation,
    stopped: &dyn Fn() -> Result<bool>,
    before_commit: &dyn Fn() -> Result<()>,
) -> Result<Removal> {
    let validate = || {
        if !stopped()? {
            return Err(Error::Running);
        }
        let current = inventory.metadata(&selection.profile.folder)?;
        if selection.account.is_primary
            || current.primary_gaia_id != selection.profile.primary_gaia_id
            || !current
                .accounts
                .iter()
                .any(|a| a == &selection.account && !a.is_primary)
        {
            return Err(Error::Identity);
        }
        inventory.safe_file(&selection.profile.folder, "Web Data")?;
        Ok(())
    };
    validate()?;
    TokenStore::open(
        &inventory.safe_file(&selection.profile.folder, "Web Data")?,
        true,
    )?
    .remove(
        &format!("AccountId-{}", selection.account.gaia_id),
        &validate,
        before_commit,
    )
}
