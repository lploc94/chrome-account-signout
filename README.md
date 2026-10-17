# Chrome Account Signout

Ứng dụng desktop nhỏ để gỡ một tài khoản Google phụ khỏi Chrome trên máy của bạn.

Danh sách tài khoản → **Log out** bên cạnh tài khoản → xác nhận → kết quả.
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
Danh sách lấy từ thông tin hồ sơ nên vẫn hiển thị khi Chrome đang mở; có thể còn
mục đã đăng xuất. App chỉ yêu cầu Chrome đóng sau khi bạn xác nhận Logout, rồi
kiểm tra lại danh tính và dữ liệu trước khi gỡ. Nếu Chrome vẫn giữ khóa sau đó,
app từ chối sửa và hướng dẫn đóng Chrome thủ công.
Nếu hệ điều hành chặn đọc dữ liệu, người dùng tự quyết định cấp quyền.

Mọi dữ liệu ví dụ/test đều được tạo giả, chẳng hạn `friend@example.test`.

## Phát triển

Tauri 2, Rust và TypeScript/CSS. Phần xử lý tài khoản dùng chung; tích hợp hệ điều hành
tách riêng. Bản Swift cũ không phải phụ thuộc của ứng dụng này.

Không có telemetry hoặc tự động cập nhật.

Chrome được tìm qua đăng ký ứng dụng/tiến trình đang chạy trên macOS và App Paths/tiến
trình đã xác minh trên Windows. Nếu có nhiều bản không xác định được chủ profile, ứng
dụng từ chối thao tác. Tiến trình báo lỗi Crashpad trên Mac được ghép với đúng thư mục
Crashpad; không đóng các Chrome có user-data-dir riêng. Nếu Chrome chạy nền hoặc từ
chối đóng, hãy lưu công việc và đóng Chrome thủ công rồi thử lại.

## Build và kiểm thử

Cài Rust, Node.js và [phụ thuộc Tauri](https://v2.tauri.app/start/prerequisites/).
Windows cần MSVC Build Tools; macOS cần Xcode Command Line Tools.

```sh
npm ci
npm run check
npm test -- --run
npm run build
cargo test --workspace --locked
npm run tauri dev
```

Build giao diện trước các lệnh Cargo biên dịch app. Giao diện được build vào
`ui-dist/`; `dist/` chỉ dành cho gói phát hành. Không đưa dữ liệu Chrome thật vào test.
Xác nhận được xử lý ở Rust: token xác nhận chỉ dùng một lần, tài khoản được kiểm tra
lại trước khi ghi, và chỉ có một thao tác được chạy kể cả khi mở nhiều cửa sổ app.

## Phát hành

Gói macOS dùng chữ ký ad-hoc, chưa được Apple notarize. Gói Windows chưa có chữ ký
nhà phát hành; Windows vẫn là bản thử nghiệm cho đến khi có kiểm tra đăng xuất thực tế.
Installer Windows cài cho người dùng hiện tại và cần Microsoft Edge WebView2 Runtime.
Không tự cấp quyền đọc dữ liệu hoặc thay đổi thiết lập bảo mật của hệ điều hành.

Gói phát hành phải qua kiểm tra nội dung riêng tư, SHA-256 và kiểm tra native trước
khi upload. Manifest ghi commit nguồn và hash của từng gói/binary. Công cụ kiểm tra
payload Windows cần `7zz` hoặc `7z`; không giải nén được thì kiểm tra thất bại.
Không đưa PDB, source map, dữ liệu trình duyệt hoặc ảnh tài khoản vào bản phát hành.

Danh sách hash ảnh được duyệt gồm icon của ứng dụng và ảnh nền wizard mặc định
`Contrib/Graphics/Wizard/win.bmp` từ [NSIS 3.13 do Tauri phân phối](https://github.com/tauri-apps/binary-releases/releases/tag/nsis-3.13).
Ảnh NSIS được đối chiếu từng byte với gói chính thức và kiểm tra trực quan;
chỉ đúng hash này được chấp nhận, các ảnh chưa được duyệt vẫn bị chặn.

Build phát hành với đường dẫn nguồn đã chuẩn hóa:

```sh
python3 scripts/build-release.py --target universal-apple-darwin --bundles app
python3 scripts/package-release.py --platform macos
python3 scripts/check-public-content.py --source --history --artifacts dist
python3 scripts/verify-release.py --dir dist
```

Trên Windows dùng `python scripts/build-release.py --bundles nsis`.
Công cụ kiểm tra bản cài Windows giữ cửa sổ app để quan sát. Chỉ chạy lại với
`-GuiConfirmed -Cleanup` sau khi đã nhìn thấy danh sách hoặc thông báo lỗi có hướng
dẫn; tùy chọn này xác nhận kiểm tra thủ công, không tự chứng minh GUI hoạt động.

Các thông báo giấy phép thư viện được giữ trong `THIRD-PARTY-NOTICES.txt` và
được đóng gói cùng app. Danh mục có cả thư viện build và thư viện của hệ khác;
đây không phải giấy phép áp dụng cho mã nguồn riêng của dự án.

Giao diện ứng dụng và thông báo kết quả/lỗi sử dụng tiếng Anh.
