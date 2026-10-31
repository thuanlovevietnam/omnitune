# ⚡ OmniTune - Universal System & Hardware Optimizer

Ứng dụng tối ưu hóa hệ thống, phần cứng và thời lượng pin chuyên sâu cho Windows (PC & Laptop), được viết 100% bằng **Rust** kết hợp Win32 API trực tiếp và GUI `egui/eframe`.

Thiết kế theo mô hình **Tinh Gọn & Thông Minh (Streamlined Context-Aware Architecture)** với 4 phân hệ cốt lõi.

---

## 🚀 4 Phân Hệ Tinh Gọn Cốt Lõi

### 1. 🎯 Trung Tâm Thông Minh (Smart Center & Auto-Pilot)
*Điều phối tự động toàn diện trên một giao diện trang chủ duy nhất:*
- **Chỉ số Sức khỏe PC (Health Score 0-100):** Đánh giá tổng hợp kèm lời khuyên chẩn đoán thông minh (RAM cao, rác nhiều, cắm sạc liên tục cần bật ngưỡng pin 80%).
- **Nút "⚡ TỐI ƯU TOÀN DIỆN" (1-Click):** Dọn rác an toàn Tầng 1 + Thu hồi RAM + Áp cờ EcoQoS cho các app nền + Cân bằng xung nhịp.
- **3 Đồng hồ đo Thời gian thực:** CPU % (Chip & Nhân/Luồng) • RAM % (Đã dùng/Tổng/Trống) • Pin % (Công suất tiêu thụ thực tế theo Watt & Điện áp V).
- **Cửa sổ Đang Focus (Foreground Auto-Boost):** Tự động phát hiện cửa sổ đang dùng, gỡ bỏ giới hạn xung nhịp và nâng quyền ưu tiên luồng xử lý (`ABOVE_NORMAL_PRIORITY_CLASS`).
- **EcoQoS (Windows Efficiency Mode):** Tự động điều tiết tiết kiệm điện `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` cho các app chạy ngầm (Drive, OneDrive, tab browser nền, Search Indexer).
- **Bộ 5 Kịch bản Tinh chỉnh & Chế độ Auto-Pilot:**
  - 🖥️ **Remote Desktop:** Khóa chống Sleep/Lock, CPU 65% máy mát 40°C không quạt 0dB, QuickSync giải mã video, tối ưu TCP buffer giảm trễ chuột, đóng băng app sync.
  - 🎮 **Gaming & Đồ họa:** Timer 1.0ms (`timeBeginPeriod(1)`), tắt GameDVR nền, CPU 90% nhường 12W TDP cho GPU Intel HD 620 đạt 1150MHz, dọn RAM mở rộng VRAM.
  - ☕ **Cà phê / Siêu tiết kiệm pin:** Khóa trần CPU 60% (~4W), quạt tắt (Passive), độ sáng 45%, đóng băng app ngầm.
  - 💻 **Lập trình & Biên dịch (Developer):** 100% Turbo Boost 3.9GHz, quạt tản nhiệt chủ động, ưu tiên CPU cho `rustc`, `cargo`, `go`.
  - 🎬 **Xem phim & Thư giãn:** Khóa chống tắt màn hình, CPU 55% êm ru 0dB, màn hình sáng 70%.

---

### 2. 🎛️ Bảng Ma Trận Tinh Chỉnh (10-Criteria Tuning Matrix)
*Khung bảng biểu hợp nhất 2 cột: Cột 1 Tiêu chí phần cứng (135px) • Cột 2 Điều khiển trực tiếp & Kiến nghị:*

| Tiêu chí | Công cụ setup & Tinh chỉnh tương tác trực tiếp |
| :--- | :--- |
| **1. ⚡ Xung CPU & TDP** | Thanh trượt trần CPU Pin (DC) & Sạc (AC) + Nút nhanh `60% Pin`, `80% Cân bằng`, `100% Turbo`. |
| **2. ⏱️ Timer & FPS** | Nút chuyển đổi Win32 `1.0 ms` (triệt tiêu lag chuột/stutter) vs `15.6 ms` (mặc định tiết kiệm điện). |
| **3. 🍃 EcoQoS Nền** | Checkbox tự động điều phối EcoQoS + Nút bấm ép EcoQoS toàn bộ app ngầm ngay lập tức. |
| **4. 🧠 RAM & VRAM** | Checkbox tự động thu hồi khi RAM vượt ngưỡng tùy chỉnh (70% - 95%) + Nút thu hồi RAM tức thì. |
| **5. 🌐 Mạng & Sync** | Nút kích hoạt TCP Low-Latency qua `netsh` + Nút đóng băng / phục hồi dịch vụ sync đám mây. |
| **6. 🔌 Chống Sleep** | Lựa chọn 3 chế độ: `Mặc định OS` \| `Giữ máy luôn thức` \| `Giữ cả Màn hình & Máy sáng`. |
| **7. ❄️ Quạt làm mát** | Nút chọn chính sách quạt: `Thụ động (Passive 0dB)` vs `Chủ động (Active làm mát sớm)`. |
| **8. 🔆 Độ sáng** | Thanh trượt độ sáng màn hình qua WMI ACPI + Nút nhanh `45% Pin`, `50% VP`, `70% Phim`. |
| **9. 🎬 GameDVR** | Nút tắt GameDVR nền (giải phóng GPU encoder) vs Bật lại theo mặc định Windows. |
| **10. 🛡️ Ngưỡng sạc & Ngủ** | Nút Bật/Tắt ngưỡng sạc pin 80% (Lenovo PM Service) + Nút kích hoạt chế độ ngủ đông Hibernate. |

---

### 3. 🧹 Dọn Rác & Điều Phối RAM (Resource Relief)
- **Bộ Dọn dẹp Rác 3 Tầng Phân loại:**
  - 🛡️ **Tầng 1 - Rác An toàn 100%:** User Temp, System Temp, Thùng rác (Recycle Bin), Crash Dumps, Edge/Chrome Cache.
  - 💻 **Tầng 2 - Rác Developer:** Node.js/NPM Cache (`%LOCALAPPDATA%\npm-cache` - dung lượng hàng GB), Go Build, Cargo Crates, Pip Cache.
  - ⚙️ **Tầng 3 - Bảo trì Hệ thống:** Windows Update Download Cache, Prefetch.
- **Điều phối RAM & Process Inspector:**
  - Thanh trạng thái RAM + Checkbox tự động dọn khi RAM > 85%.
  - Ô tìm kiếm tiến trình (Search Bar lọc theo tên ứng dụng hoặc PID).
  - Thao tác trực tiếp trên từng tiến trình: `⚡ Dọn` (thu hồi Working Set riêng), `🍃 Eco` (áp dụng EcoQoS), `❌` (kết thúc an toàn).

---

### 4. 🛡️ Khởi Động & Bảo Vệ Phần Cứng (Hardware Guard)
- **S.M.A.R.T NVMe SSD Health:** Đọc trực tiếp tình trạng sức khỏe phần cứng ổ đĩa (`Healthy / OK`) và đo % dung lượng các phân vùng.
- **Cảm biến ACPI Pin Kép:** Đọc dung lượng thiết kế, dung lượng nạp đầy, số chu kỳ sạc, % chai pin, điện áp thời gian thực (V) và công suất dòng xả/nạp (mW / Watt).
- **Khóa Ngưỡng Sạc Pin 80%:** Ngắt sạc khi pin đạt 80% để chống phồng rộp và chai pin khi cắm sạc liên tục tại bàn làm việc.
- **Chế độ Ngủ đông (Hibernate):** Quản lý trạng thái ngắt điện hoàn toàn khi gập máy.
- **Windows Telemetry Debloat:** 1-Click vô hiệu hóa dịch vụ thu thập dữ liệu ngầm `DiagTrack` của Windows, giảm tải CPU và ổ đĩa nền.
- **Bật / Tắt Ứng dụng Khởi động 1-Click:** Chuẩn cơ chế `StartupApproved` của Windows Task Manager.

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
