#!/usr/bin/env python3
"""
OmniTune Automated License Server & Webhook Handler
Hệ thống tự động hóa cấp Key bản quyền 24/7 khi khách thanh toán qua:
1. VietQR (SePay / PayOS / Casso)
2. Lemon Squeezy (International Cards / PayPal)

Cài đặt phụ thuộc:
    pip install fastapi uvicorn pydantic requests
Chạy server:
    python license_server.py
"""

import hashlib
import json
import os
import smtplib
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from typing import Optional

try:
    from fastapi import FastAPI, HTTPException, Request
    from pydantic import BaseModel
    import uvicorn
except ImportError:
    FastAPI = None

LICENSE_SALT = os.environ.get("OMNITUNE_SALT", "OMNITUNE_PRO_AUTONOMOUS_2026_THUAN_VIETNAM")
SMTP_HOST = os.environ.get("SMTP_HOST", "smtp.gmail.com")
SMTP_PORT = int(os.environ.get("SMTP_PORT", "587"))
SMTP_USER = os.environ.get("SMTP_USER", "thuanlovevietnam@gmail.com")
SMTP_PASS = os.environ.get("SMTP_PASS", "")  # App Password của Gmail

def generate_key_for(email: str) -> str:
    normalized = email.strip().lower()
    input_str = f"{LICENSE_SALT}:{normalized}"
    sha256_hash = hashlib.sha256(input_str.encode('utf-8')).hexdigest().upper()
    part1 = sha256_hash[0:4]
    part2 = sha256_hash[4:8]
    part3 = sha256_hash[8:12]
    return f"OTPRO-{part1}-{part2}-{part3}"

def generate_key_for_hwid(email: str, hwid: str) -> str:
    normalized_email = email.strip().lower()
    normalized_hwid = hwid.strip().upper()
    input_str = f"{LICENSE_SALT}:{normalized_email}:{normalized_hwid}"
    sha256_hash = hashlib.sha256(input_str.encode('utf-8')).hexdigest().upper()
    part1 = sha256_hash[0:4]
    part2 = sha256_hash[4:8]
    part3 = sha256_hash[8:12]
    return f"OTHW-{part1}-{part2}-{part3}"

def send_license_email(to_email: str, license_key: str):
    if not SMTP_PASS:
        print(f"[CẢNH BÁO] Chưa cấu hình SMTP_PASS. Giả lập gửi email đến: {to_email}")
        print(f"Key bản quyền: {license_key}")
        return True

    msg = MIMEMultipart("alternative")
    msg["Subject"] = "⚡ [OmniTune Pro] Mã kích hoạt bản quyền trọn đời của bạn"
    msg["From"] = f"OmniTune Team <{SMTP_USER}>"
    msg["To"] = to_email

    html_content = f"""
    <html>
      <body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background-color: #0b0f19; color: #f1f5f9; padding: 30px;">
        <div style="max-width: 540px; margin: 0 auto; background: #111827; border: 1px solid #1f2937; border-radius: 12px; padding: 24px;">
          <h2 style="color: #3b82f6; margin-top: 0;">⚡ OmniTune Pro Activated</h2>
          <p>Cảm ơn bạn đã nâng cấp lên bản quyền vĩnh viễn <strong>OmniTune Pro</strong>!</p>
          <div style="background: #1f2937; border: 1px dashed #3b82f6; border-radius: 8px; padding: 16px; margin: 20px 0; text-align: center;">
            <div style="font-size: 13px; color: #9ca3af; margin-bottom: 6px;">MÃ BẢN QUYỀN (LICENSE KEY)</div>
            <div style="font-size: 20px; font-weight: bold; color: #fbbf24; letter-spacing: 2px;">{license_key}</div>
          </div>
          <p><strong>Hướng dẫn kích hoạt:</strong></p>
          <ol style="color: #cbd5e1; line-height: 1.6;">
            <li>Mở ứng dụng <strong>OmniTune</strong> trên máy tính.</li>
            <li>Click vào nút <strong>★ Lên PRO</strong> ở góc trên bên phải.</li>
            <li>Nhập Email: <code>{to_email}</code> và Key ở trên.</li>
            <li>Bấm <strong>Kích hoạt</strong> để mở khóa toàn bộ tính năng VIP trọn đời!</li>
          </ol>
          <hr style="border: none; border-top: 1px solid #374151; margin: 24px 0;" />
          <p style="font-size: 12px; color: #6b7280; text-align: center;">Hỗ trợ 24/7: thuanlovevietnam@gmail.com</p>
        </div>
      </body>
    </html>
    """
    msg.attach(MIMEText(html_content, "html"))

    try:
        with smtplib.SMTP(SMTP_HOST, SMTP_PORT) as server:
            server.starttls()
            server.login(SMTP_USER, SMTP_PASS)
            server.sendmail(SMTP_USER, to_email, msg.as_string())
        print(f"[THÀNH CÔNG] Đã gửi email kích hoạt đến {to_email}")
        return True
    except Exception as e:
        print(f"[LỖI] Gửi email thất bại: {e}")
        return False

# Setup FastAPI App
if FastAPI:
    app = FastAPI(title="OmniTune License Webhook API")

    @app.get("/")
    def index():
        return {"app": "OmniTune License Automation Server", "status": "online"}

    # --- 1. WEBHOOK SEPAY (VIETQR TỰ ĐỘNG) ---
    @app.post("/webhook/sepay")
    async def sepay_webhook(request: Request):
        """
        Nhận thông báo biến động số dư VietQR từ SePay.vn
        Cú pháp chuyển khoản khách ghi: OTPRO <email>
        Ví dụ: OTPRO thuan@gmail.com
        """
        body = await request.json()
        print(f"[SEPAY WEBHOOK] Nhận dữ liệu:", body)

        content = body.get("content", "").strip()
        transfer_amount = float(body.get("transferAmount", 0))

        # Yêu cầu tối thiểu 190.000đ (hoặc theo giá niêm yết)
        if transfer_amount < 150000:
            return {"status": "ignored", "reason": "Số tiền không đủ"}

        # Trích xuất email từ nội dung giao dịch
        tokens = content.replace(",", " ").split()
        payer_email = None
        for tok in tokens:
            if "@" in tok and "." in tok:
                payer_email = tok.lower()
                break

        if not payer_email:
            return {"status": "error", "reason": "Không tìm thấy email trong nội dung chuyển khoản"}

        key = generate_key_for(payer_email)
        send_license_email(payer_email, key)
        return {"status": "success", "email": payer_email, "key": key}

    # --- 2. WEBHOOK LEMON SQUEEZY (QUỐC TẾ) ---
    @app.post("/webhook/lemonsqueezy")
    async def lemon_webhook(request: Request):
        body = await request.json()
        event_name = body.get("meta", {}).get("event_name", "")

        if event_name == "order_created":
            custom_data = body.get("meta", {}).get("custom_data", {})
            payer_email = body.get("data", {}).get("attributes", {}).get("user_email")
            hwid = custom_data.get("hwid", "")

            if payer_email:
                if hwid:
                    key = generate_key_for_hwid(payer_email, hwid)
                else:
                    key = generate_key_for(payer_email)

                send_license_email(payer_email, key)
                return {"status": "success", "email": payer_email, "key": key}

        return {"status": "ignored"}

    # --- 3. TRA CỨU / CẤP LẠI LICENSE KEY ---
    @app.post("/api/lookup-license")
    async def lookup_license(request: Request):
        body = await request.json()
        email = body.get("email", "").strip().lower()
        if not email or "@" not in email:
            raise HTTPException(status_code=400, detail="Email không hợp lệ")

        key = generate_key_for(email)
        sent = send_license_email(email, key)
        if sent:
            return {"status": "success", "message": f"Đã gửi lại License Key vào email {email}"}
        else:
            return {"status": "error", "message": "Gửi email thất bại. Vui lòng liên hệ hỗ trợ trực tiếp."}

if __name__ == "__main__":
    if FastAPI is None:
        print("FastAPI chưa được cài đặt. Đang chạy chế độ kiểm thử CLI...")
        email = "khachhang@gmail.com"
        key = generate_key_for(email)
        print(f"Key test cho {email}: {key}")
    else:
        print("Đang khởi động OmniTune License Automation Server tại http://0.0.0.0:8000 ...")
        uvicorn.run(app, host="0.0.0.0", port=8000)
