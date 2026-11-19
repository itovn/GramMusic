#!/usr/bin/env bash
# Ký gói phát hành cho cập nhật OTA: <tệp .deb> → <tệp .deb>.sig (RSA PKCS#1 v1.5 + SHA-256).
#   bash tools/sign-release.sh app/packages/vn.grammusic.app_<bản>_iphoneos-arm.deb
# Khoá riêng: keys/ota-sign.key — chỉ ở máy phát hành, KHÔNG đưa lên git (/keys/ đã nằm trong .gitignore); mất khoá thì máy
# đang chạy bản cũ phải cập nhật qua Cydia một lần (bản mới gắn khoá mới). gmupdate (app/Helper/gmupdate.c) chỉ cài gói có chữ
# ký khớp chứng chỉ gắn trong app/Helper/ota_cert.h. Tải .sig lên GitHub Release cùng .deb (cùng tên + «.sig»).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEB="${1:?cách dùng: sign-release.sh <tệp .deb>}"
KEY="$ROOT/keys/ota-sign.key"
CRT="$ROOT/keys/ota-sign.crt"
[[ -f "$KEY" && -f "$CRT" ]] || { echo "thiếu keys/ota-sign.key / keys/ota-sign.crt" >&2; exit 1; }
# Chứng chỉ ở máy này đúng là chứng chỉ gắn trong app (vân tay SHA-256 ghi ở đầu ota_cert.h)
FP="$(openssl x509 -in "$CRT" -noout -fingerprint -sha256 | sed 's/.*=//')"
grep -q "SHA-256: $FP" "$ROOT/app/Helper/ota_cert.h" || { echo "chứng chỉ không khớp app/Helper/ota_cert.h" >&2; exit 1; }
openssl dgst -sha256 -sign "$KEY" -out "$DEB.sig" "$DEB"
PUB="$(mktemp)"
trap 'rm -f "$PUB"' EXIT
openssl x509 -in "$CRT" -pubkey -noout > "$PUB"
openssl dgst -sha256 -verify "$PUB" -signature "$DEB.sig" "$DEB"
ls -la "$DEB.sig"
