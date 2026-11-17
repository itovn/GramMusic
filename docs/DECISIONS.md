# GramMusic — Các quyết định kỹ thuật

Thiết kế đầy đủ: [THIET-KE.md](THIET-KE.md). Các quyết định của GramMini (D1–D27) áp dụng cho phần chép sang (mạng, phiên, bộ
dựng, giao diện) — ghi ở đây những gì riêng của GramMusic.

## M1. Fork GramMini, giữ tên `gm-` / `GM` — 17/11/2026

- Chép `core/vendor` (grammers vá), `core/patches`, `crates/gm-net`, `gm-core/src/session.rs`, khung `cache.rs` / `service.rs` /
  `gm-ffi`, `tools/rust`, `tools-local/ios.sh`, app: GMCompat, GMShims, GMLog, GMCore, GMImages (rút gọn), GMTheme (thêm biểu
  tượng nhạc), màn Đăng nhập / Proxy.
- Không dùng chung (submodule): hai app đi hai nhịp phát hành; sửa lỗi phần chung thì chép tay (tên tệp / lớp giống nhau → so
  khác biệt dễ).
- Bundle `vn.grammusic.app`, dữ liệu `~/Library/GramMusic`, gói `.deb` không phụ thuộc mobilesubstrate (không cần tweak
  thông báo).

## M2. Thư viện = quét cả tài khoản (theo MeloVault) — 17/11/2026

- MeloVault (phân tích APK 0.6.132): tìm kiếm toàn cục lọc âm thanh rồi lọc tài liệu có đuôi nhạc, trang 100, lưu con trỏ; mục
  Nguồn lập từ chat của các bài tìm được. Người dùng không phải tự chọn chat — đúng thói quen «mở app là có nhạc».
- GramMusic làm tương tự bằng `messages.searchGlobal` (gọi thẳng TL, không qua `GlobalSearchIter` của grammers — cần lưu / nạp lại
  con trỏ `offset_rate` / `offset_peer` / `offset_id`). Tin nhắn đã lưu và kênh công khai thêm tay quét riêng bằng
  `messages.search`.
- Chỉ mục lưu JSON theo nguồn, ghi gộp 5 trang một lần (kênh lớn khỏi ghi lại cả tệp mỗi trang).

## M3. Phát qua cổng HTTP nội bộ có Range, tệp thưa + bảng khúc — 17/11/2026

- MeloVault (TDLib) tải tuần tự từ đầu và đọc tệp đang lớn dần → tua tới chỗ chưa tải phải chờ. GramMusic lấy khúc 256 KB đúng
  chỗ trình phát cần (`upload.getFile` theo offset) → tua ngay; bài nghe trọn thành tệp đầy đủ trong đệm.
- Cổng phát viết bằng Rust trong lõi (không phải ObjC như TubeMini): dữ liệu nằm sẵn ở lõi, thử được trên PC bằng `cargo test`.

## M4. AVQueuePlayer + nạp sẵn bài kế — 17/11/2026

- iOS 6 không cho app ở nền *bắt đầu* phát khi tiếng đã dừng (TubeMini D34). Hết bài mà phải tạo item mới → có quãng im → iOS có
  thể treo app. Nạp sẵn bài kế vào hàng của AVQueuePlayer ngay khi bài đang phát sẵn sàng → AVQueuePlayer tự chuyển, tiếng không
  đứt.
- **Chưa thử trên máy thật** (cần máy dựng) — mục kiểm thử bắt buộc: khoá màn, nghe liền 10 bài trên iPhone 5 và iPad 1.

## M5. FLAC / OGG / Opus để giai đoạn 2 — 17/11/2026

- AVPlayer iOS 5 / 6 không phát. Bản đầu vẫn đưa vào thư viện (nhãn đỏ, bấm thì báo) để người dùng thấy đủ bài; giai đoạn 2 giải
  mã trong lõi, cổng phát trả WAV.

## M6. Biểu tượng theo mẫu anh đưa — 17/11/2026

- Anh yêu cầu biểu tượng kết hợp logo Telegram với âm nhạc. Em báo điều khoản Telegram API mục 2.4: «You must not use the official
  Telegram logo for your app» (GramMini D14 cũng vì vậy mà tự vẽ) và vẽ thử một bản tự làm (nốt nhạc có móc là máy bay giấy).
- Anh chốt theo mẫu `Tham_Khao/logo/logo1.png`: ô vuông bo góc viền crôm kiểu iOS 6, nền kính xanh sọc chéo, máy bay giấy trắng
  giữa tai nghe, nốt nhạc nhiều màu, khoá Sol, sóng âm. `tools/icon/make_icon.py from-ref …` cắt đúng ô biểu tượng (mép ngoài viền
  đo bằng độ sáng, lùi 3 px bỏ viền tối của nền), góc ngoài trong suốt (bán kính 21,6 % > mặt nạ iOS ~17,5 %) → các cỡ iOS 5 / 6.
- Rủi ro còn lại (anh đã biết): máy bay giấy trong mẫu giống hình máy bay của logo Telegram. Bố cục khác hẳn logo (tai nghe, nốt
  nhạc, sóng âm, ô vuông viền crôm — không phải máy bay trắng trên đĩa xanh phẳng), tên app không có chữ «Telegram». Nếu Telegram
  phàn nàn / khoá api_id thì thay máy bay bằng hình khác.

## M7. Quét từng chat thay cho tìm toàn cục — 17/11/2026

- Thử thật trên iPhone 5: `messages.searchGlobal` với từ khoá rỗng + bộ lọc âm thanh trả **0 tin**, bộ lọc tài liệu trả 5 tin (trả
  hết một lần) — không dựng được thư viện (giả định ở M2 sai: MeloVault dùng `searchMessages` của TDLib).
- Làm: đọc danh sách chat (`messages.getDialogs`, cả Lưu trữ), rồi `messages.search` trong **từng chat** (lượt âm thanh, lượt tài liệu
  có đuôi nhạc). Quét toàn bộ ghi chat đã xong (`sync.json` → `done`) → tắt app giữa chừng thì quét tiếp; lấy bài mới chỉ quét chat
  có tin mới từ lần trước (`chats`: lần quét của từng chat), trong chat dừng khi gặp 10 bài đã có hoặc tới tin cũ hơn lần quét
  trước. Mở app (đã đăng nhập) tự lấy bài mới sau 3 giây; bản đổi dữ liệu quét (`VERSION`) tự quét toàn bộ lại một lần. Mỗi lần quét
  cập nhật tên / ảnh / cờ «có chủ đề» của nguồn đã có.
- iPhone 5: 7 chat quét hết trong 5,8 giây (một nhóm 315 tệp âm thanh + 102 tài liệu — 35 trong đó là nhạc). Bỏ tệp m3u / cue / lrc.

## M8. FLAC / OGG Vorbis: lõi giải mã thành WAV 16 bit cho AVPlayer — 17/11/2026

- Thư viện thật của anh: 240 / 377 bài là FLAC (64 %). AVPlayer iOS 5 / 6 không đọc FLAC; tự viết bộ phát AudioQueue (bỏ AVPlayer)
  thì phải làm lại phát nền / hàng chờ / màn hình khoá → giữ AVPlayer, lõi đổi định dạng.
- symphonia 0.5 (Rust thuần, `flac` + `ogg` + `vorbis`): tệp gốc đọc qua bộ đệm khúc (chờ tải khúc cần), WAV ghi vào tệp thưa + `.map`
  như tệp gốc, URL đuôi `.wav`. Tua: symphonia tìm khung FLAC bằng tìm nhị phân theo đầu khung (không cần SEEKTABLE) → giải từ đó,
  bỏ mẫu thừa → đúng từng mẫu. Hi-res chia tần số (96 → 48, 192 → 48 kHz), 24 bit → 16 bit, > 2 kênh → 2 kênh đầu. Giữ 3 bài đã giải
  gần nhất.
- PC (tệp mẫu `tests/data`, không có SEEKTABLE): WAV **khớp từng bit với ffmpeg** (SHA-256), đoạn tua trước = đúng đoạn đó của bản
  giải tuần tự, hi-res / OGG đúng thông số.
- iPhone 5: FLAC 6 phút 26 MB tải + giải cả bài trong 35 giây (~10 lần thời gian thực), phát nền (khoá màn) không giật; chuyển bài
  FLAC sẵn sàng sau 94–121 ms; tua 0:12 → 4:00 có tiếng lại trong < 4 giây.
- AVPlayer đọc WAV: 2 byte đầu, cả tệp, 5 KB cuối, rồi từng đoạn ~700 KB mỗi ~4 giây, nhiều kết nối. Cổng phát phải **biết khi
  AVPlayer ngắt kết nối** lúc đang chờ dữ liệu (trước đó kết nối cũ treo → AVFoundation đứng luồng chính 9 giây khi chuyển bài).
- Chưa: Opus (`gm-opus` của GramMini có libopus — khi cần), APE / WavPack / DSD / DTS (nhãn đỏ).

## M9. Danh sách theo chủ đề nhóm, tạo list nhanh — 17/11/2026

- Anh yêu cầu. Bài ghi chủ đề (đầu trả lời của tin: `forum_topic`, `reply_to_top_id` / `reply_to_msg_id` — như GramMini); nguồn ghi
  cờ «có chủ đề» + danh sách chủ đề (`messages.getForumTopics`). Nguồn có chủ đề → màn Chủ đề (số bài từng chủ đề, «Tất cả bài của
  nhóm», chạm để nghe riêng chủ đề); «Mỗi chủ đề thành một playlist» (tên «Nhóm · Chủ đề»; bấm lại thì thêm bài mới vào playlist cũ).
- Tạo list nhanh: menu «⋯» của mọi danh sách — «Lưu danh sách / kết quả tìm thành playlist» (`pl.fromQuery`: lõi tự lấy khoá theo
  đúng bộ lọc, khỏi chuyển 5000 khoá qua lại), «Chọn nhiều bài…» (phát, phát tiếp theo, thêm vào playlist, tạo playlist mới, tải về
  máy); Hàng chờ → «Lưu hàng chờ thành playlist».
- iPhone 5: nhóm «GramMusic» 15 chủ đề có nhạc / 374 bài; `pl.fromQuery` một chủ đề → đủ 14 bài.

## M10. Tải theo nhịp nghe — 17/11/2026

- Anh yêu cầu: tệp lớn (100 MB) phát sớm, tải tiếp trong lúc nghe, dừng nghe thì thôi tải. Trước đó: đang nghe là tải dồn cả bài
  (AVPlayer kéo trước tới ~33 MB sau vài giây), tạm dừng vẫn tải tiếp vì AVPlayer giữ kết nối.
- App báo vị trí đang phát (`play.pos`: mỗi giây khi phát; lúc dừng, tua, đổi bài). Lõi chỉ tải / trả dữ liệu tới **60 giây** trước
  chỗ đang phát (`AHEAD_SECS`, tối thiểu 1 MB); kết nối xin đoạn xa hơn thì chờ (vẫn theo dõi ngắt kết nối); luồng giải FLAC cũng chờ ở
  mép. Bài chưa tới lượt (AVQueuePlayer nạp sẵn bài kế): 2 MB đầu. Cuối tệp (1 MB) luôn được đọc — AVPlayer đọc thử lúc mở. «Tải về
  máy» vẫn tải trọn.
- iPhone 5, MP3 156,6 MB (1 giờ 30): nghe 18 giây → đã tải 2,8 MB; tạm dừng 20 giây → vẫn 2,8 MB; phát tiếp 30 giây → 3,5 MB; phát
  liền mạch.

## M11. Thông số tệp trên màn Đang phát — 17/11/2026

- Anh yêu cầu (ảnh mẫu: MP3 · 44.1kHz · 320 kbps · 324,0 MB). Lõi đọc phần đầu tệp **đã có trên máy** (không tải thêm): MP3 (khung
  đầu sau ID3v2: tần số, kbit/s CBR; có Xing / VBRI → VBR, hiện «~» + bitrate trung bình), FLAC (STREAMINFO: tần số, bit), WAV (fmt),
  OGG (đầu Vorbis), M4A (mục mô tả mẫu: «alac» → ALAC lossless, «mp4a» → AAC). Thiếu thì bitrate trung bình = cỡ × 8 / thời lượng.
- App: hàng ô bo tròn dưới tên bài — định dạng (lossless tô màu chủ đạo) · tần số · bit (lossless / khác 16) · bitrate · dung lượng;
  khúc đầu chưa tải thì hỏi lại sau 2 giây. iPhone 5: «FLAC · 44.1kHz · 16-bit · 606 kbps · 25,9 MB», «MP3 · 48kHz · ~241 kbps ·
  156,6 MB».

## M12. Tuỳ chọn «Tải trước» trong Cài đặt — 17/11/2026

- Anh yêu cầu. Cài đặt → Phát nhạc → **Tải trước**: 30 giây / 1 phút (mặc định, như M10) / 2 phút / 5 phút / Cả bài. Lõi giữ số giây
  (`settings.json` → `ahead_secs`, 0 = cả bài) và dùng ngay cho bài đang phát (đánh thức kết nối đang chờ, tải thêm cho đủ khoảng mới).
  «Cả bài» = giới hạn bằng cỡ tệp (tải hết khi đang nghe; vẫn thôi tải khi trình phát ngừng đọc — như trước M10).
- Lệnh `settings.get` / `settings.set` (chung cho các tuỳ chọn của lõi: `ahead_secs`, `lyrics`).

## M13. Opus: libopus của GramMini + bộ đọc Ogg tự viết — 17/11/2026

- symphonia 0.5 chưa giải Opus. Dùng lại `gm-opus` (libopus 1.4, đã chạy trên iOS 6 trong GramMini), tự đọc Ogg (`oggopus.rs`): trang
  (kiểm CRC), ghép gói qua nhiều trang, chỉ luồng có OpusHead; tổng số mẫu = granule trang cuối − pre-skip.
- Tua: tìm nhị phân theo granule tới trang kết thúc trước điểm cần **80 ms** (Opus cần «làm nóng» bộ giải — RFC 7845), giải từ gói
  nối tiếp trang đó, bỏ mẫu thừa theo mốc → đúng từng mẫu. Lõi giải mã tách thành nguồn mẫu chung (`Src`: symphonia cho FLAC / Vorbis,
  Opus) — phần ghi WAV, nhịp nghe, tua giữ nguyên.
- Đuôi `.opus` → Opus; `.ogg` / `.oga` → xem trang đầu có OpusHead không. Luôn 48 kHz; 1–2 kênh (Opus nhiều kênh chưa làm).
- Thử trên PC (`ogg_opus_exact_length_aligned_and_seek`): 960 000 mẫu đúng như ffmpeg (bỏ pre-skip 312), khớp pha sóng sin (lệch
  pre-skip sẽ ra nhiễu lớn), đoạn giải sau khi tua lệch < 600 / 32768 so với giải tuần tự. iPhone 5: tệp Opus 20 giây giải hết 1,7 giây.

## M14. Lời bài hát từ LRCLIB, phải bật mới gửi — 17/11/2026

- **LRCLIB** (lrclib.net): miễn phí, không cần khoá, nhiều lời chạy theo thời gian (LRC). Lõi gọi HTTPS (reqwest + rustls + bộ chứng
  chỉ gốc đóng sẵn — iOS 5 / 6 không tự vào được trang HTTPS mới; ring vá armv7 như GramMini, cùng phiên bản reqwest / ring / rustls).
- **Riêng tư**: gửi tên bài, ca sĩ, thời lượng của bài đang xem lời tới LRCLIB → mặc định **tắt**; bật ở Cài đặt (hoặc chạm «Chạm để
  bật» trong khung lời) có hộp hỏi nói rõ gửi gì. Bản đã lưu (`lyrics/<khoá>.json`) vẫn hiện khi tắt (không gửi gì).
- Tìm: `api/get` (tên + ca sĩ chính + thời lượng ±2 giây) với tên gốc rồi tên bỏ phần trong ngoặc («(Official MV)», «[FLAC]», số thứ
  tự); không thấy thì `api/search` chọn bản lệch thời lượng ≤ 5 giây, ưu tiên lời chạy. Có lời giữ mãi; không thấy 7 ngày sau mới hỏi
  lại.
- App: nút «Lời» trên màn Đang phát (hoặc chạm ảnh bìa) — khung lời đè lên ảnh bìa (phủ tối, chữ trắng); lời chạy tô dòng đang hát,
  tự cuộn (anh tự cuộn thì 4 giây sau mới cuộn theo), chạm dòng để tua; lời thường cuộn tay.
- Thư viện của anh chủ yếu nhạc Hoa / Việt — LRCLIB có ít (một bài Hoa đem thử không có); bài tiếng Anh phổ biến có đủ lời chạy (thử trên
  PC: «Hello — Adele» 48 dòng, 0,6 giây).

## M15. Nhạc trên máy: app Nhạc + tệp trong máy — 17/11/2026

- **Nguồn → Trên máy**: «Nhạc trên máy» (thư viện app Nhạc — bài chép từ iTunes; MediaPlayer, iOS 5 / 6 không hỏi quyền) và «Tệp
  trên máy» (duyệt thư mục như GramMini, chỉ hiện thư mục + tệp phát được; chạm một tệp → phát cả thư mục từ tệp đó).
- Khoá `ipod:<persistentID>`: app tự lo (tên, ảnh bìa, `assetURL` cho AVPlayer); bài DRM / chưa tải từ iCloud không có assetURL → nhãn
  đỏ, báo không phát được. Khoá `file:<đường dẫn>`: lõi lo (`local.rs`) — tên / ca sĩ theo tên tệp, thời lượng đọc từ đầu / cuối tệp
  (FLAC STREAMINFO, Xing / CBR của MP3, granule Ogg, mvhd của M4A, WAV); mp3 / m4a / wav… → `file://`, FLAC / OGG / Opus → lõi giải
  từ tệp trên máy qua cổng phát như bài Telegram (khoá nội bộ `local_<băm đường dẫn + cỡ + giờ sửa>`).
- Bài trên máy không có Yêu thích / Playlist / Tải về / Nghe gần đây (các danh sách của lõi chỉ giữ bài Telegram) — nút mờ đi; vẫn
  vào hàng chờ, phát nền, lời bài hát (gửi kèm tên / ca sĩ / thời lượng).
- iPhone 5: Opus, FLAC 16/44,1 và 24/96, OGG Vorbis, mp3 từ `Documents` đều phát; «Ca si - Bai mp3.mp3» → tên «Bai mp3», ca sĩ «Ca si».

## M16. Tạo AVPlayerItem ở nền — 17/11/2026

- Đo trên iPhone 5: `[AVPlayerItem playerItemWithURL:]` + gắn vào trình phát **đứng luồng chính 5,6–9,3 giây** khi mở bài FLAC chưa
  có trên máy: AVPlayer đọc thử cả cuối tệp WAV ngay lúc tạo item, lõi phải tải phần cuối tệp FLAC từ Telegram rồi giải mới trả được.
  Đây là vụ «luồng chính đứng ~5 giây» ghi ở PROGRESS hôm trước.
- Sửa: `AVURLAsset` + `loadValuesAsynchronouslyForKeys:@[playable, duration, tracks]` (đọc ở luồng nền) rồi mới tạo item trên luồng
  chính — cả bài kế nạp sẵn. Lần đo sau: «nạp tệp 511 ms (nền), gắn item 6 ms».
- Kèm: ảnh bìa lấy lỗi (mất mạng) nhớ 30 giây mới hỏi lại — danh sách vẽ lại mỗi giây theo trình phát, trước đó hỏi lõi ~3 lần / giây.

## M17. DNS mã hoá (DoH / DoT) — chép từ ChimToTV — 17/11/2026

- Anh hỏi có DoH / DoT như GramMini không: GramMini **không có**; phần này ở ChimToTV (`linux/core/src/doh.rs`) / Côn Bằng TV. Anh
  đưa mã hồ sơ NextDNS riêng cho GramMusic → `keys/nextdns.txt` (không lên git; `sync.sh` chép lên máy dựng; `gm-net/build.rs` đóng
  vào tệp chạy, không in ra nhật ký — `scrub`).
- `gm-net/src/dns.rs`: chuỗi **DoH** NextDNS → Cloudflare → Google rồi **DoT** cùng thứ tự; máy chủ gọi bằng IP cố định; NextDNS dùng
  máy chủ gần nhất (hỏi chính NextDNS «dns.nextdns.io», nhớ 6 giờ); giữ kết nối TLS để dùng lại; máy chủ lỗi nghỉ 2 phút; NXDOMAIN là
  trả lời thật; hồ sơ chặn bằng 0.0.0.0 thì hỏi máy chủ sau; lỗi hết thì **DNS của máy** (app không bao giờ hỏng vì DNS mã hoá bị
  chặn). Khác ChimToTV: chứng chỉ gốc **đóng sẵn** (webpki-roots — kho của iOS 5 / 6 cũ), bỏ phần «vượt chặn», bỏ regex / futures.
- Dùng cho mọi tên miền lõi tự tra: MTProxy / SOCKS5 ghi bằng tên miền (`connector.rs`, `socks5.rs`), reqwest (lrclib.net — bộ tra
  `DohResolve`), OTA sau này. Telegram đi thẳng nối bằng IP nên không qua đây.
- Cài đặt → **DNS mã hoá** (mặc định bật; `settings.json` → `secure_dns`). Lệnh thử `net.dns` (`host`, `fail` — giả chặn máy chủ).
- Đo: PC — 6 máy chủ đều trả lời 93–149 ms. **iPad 1 (iOS 5.1.1, A4)**: NextDNS gần nhất ở Việt Nam, `lrclib.net` qua DoH NextDNS
  157 ms; giả chặn hết DoH → DoT NextDNS 214 ms; giả chặn cả 6 → DNS của máy; tên miền không có → `NO_DOMAIN`.

## M18. Màn hình khoá iOS 5 / 6: thời gian chạy thành chữ — 17/11/2026

- Anh báo màn hình khoá thiếu thanh tiến trình và nút ⏯ ⏮ ⏭. iOS 5 / 6: thanh tiến trình trên màn hình khoá chỉ có cho app Nhạc
  (mọi app từ iOS 7; kéo để tua từ iOS 9.1 — `MPRemoteCommandCenter`), TubeMini đã thử: không thấy. Nút ⏮ ⏯ ⏭ không hiện sẵn: **bấm
  Home 2 lần** khi đang khoá (hoặc thanh đa nhiệm, vuốt sang phải) — GramMusic đã nhận nút (`beginReceivingRemoteControlEvents`,
  ⏮ / ⏭ = bài trước / bài sau).
- Làm được trong app: dòng thứ ba trên màn hình khoá (dòng album) ghi **«1:23 / 4:05 · nguồn»**, cập nhật mỗi giây khi máy khoá / app ở
  nền (giữ sẵn `MPMediaItemArtwork` để khỏi tạo lại ảnh mỗi giây).
- Thanh tiến trình thật / nút hiện sẵn trên màn hình khoá cần tweak SpringBoard (như GMNotify của GramMini) — chờ anh quyết.

## M19. Lời bài hát: ưu tiên lời chạy, căn giữa dòng đang hát — 17/11/2026

- Anh báo lời không chạy theo nhạc trên iPhone 5. Nguyên nhân: bài anh mở (một bài saxophone có hát kèm) LRCLIB `api/get`
  trả bản **chỉ có lời thường** (không mốc thời gian); hai bài khác là bài không lời. Sửa: `api/get` không có lời chạy thì tìm thêm
  (`api/search` theo cặp, rồi «tên bài ca sĩ») bản **có lời chạy** cùng thời lượng (±5 giây) — bài đó ra 43 dòng lời chạy. Bản
  lưu: lời chạy / bài không lời giữ mãi; lời thường / không thấy 7 ngày hỏi lại; bản lưu cũ (chưa có `"v": 2`) hỏi lại ngay.
- Lời thường: khung ghi «Lời chưa có mốc thời gian — cuộn tay để xem».
- iPad 1: `scrollToRowAtIndexPath:…Middle` của iOS 5 đặt dòng đang hát sát mép trên khi bảng có `contentInset` → tự tính độ cuộn cho
  dòng nằm giữa khung. Thử bằng tệp «Adele - Hello.mp3» (LRCLIB có lời chạy): dòng đậm chạy theo nhạc, nằm giữa khung.

## M20. Tweak GramLockControl — điều khiển + lời bài hát trên màn hình khoá (anh đồng ý 17/11/2026) — 17/11/2026

- Anh đặt tên **GramLockControl**, muốn bật / tắt được trong Cài đặt của iPhone (phòng xung đột với tweak khác), và hiện cả **lời bài
  hát** trên màn hình khoá. Làm như GMNotify của GramMini (D18): `tweak/GramLockControl` là dự án con của app, cùng gói
  `vn.grammusic.app` (phụ thuộc `mobilesubstrate`, `preferenceloader`); lần đầu cài cần khởi động lại SpringBoard.
- Trong SpringBoard: móc `-[SBAwayView didMoveToWindow]` (lớp tự có) và `layoutSubviews` (lớp không có → thêm hàm vào lớp, gọi hàm của
  lớp cha lúc chạy — không đụng UIView của view khác); thiếu lớp thì đứng ngoài. Khung nằm ngay trên thanh «trượt để mở khoá»
  (`SBAwayLockBar`, chèn dưới thanh để bàn phím mật mã che được): dòng lời đang hát + dòng kế, thanh tiến trình kéo để tua, nút bài
  trước / phát / bài sau. Chỉ hiện khi GramMusic là app đang phát (`SBMediaController nowPlayingApplication`).
- App ↔ tweak: app ghi `~/Library/GramMusic/lockstate.plist` (bài, vị trí lúc ghi, đang phát?, lời chạy) khi đổi bài / phát–dừng / tua
  và báo Darwin `vn.grammusic.lockcontrol/state`; tweak tự cộng thời gian trôi. Phát / dừng / bài trước–sau qua `SBMediaController`
  (đánh thức được app đang treo ở nền), dự phòng `…/cmd.toggle|next|prev`; tua `…/cmd.seek` + `notify_set_state` (mili giây).
- Cài đặt của iPhone → GramLockControl (PreferenceLoader, plist): Bật (tắt là gỡ khung, không làm gì nữa), Lời bài hát, Thanh tiến
  trình, Nút điều khiển.
- Thử: iPad 1 (iOS 5.1.1) và iPhone 5 (iOS 6.1.4) — nạp được, tìm thấy `SBAwayView` + `SBAwayLockBar`, gắn khung, SpringBoard không lỗi;
  iOS 6 có đủ `SBMediaController` (nowPlayingApplication, togglePlayPause, changeTrack:), `SBAwayController` (restartDimTimer,
  isDimmed).
- **Bài học**: không khoá máy từ xa bằng `-[SBAwayController lock]` trên iOS 6 — chỉ bật cờ «đã khoá» mà không hiện màn hình khoá,
  SpringBoard kẹt, nút Home không ăn (anh báo máy đơ; gỡ bằng khởi động lại SpringBoard). Đã bỏ lệnh thử đó; màn hình khoá thử bằng tay.
- Sau khi anh thử (17/11, iPhone 5): lời trên màn hình khoá hiện đúng. Sửa thêm:
  - Tên bài phía trên màn hình khoá **giật**: app cập nhật `nowPlayingInfo` mỗi giây (M18) → iOS 6 vẽ lại cả cụm tên bài, chữ chạy của
    tên dài bị đặt lại từ đầu. Có tweak hiện thanh tiến trình thì app thôi ghi thời gian vào dòng thứ ba và chỉ cập nhật khi đổi bài /
    phát–dừng / tua; không có tweak thì 10 giây một lần.
  - «Mất lời» với hai bài saxophone: LRCLIB có 20 bản ghi mỗi bài, **tất cả đánh dấu không lời** — đúng là nhạc
    không lời. Khung màn hình khoá giờ ghi «♪ Nhạc không lời» thay vì để trống.
  - Lõi: khung lời trong app và màn hình khoá cùng hỏi một bài → gộp thành một lần (khoá theo bài, bên sau đọc bản đã lưu); kết nối
    HTTPS nghỉ quá 20 giây thì bỏ (một lần tra mất 21 giây vì dùng lại kết nối đã chết khi mạng iPhone chập chờn).

## M21. Dòng mô tả gói trong Cydia viết không dấu — 17/11/2026

- Anh báo: Cydia → Đa Phương Tiện, dòng giới thiệu dưới tên gói bị lỗi tiếng Việt — cả Côn Bằng TV / TubeMini / Roza cũng vậy.
  Ảnh chụp trên iPhone 5: «Nghe nh??c t?? Telegram tr?n iPhone / …», Roza «Nh??n tin Zalo tr?n …», GramMini cũng thế.
- Nguyên nhân: Cydia 1.1.30 (bản trên máy iOS 5 / 6) lấy dòng `Description` qua `libapt-pkg` (apt7-lib 0.7.25.3) — hàm `LongDesc`
  đổi chữ từ UTF-8 sang bảng mã của tiến trình (`nl_langinfo(CODESET)`); iOS không có locale UTF-8 (không có `/usr/share/locale`)
  nên là US-ASCII, ký tự không đổi được thành «?». `Name`, `Section` Cydia đọc thẳng nên không bị. Trong thư viện có đúng chuỗi
  «conversion from 'UTF-8' to '%s' not available», `nl_langinfo`, `iconv_open`.
- Chọn (anh duyệt): `Description` trong `app/control` chỉ dùng chữ ASCII — không dấu, không «—»; giới thiệu có dấu để ở trang mô tả
  (Depiction là trang web, hiện đúng). Bản **1.0.1** chỉ đổi dòng này. Cách khác (tweak trong Cydia báo bảng mã UTF-8) sửa được mọi
  nguồn nhưng chỉ trên máy có cài — không làm.
- Làm cùng cho Côn Bằng TV Legacy, TubeMini, Roza, GramMini (anh nhờ): sửa `control` trong mã nguồn (bản sau tự đúng) và sửa dòng `Description`
  trong `Packages` của nguồn Cydia đang chạy (tính lại `.gz` / `.bz2` / mã băm trong `Release`), không dựng lại app của các dự án đó.
- Bài học khi sửa nguồn Cydia trên máy Windows: git có `core.autocrlf=true` — repo không có `.gitattributes` (TubeMini) thì `Packages`
  lấy về mang CRLF; băm theo bản LF và commit với `-c core.autocrlf=false`, kẻo mã băm trong `Release` lệch với tệp trên trang.

## M22. Không để mediaserverd chờ quá 10 giây — mất tiếng khi mạng chập chờn — 17/11/2026

- Anh báo (iPhone 5, 18:52): đang nghe thì «app tự thoát». App không crash (vẫn chạy); **mediaserverd** (dịch vụ âm thanh của iOS) bị giết:
  báo cáo `/Library/Logs/CrashReporter/mediaserverd-*.plist`, mã `0xbe18d1ee`, lý do `fig rpc timeout -- itemremote_CopyProperty
  [ Server_Duration ]`. Hôm đó có 4 lần (16:06, 16:28, 17:26, 18:52), lần nào GramMusic cũng đang phát.
- Cơ chế (iOS 5 / 6): AVPlayer chạy trong mediaserverd; app hỏi thuộc tính của AVPlayerItem (`duration`, `loadedTimeRanges`…) là
  gọi sang mediaserverd **và chờ**. Quá ~10 giây không trả lời thì iOS giết mediaserverd → mọi item báo lỗi `-11819`
  (AVErrorMediaServicesWereReset), trình phát cũ hỏng. Bản cũ coi đó là bài lỗi → nhảy qua 2 bài nữa (cũng lỗi) → dừng hẳn.
- mediaserverd chặn khi **mở một đoạn đọc mới** (sau khi tua, hoặc khi cổng phát đóng kết nối) mà đoạn đó chưa có dữ liệu — mạng
  chậm, lõi chưa tải / giải kịp. Đang đọc dở mà phải chờ (nhịp nghe M10) thì không sao. Giải ký hiệu ảnh chụp trạng thái: lần 18:52 luồng
  chính kẹt ở `-[GMPlayer duration]` ← `updateNowPlaying` ← KVO `rate` (vừa hết dữ liệu); lúc thử lại (mở bài ở giây 222 rồi tua)
  luồng chính kẹt trong chính AVFoundation (khối của nó trên hàng đợi chính hỏi `PlayableTimeIntervals`) — app có tránh gọi cũng không
  đủ, phải tránh để mediaserverd chặn lâu.
- Sửa:
  - Lõi: lỗi tải tạm thời (NETWORK / TIMEOUT / FLOOD_WAIT, «quá lâu») giữa lúc trả WAV / tệp gốc → **giữ kết nối**, chờ 2 giây rồi
    thử lại (trước: đóng → AVPlayer mở lại đoạn đọc → chặn). Mở WAV hỏng thì thử lại 3 lần trước khi trả 500.
  - Lõi: lệnh `play.ready {key, pos, wait}` — chờ tới khi khúc chứa giây `pos` (+ khúc sau) đã giải / đã tải; hết giờ trả
    `ready: false`.
  - App: tua (và mở bài ở giữa bài — Nghe tiếp, dựng lại trình phát) chỉ ra lệnh cho AVPlayer khi `play.ready` báo có; trong lúc chờ:
    vị trí hiện chỗ mới, nhịp nghe dời tới đó, mở giữa bài thì chưa phát (khỏi nghe đoạn đầu bài).
  - App: không hỏi `AVPlayerItem.duration` / `currentTime` / `rate` mỗi lần cần — độ dài đọc từ AVURLAsset lúc nạp, vị trí từ
    `addPeriodicTimeObserver` (+ thời gian trôi, tối đa 1 giây), tốc độ lúc trình phát báo đổi.
  - App: mediaserverd khởi động lại (`kAudioSessionProperty_ServerDied` — có cả iOS 5; hoặc item lỗi `-11819`) → bỏ trình phát cũ,
    chờ 1,5 giây, tạo trình phát mới, mở lại đúng bài ở đúng chỗ. Quá 3 lần trong 2 phút thì tạm dừng và báo (khỏi vòng lặp).
- Thử trên iPhone 5 (bản thử): mở lại bài FLAC ở giây 228 — có dữ liệu, tua ngay; `killall -9 mediaserverd` lúc đang phát → 3 giây sau
  phát tiếp ở đúng giây 277; tua tới giây 290 của bài FLAC chưa tải → chờ 3 giây rồi tua. Không còn báo cáo mediaserverd nào. Trước khi
  sửa, mở bài ở giây 222 lúc mạng chậm làm mediaserverd bị giết lặp lại mỗi 11 giây (6 lần liền).
- Test lõi: `transient_errors_keep_connection_open` (khúc giữa «mất mạng» 1,5 giây — bản cũ cắt ngang WAV sau ~150 ms),
  `ready_at_waits_for_data_at_position` (WAV từ FLAC, tệp gốc; mất mạng → `false` khi hết giờ chờ). Tệp mẫu mới
  `tests/data/noise44_8s.flac` (8 giây nhiễu, 1,4 MB — đủ nhiều khúc để có «khúc giữa»).

## M23. Mở lại cổng phát khi iOS thu hồi lúc app nằm nền — 17/11/2026

- Anh báo (iPad mini 1, bản 1.0.1): để app nghỉ lâu rồi vào lại thì không phát được bài nào. Nhật ký: mọi item lỗi
  `NSURLErrorDomain -1004` «Không thể kết nối máy chủ» tới `http://127.0.0.1:63899/…`; lõi không nhận yêu cầu nào (không có dòng
  `phát: GET`); app coi là bài lỗi, nhảy qua cả chục bài.
- Nguyên nhân: tạm dừng → app nằm nền bị treo; iOS thu hồi cổng nghe (listening socket) của app bị treo — cổng phát chết hẳn, lõi vẫn
  đưa URL cũ. Côn Bằng TV Legacy gặp đúng chuyện này (bản vá `0002-revive-local-ports`) — làm theo cùng cách.
- Sửa:
  - Lõi: cổng phát mở lại được — `Streams::revive` / lệnh `stream.revive`: thử kết nối vào chính cổng, chết thì bỏ việc nhận cũ
    và mở lại **đúng số cổng cũ** nếu được (item đang mở dùng tiếp địa chỉ cũ), không thì cổng khác. `url()` (mọi `play.url`) cũng
    kiểm trước khi đưa địa chỉ.
  - App: `applicationDidBecomeActive` gọi `stream.revive`; `play` kiểm cổng trước khi cho AVPlayer đọc tiếp (bấm phát ở màn hình khoá /
    tai nghe đánh thức app ngay ở nền — không qua `applicationDidBecomeActive`; lõi không trả lời trong 1,5 giây thì phát luôn);
    item lỗi `-1004` → mở lại cổng, phát lại đúng bài ở đúng chỗ (một lần mỗi bài) thay vì nhảy bài.
- Test lõi `revive_reopens_reclaimed_port`: giả thu hồi (bỏ việc nhận → kết nối bị từ chối) → `revive` mở lại đúng số cổng, URL cũ
  phát được; cổng còn sống thì không đụng. Lệnh thử `stream.drop` (giả thu hồi trên máy thật).
- Thử trên máy: iPhone 5 — đóng cổng (`stream.drop`) rồi bấm phát / đưa app xuống nền rồi mở lại → mở lại đúng cổng, phát tiếp. iPad mini 1
  (tái hiện thật): dừng nhạc, app nằm nền 6 phút, máy tự khoá 7 phút → lõi ghi «nhận kết nối lỗi: Bad file descriptor (os error 9)»
  (iOS đã thu hồi cổng); mở app → mở lại đúng cổng 64109 sau 1 ms, bấm phát → phát bình thường. Nằm nền 10 phút mà máy không khoá
  (iPhone 5) thì iOS chưa thu hồi.


## M24. Rà soát toàn bộ app, sửa lỗi — bản 1.0.3 — 18/11/2026

- Anh nhờ rà soát cả app tìm lỗi và tự sửa, tự thử, tự phát hành. Rà theo 5 phần (lõi phát / giải mã, lõi dịch vụ / thư viện, mạng,
  trình phát + tweak, giao diện); mỗi lỗi đọc lại mã cho chắc trước khi sửa, lỗi không tái hiện được trên mã thì bỏ.
- Lỗi làm app tắt (bản phát hành `panic = abort` — panic nào trong lõi cũng tắt app):
  - `info.rs`: tệp WAV cụt / lạ (chữ «fmt » sát cuối phần đầu tệp) → cắt mảng quá cỡ. Sửa: `get`.
  - `lyrics.rs` `main_artist`: tìm dấu ngăn trong chuỗi `to_lowercase` (đổi số byte với «İ»…) rồi cắt chuỗi gốc → cắt giữa ký tự.
    Sửa: `to_ascii_lowercase` (giữ số byte).
  - «Chọn nhiều bài» mà danh sách tải lại (tìm, xếp, quét xong…): bảng giữ bố cục cũ, `sections` mới ít hơn → `objectAtIndex`
    quá cỡ; bài đã chọn đọc lại theo dòng → thao tác nhầm bài. Sửa: lúc chọn thì hoãn tải lại tới khi thôi chọn, khoá ô tìm / nút xếp,
    bài đã chọn giữ theo khoá.
  - iPad: bảng thao tác «⋯» (popover) còn mở khi bấm «Quay lại» → delegate đã giải phóng. Sửa: đóng bảng khi rời màn, bỏ delegate
    khi giải phóng.
  - Khung lời: bài đang chờ thông tin (khoá nil) mà bấm «Bật» lời → `dictionaryWithObject:nil`. Sửa: không tải khi chưa có khoá, bỏ
    kết quả của bài trước.
- Mất nhạc / phát sai:
  - Opus: đọc lỗi (mạng mất lúc chờ khúc) bị coi là hết tệp → phần còn lại ghi thành im lặng và nhớ luôn. Sửa: lỗi đọc báo lên (chữ
    «đọc tệp lỗi» như FLAC — stream.rs giữ kết nối chờ mạng, M22); gói Opus hỏng giới hạn 128 KB.
  - Xoá đệm / tự dọn / xoá bản tải xoá tệp mà lõi vẫn nhớ bản giải (Pcm) / bài (Track) → đọc lỗi mãi tới khi mở lại app. Sửa:
    chỉ dùng lại khi tệp còn trên đĩa; `forget` bỏ cả bản giải.
  - Bản giải của tệp trên máy (`local_<băm>.pcm.wav`) không có khoá `<peer>_<id>` → «Xoá đệm» không biết đang dùng. Sửa: so cả
    theo tên tệp.
  - Hai yêu cầu cùng lúc (play.ready + GET của AVPlayer) mở hai bản giải cùng một bài, hai luồng ghi đè một tệp. Sửa: một lượt mở
    mỗi bài. Track còn có luồng giải cầm thì không bỏ khỏi bảng (không mở bản thứ hai).
  - **WAV gốc trên Telegram** (gửi dạng tài liệu, không có thời lượng): sau dữ liệu âm thanh có chunk thông tin ~1,3 MB (ảnh bìa
    ID3); iOS đọc phần đó trước khi phát, lõi chỉ trả 1 MB cuối, phần còn lại chờ nhịp nghe → iOS chờ ~19 giây rồi lỗi -1001,
    bỏ bài (5 lần trong nhật ký iPhone 5 từ trước 1.0.2). Thêm: không có thời lượng thì lõi ước 64 KB/s (WAV thật 176 KB/s) → phát
    được ~30 giây là đứng. Sửa (`RawMeta`): đọc phần đầu tệp — bitrate thật (chỉ phần âm thanh / thời lượng) và chỗ bắt đầu phần
    thông tin sau dữ liệu (WAV: chunk sau «data»; M4A: moov sau mdat) — phần đó trả luôn.
  - Mở giữa bài (Nghe tiếp, dựng lại trình phát, -1004): gắn item xong là `play` → nghe đầu bài trong lúc chờ dữ liệu ở chỗ cần tua.
    Tua ngay lúc item vừa sẵn sàng (dữ liệu có sẵn) có khi bị AVPlayer bỏ qua → nghe đầu bài 3–4 giây rồi mới nhảy. Sửa: mở giữa
    bài thì chưa phát; tua bằng `seekToTime:completionHandler:` (iOS 5), xong mới phát, chưa tới chỗ thì tua lại (tối đa 2 lần).
  - `dropCurrent`: `removeAllItems` khi `_curItem` còn → KVO currentItem → nil tới ngay, tưởng hết bài → tự sang thêm một bài (bấm
    bài kế lúc gần hết bài / bài chưa biết độ dài). Sửa: bỏ `_curItem` trước.
  - Hẹn giờ «Hết bài này» chỉ xét ở `itemEnded` — KVO hết bài tới trước thì phát thêm một bài. Sửa: xét ở `advanceAfterEnd`.
  - Hết hàng chờ, tua rồi bấm phát → phát từ đầu (item cũ đã rời trình phát). Sửa: nhớ chỗ, bấm phát mở lại ở đó.
  - Bị app khác giành tiếng mà iOS không báo hết gián đoạn → app vẫn «đang phát», bấm hai lần mới dừng. Sửa: hết gián đoạn khi người
    dùng bấm phát / khi có tiếng; `playing` không tính lúc bị gián đoạn.
  - `mediaServicesReset:` dựng lại trình phát sau 1,5 giây theo ý người dùng *lúc mất dịch vụ* → bấm dừng / tua / sang bài trong lúc
    đó bị bỏ qua. Sửa: theo `_wantPlay` / `_resumeAt` lúc dựng lại.
  - Sang bài bằng nút màn hình khoá / tai nghe khi app ở nền: không xin thêm giờ chạy (như khi hết bài) → có thể bị treo trước khi có
    tiếng. Sửa: `holdBackground` trong `loadCurrent`.
  - Đang chờ dữ liệu: nút phát / dừng bị ẩn sau vòng quay → không dừng được. Sửa: vòng quay thay hình, nút vẫn bấm được.
  - Tweak GramLockControl: GramMusic chết khi màn hình khoá đang mở → khung vẫn chạy «đang phát». Sửa: 2 giây hỏi lại iOS một lần.
- Thư viện / đăng nhập:
  - «Lấy bài mới» lọc chat theo mốc chung `last_sync` — chat lỗi giữa chừng (FLOOD_WAIT, PEER_UNKNOWN…) bị bỏ hẳn tới khi có tin
    mới. Sửa: theo mốc của từng chat; FLOOD_WAIT dừng lượt. Quét dở (mạng rớt ở trang 2) → lần sau trang đầu toàn bài đã có, đếm
    «10 bài đã có» dừng sớm → bỏ sót phần sau mãi. Sửa: có mốc ngày thì đi tới mốc, không đếm.
  - Kênh nhận theo chuỗi «-100…» → nhóm thường có id bắt đầu bằng 100 bị coi là kênh, tin bị xoá không bao giờ bỏ khỏi thư viện.
    Sửa: theo cách đánh số Bot API (kênh ≤ −10¹²).
  - «Mỗi chủ đề thành một playlist»: bấm «Quay lại» giữa chừng → nguồn rỗng = mọi nguồn → playlist gom cả thư viện. Sửa: giữ nguồn từ
    đầu.
  - `auth.signIn`: lỗi mạng / FLOOD_WAIT làm mất lượt đăng nhập → «Chưa gửi mã». Sửa: giữ lượt (trừ mã hết hạn); app: mã hết hạn → về
    bước xin mã; đang kiểm mã / mật khẩu thì khoá «Quay lại».
  - Phiên bị Telegram huỷ: tệp đã dời nhưng lần lưu sau (app ra nền) ghi lại auth key chết. Sửa: `FileSession::disable`.
  - `session.json` hỏng → lõi không khởi động, app không dùng được gì. Sửa: dời tệp hỏng sang bên, chạy như chưa đăng nhập.
  - Ghi tệp (phiên, thư viện): `sync_all` trước khi đổi tên (mất điện / máy treo không còn tệp rỗng); `flush` lỗi thì giữ lại để
    ghi sau; ghi thư viện ở luồng blocking (`flush_lib`, một lượt một lúc) — không chặn luồng duy nhất của lõi (cổng phát) khi quét
    kênh lớn.
- Mạng: tra DNS mã hoá tối đa 5 giây cho một kết nối (connector chỉ cho 10 giây cả tra lẫn nối — chuỗi DNS bị nuốt gói làm proxy
  không nối được), lần tra chạy riêng để kết quả / máy chủ chậm vẫn được nhớ; MTProxy fake-TLS từ chối → lần sau đo lại giờ (giờ máy
  vừa được chỉnh thì độ lệch cũ sai, trước đây chờ 6 giờ); giờ máy lùi → bỏ hạn nhớ DNS cũ.
- App: hàm nhận kết quả / sự kiện của lõi (luồng Rust) bọc `@autoreleasepool` — iOS 5 / 6 không tự có bể cho luồng lạ, mỗi sự kiện
  (tiến độ tải ~mỗi giây) rò đối tượng tạm. Thông báo nhỏ trong bảng đã cuộn hiện đúng chỗ (cộng `bounds.origin`). Hàng chờ không
  tải lại dòng ngay trong `cellForRowAtIndexPath:`. Chạm thanh thời gian mà không kéo thì không tua. «Phát tiếp theo» khi chưa phát
  gì giữ đúng thứ tự đã chọn. Nhật ký không ghi đường dẫn tệp trên máy.
- Không sửa: thêm tệp `keys/nextdns.txt` sau khi đã dựng (chưa có thư mục `keys/`) thì máy dựng không tự dựng lại `gm-net` — chỉ liên
  quan máy dựng (máy dựng / PC đều có sẵn `keys/`).
- Test lõi thêm: `truncated_headers_do_not_panic`, `read_error_is_not_end_of_file` (Opus), `raw_meta_reads_wav_and_m4a`,
  `raw_wav_tail_metadata_served_without_pace`, `clear_keeps_busy_local_decode`, `main_artist` với «İ», `remove_songs` theo cách
  đánh số kênh. gm-core 44, gm-net 27.
