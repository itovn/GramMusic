"""Biểu tượng GramMusic theo mẫu anh đưa (Tham_Khao/logo/):
  - logo2.png (đang dùng từ 1.0.4 — DECISIONS M25): ô bo góc viền kính neon xanh → tím, nền vũ trụ, máy bay giấy có nút phát,
    sóng âm, nốt nhạc.
  - logo1.png (1.0.0 – 1.0.3 — DECISIONS M6): viền crôm, nền kính xanh sọc chéo, máy bay giấy trắng giữa tai nghe.

    python tools/icon/make_icon.py from-ref Tham_Khao/logo/logo2.png   # cắt ô biểu tượng từ ảnh mẫu → tools/icon/icon-1024.png
    python tools/icon/make_icon.py app/Resources                       # các cỡ iOS 5 / 6 từ icon-1024.png (+ chép cỡ 29 / 58
                                                                       # cho biểu tượng GramLockControl trong Cài đặt)

Góc ngoài viền để trong suốt (SpringBoard iOS 5 / 6 giữ kênh alpha; bán kính góc của mẫu ≈ 21 % — lớn hơn mặt nạ của iOS
≈ 17,5 % nên iOS không cắt vào viền). `UIPrerenderedIcon` = có → iOS không phủ thêm vệt bóng (mẫu đã có sẵn)."""
import os
import shutil
import sys

from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
MASTER = os.path.join(HERE, "icon-1024.png")
S = 1024
PREFS = os.path.join(HERE, "..", "..", "tweak", "GramLockControl", "Prefs")
# Ô biểu tượng trong từng ảnh mẫu 2752 × 1536: x, y, rộng, cao, bán kính góc — mép ngoài viền đo bằng độ sáng.
REFS = {
    # lùi 3 px vào trong mép viền: bỏ viền tối của nền ảnh mẫu
    "logo1.png": (887, 175, 978, 978, 210),
    # viền x 823–1928, y 207–1350 (đáy có gờ dày 3D nên ô cao hơn rộng ~3 % — co dọc lại khi đưa về ô vuông); bán kính đo theo
    # đường chéo hai góc dưới (không có đốm sáng): mép cách góc ~69 px → r ≈ 236. Lùi 2 px bỏ viền tối + vệt sáng mảnh ngoài viền.
    "logo2.png": (825, 209, 1102, 1140, 234),
}

SIZES = {"Icon.png": 57, "Icon@2x.png": 114, "Icon-72.png": 72, "Icon-72@2x.png": 144, "Icon-Small.png": 29,
         "Icon-Small@2x.png": 58, "Icon-Small-50.png": 50, "Icon-Small-50@2x.png": 100}


def rounded_mask(w, h, radius, aa=4):
    """Mặt nạ ô w × h bo góc, khử răng cưa (vẽ to `aa` lần rồi thu nhỏ)."""
    big = Image.new("L", (w * aa, h * aa), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, w * aa - 1, h * aa - 1), radius=radius * aa, fill=255)
    return big.resize((w, h), Image.LANCZOS)


def from_ref(path):
    name = os.path.basename(path)
    if name not in REFS:
        sys.exit("chưa đo ô biểu tượng của %s — thêm vào REFS" % name)
    src = Image.open(path).convert("RGBA")
    x, y, w, h, r = REFS[name]
    if src.size != (2752, 1536):
        # ảnh mẫu khác cỡ: co giãn toạ độ theo tỉ lệ
        k = src.size[0] / 2752
        x, y, w, h, r = (int(v * k) for v in (x, y, w, h, r))
    # bo góc ngay trên ô cắt (đúng hình viền), rồi mới đưa về ô vuông 1024
    icon = src.crop((x, y, x + w, y + h))
    icon.putalpha(rounded_mask(w, h, r))
    icon.resize((S, S), Image.LANCZOS).save(MASTER)
    print("đã cắt", MASTER)


def sizes(out):
    big = Image.open(MASTER).convert("RGBA")
    os.makedirs(out, exist_ok=True)
    for name, px in SIZES.items():
        img = big.resize((px, px), Image.LANCZOS)
        if px <= 72:
            # cỡ nhỏ: làm nét lại chút (mẫu nhiều chi tiết mảnh — nốt nhạc, sóng âm)
            a = img.getchannel("A")
            img = img.filter(ImageFilter.UnsharpMask(radius=1, percent=60, threshold=2))
            img.putalpha(a)
        img.save(os.path.join(out, name))
    print("đã tạo", len(SIZES), "cỡ trong", os.path.normpath(out))
    if os.path.isdir(PREFS):
        # biểu tượng dòng GramLockControl trong Cài đặt = biểu tượng app cỡ nhỏ
        for src, dst in (("Icon-Small.png", "GramLockControl.png"), ("Icon-Small@2x.png", "GramLockControl@2x.png")):
            shutil.copyfile(os.path.join(out, src), os.path.join(PREFS, dst))
        print("đã chép cỡ 29 / 58 sang", os.path.normpath(PREFS))


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")  # bảng mã cp1252 của Windows không in được tiếng Việt
    if len(sys.argv) == 3 and sys.argv[1] == "from-ref":
        from_ref(sys.argv[2])
    elif len(sys.argv) == 2:
        sizes(sys.argv[1])
    else:
        print(__doc__)
        sys.exit(2)
