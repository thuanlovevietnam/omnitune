# ⚡ OmniTune - Universal System & Hardware Optimizer

Ứng dụng tối ưu hóa hệ thống, phần cứng và thời lượng pin chuyên sâu cho Windows (PC & Laptop), được viết 100% bằng **Rust** kết hợp Win32 API trực tiếp và GUI `egui/eframe`.

---

## 🚀 Tính năng nổi bật

### 1. 📊 Chỉ số Sức khỏe Hệ thống (PC Health Score) & Tối ưu 1-Click
- **Đánh giá điểm số 0 - 100 điểm:** Tổng hợp tình trạng RAM, dung lượng rác SSD, độ chai pin, dịch vụ Telemetry và ứng dụng khởi động.
- **Nút "⚡ TỐI ƯU TOÀN DIỆN" (1-Click):**
  - Tự động dọn sạch rác an toàn Tầng 1.
  - Thu hồi Working Set RAM toàn hệ thống.
  - Áp dụng chế độ tiết kiệm điện EcoQoS cho các tiến trình nền.
  - Tái chấm điểm nâng sức khỏe máy tính lên mức **Tối ưu 95 - 100/100**.

---

### 2. 🎯 Tối ưu theo Kịch bản thực tế (Smart Scenarios) & Foreground EcoQoS
Tự động tinh chỉnh CPU, GPU, bộ nhớ RAM, mạng và quyền ưu tiên tiến trình theo đúng tác vụ bạn đang làm:

- **Ứng dụng đang Focus (Foreground Auto-Boost):** Tự động phát hiện cửa sổ đang tương tác qua Win32 `GetForegroundWindow`, gỡ bỏ trần xung nhịp và nâng quyền ưu tiên luồng xử lý.
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

### 3. 🧹 Bộ Dọn dẹp Rác 3 Tầng (3-Tier Cleaner)
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

### 4. 🧠 Tối ưu bộ nhớ RAM & Giám sát Tiến trình (Process Inspector)
- **Thu hồi Working Set nhàn rỗi:** Gọi `K32EmptyWorkingSet` đo lường dung lượng RAM giải phóng chính xác theo thời gian thực qua `K32GetProcessMemoryInfo`.
- **Tự động dọn khi RAM > 85%:** Bảo vệ hệ thống khỏi tình trạng treo đơ do cạn bộ nhớ.
- **Bộ lọc tìm kiếm tiến trình (Process Search Bar):** Tìm kiếm theo tên hoặc PID.
- **Hành động trực tiếp trên từng tiến trình:**
  - `⚡ Dọn`: Thu hồi Working Set riêng của tiến trình đó.
  - `🍃 Eco`: Bật chế độ EcoQoS tiết kiệm điện cho tiến trình.
  - `❌`: Kết thúc an toàn tiến trình (`TerminateProcess`).

---

### 5. 🔋 Quản lý Pin & Cảm biến ACPI thời gian thực (Hardware Power Control)
- **Đo dòng xả/nạp thời gian thực (Discharge/Charge Rate in mW):** Tính toán chính xác công suất Watt máy đang tiêu thụ (ví dụ: `⚡ 7.8 W`).
- **Đo điện áp thời gian thực (Voltage in V):** Đọc cảm biến ACPI từ bo mạch chủ.
- **Hỗ trợ Pin kép (Dual Battery):** Theo dõi chi tiết cả 2 pack pin (dung lượng thiết kế, dung lượng nạp đầy, số lần chu kỳ sạc, % chai pin).
- **Ngưỡng sạc pin 80% (Battery Charge Threshold):** Bật/tắt chế độ ngắt sạc ở 80% để chống phù pin khi cắm sạc liên tục tại bàn làm việc.

---

### 6. 🚀 Quản lý Khởi động & Bảo vệ Máy (Startup & Telemetry Debloat)
- **Bật / Tắt Ứng dụng Khởi động 1-Click:** Chuẩn cơ chế `StartupApproved` của Windows Task Manager.
- **Windows Telemetry Debloat:** 1-Click vô hiệu hóa dịch vụ thu thập dữ liệu ngầm `DiagTrack` của Windows, giảm tải CPU và ổ đĩa nền.
- **S.M.A.R.T NVMe SSD Health:** Đọc tình trạng sức khỏe phần cứng ổ đĩa (`Healthy / OK`).
- **Chế độ ngủ đông (Hibernate Protection):** Quản lý trạng thái ngắt điện hoàn toàn khi gập máy.

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
