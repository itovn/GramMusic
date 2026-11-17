# GramMusic — Hợp đồng giữa app (Objective-C) và lõi Rust

Lõi: `core/crates/gm-ffi` → `libgm_ffi.a` (dịch vụ ở `core/crates/gm-core/src/service.rs`). App: `app/Core/GMCore.m`. Cách gọi
giống hệt GramMini.

## Hàm C

```c
typedef void (*gm_cb)(const char *json, void *ctx);
int  gm_init(const char *config_json, gm_cb on_event, void *ctx);
void gm_call(const char *request_json, gm_cb on_result, void *ctx);
```

- `gm_init` — một lần lúc mở app. `config_json`: `{"data_dir", "device_model", "system_version", "app_version", "lang"}`. Trả 0 =
  được; 1 = cấu hình sai; 2 = bản dựng thiếu api_id (`keys/telegram.txt`); 3 = không mở được dữ liệu; 4 = đã mở.
- `gm_call` — chạy nền. `request_json`: `{"m": "<nhóm.lệnh>", "a": {…}}`. `on_result` được gọi **đúng một lần** với
  `{"ok": <kết quả>}` hoặc `{"err": {"code", "msg", …}}`.
- Dữ liệu ở `data_dir` (`~/Library/GramMusic`): xem THIET-KE mục 3.3.

## Kiểu dữ liệu

- **Khoá peer**: chuỗi số kiểu Bot API (`"123"` người, `"-123"` nhóm thường, `"-100123"` kênh / siêu nhóm).
- **Khoá bài**: `"<peer>_<id tin>"`. Nhạc trên máy (DECISIONS M15): `"file:<đường dẫn>"` (tệp trong máy — lõi đọc tệp) và
  `"ipod:<persistentID>"` (thư viện app Nhạc — app tự lo, không gửi cho lõi).
- `Song`: `{"key", "peer", "id", "title", "artist", "dur" (giây), "size" (byte), "ext", "date" (Unix), "thumb" (có ảnh bìa),
  "ok" (phát được — gồm FLAC / OGG Vorbis lõi giải mã), "src" (tên nguồn), "topic" (chủ đề của nhóm diễn đàn, 0 = không), "fav", "dl": null | "cached" (nghe trọn — có trong đệm) | "queued" (chờ / đang tải
  về máy) | "offline" (đã tải về máy)}`; bài không còn trong thư viện (trong playlist): `{"key", "title", "artist", "dur",
  "missing": true, "ok": false}`. Tệp trên máy: như trên + `"local": true`, `"path"`, `"src": "Tệp trên máy"` (tên / ca sĩ theo tên
  tệp, thời lượng đọc từ tệp; không có `fav` / `dl`).
- `Source`: `{"peer", "name", "kind": self|user|bot|group|channel, "username", "photo", "photo_dc", "enabled" (hiện trong Thư
  viện), "manual" (thêm tay), "count" (số bài đã quét), "synced", "forum" (nhóm có chủ đề)}`.

## Lệnh

| Lệnh | Tham số | Kết quả |
| --- | --- | --- |
| `auth.state` / `auth.sendCode` / `auth.signIn` / `auth.checkPassword` / `auth.logout` | như GramMini | như GramMini; đăng nhập xong lõi tự quét toàn bộ |
| `proxy.get` / `proxy.set` / `proxy.test` | như GramMini | |
| `net.state` / `net.check` / `app.flush` | | |
| `library.query` | `q`?, `source`? (khoá peer), `topic`? (chủ đề; 1 = General gồm cả bài không thuộc chủ đề nào), `sort`: date \| title \| artist, `filter`: all \| offline \| cached, `offset`, `limit` (≤ 500) | `{total, gen, offset, items: [Song], sections: [{t, i, n}]}` — `sections` (chữ cái, vị trí đầu, số bài) khi xếp theo tên / ca sĩ |
| `library.keys` | như trên (không offset / limit) | `[khoá]` (≤ 5000) — hàng chờ «Phát hết» |
| `library.songs` | `keys` | `[Song]` theo đúng thứ tự (bài đã mất: `missing`); khoá `file:` → đọc tệp trên máy |
| `library.stats` | — | `{total, sources, gen, offline, full_done, last_sync, partial, sync}` |
| `sources.list` | — | `[Source]` (Tin nhắn đã lưu đầu tiên, rồi nhiều bài trước) |
| `sources.set` | `peer`, `enabled` | `{enabled}` + sự kiện `library.changed` |
| `sources.add` | `link` (@tên, t.me/…, tg://resolve?domain=…) | `{source, syncing}` — kênh / nhóm công khai, quét riêng |
| `sources.topics` | `peer` | `{forum, name, total, topics: [{id, title, color, count}]}` — chủ đề có nhạc (lấy lại từ Telegram; mất mạng thì bản đã lưu) |
| `sources.remove` | `peer` | `{}` (xoá chỉ mục; nguồn có từ quét tài khoản sẽ hiện lại ở lần quét sau) |
| `sync.start` | `full`? (quét toàn bộ / tiếp lần dở), `peer`? (chỉ một nguồn) | `{started}` (false = đang quét) |
| `sync.stop` | — | `{}` |
| `sync.state` | — | như sự kiện `sync.state` |
| `play.url` | `key` | `{url, cached}` — URL cổng phát nội bộ (FLAC / OGG / Opus: đuôi `.wav` — lõi giải mã); khoá `file:` → `file://…` (định dạng AVPlayer đọc được) hoặc URL cổng phát (FLAC / OGG / Opus giải từ tệp trên máy); lỗi `FORMAT` (chưa phát được), `NO_SONG` |
| `play.pos` | `key`, `pos` (giây), `playing` | `{}` — nhịp nghe: lõi chỉ tải tới `ahead_secs` giây trước vị trí này (M10, M12) |
| `play.info` | `key` | `{format, rate, bits, channels, kbps, vbr, lossless, hires, size, ready}` — thông số tệp từ phần đầu tệp đã có (`ready` false: chưa có khúc đầu) |
| `play.prefetch` | `key` | `{}` — tải trước 2 khúc đầu |
| `play.played` | `key` | `{}` — ghi vào «Nghe gần đây» |
| `play.cached` | `key` | `{cached}` 0…1 |
| `dl.add` | `keys` | `{added}` — vào hàng tải về máy |
| `dl.remove` | `keys` | `{freed, files}` — bỏ khỏi «Đã tải», xoá tệp |
| `dl.state` | — | `{queue, running}` |
| `media.cover` | `key` | `{path}` JPEG ảnh bìa (`covers/<khoá>.jpg`); lỗi `NO_COVER` |
| `media.avatar` | `peer`, `photo`, `dc` | `{path}` ảnh nguồn |
| `pl.list` | — | `{lists: [{id, name, count, updated, cover}], fav, recent}` |
| `pl.create` | `name`, `keys`? | `{id}` |
| `pl.fromQuery` | `name` + bộ lọc như `library.keys` (`q`, `source`, `topic`, `sort`, `filter`) | `{id, count, added}` — lưu cả danh sách đang xem thành playlist; trùng tên thì thêm vào playlist cũ |
| `pl.rename` / `pl.delete` | `id` (+ `name`) | `{}` |
| `pl.items` | `id` (số, "fav", "recent") | `[Song]` |
| `pl.add` | `id`, `keys` | `{added}` (bài đã có bỏ qua) |
| `pl.remove` | `id`, `keys` ("recent": xoá cả lịch sử) | `{}` |
| `pl.move` | `id`, `from`, `to` | `{}` |
| `fav.set` | `key`, `on` | `{fav}` |
| `cache.stats` | — | `{total, files, kinds: {offline, music, cover, avatar, other}, sources: [{peer, name, bytes}], limit_mb, keep_days, free, disk}` |
| `cache.clear` | `kinds`? (không ghi = mọi loại trừ «offline»), `peer`? | `{freed, files}` + sự kiện `cache.cleared` |
| `cache.set` | `limit_mb`?, `keep_days`? | `{}` |
| `settings.get` | — | `{ahead_secs, lyrics, secure_dns, dns_providers}` — tải trước bao nhiêu giây (0 = cả bài, mặc định 60), lời bài hát đã bật chưa (mặc định tắt), DNS mã hoá (mặc định bật), chuỗi nhà cung cấp («NextDNS → Cloudflare → Google») |
| `settings.set` | `ahead_secs`? (≤ 3600), `lyrics`?, `secure_dns`? | `{}` — có hiệu lực ngay |
| `net.dns` | `host`, `fail`? ([nhãn máy chủ] coi như bị chặn), `fresh`? | `{ip, via (máy chủ đã trả lời / «DNS của máy»), ms, secure}`; tên miền không có → `NO_DOMAIN` (M17, để thử) |
| `lyrics.get` | `key` (+ `title`, `artist`, `dur` cho bài trên máy), `refresh`? | `{found, synced, lines: [{t (giây), s}], plain, instrumental, title, artist, v}` — LRCLIB (M14, M19: ưu tiên bản có lời chạy); bản đã lưu trả ngay; chưa bật → lỗi `LYRICS_OFF`; mất mạng → `NETWORK` |

Lỗi chung: `FLOOD_WAIT` (kèm `value` giây), `NETWORK`, `TIMEOUT`, `NOT_STARTED`, `BAD_ARGS`, `NO_METHOD`, `PEER_UNKNOWN`, `IO`, hoặc
tên lỗi RPC của Telegram.

## Sự kiện

| `e` | Trường |
| --- | --- |
| `auth.state` / `auth.dead` / `net.state` | như GramMini |
| `sync.state` | `running`, `scope` (all \| chat), `phase` (music \| document), `pages`, `found` (bài mới), `wait`?, `try`? (đang chờ FLOOD_WAIT); lúc xong: `error`?, `last_sync`, `full_done` |
| `library.changed` | `gen`, `total` — thư viện đổi (quét, bài mới, ẩn / hiện nguồn) |
| `stream.progress` | `key`, `have`, `total` (khúc) — ~1 lần / giây khi đang tải bài |
| `stream.done` | `key` — bài đã đủ trên máy |
| `stream.error` | `key`, `code`, `msg` |
| `dl.progress` / `dl.done` / `dl.error` | `key` (+ `have`, `total` / `code`, `msg`) — hàng tải về máy |
| `cache.cleared` | — |

## Cổng phát

`GET|HEAD http://127.0.0.1:<cổng>/s/<mã>/<khoá>.<đuôi>`, `Range: bytes=a-b | a- | -n` → 200 / 206 (`Content-Range`) / 404 (mã
sai, bài không có) / 416. `Content-Type` theo đuôi (`audio/mpeg`, `audio/x-m4a`, `audio/x-wav`…), `Accept-Ranges: bytes`,
`Connection: close`. Bài FLAC / OGG / Opus: `…/<khoá>.wav` → WAV PCM 16 bit lõi giải dần (tua được — M8, M13); tệp trên máy cần
giải mã: `…/local_<băm>.wav`. Kết nối xin đoạn quá nhịp nghe (M10, M12) thì chờ, trừ 2 MB đầu và 1 MB cuối tệp.
