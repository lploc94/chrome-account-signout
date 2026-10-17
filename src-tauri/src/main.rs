#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    chrome_account_signout_lib::run()
}
