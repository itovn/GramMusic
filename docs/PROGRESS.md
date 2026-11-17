# GramMusic — Tiến độ

## 17/11/2026 — khởi động dự án

- Đọc GramMini (tài liệu, lõi Rust, app) và phân tích ngược MeloVault 0.6.132 (APK: chuỗi trong dex, lời gọi TDLib, lược đồ
  SQLite, giao thức đồng bộ playlist, chuỗi giao diện) → [THIET-KE.md](THIET-KE.md), [DECISIONS.md](DECISIONS.md) (M1–M5),
  [FFI.md](FFI.md).
- **Lõi** (`core/`, workspace Rust): `gm-net` + grammers vá chép nguyên từ GramMini; `gm-core` mới:
  - `text.rs` — bỏ dấu (tiếng Việt + Latin), tách tên bài / ca sĩ từ tên tệp, khoá xếp, chữ cái, đuôi / kiểu MIME;
  - `library.rs` — nguồn, chỉ mục theo nguồn, lọc trùng tệp, tìm, xếp Mới / A–Z / Ca sĩ + mục chữ cái, lấy theo trang;
  - `sync.rs` + `service.rs` — quét toàn tài khoản (`messages.searchGlobal`: lượt âm thanh, lượt tài liệu), con trỏ lưu được,
    lấy bài mới (dừng khi gặp liền 10 bài đã có), FLOOD_WAIT, Tin nhắn đã lưu + kênh công khai thêm tay quét riêng, cập nhật trực
    tiếp (bài mới / tin bị xoá);
  - `stream.rs` — cổng HTTP `127.0.0.1` có Range cho AVPlayer, khúc 256 KB qua `upload.getFile`, tệp thưa + bảng khúc, tua tới
    đâu tải tới đó, bài nghe trọn thành tệp đầy đủ, tải trước bài kế, tải về máy;
  - `playlists.rs` — playlist, Yêu thích, Nghe gần đây; `cache.rs` — đệm (bài đã tải không tự xoá);
  - `gm-ffi` (`gm_init` / `gm_call`), `gm-cli` (`probe`, `login`, `svc`, `repl`).
- **Thử trên PC (Windows)**: `cargo build` cả workspace; **35 phép thử đạt** (19 `gm-core`: bỏ dấu, tách tên tệp, xếp + mục chữ
  cái, lọc trùng, gộp chỉ mục, playlist, đệm / bài đã tải, **cổng phát** — thăm dò `bytes=0-1` kiểu AVPlayer, tua vắt khúc, cả
  bài, 404 / 416, lỗi mạng rồi thử lại, «tắt app» rồi tải tiếp từ `.map`; 16 `gm-net`). `gm-cli probe`: auth key DC2 qua đường
  thẳng 2,5 s.
- **App** (`app/`, Theos, iOS 5.0): GMPlayer (AVQueuePlayer, nạp sẵn bài kế, màn hình khoá, cuộc gọi / rút tai nghe, trộn / lặp,
  hẹn giờ tắt, Nghe tiếp), GMSongList (trang 100 bài), khung chính tự làm (Thư viện / Nguồn / Playlist / Đã tải / Cài đặt + thanh
  phát nhỏ), danh sách bài (tìm không dấu, xếp, thanh chữ cái, Phát hết / Ngẫu nhiên, bảng «⋯»), Đang phát + Hàng chờ, Nguồn
  (ẩn / hiện, thêm kênh bằng @tên), Playlist, Cài đặt, Dữ liệu và bộ nhớ; Đăng nhập / Proxy / GMTheme / GMCompat từ GramMini;
  biểu tượng tím + nốt nhạc (`tools/icon/make_icon.py`).
- **Chưa dựng được app**: máy này (Windows) không có Theos; cần máy dựng Ubuntu (như GramMini) để dựng `.deb` và thử trên máy
  thật.
- **Rà mã app (đọc, chưa dựng)** — không thấy lỗi dựng; đã sửa 15 chỗ: crash lúc mở (`UISearchBar.keyboardAppearance` không có trên
  iOS 5 / 6 → đặt cho ô nhập bên trong, như TubeMini), crash xoá / dời bài trong Hàng chờ khi bài đổi giữa chừng, crash hiếm khi đổi
  cách xếp (số mục chữ cái), bài nạp sẵn hỏng làm kẹt chuyển bài, «Hết bài này» vẫn chuyển bài, mở bài ở nền không giữ app chạy,
  ngắt (cuộc gọi) không cho phát tiếp thì tự phát lại, màn Đang phát tự đóng khi nhảy tới bài chưa có thông tin, nút Phát / Tiếp của
  thanh phát nhỏ bị chạm nuốt trên iOS 5, đổi giao diện lúc đang mở màn bật lên, dời bài trùng khoá, thông báo AVPlayerItem ngoài
  luồng chính, «Xoá lịch sử nghe» / bỏ Yêu thích, biểu tượng hàng chờ, ảnh bìa màn hình khoá.
- **Biểu tượng** theo mẫu anh đưa (`Tham_Khao/logo/logo1.png`, DECISIONS M6).

### Còn lại trước bản thử đầu tiên

- [ ] `keys/telegram.txt` (api_id / api_hash) cho GramMusic.
- [ ] Dựng trên máy dựng (`tools-local/deploy.sh`), sửa lỗi dựng, `symcheck` trên iOS 5 và iOS 6.
- [ ] Đăng nhập trên máy thật, quét thư viện thật (đo thời gian / FLOOD_WAIT với tài khoản nhiều nhạc).
- [ ] AVPlayer iOS 5 / 6 qua cổng phát: mp3, m4a, wav; tua; chuyển bài ở nền khi khoá màn (M4); cuộc gọi đến; rút tai nghe.
- [ ] iPad 1 (A4, 256 MB): thư viện 10.000 bài, cuộn, RAM.
- [ ] Thử lõi trên PC với phiên riêng của PC (`gm-cli login pc-data`) — không dùng chung phiên với máy thử (GramMini D24).

## 17/11/2026 (chiều) — chạy trên máy thật

- Khoá api (`keys/telegram.txt`, từ `Tham_Khao/tele_key.txt`): Telegram nhận. `.deb` dựng được ngay lần đầu (1,9 MB); `symcheck`:
  313 ký hiệu, **thiếu 0** trên iOS 6.1.4 và iOS 5.1.1. Cài iPhone 5 + iPad 1; anh đăng nhập trên iPhone 5.
- Quét từng chat (M7): thư viện của anh 377 bài (FLAC 240, MP3 100, WAV 33…).
- FLAC / OGG Vorbis (M8), chủ đề nhóm + tạo list nhanh (M9), tải theo nhịp nghe (M10), thông số tệp trên màn Đang phát (M11) — thử
  trên iPhone 5: đạt (số đo trong DECISIONS).
- Lõi: 27 phép thử đạt trên PC (thêm: giải FLAC khớp từng bit với ffmpeg + tua không SEEKTABLE, hi-res, OGG, nhịp nghe, thông số
  MP3 / FLAC / OGG / WAV).
- Công cụ thử `GMDebug` (bản thử): lệnh qua `/var/mobile/Documents/GramMusic-cmd.txt` (`call`, `tab`, `select`, `playkey`, `seek`,
  `toggle`, `state`…).
- Sửa khi thử: luồng chính đứng 9 giây khi chuyển bài (cổng phát chờ dữ liệu cho kết nối AVPlayer đã bỏ → theo dõi socket đóng); tua
  lúc app ở nền → giữ app chạy tới khi có tiếng; nguồn cũ không nhận cờ «có chủ đề»; crash của chính GMDebug (chạm hàng khi bảng chưa
  tải); bộ lấy dữ liệu giả trong phép thử trừ đếm không nguyên tử.
- Ghi nhận: một lần iOS giết app (`8badf00d`) khi xuống nền — luồng chính kẹt trong tweak `libstatusbar` (chờ SpringBoard), không
  phải mã của app; một số lần mở bài đầu tiên sau khi mở app luồng chính đứng ~5 giây (lần đo sau: 175 ms) — theo dõi tiếp.

### Còn lại

- [ ] iPad 1 (A4): đăng nhập rồi đo giải FLAC (16 / 44,1 và 24 / 96), RAM khi AVPlayer đệm WAV.
- [ ] Nghe liền nhiều bài khi khoá màn (chuyển bài ở nền, cả bài FLAC kế tiếp) — 1 giờ.
- [x] Opus; lời bài hát (LRCLIB); nhạc trên máy; tuỳ chọn độ dài «tải trước» trong Cài đặt (xem dưới).
- [ ] OTA.

## 17/11/2026 (tối) — Opus, lời bài hát, nhạc trên máy, «Tải trước»

- **«Tải trước»** trong Cài đặt → Phát nhạc (M12): 30 giây / 1 phút / 2 phút / 5 phút / Cả bài; lệnh `settings.get` / `settings.set`.
- **Opus** (M13): libopus của GramMini + bộ đọc Ogg tự viết, tua đúng từng mẫu (làm nóng 80 ms); lõi giải mã tách nguồn mẫu chung
  (symphonia / Opus).
- **Lời bài hát** (M14): LRCLIB qua HTTPS (reqwest + rustls như GramMini), mặc định tắt — bật mới gửi tên / ca sĩ / thời lượng; khung
  lời trên màn Đang phát (lời chạy tô dòng, tự cuộn, chạm để tua).
- **Nhạc trên máy** (M15): Nguồn → Trên máy → «Nhạc trên máy» (app Nhạc) và «Tệp trên máy» (duyệt thư mục; FLAC / OGG / Opus lõi giải
  từ tệp).
- **Sửa** (M16): luồng chính đứng 5,6–9,3 giây khi mở bài FLAC chưa có trên máy (tạo AVPlayerItem đọc thử cuối tệp) → nạp ở nền; ảnh
  bìa lỗi mạng bị hỏi lại ~3 lần / giây → nhớ lỗi 30 giây; luồng giải mã có thể kẹt cờ «đang chạy» khi mở nguồn lỗi.
- Lõi: **36 phép thử đạt** trên PC (thêm: Opus đúng độ dài / đúng mốc / tua, tệp `.ogg` chứa Opus, giải tệp trên máy không tải gì,
  «Tải trước» đổi giới hạn, thời lượng đọc từ tệp, tách LRC, chọn bản lời hợp nhất, tên để tìm lời) + 1 phép thử gọi thật LRCLIB.
- iPhone 5 (iOS 6.1.4): Cài đặt hiện nhóm Phát nhạc; Opus / FLAC 24/96 / OGG / mp3 trong `Documents` phát được, thông số «OPUS ·
  48kHz · 87 kbps · 213 KB»; LRCLIB trả «không có» cho bài Hoa thử (đúng — kho thiếu bài). `symcheck`: 337 ký hiệu, **thiếu 0** trên iOS
  6.1.4 và iOS 5.1.1.
- Đã trả cài đặt trên iPhone 5 về mặc định (lời bài hát: tắt; tải trước: 1 phút) sau khi thử.

### Còn lại

- [ ] «Nhạc trên máy» (app Nhạc) chưa thử trên máy thật (anh đang dùng iPhone lúc em thử — em dừng điều khiển giao diện).
- [ ] Lời bài hát trên máy thật với bài có trong LRCLIB (mạng iPhone chập chờn lúc thử).
- [ ] iPad 1 (A4): giải FLAC 16 / 44,1 và 24 / 96, Opus; RAM khi AVPlayer đệm WAV — anh sẽ thử sau khi app hoàn thiện; máy yếu quá
  thì làm bản riêng cho iOS 5.
- [ ] OTA.

## 17/11/2026 (đêm) — DNS mã hoá, sửa lời chạy, màn hình khoá

- **DNS mã hoá** (M17): DoH → DoT, NextDNS (hồ sơ riêng của GramMusic) → Cloudflare → Google, lỗi hết thì DNS của máy — chép từ
  ChimToTV; dùng cho MTProxy / SOCKS5 ghi tên miền và lrclib.net. Cài đặt → DNS mã hoá (mặc định bật). iPad 1 (iOS 5): DoH 157 ms,
  DoT 214 ms, dự phòng đủ bậc.
- **Lời bài hát** (M19): tìm thêm bản có lời chạy khi `api/get` chỉ có lời thường; ghi rõ khi chỉ có lời thường; dòng đang hát nằm
  giữa khung (iOS 5 tính lệch). iPad 1: lời chạy theo nhạc đúng.
- **Màn hình khoá** (M18): «1:23 / 4:05 · nguồn» ở dòng thứ ba, mỗi giây. Thanh tiến trình thật cần tweak SpringBoard — chờ anh quyết.
- Lõi: 36 phép thử `gm-core` + 27 `gm-net` (thêm 11 của DNS mã hoá) đạt; gọi thật 6 máy chủ DNS + LRCLIB đạt.
- Ghi nhận khi anh nghe thật trên iPhone 5: mở bài FLAC chưa có trên máy lúc mạng chậm mất ~23 giây (AVPlayer đọc thử cuối tệp WAV →
  lõi phải tải phần cuối tệp FLAC trước) — app không đứng nữa (M16) nhưng chờ lâu; cần làm riêng.

### Còn lại

- [ ] Tweak SpringBoard cho thanh tiến trình / nút trên màn hình khoá (nếu anh đồng ý).
- [ ] Mở bài FLAC nhanh hơn khi mạng chậm (phần cuối tệp WAV lúc AVPlayer đọc thử).
- [ ] «Nhạc trên máy» (app Nhạc) trên máy thật; iPad 1 đo giải mã + RAM.
- [x] Đưa lên GitHub (itovn/GramMusic) + trang giới thiệu kiểu iOS 6 như GramMini (xem dưới).
- [ ] OTA.

## 17/11/2026 — phát hành 1.0.0

- Tweak **GramLockControl** (M20) trên màn hình khoá: lời đang hát / «♪ Nhạc không lời», thanh tiến trình kéo để tua, nút điều khiển;
  bật / tắt trong Cài đặt của iPhone. Anh thử trên iPhone 5: lời hiện đúng; tên bài phía trên hết giật.
- Bản **1.0.0** (`tools/build-release.sh`): 3,1 MB, không còn mã thử; `symcheck` 340 ký hiệu, thiếu 0 trên iOS 6.1.4 và iOS 5.1.1.
- GitHub: repo công khai **itovn/GramMusic** (nhánh `main`), bản phát hành **v1.0.0** kèm `.deb`, trang giới thiệu kiểu iOS 6 +
  nguồn Cydia ở nhánh `gh-pages` (https://itovn.github.io/GramMusic/ — `tools/update-cydia-repo.sh`). Trước khi đưa lên: rà khoá /
  IP / số điện thoại / mã chat / tên bài trong thư viện của anh khỏi tài liệu.

- Ảnh giao diện cho trang giới thiệu (dữ liệu mẫu tự đặt: 5 nguồn, 19 bài, bìa và lời tự vẽ / tự viết): Thư viện, Nguồn, Đang phát
  có lời chạy — chụp bằng lệnh thử `snap` (vẽ lại cửa sổ app, không dính nút AssistiveTouch), ghép thanh trạng thái 9:41.
- Cydia không hiện gói trong nguồn GramMusic: máy đang cài bản thử `1.0.0-1+debug` (mới hơn bản trên nguồn → Cydia coi là gói cài tay).
  Cài lại bản phát hành là hết; sau mỗi lần thử trên máy anh dùng thì cài lại bản phát hành.

## 17/11/2026 — phát hành 1.0.1

- Dòng mô tả gói trong Cydia viết không dấu (M21) — Cydia trên iOS 5 / 6 hiện chữ có dấu ở dòng này thành «?». App không đổi gì.
  `symcheck` 340 ký hiệu, thiếu 0 trên iOS 5.1.1 (iPad 1).
- Cùng sửa nguồn Cydia + `control` của Côn Bằng TV Legacy, TubeMini, Roza, GramMini.

## 17/11/2026 — mất tiếng khi mạng chập chờn (M22)

- Anh báo «đang nghe thì app tự thoát» — thật ra mediaserverd bị iOS giết (chờ quá 10 giây), app coi là bài lỗi rồi dừng. Sửa ở lõi
  (giữ kết nối khi tải lỗi tạm thời, `play.ready`) và app (tua khi có dữ liệu, không hỏi thuộc tính AVPlayerItem mỗi lần, dựng lại
  trình phát khi mediaserverd khởi động lại). Thử trên iPhone 5: hết bị giết; giết tay mediaserverd → 3 giây sau phát tiếp đúng chỗ.
  Lõi 38 test, gm-net 27 test.

### Còn lại

- [ ] Ảnh màn hình khoá có khung GramLockControl cho trang giới thiệu (cần anh bấm nút nguồn).
- [ ] OTA (tự cập nhật như GramMini); mở bài FLAC nhanh hơn khi mạng chậm; iPad 1 đo giải mã + RAM.
