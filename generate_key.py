#!/usr/bin/env python3
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

def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    if len(sys.argv) > 1:
        email = sys.argv[1]
    else:
        email = input("Nhap email khach hang: ").strip()

    if not email:
        print("Loi: Email khong duoc de trong!")
        sys.exit(1)

    key = generate_key_for(email)
    print("\n" + "="*50)
    print(f"Customer: {email}")
    print(f"License Key: {key}")
    print("="*50)
    print(f"\nEmail: {email}\nKey: {key}\n")

if __name__ == "__main__":
    main()
