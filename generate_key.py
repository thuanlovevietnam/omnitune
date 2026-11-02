#!/usr/bin/env python3
"""
OmniTune Pro License Key Generator
Tạo mã bản quyền Pro (OTPRO hoặc OTHW) cho khách hàng.
"""

import hashlib
import sys

LICENSE_SALT = "OMNITUNE_PRO_AUTONOMOUS_2026_THUAN_VIETNAM"

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

def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')

    email = ""
    hwid = ""

    if len(sys.argv) > 2:
        email = sys.argv[1].strip()
        hwid = sys.argv[2].strip()
    elif len(sys.argv) == 2:
        email = sys.argv[1].strip()
    else:
        email = input("Nhập Email khách hàng: ").strip()
        hwid = input("Nhập Machine HWID (để trống nếu muốn tạo key theo email): ").strip()

    if not email:
        print("Lỗi: Email không được để trống!")
        sys.exit(1)

    if hwid:
        key = generate_key_for_hwid(email, hwid)
        mode = f"Khóa theo thiết bị ({hwid})"
    else:
        key = generate_key_for(email)
        mode = "Bản quyền trọn đời theo Email"

    print("\n" + "=" * 56)
    print("       ⚡ OMNITUNE PRO LICENSE GENERATOR")
    print("=" * 56)
    print(f"  Khách hàng : {email}")
    print(f"  Chế độ     : {mode}")
    print(f"  Mã kích hoạt: {key}")
    print("=" * 56)
    print(f"\nNội dung gửi khách:")
    print("-" * 56)
    print(f"Chào bạn, cảm ơn bạn đã ủng hộ OmniTune Pro!")
    print(f"Thông tin kích hoạt:")
    print(f"- Email: {email}")
    print(f"- License Key: {key}")
    print(f"Vui lòng mở OmniTune, click vào nút 'Lên PRO' và nhập 2 thông tin trên.")
    print("-" * 56 + "\n")

if __name__ == "__main__":
    main()
