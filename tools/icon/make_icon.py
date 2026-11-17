"""Biểu tượng GramMusic theo mẫu anh đưa (Tham_Khao/logo/logo1.png — DECISIONS M6): ô vuông bo góc viền crôm kiểu iOS 6, nền kính
xanh sọc chéo, máy bay giấy trắng giữa tai nghe, nốt nhạc nhiều màu, sóng âm.

    python tools/icon/make_icon.py from-ref Tham_Khao/logo/logo1.png   # cắt ô biểu tượng từ ảnh mẫu → tools/icon/icon-1024.png
    python tools/icon/make_icon.py app/Resources                       # các cỡ iOS 5 / 6 từ icon-1024.png

Góc ngoài viền để trong suốt (SpringBoard iOS 5 / 6 giữ kênh alpha; bán kính góc của mẫu ≈ 21,6 % — lớn hơn mặt nạ của iOS
≈ 17,5 % nên iOS không cắt vào viền). `UIPrerenderedIcon` = có → iOS không phủ thêm vệt bóng (mẫu đã có sẵn)."""
import os
import sys

from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
MASTER = os.path.join(HERE, "icon-1024.png")
S = 1024
# Ô biểu tượng trong ảnh mẫu 2752 × 1536: mép ngoài viền crôm (đo bằng độ sáng), bán kính góc
REF_BOX = (887, 175, 978)  # x, y, cạnh (lùi 3 px vào trong mép viền: bỏ viền tối của nền ảnh mẫu)
REF_RADIUS = 210

SIZES = {"Icon.png": 57, "Icon@2x.png": 114, "Icon-72.png": 72, "Icon-72@2x.png": 144, "Icon-Small.png": 29,
         "Icon-Small@2x.png": 58, "Icon-Small-50.png": 50, "Icon-Small-50@2x.png": 100}


def rounded_mask(size, radius, aa=4):
    """Mặt nạ ô bo góc, khử răng cưa (vẽ to `aa` lần rồi thu nhỏ)."""
    big = Image.new("L", (size * aa, size * aa), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * aa - 1, size * aa - 1), radius=radius * aa, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def from_ref(path):
    src = Image.open(path).convert("RGBA")
    x, y, side = REF_BOX
    if src.size != (2752, 1536):
        # ảnh mẫu khác cỡ: co giãn toạ độ theo tỉ lệ
        k = src.size[0] / 2752
        x, y, side = int(x * k), int(y * k), int(side * k)
    icon = src.crop((x, y, x + side, y + side)).resize((S, S), Image.LANCZOS)
    icon.putalpha(rounded_mask(S, int(REF_RADIUS * S / REF_BOX[2])))
    icon.save(MASTER)
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
