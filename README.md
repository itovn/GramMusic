# GramMusic

App **nghe nhạc từ Telegram** (không chính thức) cho **iPhone / iPad / iPod touch chạy iOS 5 và iOS 6 đã jailbreak**. Mở app là có
thư viện: GramMusic tự tìm mọi bài hát trong Tin nhắn đã lưu, nhóm, kênh của tài khoản Telegram; tìm không dấu, xếp theo tên / ca sĩ,
phát nền khi khoá màn, tải về máy để nghe không cần mạng. Phát được cả **FLAC / OGG / Opus** (lõi tự giải mã), có **lời bài hát chạy
theo nhạc** ngay trên màn hình khoá.

Cùng nền với [GramMini](https://github.com/itovn/GramMini): lõi **Rust** nói chuyện thẳng với Telegram bằng MTProto
([grammers](https://codeberg.org/Lonami/grammers)), vỏ **Objective-C / UIKit** dựng bằng Theos, giao diện iOS 6 + kính. Không cần
server riêng — phiên đăng nhập chỉ nằm trên máy. Một gói `.deb` chạy cả iOS 5 và 6, cài qua Cydia.

> GramMusic dùng Telegram API nhưng **không phải** sản phẩm của Telegram.

**Trang giới thiệu:** <https://itovn.github.io/GramMusic/> · **Nhóm Telegram:** [t.me/tivikit](https://t.me/tivikit) — hỏi đáp, báo lỗi, bản mới.

## Máy chạy được

iOS 5.0 – 6.1.x, đã jailbreak (cần MobileSubstrate, PreferenceLoader): iPhone 4 / 4S / 5, iPad 1 / 2 / 3 / 4 / mini 1, iPod touch
4 / 5. Đã thử trên iPhone 5 (iOS 6.1.4), iPad mini 1 (iOS 6.1.3) và iPad 1 (iOS 5.1.1).

## Có gì

- **Thư viện từ cả tài khoản**: quét lần đầu từng chat (nhạc gửi dạng bài hát lẫn dạng tệp), sau đó tự lấy bài mới; **Nguồn**: ẩn /
  hiện từng chat, thêm kênh / nhóm công khai bằng @tên hoặc link t.me; nhóm có chủ đề — nghe / lưu playlist theo từng chủ đề.
- Tìm không dấu; xếp Mới nhất / Tên A–Z / Ca sĩ + thanh chữ cái; Phát hết / Ngẫu nhiên; chọn nhiều bài; lưu nhanh danh sách / kết quả
  tìm / hàng chờ thành playlist.
- **Phát ngay, tua ngay** qua cổng phát nội bộ: tệp lớn tải theo nhịp nghe (mặc định 1 phút phía trước — chỉnh ở Cài đặt → «Tải
  trước»), tạm dừng thì thôi tải; bài nghe trọn được giữ để lần sau nghe không cần mạng.
- Định dạng: mp3, m4a (AAC / ALAC), wav, aiff; **FLAC** (cả hi-res 24 bit / 96–192 kHz), **OGG Vorbis**, **Opus** — lõi giải mã thành
  WAV cho AVPlayer, tua được tới từng mẫu.
- Màn Đang phát: thông số tệp (định dạng · tần số · bit · bitrate · dung lượng), hàng chờ, trộn, lặp, hẹn giờ tắt, Nghe tiếp.
- **Lời bài hát** (LRCLIB, phải bật trong Cài đặt): lời chạy tô dòng đang hát, chạm để tua.
- **Màn hình khoá** (tweak **GramLockControl** đi kèm, bật / tắt trong Cài đặt của iPhone): lời đang hát, thanh tiến trình kéo để tua,
  nút bài trước / phát / bài sau — iOS 5 / 6 vốn không có cho app ngoài.
- **Nhạc trên máy**: bài trong app Nhạc và tệp nhạc trong máy (cả FLAC / OGG / Opus).
- Playlist, Yêu thích, Nghe gần đây, **Tải về máy**, quản lý bộ nhớ đệm.
- **MTProxy** (`dd`, `ee` fake-TLS) và SOCKS5; **DNS mã hoá** (DoH / DoT: NextDNS → Cloudflare → Google, lỗi hết thì DNS của máy).
- Phát nền, màn hình khoá / tai nghe, cuộc gọi đến, rút tai nghe; chế độ sáng / tối.

## Cài đặt

- **Cydia:** thêm nguồn `https://itovn.github.io/GramMusic/cydia/` rồi cài **GramMusic**. Cài xong Cydia xin khởi động lại
  SpringBoard (tweak GramLockControl cho màn hình khoá).
- **Tay:** tải `vn.grammusic.app_<bản>_iphoneos-arm.deb` ở [Releases](https://github.com/itovn/GramMusic/releases), chép vào máy,
  `dpkg -i` rồi khởi động lại SpringBoard.

## Dựng từ mã nguồn

Máy Ubuntu có [Theos](https://theos.dev) (SDK iPhoneOS 9.3) và Rust nightly (`rust-src`):

1. Tạo `api_id` / `api_hash` của riêng bạn ở [my.telegram.org](https://my.telegram.org), ghi vào `keys/telegram.txt` (`api_id=…`,
   `api_hash=…` — không có trên git). Tuỳ chọn: mã hồ sơ NextDNS vào `keys/nextdns.txt` (không có thì DNS mã hoá đi thẳng
   Cloudflare → Google).
2. `bash tools/build-app.sh` — bản thử (có công cụ thử `app/Debug/`); `bash tools/build-release.sh` — bản phát hành.

Lõi dựng cho armv7 Cortex-A8 bằng target tự định nghĩa (`tools/rust/armv7-apple-ios6.json`, `-Z build-std`); thư viện mã hoá `ring`
được vá cho ARM 32-bit của Apple (`tools/rust/vendor-ring.sh`). Chạy thử lõi trên PC:

```sh
cd core
cargo test -p gm-core -p gm-net
cargo run -p gm-cli -- probe --dc 2
cargo run -p gm-cli -- login ../pc-data      # phiên riêng của PC
cargo run -p gm-cli -- repl ../pc-data       # gõ: library.query {"limit":5}
```

## Tài liệu

| Tệp | Nội dung |
| --- | --- |
| [docs/THIET-KE.md](docs/THIET-KE.md) | Thiết kế: kiến trúc, thư viện, phát nhạc, giao diện, lộ trình |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Các quyết định kỹ thuật và kết quả kiểm chứng |
| [docs/FFI.md](docs/FFI.md) | Hợp đồng lệnh / sự kiện giữa app và lõi, cổng phát |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Nhật ký tiến độ |

## Giấy phép

[GPL-3.0](LICENSE) (dùng lại mã GPL-3.0 của GramMini). `core/vendor/grammers-*` giữ giấy phép MIT / Apache-2.0 của grammers;
`core/vendor/opus-1.4` giữ giấy phép BSD của Opus. DNS mã hoá chép từ ChimToTV (GPL-3.0).
