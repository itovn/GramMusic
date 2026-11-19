# GramMusic — Phương án thiết kế (bản nháp 17/11/2026)

App **nghe nhạc từ Telegram** (không chính thức) cho **iPhone / iPad / iPod touch iOS 5 – 6 đã jailbreak**. Lõi **Rust** nói
chuyện thẳng với Telegram bằng MTProto, vỏ **Objective-C / UIKit** dựng bằng Theos — cùng nền với GramMini. Một gói `.deb` chạy cả
iOS 5 và 6.

| Tham khảo | Lấy gì |
| --- | --- |
| `E:\DEV_CODE\GramMini` | Toàn bộ phần Telegram: lõi Rust (grammers đã vá, `gm-net`: đi thẳng / MTProxy `dd` `ee` / SOCKS5, giờ NTP), phiên lưu tệp, đăng nhập (mã, 2FA), proxy, cầu JSON `gm_init` / `gm_call`, bộ nhớ đệm, bộ dựng iOS 5 / 6 (target armv7, build-std, shim, `mio_unsupported_force_waker_pipe`), giao diện iOS 6 + kính (GMTheme), GMCompat, GMImages, màn đăng nhập / proxy |
| MeloVault 0.6.132 (Android, chỉ có APK) | Cách dựng thư viện nhạc và danh sách tính năng (phân tích ngược hành vi — không chép mã): quét toàn tài khoản bằng tìm kiếm toàn cục lọc âm thanh rồi lọc tài liệu có đuôi nhạc, trang 100, lưu con trỏ để quét tiếp; tab Thư viện / Nguồn / Playlist / Đã tải; tìm không dấu; Mới nhất / Tên A–Z + thanh chữ cái; Yêu thích, playlist; Nghe tiếp; hẹn giờ tắt; lặp / trộn; đồng bộ playlist qua kênh riêng (để sau) |
| `E:\DEV_CODE\TubeMini` | Máy chủ HTTP nội bộ `127.0.0.1` có Range cho AVPlayer; bài học phát nền iOS 6 (D34: app ở nền không *bắt đầu* phát được nếu item chưa sẵn sàng) |

---

## 0. Tóm tắt các quyết định

1. **Dùng lại nguyên kiến trúc GramMini**: client dày, lõi Rust trên máy, không server; MTProto qua TCP (không dính lỗi TLS của
   iOS 6); MTProxy / SOCKS5 có từ màn đăng nhập.
2. **Chép (fork) chứ không dùng chung mã với GramMini**: `gm-net`, grammers đã vá, `session.rs`, khung `service.rs` / `gm-ffi`,
   bộ dựng, GMTheme… chép sang; phần nhạc viết mới. Tên crate / tiền tố lớp giữ `gm-` / `GM` (GramMusic cũng viết tắt GM) → sửa
   lỗi ở bên nào thì chép sang bên kia dễ. Hai app là hai tiến trình, hai thư mục dữ liệu (`~/Library/GramMini`,
   `~/Library/GramMusic`), hai bundle id — không đụng nhau.
3. **Thư viện = quét cả tài khoản** (như MeloVault), không bắt người dùng chọn chat: quét **từng chat** (`messages.search` lọc
   `InputMessagesFilterMusic`, rồi `InputMessagesFilterDocument` giữ tệp có đuôi nhạc) — tìm toàn cục với từ khoá rỗng không trả gì
   (DECISIONS M7); kênh công khai thêm tay quét riêng. Mục **Nguồn** tự lập từ các chat có nhạc; ẩn được từng nguồn; nhóm có chủ đề
   xem / lưu playlist theo chủ đề (M9).
4. **Phát qua cổng HTTP nội bộ của lõi** (`http://127.0.0.1:<cổng>/s/<mã>/<khoá>.<đuôi>`, có Range) cho **AVQueuePlayer**. Lõi
   lấy từng khúc 256 KB bằng `upload.getFile`, ghi vào tệp thưa trên đĩa + bảng khúc → **tua được ngay** cả khi bài chưa tải
   xong (MeloVault phải tải tuần tự từ đầu), bài nghe trọn thì nằm trong bộ nhớ đệm, lần sau nghe không cần mạng.
5. **Phát nền**: `UIBackgroundModes = audio`, phiên `AVAudioSessionCategoryPlayback`, bài kế được nạp sẵn vào hàng của
   AVQueuePlayer ngay khi bài đang phát sẵn sàng → hết bài là chuyển liền, không có lúc «im» để iOS treo app.
6. **Định dạng**: iOS 5 / 6 phát thẳng mp3, AAC / ALAC (m4a, mp4), ADTS, WAV, AIFF, CAF; **FLAC và OGG Vorbis: lõi giải mã thành WAV
   16 bit** (symphonia, tua được — DECISIONS M8). Opus, APE, DSD, DTS: nhãn đỏ. Tải theo nhịp nghe (60 giây phía trước — M10).
7. **Không làm** (ít nhất ở bản đầu): video / «nghe video như nhạc», TikTok, nhiều tài khoản, widget, bộ chỉnh âm (EQ) — xem mục 12.

---

## 1. Máy đích

Như GramMini: iPad 1 (A4, 256 MB, iOS 5.1.1) là máy yếu nhất để đặt ngân sách; iPhone 5 (iOS 6.1.4), iPad mini 1 (iOS 6.1.3).

- Mở app tới Thư viện (đọc chỉ mục trên máy): ≤ 2 giây với 10.000 bài. Cuộn mượt: bảng lấy **từng trang 100 bài** từ lõi, không
  nạp cả thư viện vào ObjC.
- Bấm bài tới có tiếng: ≤ 3 giây trên mạng 3G (khúc đầu 256 KB), ngay lập tức với bài đã có trên máy.
- RAM app ≤ 40 MB khi nghe; chỉ mục 10.000 bài ≈ 3–4 MB trong lõi.

---

## 2. Kiến trúc

```text
┌───────────────────────────────────────────────────────────────────────┐
│ App ObjC / UIKit (Theos, armv7, iOS 5.0)                                │
│  GMRootController: Thư viện │ Nguồn │ Playlist │ Đã tải │ Cài đặt + thanh phát nhỏ │
│  GMNowPlaying · GMQueue · GMSongsViewController (danh sách trang 100 bài)│
│  GMPlayer: AVQueuePlayer · MPNowPlayingInfoCenter · điều khiển từ xa     │
└──────────┬───────────────────────────────────────────▲──────────────────┘
   gm_call (JSON)                         sự kiện JSON │      AVPlayer: GET + Range
┌──────────▼───────────────────────────────────────────┴───────▲─────────┐
│ libgm_ffi.a (Rust, armv7, build-std, panic=abort)               │         │
│  gm-core: service (lệnh) · library (chỉ mục, tìm, xếp) · sync (quét)     │
│           stream (cổng 127.0.0.1 + tệp thưa + bảng khúc) · playlists     │
│           cache (đệm, bài đã tải) · session (phiên — GramMini)           │
│  gm-net (GramMini): thẳng / MTProxy dd ee / SOCKS5, obfuscated2, NTP     │
│  grammers 0.12 (vá: connector, build_peer_map)                           │
└──────────┬──────────────────────────────────────────────────────────────┘
           │ MTProto 2.0 / TCP
           ▼
    Máy chủ Telegram: messages.searchGlobal / search, upload.getFile (DC giữ tệp)
```

---

## 3. Thư viện nhạc

### 3.1 Bài hát là gì

Một tin có tệp:

- có thuộc tính âm thanh (`DocumentAttributeAudio`, không phải tin thoại), hoặc
- là tài liệu có đuôi `mp3 m4a m4b flac wav ogg opus aac aif aiff alac` (bài gửi dạng «tệp», hay gặp với FLAC).

Tên bài / ca sĩ / thời lượng lấy từ thuộc tính; thiếu thì đoán từ tên tệp (`01. Ca sĩ - Tên bài.mp3` → bỏ số thứ tự, tách «Ca sĩ
- Tên»). Không có album (Telegram không có trường này) — giai đoạn 2 đọc thẻ ID3 trong khúc đầu.

**Khoá bài** = `<peer>_<id tin>` (peer kiểu Bot API như GramMini: người `123`, nhóm `-123`, kênh `-100123`). Bài trùng tệp (cùng
mã tài liệu, chuyển tiếp sang nhiều chat) chỉ hiện một lần ở «Thư viện» (giữ bản mới nhất); xem riêng một nguồn thì đủ bài.

### 3.2 Quét (`sync.rs`, `service.rs`)

| Bước | Cách |
| --- | --- |
| Quét toàn bộ (lần đầu sau đăng nhập, hoặc bấm) | `messages.searchGlobal` lượt `music` rồi lượt `document`, mỗi trang 100, nhịp 120 ms; mỗi 5 trang ghi chỉ mục + con trỏ (`offset_rate`, `offset_peer`, `offset_id`) vào `sync.json` → tắt app giữa chừng thì lần sau quét tiếp |
| Lấy bài mới (mở app, kéo xuống, nút làm mới) | Hai lượt từ tin mới nhất xuống, gặp liền 10 bài đã có thì dừng |
| Tin nhắn đã lưu, kênh công khai thêm tay | `messages.search` trong chat đó (hai lượt); chưa quét hết lần nào thì quét hết |
| Bài mới gửi vào chat khi app đang mở | Vòng cập nhật của grammers (như GramMini) → thêm vào chỉ mục ngay |
| Tin bị xoá | `MessageDeleted` → bỏ khỏi chỉ mục; bài phát lỗi «tin đã xoá» thì báo |
| FLOOD_WAIT | Chờ đúng số giây Telegram bảo (≤ 10 phút), thử lại tối đa 3 lần, báo lên app («Telegram bảo chờ X giây…») |

### 3.3 Lưu trữ

| Tệp | Nội dung |
| --- | --- |
| `sources.json` | Nguồn: peer, tên, loại (self / user / bot / group / channel), @tên, ảnh, hiện / ẩn, thêm tay, lần quét |
| `library/<peer>.json` | Bài của một nguồn (tin mới → cũ): id, tên, ca sĩ, thời lượng, cỡ, mime, tên tệp, đuôi, ngày, mã tài liệu, có ảnh bìa |
| `sync.json` | Lượt / con trỏ của lần quét toàn bộ đang dở, đã quét toàn bộ chưa, lần quét cuối, lỗi |
| `playlists.json` | Playlist (tên, các bài + bản chép tên / ca sĩ để vẫn hiện khi nguồn bị gỡ), Yêu thích, Nghe gần đây (200) |
| `offline.json` | Khoá các bài «Tải về máy» |

Không dùng SQLite (GramMini D10 cũng chưa đưa libsql lên armv7): JSON đọc một lần lúc mở lõi, tìm / xếp trong bộ nhớ. Thứ tự xếp
được đệm theo (nguồn, cách xếp) tới khi thư viện đổi.

### 3.4 Tìm, xếp

- Tìm **không dấu**: chữ đã bỏ dấu (tiếng Việt + Latin có dấu, `đ` → `d`) của «tên + ca sĩ + tên tệp»; mọi từ phải có mặt.
- Xếp: **Mới** (ngày gửi), **A–Z** (tên — bỏ dấu, bỏ ký tự đầu không phải chữ, bỏ «The »), **Ca sĩ**. Hai cách sau có mục chữ
  cái A–Z, «#» cuối → thanh chữ cái của UITableView (iOS 5 có sẵn).

---

## 4. Phát nhạc

### 4.1 Cổng phát nội bộ (`stream.rs`)

- `play.url` → `http://127.0.0.1:<cổng>/s/<mã ngẫu nhiên>/<khoá>.<đuôi>`; mã đổi mỗi lần mở lõi (app khác không đọc ké).
- Hỗ trợ `GET` / `HEAD`, `Range: bytes=a-b | a- | -n`, trả 200 / 206 / 416, `Content-Type` theo đuôi (AVPlayer iOS 5 / 6 cần đúng
  kiểu), `Connection: close`.
- Tệp `music/<khoá>.<đuôi>.part` (thưa, đủ cỡ) + `.map` (khúc nào đã có). Khúc 256 KB thoả ràng buộc của `upload.getFile` (bội
  4 KB, 1 MB chia hết cho limit, không vắt ranh giới 1 MB). Đủ khúc → đổi tên `<khoá>.<đuôi>`.
- Ưu tiên khúc ở vị trí trình phát đang đọc (tua → tải ngay chỗ mới), rồi tải nốt phần còn lại khi bài còn được nghe; không còn ai
  đọc quá 6 giây thì thôi (trừ bài «Tải về máy»). Mỗi bài 2 khúc song song, cả lõi tối đa 4.
- `file_reference` hết hạn → đọc lại tin lấy vị trí mới; lỗi mạng / FLOOD_WAIT → thử lại; tin bị xoá → lỗi hẳn.
- Thử trên PC (`cargo test -p gm-core`): thăm dò `bytes=0-1` kiểu AVPlayer, tua vắt qua khúc, cả bài, mã sai → 404, ngoài cỡ →
  416, lỗi mạng rồi thử lại, «tắt app» rồi tải tiếp từ `.map`.

### 4.2 Trình phát (`GMPlayer`)

- **AVQueuePlayer** (iOS 4.1+). Bài đang phát sẵn sàng → nạp sẵn bài kế (`insertItem:afterItem:`), lõi bắt đầu tải bài kế.
- Hàng chờ = danh sách khoá (tới 5000 — «Phát hết» cả thư viện), thông tin bài lấy theo cửa sổ quanh bài đang phát.
- **Trộn** (bài đang phát lên đầu, còn lại xáo Fisher–Yates; tắt trộn trả thứ tự gốc), **Lặp**: không / cả hàng / một bài.
- **Màn hình khoá / tai nghe**: `MPNowPlayingInfoCenter` (iOS 5+: tên, ca sĩ, nguồn, thời lượng, vị trí, ảnh bìa) +
  `beginReceivingRemoteControlEvents`.
- **Cuộc gọi đến**: `AVAudioSessionDelegate` (iOS 5 chưa có thông báo ngắt của iOS 6) — gọi xong tự phát tiếp nếu iOS cho phép.
  **Rút tai nghe**: tạm dừng (`kAudioSessionProperty_AudioRouteChange`).
- **Nghe tiếp**: hàng chờ (≤ 3000 bài quanh bài đang phát), bài, vị trí lưu 10 giây một lần / khi dừng / xuống nền; mở app
  không tự phát, thanh phát nhỏ ghi «Nghe tiếp từ 1:23»; bài chưa tải đủ vẫn tua được tới chỗ cũ (Range).
- **Hẹn giờ tắt**: 15 / 30 / 45 / 60 phút, hết bài này.
- Bài không phát được (định dạng, tin đã xoá, mạng): báo ngắn, sang bài sau; lỗi liền 3 bài thì dừng.
- Xuống nền lúc bài đang mở dở (chưa có tiếng): xin thêm thời gian chạy tới khi có tiếng (TubeMini D34).

---

## 5. Giao diện

Giao diện iOS 6 + kính của GramMini (D23 — anh đã duyệt): hình nền loang màu, thanh trên bóng, bảng kính, chế độ tối.

| Mục | Có gì |
| --- | --- |
| **Thư viện** | Ô tìm (không dấu), «Phát hết», «Ngẫu nhiên», xếp Mới / A–Z / Ca sĩ + thanh chữ cái, dòng trạng thái quét; ô bài: ảnh bìa, tên, «ca sĩ · 3:45 · nguồn», nhãn đỏ cho định dạng chưa phát được, dấu đã tải; nút «⋯»; kéo xuống (iOS 6) / nút làm mới = lấy bài mới |
| **Nguồn** | Trạng thái quét + «Lấy bài mới» / «Quét toàn bộ» (/ «Dừng quét»); danh sách nguồn (Tin nhắn đã lưu đầu tiên, rồi theo số bài) với ảnh, loại, số bài, công tắc hiện / ẩn trong Thư viện; chạm để xem bài của nguồn; «Thêm» kênh công khai bằng @tên / link |
| **Playlist** | Yêu thích, Nghe gần đây, playlist của bạn (tạo, đổi tên bằng ⓘ, vuốt để xoá) |
| **Đã tải** | Bài «Tải về máy» (xếp A–Z), tiến độ đang tải |
| **Cài đặt** | Tài khoản, Lấy bài mới / Quét toàn bộ, Proxy, Dữ liệu và bộ nhớ, Giao diện (Sáng / Tối / Tự động), phiên bản, Đăng xuất |
| **Thanh phát nhỏ** | Trên thanh dưới: ảnh bìa, tên, ca sĩ (hoặc «Nghe tiếp từ …»), phát / dừng, tiếp, vạch tiến độ; chạm mở Đang phát |
| **Đang phát** | Ảnh bìa lớn (vuốt trái / phải = bài sau / trước), tên, ca sĩ, nguồn + vị trí trong hàng, thanh thời gian, Trộn / Lùi / Phát / Tiếp / Lặp, Yêu thích, Thêm vào playlist, Tải về máy, Hẹn giờ tắt, âm lượng + AirPlay, Hàng chờ (kéo đổi thứ tự, vuốt bỏ) |
| **Bảng «⋯»** | Phát tiếp theo, Thêm vào hàng chờ, Yêu thích, Thêm vào playlist…, Tải về máy / Xoá bản đã tải, Mở nguồn, (trong playlist) Bỏ khỏi playlist |

Thanh dưới tự làm (GMRootController — API chứa màn con của iOS 5) thay UITabBarController để đặt thanh phát nhỏ giữa nội dung và
thanh dưới. Biểu tượng mục / nút tự vẽ (GMArt, GMTheme). Biểu tượng app: theo mẫu anh đưa — ô bo góc viền kính neon xanh → tím, máy bay giấy có
nút phát, sóng âm, nốt nhạc (`tools/icon/make_icon.py`, DECISIONS M25; mẫu trước: M6).

---

## 6. Bộ nhớ đệm và tải về máy

- **Đã tải về máy** (`dl.add`): tải lần lượt từng bài, không bao giờ tự xoá; mất mạng / tắt app thì lần sau tải tiếp (`.map`).
- **Bài đã nghe**: là đệm — giới hạn mặc định 1 GB, giữ 30 ngày, đầy thì xoá bài lâu không nghe nhất; máy còn < 300 MB thì dọn
  tới khi còn 500 MB (như GramMini D15). Bài đang phát / đang tải không bị xoá.
- Cài đặt → Dữ liệu và bộ nhớ: dung lượng theo loại, xoá từng loại, giới hạn, hạn giữ.

---

## 7. Hợp đồng lõi ↔ app

Xem [FFI.md](FFI.md). Hai hàm C như GramMini (`gm_init`, `gm_call`), JSON hai chiều.

---

## 8. Giai đoạn 2 — định dạng khác, lời bài hát, nhạc trên máy

| Việc | Cách |
| --- | --- |
| **FLAC** (phổ biến ở kênh lossless) | Lõi giải mã (claxon / symphonia — Rust thuần) sang PCM, cổng phát trả **WAV** (`/w/<khoá>.wav`, Range quy đổi sang mẫu; FLAC có bảng tìm kiếm thì tua được, không thì giải lại từ đầu tới chỗ cần). Thử tốc độ giải mã trên A4 trước |
| **OGG Vorbis / Opus** | Opus: `gm-opus` của GramMini (libopus) đã có; Vorbis: lewton. Cùng đường WAV |
| **Thẻ ID3 / MP4** | Đọc album, số bài, năm, **ảnh bìa nhúng** từ khúc đầu đã tải → bài không có ảnh thu nhỏ trên Telegram vẫn có bìa |
| **Lời bài hát** (LRCLIB, chỉ khi người dùng bật) | Lõi có rustls (bộ gốc Mozilla) như OTA của GramMini; `GET https://lrclib.net/api/get?track_name&artist_name&duration`, lời đồng bộ `[mm:ss.xx]` chạy theo bài |
| **Nhạc trên máy** | MPMediaQuery (thư viện iPod — bài không DRM phát bằng `ipod-library://`), tệp nhạc trong máy (app ở /Applications không sandbox — trình duyệt tệp của GramMini D16) |
| **OTA, nguồn Cydia** | Như GramMini (GitHub Releases + công cụ setuid, kiểm SHA-256 + chữ ký số) — có từ 1.0.4 (DECISIONS M26) |

## 9. Giai đoạn 3 (nếu cần)

Đồng bộ playlist / Yêu thích giữa các máy qua Telegram (MeloVault: kênh riêng «… Sync» tắt tiếng, lưu trữ; mỗi thay đổi là một
tin chữ `#mvsync` có tổng kiểm, đánh dấu nhân quả) · bộ chỉnh âm (cần tự giải mã + AudioUnit, bỏ AVPlayer) · nhiều tài khoản ·
chủ đề diễn đàn · iPad hai khung.

---

## 10. Bảo mật, điều khoản

Như GramMini mục 10: api_id / api_hash riêng (`keys/telegram.txt`, không lên git), tên không chứa «Telegram», màn Cài đặt ghi
«ứng dụng không chính thức»; thử bằng tài khoản phụ trước; tôn trọng FLOOD_WAIT (quét có nhịp, chờ đúng lời Telegram); phiên
0600; **một phiên chỉ dùng ở một nơi** (D24 của GramMini); nhật ký không ghi tên bài / số điện thoại.

## 11. Dựng, kiểm thử

Như GramMini mục 11: máy Ubuntu có Theos + SDK 9.3 + Rust nightly; `tools/build-app.sh` → `.deb`; `tools-local/deploy.sh <máy>`
cài lên máy thử; `tools-local/symcheck.sh` kiểm ký hiệu trên iOS 5 và iOS 6. Lõi thử trên PC: `cargo test`, `gm-cli`
(`login`, `svc`, `repl` — gọi thẳng lệnh JSON của app).

## 12. Rủi ro

| Rủi ro | Giảm thế nào |
| --- | --- |
| AVPlayer iOS 5 / 6 kén máy chủ HTTP (Content-Type, Range) | Trả đúng kiểu theo đuôi, 206 + Content-Range, thử `bytes=0-1`; TubeMini đã chạy AVPlayer qua 127.0.0.1 |
| Chuyển bài ở nền bị iOS treo app | Nạp sẵn bài kế vào AVQueuePlayer; xin thêm giờ khi bài đang mở dở lúc xuống nền; cần thử thật trên iPhone 5 / iPad 1 (khoá màn 1 giờ liền) |
| Tài khoản nhiều nhạc → quét lâu / FLOOD_WAIT | Trang 100, nhịp 120 ms, chờ đúng FLOOD_WAIT, con trỏ lưu được, báo tiến độ |
| `searchGlobal` không trả Tin nhắn đã lưu / kênh chưa vào | Quét riêng Tin nhắn đã lưu và kênh thêm tay |
| FLAC nhiều → thư viện đầy bài không phát được | Nhãn đỏ ngay ở ô bài; giai đoạn 2 giải mã |
| RAM iPad 1 với thư viện lớn | Trang 100 bài, chỉ mục gọn trong lõi, ảnh bìa ImageIO thu nhỏ, bộ đệm ảnh 8 MB |
