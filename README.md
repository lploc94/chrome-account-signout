# Chrome Account Signout

Ứng dụng desktop nhỏ để gỡ một tài khoản Google phụ khỏi Chrome trên máy của bạn.

Danh sách tài khoản → **Logout** bên cạnh tài khoản → xác nhận → kết quả.
Ứng dụng đóng Chrome một cách bình thường, gỡ đúng token cục bộ rồi mở lại profile.
Tài khoản đồng bộ chính được bảo vệ. Không có chức năng khôi phục.

Đây là thao tác dữ liệu cục bộ, không thu hồi phiên Google trên các thiết bị khác.
Chrome có thể thay đổi định dạng dữ liệu; ứng dụng từ chối thao tác nếu không xác định
được tài khoản, cấu trúc dữ liệu, đường dẫn hoặc tiến trình đang dùng profile.

## Hỗ trợ

- macOS 14 trở lên: Apple Silicon và Intel.
- Windows 10/11 x64: đang thử nghiệm, cần WebView2.
- Google Chrome stable, dữ liệu người dùng hiện tại tại vị trí mặc định; hỗ trợ thư mục
gốc được chuyển bằng liên kết. Không hỗ trợ chọn user-data-dir tùy ý, Beta/Canary,
Edge hoặc tài khoản đồng bộ chính.
- Không cần mật khẩu Google; không giải mã token; không gửi dữ liệu tài khoản ra mạng.
Nếu hệ điều hành chặn đọc dữ liệu, người dùng tự quyết định cấp quyền.

Mọi dữ liệu ví dụ/test đều được tạo giả, chẳng hạn `friend@example.test`.

## Phát triển

Tauri 2, Rust và TypeScript/CSS. Phần xử lý tài khoản dùng chung; tích hợp hệ điều hành
tách riêng. Bản Swift cũ không phải phụ thuộc của ứng dụng này.

Dự án đang được chuyển đổi; hướng dẫn build và gói tải xuống được hoàn thiện sau khi
kiểm tra native trên từng hệ điều hành. Không có telemetry hoặc tự động cập nhật.

Chrome được tìm qua đăng ký ứng dụng/tiến trình đang chạy trên macOS và App Paths/tiến
trình đã xác minh trên Windows. Nếu có nhiều bản không xác định được chủ profile, ứng
dụng từ chối thao tác. Tiến trình báo lỗi Crashpad trên Mac được ghép với đúng thư mục
Crashpad; không đóng các Chrome có user-data-dir riêng. Nếu Chrome chạy nền hoặc từ
chối đóng, hãy lưu công việc và đóng Chrome thủ công rồi thử lại.
