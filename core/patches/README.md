# Bản vá cho mã đã chép vào `core/vendor/`

Mã trong `vendor/` là bản đã vá (Cargo dùng qua `[patch.crates-io]` trong `core/Cargo.toml`). Tệp `.patch` ở đây ghi lại
phần đã sửa so với bản gốc trên crates.io, để lên phiên bản grammers mới thì áp lại:

```sh
# chép bản mới từ ~/.cargo/registry/src/*/grammers-mtsender-<bản>/ vào vendor/grammers-mtsender rồi:
cd vendor/grammers-mtsender && patch -p1 < ../../patches/0001-grammers-mtsender-connector.patch
# tương tự grammers-client:
cd vendor/grammers-client && patch -p1 < ../../patches/0002-grammers-client-build-peer-map.patch
```

| Bản vá | Việc |
| --- | --- |
| `0001-grammers-mtsender-connector.patch` (grammers-mtsender 0.12.0) | `ConnectionParams::connector`: lõi tự mở luồng tới DC (biết số DC) — MTProxy, SOCKS5, obfuscated2 nằm ở `crates/gm-net`. `NetStream` nhận mọi luồng byte; TCP bật nodelay. |
| `0002-grammers-client-build-peer-map.patch` (grammers-client 0.12.0) | `Client::build_peer_map` công khai: dựng `Message` từ phản hồi API thô (`messages.getReplies`, `messages.getForumTopics` — chủ đề diễn đàn, D28). |
