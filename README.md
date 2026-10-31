# ⚡ OmniTune - Autonomous System Assistant

Trợ lý tối ưu hóa hệ thống, phần cứng và thời lượng pin **tự hành thông minh (Zero-Click Autonomous Assistant)** cho Windows (PC & Laptop), được viết 100% bằng **Rust** kết hợp Win32 API trực tiếp và GUI `egui/eframe`.

---

## 🌟 Trải Nghiệm Mới: Widget Tự Hành Thông Minh (Zero-Click Autonomous Widget)

Thay vì buộc người dùng phải tự làm kỹ sư hệ thống với hàng chục tab và thanh trượt phức tạp, **OmniTune** chuyển đổi sang mô hình **Trợ lý tự hành**:

- **Kích thước Widget mini:** `440 x 560 px`, hiển thị gọn gàng ngay giữa màn hình.
- **Tự động nhận diện ngữ cảnh 24/7 (Auto-Pilot):**
  - Mở Game -> Tự kích hoạt Timer 1.0ms, tắt GameDVR, cấp 100% GPU, dọn VRAM.
  - Mở Remote Desktop (`MiniRemote`, `RustDesk`, `RDP`) -> Tự khóa sleep, hãm CPU 65% mát 40°C, quạt êm 0dB, đóng băng sync để giữ chuột nhạy 0ms.
  - Rút sạc -> Tự hạ xung 65% (~4W), tắt quạt tản nhiệt, hạ độ sáng 45% (kéo dài pin 6 - 8 tiếng).
  - Cắm sạc -> Tự bung 100% Turbo Boost và khôi phục hiệu năng tối đa.
  - Compile code -> Tự cấp quyền ưu tiên cao cho `cargo`, `rustc`, `go`.
- **Nhật ký tự hành thời gian thực (Live Autonomous Stream):** Hiển thị minh bạch từng hành động ứng dụng vừa tự động tối ưu ngầm dưới máy.
- **Bảng điều khiển chuyên sâu [⚙️ Tinh chỉnh]:** Mở ra ma trận 10 tiêu chí phần cứng, dọn rác 3 tầng, giám sát tiến trình và cảm biến pin khi cần can thiệp sâu.

---

## 🚀 Tính năng cốt lõi

### 1. 🎯 Trung Tâm Sức Khỏe & Auto-Pilot
- **PC Health Score (0 - 100):** Tự động phát hiện dung lượng rác SSD, áp lực RAM, trạng thái Telemetry để đưa ra khuyến nghị tức thì.
- **Nút "⚡ TỐI ƯU HÓA TỨC THÌ (1-CLICK)":** Dọn rác an toàn Tầng 1 + Thu hồi RAM nhàn rỗi + Ép EcoQoS app ngầm.
- **3 Chip Vitals Tối giản:**
  - `CPU`: % tải thực tế • Model chip • Số nhân/luồng.
  - `RAM`: % tải • Đã dùng / Tổng • Dung lượng trống.
  - `PIN`: % pin • Công suất tiêu thụ thực tế theo **Watt** • Điện áp **V**.
- **Cửa sổ Focus Auto-Boost & EcoQoS:** Nhận diện cửa sổ đang làm việc để nâng luồng ưu tiên, tự động ép các ứng dụng nền (Chrome/Drive/OneDrive) vào cờ tiết kiệm điện `PROCESS_POWER_THROTTLING_EXECUTION_SPEED`.

### 2. 🎛️ Ma Trận Tinh Chỉnh Chuyên Sâu [⚙️]
- **Bảng 2 cột:** Cột 1 Tiêu chí (135px) • Cột 2 Điều khiển trực tiếp & Kiến nghị tối ưu:
  - Xung CPU & TDP (DC Pin / AC Sạc).
  - Độ trễ Timer 1.0ms vs 15.6ms.
  - Độ sáng màn hình qua WMI ACPI.
  - Chính sách quạt tản nhiệt (Passive 0dB vs Active).
  - Tắt GameDVR nền & Khóa chống Sleep màn hình.
  - Ngưỡng sạc pin 80% (Lenovo PM Service) & Chế độ ngủ đông Hibernate.

### 3. 🧹 Bộ Dọn Rác 3 Tầng & Process Inspector [⚙️]
- **3 Tầng Rác:** 🛡️ Tầng 1 Rác An toàn 100% • 💻 Tầng 2 Rác Developer (NPM Cache 3.3GB, Go Build, Cargo Cache) • ⚙️ Tầng 3 Bảo trì hệ thống.
- **Process Inspector:** Tìm kiếm tiến trình, thu hồi RAM riêng từng app, ép EcoQoS, hoặc kết thúc tiến trình bị treo.

### 4. 🛡️ Khởi Động & Bảo Vệ Phần Cứng [⚙️]
- S.M.A.R.T NVMe SSD Health (`Healthy / OK`).
- Cảm biến ACPI Pin Kép ThinkPad (Dung lượng thiết kế, dung lượng nạp đầy, số lần chu kỳ sạc, % chai pin).
- Tắt dịch vụ theo dõi ngầm `DiagTrack` của Windows.
- Bật / Tắt ứng dụng khởi động cùng Windows 1-Click (`StartupApproved`).

---

## 💎 Mô hình Bản quyền & Kiếm tiền (Monetization)
OmniTune được thiết kế theo mô hình **Freemium Pro ($9 Trọn đời)**:
- **Bản Miễn phí:** Auto-Pilot tự nhận diện, dọn rác an toàn Tầng 1, theo dõi pin và CPU.
- **Bản Pro VIP:** Mở khóa dọn rác Developer (NPM, Cargo, Go 3.6GB+), Timer 1.0ms tối ưu game & remote, khóa sạc pin 80%, không giới hạn.
- **Tạo Key cho khách hàng:** Sử dụng công cụ `generate_key.py`:
  ```bash
  python generate_key.py khachhang@gmail.com
  ```
- **Landing Page bán hàng:** Nằm tại thư mục `website/index.html` (sẵn sàng deploy lên GitHub Pages hoặc Vercel).

---

## 🛠️ Hướng dẫn cài đặt & Biên dịch

### Yêu cầu:
- Windows 10 / Windows 11 (64-bit)
- Rust toolchain (`rustc` & `cargo`)

### Biên dịch từ mã nguồn:
```bash
git clone https://github.com/thuanlovevietnam/omnitune.git
cd omnitune
cargo build --release
```

File thực thi độc lập sẽ nằm tại: `target/release/omnitune.exe`.

---

## 📄 Giấy phép Bản quyền (License)
Bản quyền thuộc về © 2026 **thuanlovevietnam**. Mọi quyền được bảo lưu (All Rights Reserved).  
Mã nguồn chỉ dùng cho mục đích cá nhân và đánh giá. Không sao chép, phân phối lại hoặc thương mại hóa khi chưa có sự đồng ý của tác giả.
