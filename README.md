# Claude Desktop Manager

Claude Desktop Manager là ứng dụng macOS giúp bạn chạy nhiều Claude Desktop cùng lúc, mỗi bản một tài khoản/profile riêng biệt, gom chúng vào nhóm cho dễ quản lý, theo dõi giới hạn sử dụng và tự cập nhật phiên bản mới.

![Tổng quan tab Profiles trong Claude Desktop Manager](docs/images/overview.png)

## Mục lục

1. [Cài đặt](#1-cài-đặt)
2. [Hướng dẫn sử dụng](#2-hướng-dẫn-sử-dụng)
   1. [Thêm profile](#21-thêm-profile)
   2. [Định vị Claude Desktop](#22-định-vị-claude-desktop)
   3. [Nhóm profile](#23-nhóm-profile)
   4. [Khởi chạy](#24-khởi-chạy)
   5. [Cập nhật](#25-cập-nhật)
3. [Đồng bộ session giữa các profile](#3-đồng-bộ-session-giữa-các-profile)
   1. [Bật đồng bộ](#31-bật-đồng-bộ)
   2. [Tắt đồng bộ](#32-tắt-đồng-bộ)
   3. [Lưu ý](#33-lưu-ý)
4. [Cấu hình MCP (tuỳ chọn)](#4-cấu-hình-mcp-tuỳ-chọn)

---

## 1. Cài đặt

1. Mở trang [Releases](../../releases/latest) của repo này, tải file `.dmg` đúng chip máy bạn: bản `aarch64` cho Apple Silicon (M1/M2/M3…), bản `x64` cho Intel.
2. Mở file `.dmg` vừa tải, kéo **Claude Desktop Manager** vào thư mục Applications.
3. Mở ứng dụng từ Applications (hoặc Spotlight).

Nếu macOS báo "Claude Desktop Manager" is damaged and can't be opened, chạy lệnh sau trong Terminal rồi mở lại app:

```bash
xattr -dr com.apple.quarantine "/Applications/Claude Desktop Manager.app"
```

Nếu vẫn không mở được, chạy thêm:

```bash
codesign --force --deep --sign - "/Applications/Claude Desktop Manager.app"
```

---

## 2. Hướng dẫn sử dụng

Sau khi mở, Claude Desktop Manager nằm ở thanh menu bar, không có icon trên Dock. Cửa sổ Preferences có ba tab: "Profiles", "Updates" và "General" — các mục dưới đây đều nằm trong ba tab đó.

### 2.1. Thêm profile

Có hai cách thêm một profile: tạo mới, hoặc "nhận" (adopt) một thư mục Claude Desktop có sẵn.

**Tạo mới:**

1. Ở tab "Profiles", nhấn nút "New Profile" (dấu +) trên toolbar — hoặc nhấn "New Profile" ngay giữa màn hình nếu bạn chưa có profile nào.
2. Nhập tên vào ô "Name" (ví dụ `Work`), nhấn "Create".
3. Mở profile vừa tạo và đăng nhập Claude ngay lần đầu chạy — bạn chưa cần đăng nhập ở bước tạo.

![Hộp thoại tạo profile mới với tên Work](docs/images/add-profile.png)

**Nhận thư mục có sẵn:** nếu Claude Desktop Manager phát hiện thư mục nào đó trông giống một profile Claude Desktop chưa được quản lý, một banner sẽ hiện, ví dụ "2 folders here look like Claude profiles.", kèm nút "Review…" — hoặc bạn tự mở bằng nút "…" (More Actions) → "Add Existing Folder…".

1. Trong hộp thoại "Add Existing Profiles", các thư mục tìm thấy được tick sẵn — bỏ tick thư mục nào bạn không muốn thêm.
2. Sửa lại tên ở ô "Name" cạnh mỗi thư mục nếu muốn.
3. Nhấn "Add Profile" (hoặc "Add N Profiles" nếu chọn nhiều thư mục).

Thao tác này chỉ thêm một file đánh dấu nhỏ vào thư mục — không có gì bị di chuyển hay thay đổi.

![Danh sách thư mục có thể nhận vào làm profile](docs/images/adopt-profile.png)

Ghi chú: mỗi profile có một file cấu hình MCP riêng cho Claude Desktop của nó. Nhấn chuột phải vào profile → "Edit MCP Config…" để mở file này bằng trình soạn thảo mặc định của bạn.

### 2.2. Định vị Claude Desktop

Nếu Claude Desktop Manager không tìm thấy Claude Desktop đã cài trên máy (thường gặp khi bạn thử chạy một profile lần đầu), hộp thoại "Can't find Claude Desktop." sẽ hiện ra với hai lựa chọn:

- "Locate Claude Desktop…" — mở hộp thoại chọn file, bắt đầu tại thư mục Applications, để bạn tự chỉ đến app Claude đã cài.
- "Get Claude Desktop" — mở trang tải Claude Desktop trong trình duyệt.

![Hộp thoại không tìm thấy Claude Desktop với hai nút định vị và tải về](docs/images/locate-binary.png)

### 2.3. Nhóm profile

- **Tạo nhóm:** nhấn "…" (More Actions) trên toolbar → "New Group…" → nhập "Name" (ví dụ `Work`) → "Create".
- **Đổi icon nhóm:** nhấn chuột phải vào tên nhóm → "Choose Icon…" → chọn tab "Emoji" hoặc "Icons", có thể gõ vào ô "Search icons" để tìm nhanh, nhấn vào một icon để chọn ngay hoặc nhấn "Remove Icon" để bỏ icon hiện tại.
- **Đổi tên / xoá nhóm:** cũng từ menu chuột phải trên nhóm — "Rename Group…" hoặc "Delete Group…" (xoá nhóm không xoá các profile bên trong, chúng quay về mục "Ungrouped").
- **Di chuyển profile vào nhóm:** kéo tay cầm bên phải mỗi dòng profile để sắp xếp lại thứ tự hoặc thả sang nhóm khác; hoặc nhấn chuột phải vào profile → "Assign to Group…" → chọn nhóm (hoặc "No group") → "Assign".

![Bộ chọn icon đang mở cho một nhóm profile](docs/images/groups.png)

### 2.4. Khởi chạy

1. Chọn một profile trong danh sách bên trái.
2. Nhấn nút "Launch" lớn trong khung chi tiết bên phải (hoặc double-click vào dòng profile, hoặc nhấn chuột phải → "Launch"). Nút tạm thời đổi thành "Launching…".
3. Khi đang chạy, trạng thái "Running" hiện trên cả dòng profile lẫn khung chi tiết.
4. Bạn có thể chạy nhiều profile — tức nhiều tài khoản Claude — cùng lúc; mỗi profile là một tiến trình Claude Desktop riêng, dữ liệu tách biệt hoàn toàn, không đụng nhau (trừ khi bạn chủ động bật [đồng bộ session](#3-đồng-bộ-session-giữa-các-profile)).
5. Muốn dừng một profile, bạn thoát cửa sổ Claude Desktop của nó như một app bình thường (⌘Q). Claude Desktop Manager không có nút thoát riêng — chỉ khi bạn đổi tên hoặc xoá một profile đang chạy, app mới tự yêu cầu thoát trước: nút xác nhận đổi thành "Quit & Rename" / "Quit & Delete". Nếu Claude không chịu thoát, hộp thoại báo "isn't quitting" hiện ra kèm nút "Force Quit".

![Một profile đang chạy với trạng thái Running](docs/images/launch.png)

### 2.5. Cập nhật

1. Mở tab "Updates" trong Preferences.
2. Claude Desktop Manager tự kiểm tra bản mới theo định kỳ; bạn cũng có thể nhấn "Check for Updates" để kiểm tra ngay.
3. Nếu có bản mới, dòng "Version X is available." hiện ra kèm nút "Update" — nhấn để tải và cài.
4. Sau khi cài xong, dòng "Version X is installed. Restart Claude Desktop Manager to start using it." hiện ra kèm nút "Restart Now". Các profile đang chạy không bị ảnh hưởng — bạn cũng có thể bỏ qua và để bản mới tự áp dụng ở lần mở app kế tiếp.
5. Nếu không dùng luồng cập nhật trong app, bạn luôn có thể vào thẳng trang [Releases](../../releases/latest), tải bản `.dmg` mới nhất và cài đè lên bản cũ như ở mục [Cài đặt](#1-cài-đặt).

![Thông báo có bản cập nhật mới kèm nút Update](docs/images/update.png)

---

## 3. Đồng bộ session giữa các profile

Bình thường mỗi profile giữ lịch sử session/chat Claude Code của riêng nó. Bật đồng bộ sẽ đưa profile vào một kho session chung: mọi profile đã bật cùng nhìn thấy một danh sách session — tạo ở profile này thì mở được ở profile kia. Đăng nhập tài khoản và cấu hình MCP của từng profile vẫn riêng, không bị chia sẻ.

### 3.1. Bật đồng bộ

1. Thoát Claude Desktop của profile đó trước (⌘Q) — không thể bật/tắt đồng bộ khi profile đang chạy.
2. Chọn profile trong danh sách bên trái, bật công tắc "Sync Sessions Across Profiles" trong khung chi tiết (dòng gợi ý bên dưới: "Share logins and chats with other profiles") — hoặc nhấn chuột phải vào profile → "Sync Sessions Across Profiles".
3. Session sẵn có của profile được gộp vào kho chung: lấy hợp của hai bên, file nào trùng đường dẫn thì bản mới hơn thắng, không có gì bị xoá. Khi xong, khung chi tiết hiện chữ "Synced" và dòng profile có thêm nhãn "Sync".

Nếu một số thư mục tài khoản của profile đã được liên kết đi nơi khác từ trước, hộp thoại "Some account folders were already linked elsewhere and were skipped:" sẽ liệt kê các thư mục bị bỏ qua — những tài khoản đó không tham gia đồng bộ.

### 3.2. Tắt đồng bộ

Tắt bằng chính công tắc đó trong khung chi tiết, hoặc chuột phải → "✓ Sync Sessions Across Profiles". Profile giữ lại một bản chụp toàn bộ kho chung tại thời điểm rời — gồm cả session các profile khác đã thêm vào trong lúc đồng bộ — chứ không quay về đúng bộ session ban đầu. Sau khi rời, profile tách biệt hoàn toàn trở lại; thay đổi sau đó của các profile còn lại không ảnh hưởng đến nó nữa.

### 3.3. Lưu ý

- Phải thoát profile trước khi bật/tắt — nếu profile đang chạy, thao tác thất bại với thông báo "Couldn't join session sync." / "Couldn't leave session sync.".
- Kho chung là MỘT danh sách gộp cho mọi thành viên: hai profile đăng nhập hai tài khoản Claude khác nhau mà cùng bật đồng bộ sẽ thấy lịch sử chat của nhau.
- Xoá một session ở profile này thì các profile thành viên khác cũng mất session đó (có thể cần mở lại Claude Desktop của chúng mới thấy).
- Profile đang chạy chưa nhận ngay thay đổi từ kho chung — mỗi profile tự đối soát lại với kho ở lần khởi chạy kế tiếp, không cần thao tác gì thêm.
- Đừng xoá thư mục `session-pool` trong dữ liệu của Claude Desktop Manager: các profile đang đồng bộ sẽ mất toàn bộ session chung và app không tự khôi phục được.

---

## 4. Cấu hình MCP (tuỳ chọn)

Claude Desktop Manager có một MCP debug server nội bộ, cho phép một MCP client (ví dụ Claude Code) đọc và điều khiển chính ứng dụng khi nó đang chạy. Mặc định server này TẮT.

1. Mở tab "General" trong Preferences, tìm mục "MCP debugging".
2. Bật công tắc "Run the MCP debug server".
3. Cổng mặc định là `20205`, có thể đổi ở ô "Port" ngay bên dưới nếu cổng đó đang bị chương trình khác chiếm.
4. Khi đã chạy, dòng "Listening on http://127.0.0.1:<port>/mcp" hiện ra kèm nút "Copy URL" để chép nhanh địa chỉ; bên dưới còn khung "Log" ghi lại các request gần nhất (nút "Clear" để xoá log).

Trỏ Claude Code (hoặc một MCP client khác hỗ trợ HTTP) vào server này bằng một entry dạng:

```json
{
  "mcpServers": {
    "cdm": {
      "type": "http",
      "url": "http://127.0.0.1:20205/mcp"
    }
  }
}
```

Nếu bạn tự đặt biến môi trường `CDM_MCP_PORT` khi mở app, giá trị đó sẽ đè lên công tắc và ô Port ở trên; tab "General" sẽ hiện thêm một dòng ghi chú giải thích điều này.

Ghi chú: đây là MCP server để điều khiển chính Claude Desktop Manager. Cấu hình MCP servers cho từng Claude Desktop bên trong mỗi profile lại là chuyện khác — mở qua chuột phải profile → "Edit MCP Config…" như ở mục [Thêm profile](#21-thêm-profile).

![Tab General với mục MCP debug server đang bật](docs/images/mcp.png)
