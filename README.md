# ⚡ OmniTune - Universal System & Hardware Optimizer

Ứng dụng tối ưu hóa hệ thống, phần cứng và thời lượng pin chuyên sâu cho Windows (PC & Laptop), được viết 100% bằng **Rust** kết hợp Win32 API trực tiếp và GUI `egui/eframe`.

---

## 🚀 Tính năng nổi bật

### 1. 🎯 Tối ưu theo Kịch bản thực tế (Smart Scenarios) & Foreground EcoQoS
Tự động tinh chỉnh CPU, GPU, bộ nhớ RAM, mạng và quyền ưu tiên tiến trình theo đúng tác vụ bạn đang làm:

- **Ứng dụng đang Focus (Foreground Auto-Boost):** Tự động phát hiện cửa sổ đang tương tác, gỡ bỏ trần xung nhịp và nâng quyền ưu tiên luồng xử lý.
- **EcoQoS (Windows Efficiency Mode):** Tự động áp dụng cờ điều tiết tiết kiệm điện `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` cho các tiến trình chạy ngầm (Google Drive, OneDrive, tab trình duyệt nền, Search Indexer).
- **🖥️ Điều khiển từ xa (Remote Desktop):**
  - Khóa chống Sleep & Lock màn hình (`SetThreadExecutionState`).
  - Khóa CPU 65% máy mát 40°C, quạt êm ru 0dB, thời lượng pin kéo dài 6 - 8 tiếng.
  - Tối ưu TCP buffer qua `netsh` giảm trễ chuột cho **MiniRemote**, **RustDesk**, **RDP**.
  - Tự động đóng băng các ứng dụng sync ngầm để nhường trọn băng thông điều khiển.
- **🎮 Chơi Game & Đồ họa (Gaming Mode):**
  - Windows Timer Resolution **1.0ms** (`timeBeginPeriod(1)`) triệt tiêu hiện tượng khựng khung hình (micro-stuttering) và trễ chuột.
  - Tắt ngầm Windows GameDVR background capture (`AppCaptureEnabled = 0`) để giải phóng toàn bộ năng lực GPU.
  - Cân bằng xung nhịp CPU 90% nhường trọn 12W điện năng cho GPU đạt đỉnh xung nhịp mà không bị hãm nhiệt.
  - Thu hồi Working Set RAM để mở rộng dung lượng Shared VRAM.
- **☕ Cà phê & Di động (Cafe / Siêu tiết kiệm pin):**
  - Khóa trần CPU 60% chỉ tiêu thụ 3W - 5W điện năng.
  - Tản nhiệt thụ động (Passive cooling - tắt quạt làm mát).
  - Tự động đặt độ sáng 45% và đưa các dịch vụ nền xuống trạng thái nhàn rỗi.
- **💻 Lập trình & Biên dịch (Developer):**
  - Mở tối đa 100% Turbo Boost toàn bộ nhân/luồng.
  - Quạt tản nhiệt quay chủ động để đẩy nhanh nhiệt khi biên dịch nặng.
  - Tự động phát hiện và nâng quyền ưu tiên của `rustc`, `cargo`, `go`, `node` lên `HIGH_PRIORITY_CLASS`.
  - Dọn sạch Working Set RAM để compiler đọc ghi file SSD NVMe nhanh nhất.
- **🎬 Xem phim & Thư giãn (Media & Chill):**
  - Khóa chống tắt màn hình và ngăn sleep khi đang xem video.
  - CPU 55% im lặng tuyệt đối 0dB, nhường phần cứng GPU giải mã video 1080p/4K mượt mà.
- **🤖 Tự động nhận diện thông minh (Smart Auto-Detect):**
  - Tự động chuyển kịch bản tương ứng khi phát hiện mở MiniRemote/RustDesk, chạy lệnh compile hoặc khi pin sắp cạn.

---

### 2. 🧹 Bộ Dọn dẹp Rác 3 Tầng (3-Tier Cleaner)
- **🛡️ Tầng 1: Rác An toàn 100% (Safe System & Browser Junk):**
  - User Temp (`%TEMP%`), System Temp (`C:\Windows\Temp`), Thùng rác (`SHQueryRecycleBinW`/`SHEmptyRecycleBinW`), Crash Dumps, Browser Cache (Edge/Chrome).
- **💻 Tầng 2: Rác Phát triển Phần mềm (Developer & Build Cache):**
  - Node.js / NPM Cache (`%LOCALAPPDATA%\npm-cache` - dung lượng hàng Gigabyte).
  - Go Build & Module Cache (`%LOCALAPPDATA%\go-build`).
  - Cargo Crates Download Cache (`%USERPROFILE%\.cargo\registry\cache`).
  - Python Pip Cache (`%LOCALAPPDATA%\pip\cache`).
- **⚙️ Tầng 3: Bảo trì Hệ thống Sâu (System Maintenance):**
  - Windows Update Download Cache (`SoftwareDistribution\Download`).
  - Windows Prefetch (`C:\Windows\Prefetch`).

---

### 3. 🧠 Tối ưu bộ nhớ RAM (Memory Booster)
- Thu hồi Working Set nhàn rỗi (`K32EmptyWorkingSet`) từ các tiến trình đang chạy.
- Đo lường dung lượng RAM giải phóng chính xác theo thời gian thực qua `K32GetProcessMemoryInfo`.
- Hiển thị bảng xếp hạng Top ứng dụng chiếm dụng RAM nhiều nhất.

---

### 4. 🔋 Quản lý Pin & Cảm biến ACPI thời gian thực (Hardware Power Control)
- **Đo dòng xả/nạp thời gian thực (Discharge/Charge Rate in mW):** Tính toán chính xác công suất Watt máy đang tiêu thụ (ví dụ: `⚡ 7.8 W`).
- **Đo điện áp thời gian thực (Voltage in V):** Đọc cảm biến ACPI từ bo mạch chủ.
- **Hỗ trợ Pin kép (Dual Battery):** Theo dõi chi tiết cả 2 pack pin (dung lượng thiết kế, dung lượng nạp đầy, số lần chu kỳ sạc, % chai pin).
- **Điều khiển trần CPU & Độ sáng:** Tương tác WMI `WmiSetBrightness` và Power Policy.

---

### 5. 🚀 Quản lý Khởi động & Bảo vệ Máy (Startup & Telemetry Debloat)
- **Windows Telemetry Debloat:** 1-Click vô hiệu hóa dịch vụ thu thập dữ liệu ngầm `DiagTrack` của Windows, giảm tải CPU và ổ đĩa nền.
- **Startup Applications Inspector:** Liệt kê toàn bộ ứng dụng khởi động từ Registry (HKCU/HKLM) và Startup Folders.
- **Chế độ ngủ đông (Hibernate Protection):** Quản lý trạng thái ngắt điện hoàn toàn khi gập máy.
- **Bảo vệ ổ cứng SSD NVMe:** Đo lường dung lượng trống và cảnh báo đầy bộ nhớ.
- **Khuyến nghị ngưỡng sạc 80% (Battery Charge Threshold):** Hướng dẫn chống chai phồng pin khi cắm sạc liên tục.

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
