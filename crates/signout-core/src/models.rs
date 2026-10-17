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
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
 Self::Metadata=>"Không đọc được hồ sơ Chrome. Hãy mở Chrome một lần rồi tải lại.",
 Self::Identity=>"Danh tính tài khoản đã thay đổi hoặc đây là tài khoản đồng bộ chính. Hãy tải lại.",
 Self::Path=>"Đường dẫn dữ liệu không an toàn hoặc không truy cập được.",
 Self::Schema=>"Định dạng dữ liệu Chrome này chưa được hỗ trợ.",
 Self::Storage=>"Không truy cập được cơ sở dữ liệu Chrome. Hãy đóng Chrome rồi thử lại.",
 Self::Running=>"Chrome vẫn đang chạy; chưa thay đổi tài khoản. Hãy đóng Chrome thủ công rồi thử lại.",
 Self::Busy=>"Một thao tác đăng xuất khác đang chạy. Hãy chờ hoàn tất.",
 Self::MissingChrome=>"Không tìm thấy Google Chrome stable. Hãy mở Chrome rồi tải lại.",
 Self::AmbiguousChrome=>"Có nhiều bản Chrome; không xác định được bản đang dùng hồ sơ. Hãy đóng Chrome rồi thử lại.",
 Self::Process=>"Không xác định được tiến trình Chrome. Hãy đóng Chrome thủ công rồi thử lại.",
 Self::Cancelled=>"Đã hủy thao tác; chưa gỡ tài khoản."
 })
    }
}
impl std::error::Error for Error {}
