# ⚡ OmniTune - Universal System & Hardware Optimizer

Ứng dụng tối ưu hóa hệ thống, phần cứng và thời lượng pin chuyên sâu cho Windows (PC & Laptop), được viết 100% bằng **Rust** kết hợp Win32 API trực tiếp và GUI `egui/eframe`.

---

## 🚀 Tính năng nổi bật

### 1. 🎯 Tối ưu theo Kịch bản thực tế (Smart Scenarios)
Tự động tinh chỉnh CPU, GPU, bộ nhớ RAM, mạng và quyền ưu tiên tiến trình theo đúng tác vụ bạn đang làm:

- **🖥️ Điều khiển từ xa (Remote Desktop):**
  - Khóa chống Sleep & Lock màn hình (`SetThreadExecutionState`).
  - Khóa CPU 65% máy mát 40°C, quạt êm ru 0dB, thời lượng pin kéo dài 6 - 8 tiếng.
  - Tối ưu TCP buffer qua `netsh` giảm trễ chuột cho **MiniRemote**, **RustDesk**, **RDP**.
  - Tự động đóng băng các ứng dụng sync ngầm (Google Drive, OneDrive) để nhường trọn băng thông điều khiển.

- **🎮 Chơi Game & Đồ họa (Gaming Mode):**
  - Windows Timer Resolution **1.0ms** (`timeBeginPeriod(1)`) triệt tiêu hiện tượng khựng khung hình (micro-stuttering) và trễ chuột.
  - Tắt ngầm Windows GameDVR background capture (`AppCaptureEnabled = 0`) để giải phóng toàn bộ năng lực GPU.
  - Cân bằng xung nhịp CPU nhường điện năng cho GPU đạt đỉnh xung nhịp mà không bị hãm nhiệt.
  - Thu hồi Working Set RAM để mở rộng dung lượng Shared VRAM.

- **☕ Cà phê & Di động (Cafe / Siêu tiết kiệm pin):**
  - Khóa trần CPU 60% chỉ tiêu thụ 3W - 5W điện năng.
  - Tản nhiệt thụ động (Passive cooling - tắt quạt làm mát).
  - Tự động đặt độ sáng 45% và đưa các dịch vụ nền xuống trạng thái nhàn rỗi (`IDLE_PRIORITY_CLASS`).

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

### 2. 🧹 Dọn dẹp rác & Cache ổ đĩa (Junk Cleaner)
- Quét và xóa file rác thực tế trên đĩa: `%TEMP%`, `C:\Windows\Temp`, `Windows Prefetch`, `Windows Update Download Cache`, `CrashDumps`, Browser Cache (Edge/Chrome).
- Dọn Thùng rác hệ thống (Recycle Bin) trực tiếp qua Win32 Shell API `SHEmptyRecycleBinW`.

### 3. 🧠 Tối ưu bộ nhớ RAM (Memory Booster)
- Thu hồi Working Set nhàn rỗi (`K32EmptyWorkingSet`) từ các tiến trình đang chạy.
- Đo lường dung lượng RAM giải phóng chính xác theo thời gian thực qua `K32GetProcessMemoryInfo`.
- Hiển thị bảng xếp hạng Top ứng dụng chiếm dụng RAM nhiều nhất.

### 4. 🔋 Quản lý Pin & Xung nhịp CPU (Hardware Power Control)
- Đọc trực tiếp cảm biến vi điều khiển phần cứng ACPI: Dung lượng thiết kế gốc (`BatteryStaticData`), dung lượng nạp đầy thực tế (`BatteryFullChargedCapacity`), và số chu kỳ sạc (`BatteryCycleCount`).
- Tự động tính toán % chai pin thực tế.
- Hỗ trợ đầy đủ laptop pin đơn và hệ thống **Pin kép (Dual Battery)**.
- Điều khiển trần xung nhịp CPU và độ sáng màn hình qua WMI `WmiSetBrightness`.

### 5. 🛡️ Bảo vệ tuổi thọ máy (Hardware Protection)
- Quản lý chế độ ngủ đông (Hibernate) chống sụt pin khi gập máy.
- Giám sát sức khỏe ổ cứng SSD NVMe.
- Cảnh báo và hướng dẫn đặt ngưỡng sạc pin 80% (Battery Charge Threshold) chống phồng pin.

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
