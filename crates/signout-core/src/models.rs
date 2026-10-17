use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub gaia_id: String,
    pub email: String,
    pub name: String,
    pub is_primary: bool,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Profile {
    pub folder: String,
    pub name: String,
    pub primary_gaia_id: String,
    pub accounts: Vec<Account>,
    pub unavailable_reason: Option<String>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Selection {
    pub profile: Profile,
    pub account: Account,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Removal {
    Removed,
    AlreadyRemoved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Metadata,
    Identity,
    Path,
    Schema,
    Storage,
    Running,
    Busy,
    MissingChrome,
    AmbiguousChrome,
    Process,
    Cancelled,
    OutcomeUnknown,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
 Self::Metadata=>"Cannot read Chrome profiles. Check Chrome profile setup and this app's access to Chrome data, then refresh.",
 Self::Identity=>"The account identity changed or this is the primary sync account. Refresh and try again.",
 Self::Path=>"The Chrome data path is unsafe or unavailable.",
 Self::Schema=>"This Chrome data format is not supported.",
 Self::Storage=>"Cannot access Chrome's database. Close Chrome and try again.",
 Self::Running=>"Chrome is still running. No account was changed. Close Chrome manually and try again.",
 Self::Busy=>"Another logout operation is running. Wait for it to finish.",
 Self::MissingChrome=>"Google Chrome stable was not found. Open Chrome and refresh.",
 Self::AmbiguousChrome=>"Multiple Chrome installations found. Cannot identify the one using this profile. Close Chrome and try again.",
 Self::Process=>"Cannot identify Chrome processes. Close Chrome manually and try again.",
 Self::OutcomeUnknown=>"Could not confirm whether the data change was saved. Refresh and check Chrome.",
 Self::Cancelled=>"Cancelled. No account was removed."
 })
    }
}
impl std::error::Error for Error {}
