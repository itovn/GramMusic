//! Dịch vụ lõi cho app: một tài khoản, lệnh dạng JSON (`call`), sự kiện đẩy lên app (`Emit`). Chạy trong runtime tokio của
//! `gm-ffi`. Phần đăng nhập / proxy / trạng thái mạng chép từ GramMini (`gm-core/src/service.rs`); phần nhạc viết mới.
//!
//! Dữ liệu ở `data_dir`: `session.json` (phiên), `settings.json` (proxy, chính sách đệm), `sources.json` + `library/`
//! (thư viện), `sync.json` (quét), `playlists.json`, `offline.json` (bài đã tải về máy), `core.log`; tệp đệm ở `music/`,
//! `covers/`, `avatars/`. Hợp đồng lệnh / sự kiện: docs/FFI.md.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gm_net::{ClientInfo, GmConnector, ProxyConfig};
use grammers_client::client::{LoginToken, PasswordToken, SignInError};
use grammers_client::media::Media;
use grammers_client::message::Message;
use grammers_client::peer::Peer;
use grammers_client::tl;
use grammers_client::update::Update;
use grammers_client::{Client, InvocationError};
use grammers_mtsender::{SenderPool, SenderPoolFatHandle, ServerAddr, UpdatesConfiguration};
use grammers_session::types::{PeerId, PeerRef};
use grammers_session::updates::UpdatesLike;
use grammers_session::Session as _;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::library::{split_key, Library, Song, Sort, Source};
use crate::media::{self, Loc};
use crate::playlists::{Item, Playlists};
use crate::stream::{BoxFut, Fetch, FetchError, Streams, BLOCK};
use crate::sync::{Cursor, Overlap, SyncState, PAGE, PHASES};
use crate::FileSession;

/// Hàm đẩy sự kiện lên app (gọi từ luồng của lõi).
pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

/// Thông tin lúc mở lõi.
pub struct Config {
    pub data_dir: PathBuf,
    pub api_id: i32,
    pub api_hash: String,
    pub client: ClientInfo,
}

/// Lỗi trả cho app: `code` máy đọc (vd `PHONE_CODE_INVALID`, `FLOOD_WAIT`), `msg` câu báo tiếng Việt.
#[derive(Debug)]
pub struct CoreError {
    pub code: String,
    pub msg: String,
    pub extra: Value,
}

impl CoreError {
    pub fn new(code: &str, msg: impl Into<String>) -> Self {
        Self { code: code.into(), msg: msg.into(), extra: Value::Null }
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({"code": self.code, "msg": self.msg});
        if let (Value::Object(m), Value::Object(x)) = (&mut v, &self.extra) {
            m.extend(x.clone());
        }
        v
    }
}

impl From<InvocationError> for CoreError {
    fn from(e: InvocationError) -> Self {
        match &e {
            InvocationError::Rpc(r) => {
                let msg = match r.name.as_str() {
                    "FLOOD_WAIT" => format!("Telegram bảo chờ {} giây rồi thử lại.", r.value.unwrap_or(0)),
                    "PHONE_NUMBER_INVALID" => "Số điện thoại không đúng.".into(),
                    "PHONE_CODE_INVALID" => "Mã không đúng.".into(),
                    "PHONE_CODE_EXPIRED" => "Mã đã hết hạn — gửi mã mới.".into(),
                    "PHONE_NUMBER_BANNED" => "Số điện thoại này bị Telegram khoá.".into(),
                    "AUTH_KEY_UNREGISTERED" | "SESSION_REVOKED" => "Phiên đăng nhập đã hết — đăng nhập lại.".into(),
                    "SRP_ID_INVALID" => "Phiên kiểm tra mật khẩu đã hết — nhập lại mật khẩu.".into(),
                    "AUTH_KEY_DUPLICATED" => "Phiên đăng nhập bị Telegram huỷ (dùng cùng lúc ở hai nơi) — đăng nhập lại.".into(),
                    "USERNAME_NOT_OCCUPIED" | "USERNAME_INVALID" => "Không có kênh / nhóm nào dùng tên này.".into(),
                    "CHANNEL_PRIVATE" => "Kênh riêng tư — cần vào kênh bằng app Telegram trước.".into(),
                    _ => format!("Telegram báo lỗi {} ({})", r.name, r.code),
                };
                let mut ce = CoreError::new(&r.name, msg);
                if let Some(v) = r.value {
                    ce.extra = json!({"value": v});
                }
                ce
            }
            _ => CoreError::new("NETWORK", format!("Lỗi mạng: {e}")),
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::new("IO", format!("Lỗi ghi / đọc tệp: {e}"))
    }
}

type Res = Result<Value, CoreError>;

fn arg_str<'a>(a: &'a Value, k: &str) -> Result<&'a str, CoreError> {
    a.get(k).and_then(Value::as_str).ok_or_else(|| CoreError::new("BAD_ARGS", format!("thiếu «{k}»")))
}

fn arg_keys(a: &Value) -> Vec<String> {
    match a.get("keys") {
        Some(Value::Array(v)) => v.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => a.get("key").and_then(Value::as_str).map(|k| vec![k.to_string()]).unwrap_or_default(),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Lấy dữ liệu bài qua `upload.getFile` (grammers tự chuyển DC / chép quyền đăng nhập sang DC giữ tệp). Vị trí tệp có
/// `file_reference` hết hạn sau vài giờ → gặp FILE_REFERENCE_* thì đọc lại tin để lấy vị trí mới.
struct TgFetch {
    client: Client,
    session: Arc<FileSession>,
    locs: Mutex<HashMap<String, Loc>>,
}

impl TgFetch {
    async fn locate(&self, song: &Song) -> Result<Loc, FetchError> {
        if let Some(l) = lock(&self.locs).get(&song.key()) {
            return Ok(l.clone());
        }
        let fatal = |code: &str, msg: &str| FetchError { code: code.into(), msg: msg.into(), retry: None };
        let pr = peer_ref_of_key(&self.session, &song.peer).await.ok_or_else(|| fatal("PEER_UNKNOWN", "Chưa biết chat chứa bài — quét lại thư viện."))?;
        let msg = match self.client.get_messages_by_id(pr, &[song.id]).await {
            Ok(v) => v.into_iter().flatten().next(),
            Err(e) => return Err(fetch_err(e)),
        };
        let Some(Media::Document(d)) = msg.as_ref().and_then(|m| m.media()) else {
            return Err(fatal("NO_MEDIA", "Tin chứa bài này đã bị xoá."));
        };
        let loc = media::document_loc(&d).ok_or_else(|| fatal("NO_MEDIA", "Tệp nhạc không tải được."))?;
        lock(&self.locs).insert(song.key(), loc.clone());
        Ok(loc)
    }
}

/// Lỗi Telegram → lỗi lấy dữ liệu (mạng / FLOOD_WAIT thì thử lại).
fn fetch_err(e: InvocationError) -> FetchError {
    match &e {
        InvocationError::Rpc(r) if r.name == "FLOOD_WAIT" => FetchError {
            code: "FLOOD_WAIT".into(),
            msg: format!("Telegram bảo chờ {} giây.", r.value.unwrap_or(5)),
            retry: Some(Duration::from_secs(r.value.unwrap_or(5) as u64)),
        },
        InvocationError::Rpc(r) => FetchError { code: r.name.clone(), msg: CoreError::from(e).msg, retry: None },
        _ => FetchError { code: "NETWORK".into(), msg: format!("Lỗi mạng: {e}"), retry: Some(Duration::from_secs(2)) },
    }
}

impl Fetch for TgFetch {
    fn fetch<'a>(&'a self, song: &'a Song, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>> {
        Box::pin(async move {
            for attempt in 0..2 {
                let loc = self.locate(song).await?;
                let mut it = self.client.iter_download(&loc).chunk_size(BLOCK as i32).skip_chunks((offset / BLOCK) as i32);
                match tokio::time::timeout(Duration::from_secs(40), it.next()).await {
                    Ok(Ok(Some(d))) => return Ok(d),
                    Ok(Ok(None)) => return Ok(Vec::new()),
                    Ok(Err(InvocationError::Rpc(r))) if r.name.starts_with("FILE_REFERENCE") && attempt == 0 => {
                        lock(&self.locs).remove(&song.key());
                        continue;
                    }
                    Ok(Err(e)) => return Err(fetch_err(e)),
                    Err(_) => {
                        return Err(FetchError { code: "TIMEOUT".into(), msg: "Tải nhạc quá lâu.".into(), retry: Some(Duration::from_secs(1)) })
                    }
                }
            }
            Err(FetchError { code: "FILE_REFERENCE".into(), msg: "Không lấy được tệp nhạc.".into(), retry: None })
        })
    }
}

async fn peer_ref_of_key(session: &FileSession, key: &str) -> Option<PeerRef> {
    let id = key.parse::<i64>().ok().and_then(PeerId::from_bot_api_dialog_id)?;
    session.peer_ref(id).await.ok().flatten()
}

pub struct Core {
    cfg: Config,
    client: Client,
    session: Arc<FileSession>,
    conn: Arc<GmConnector>,
    emit: Emit,
    handle: SenderPoolFatHandle,
    login: Mutex<Option<LoginToken>>,
    password: Mutex<Option<PasswordToken>>,
    updates: Mutex<Option<mpsc::Receiver<UpdatesLike>>>,
    log: Mutex<Option<std::fs::File>>,
    /// Mạng: 0 chưa biết, 1 kết nối được, 2 mất kết nối (sự kiện `net.state` khi đổi).
    net: AtomicU8,
    /// Phiên đã bị Telegram huỷ — tệp phiên đã dời đi, chờ app mở lại để đăng nhập mới.
    dead: AtomicBool,
    /// Tải ảnh bìa / ảnh nguồn: tối đa 3 tệp cùng lúc (máy A4 một nhân).
    dl: tokio::sync::Semaphore,
    cache: crate::cache::Cache,
    lib: Arc<Mutex<Library>>,
    pls: Mutex<Playlists>,
    streams: Arc<Streams>,
    /// Bài đã tải về máy (`offline.json`): không tự xoá.
    offline: Mutex<HashSet<String>>,
    /// Hàng tải về máy (tải lần lượt từng bài).
    dlq: Mutex<VecDeque<String>>,
    dl_running: AtomicBool,
    syncing: AtomicBool,
    sync_stop: AtomicBool,
    /// Nguồn thêm tay trong lúc đang quét: quét riêng ngay sau lượt đang chạy.
    sync_pending: Mutex<Vec<String>>,
    sync_info: Mutex<Value>,
    /// Khoá peer của tài khoản (Tin nhắn đã lưu).
    me: Mutex<Option<String>>,
}

impl Core {
    /// Mở lõi (không đụng mạng). Phải gọi trong runtime tokio.
    pub fn start(cfg: Config, emit: Emit) -> Result<Arc<Core>, CoreError> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(|e| CoreError::new("IO", format!("tạo thư mục dữ liệu: {e}")))?;
        let session = Arc::new(
            FileSession::open(cfg.data_dir.join("session.json")).map_err(|e| CoreError::new("IO", e.to_string()))?,
        );
        let settings = Self::settings(&cfg.data_dir);
        let proxy = settings.get("proxy").and_then(Value::as_str).and_then(|l| gm_net::parse_link(l).ok()).unwrap_or_default();
        // DNS mã hoá (M17): mặc định bật
        gm_net::dns::set_enabled(settings.get("secure_dns").and_then(Value::as_bool).unwrap_or(true));
        let policy = {
            let d = crate::cache::Policy::default();
            crate::cache::Policy {
                limit_mb: settings.get("cache_limit_mb").and_then(Value::as_u64).unwrap_or(d.limit_mb),
                keep_days: settings.get("cache_keep_days").and_then(Value::as_u64).unwrap_or(d.keep_days),
            }
        };
        let conn = GmConnector::new(proxy);
        let params = conn.connection_params(cfg.client.clone());
        let SenderPool { runner, updates, handle } = SenderPool::with_configuration(Arc::clone(&session), cfg.api_id, params);
        let client = Client::new(handle.clone());
        tokio::spawn(runner.run());
        let log_path = cfg.data_dir.join("core.log");
        if std::fs::metadata(&log_path).map(|m| m.len() > 1 << 20).unwrap_or(false) {
            let _ = std::fs::remove_file(&log_path); // nhật ký quá 1 MB: bắt đầu lại
        }
        let log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
        let lib = Arc::new(Mutex::new(Library::open(&cfg.data_dir)));
        let fetch = Arc::new(TgFetch { client: client.clone(), session: Arc::clone(&session), locs: Mutex::new(HashMap::new()) });
        let lib2 = Arc::clone(&lib);
        let streams = Streams::new(cfg.data_dir.join("music"), fetch, Arc::clone(&emit), Box::new(move |k| lock(&lib2).song(k).cloned()));
        let offline: HashSet<String> = std::fs::read(cfg.data_dir.join("offline.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        let me = settings.get("me").and_then(Value::as_str).map(str::to_string);
        if let Some(a) = settings.get("ahead_secs").and_then(Value::as_u64) {
            streams.set_ahead(a);
        }
        let pls = Playlists::open(&cfg.data_dir);
        let core = Arc::new(Core {
            cfg,
            client,
            session,
            conn,
            emit,
            handle,
            login: Mutex::new(None),
            password: Mutex::new(None),
            updates: Mutex::new(Some(updates)),
            log: Mutex::new(log),
            net: AtomicU8::new(0),
            dead: AtomicBool::new(false),
            dl: tokio::sync::Semaphore::new(3),
            cache: crate::cache::Cache::new(policy),
            lib,
            pls: Mutex::new(pls),
            streams,
            offline: Mutex::new(offline),
            dlq: Mutex::new(VecDeque::new()),
            dl_running: AtomicBool::new(false),
            syncing: AtomicBool::new(false),
            sync_stop: AtomicBool::new(false),
            sync_pending: Mutex::new(Vec::new()),
            sync_info: Mutex::new(json!({"running": false})),
            me: Mutex::new(me),
        });
        core.log(&format!("mở lõi, proxy: {:?}", core.conn.config()));
        let weak = Arc::downgrade(&core);
        let weak2 = weak.clone();
        gm_net::dns::set_log(Arc::new(move |m: &str| {
            if let Some(c) = weak2.upgrade() {
                c.log(m);
            }
        }));
        crate::set_logger(Arc::new(move |m: &str| {
            if let Some(c) = weak.upgrade() {
                c.log(m);
            }
        }));
        // Mất kết nối thì 10 giây hỏi lại một lần (updates.getState — rẻ) tới khi được
        let h = Arc::clone(&core);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if h.net.load(Ordering::Relaxed) != 1 {
                    h.check_net().await;
                }
            }
        });
        // Tự dọn bộ nhớ đệm: 20 giây sau khi mở, rồi 30 phút một lần
        let h = Arc::clone(&core);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                h.trim_soon(true);
                tokio::time::sleep(Duration::from_secs(30 * 60)).await;
            }
        });
        // Đã đăng nhập thì nghe cập nhật (bài mới gửi vào các nguồn), rồi tải tiếp hàng tải về máy còn dở
        let c = Arc::clone(&core);
        tokio::spawn(async move {
            match c.client.is_authorized().await {
                Ok(true) => {
                    (c.emit)(json!({"e": "auth.state", "authorized": true}));
                    c.after_login().await;
                }
                Ok(false) => (c.emit)(json!({"e": "auth.state", "authorized": false})),
                Err(e) => {
                    c.log(&format!("is_authorized: {e}"));
                    let ce = CoreError::from(e);
                    c.check_dead(&ce.code);
                }
            }
        });
        Ok(core)
    }

    /// Sau khi đăng nhập / mở lõi đã đăng nhập: nhớ mã tài khoản, nghe cập nhật, tải tiếp bài chờ tải về máy.
    async fn after_login(self: &Arc<Self>) {
        if lock(&self.me).is_none() {
            if let Ok(me) = self.client.get_me().await {
                let key = peer_key(me.id());
                let _ = self.save_settings(&[("me", json!(key))]);
                *lock(&self.me) = Some(key);
            }
        }
        self.start_updates();
        // Mở app: tự lấy bài mới (bản mới đổi dữ liệu quét → tự quét toàn bộ lại một lần)
        let c = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            c.start_sync(false, None);
        });
        let pending: Vec<String> = {
            let off = lock(&self.offline);
            let lib = lock(&self.lib);
            off.iter().filter(|k| lib.song(k).is_some_and(|s| self.streams.complete_path(s).is_none())).cloned().collect()
        };
        if !pending.is_empty() {
            lock(&self.dlq).extend(pending);
            self.run_downloads();
        }
    }

    async fn password_token(&self) -> Result<PasswordToken, CoreError> {
        let p: tl::types::account::Password = self.client.invoke(&tl::functions::account::GetPassword {}).await?.into();
        Ok(PasswordToken::new(p))
    }

    /// Phiên chết (AUTH_KEY_DUPLICATED, SESSION_REVOKED, tài khoản khoá) → dời `session.json`, báo app (như GramMini D24).
    fn check_dead(&self, code: &str) -> bool {
        if !matches!(code, "AUTH_KEY_DUPLICATED" | "SESSION_REVOKED" | "USER_DEACTIVATED" | "USER_DEACTIVATED_BAN") {
            return false;
        }
        if self.dead.swap(true, Ordering::SeqCst) {
            return true;
        }
        let from = self.cfg.data_dir.join("session.json");
        let to = self.cfg.data_dir.join(format!("session.dead-{}.json", now()));
        let moved = std::fs::rename(&from, &to).is_ok();
        self.log(&format!("phiên chết ({code}) — dời tệp phiên: {moved}"));
        (self.emit)(json!({"e": "auth.dead", "code": code}));
        (self.emit)(json!({"e": "auth.state", "authorized": false, "dead": true}));
        true
    }

    /// Tải tệp nhỏ (ảnh bìa, ảnh nguồn) về `path` qua tệp tạm riêng của lần tải.
    async fn download(self: &Arc<Self>, loc: &Loc, path: &std::path::Path) -> Result<(), CoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = part_path(path);
        let fetch = async {
            // Ảnh nhỏ (vài chục KB): gom trong bộ nhớ rồi ghi một lần
            let mut buf = Vec::new();
            let mut it = self.client.iter_download(loc);
            while let Some(chunk) = it.next().await? {
                buf.extend_from_slice(&chunk);
            }
            std::fs::write(&tmp, &buf)?;
            Ok::<(), CoreError>(())
        };
        match tokio::time::timeout(Duration::from_secs(60), fetch).await {
            Ok(Ok(())) => {
                std::fs::rename(&tmp, path)?;
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
                Err(CoreError::new("TIMEOUT", "Tải quá lâu."))
            }
        }
    }

    // ───────────────────────── Quét thư viện ─────────────────────────

    fn me_key(&self) -> Option<String> {
        lock(&self.me).clone()
    }

    /// Nguồn mới cho bài lấy từ tin `m`.
    fn source_for(&self, m: &Message, key: &str) -> Source {
        let me = self.me_key();
        let mut s = match m.peer() {
            Some(p) => source_from_peer(p),
            None => Source { name: format!("Chat {key}"), kind: "user".into(), enabled: true, ..Default::default() },
        };
        if me.as_deref() == Some(key) {
            s.kind = "self".into();
            s.name = "Tin nhắn đã lưu".into();
        }
        s.peer = key.to_string();
        s.added = now();
        s
    }

    /// Một trang kết quả tìm (toàn tài khoản khi `peer` None) → (tin, next_rate).
    /// Một trang kết quả tìm → (tin, next_rate, tổng máy chủ báo, đã trả hết trong một lần).
    async fn search_page(&self, peer: Option<&PeerRef>, filter: &str, cur: &Cursor) -> Result<(Vec<Message>, Option<i32>, Option<i32>, bool), CoreError> {
        use tl::enums::messages::Messages;
        use tl::enums::MessagesFilter as F;
        let filter = if filter == "document" { F::InputMessagesFilterDocument } else { F::InputMessagesFilterMusic };
        let resp = match peer {
            Some(p) => {
                self.client
                    .invoke(&tl::functions::messages::Search {
                        peer: p.clone().into(),
                        q: String::new(),
                        from_id: None,
                        saved_peer_id: None,
                        saved_reaction: None,
                        top_msg_id: None,
                        filter,
                        min_date: 0,
                        max_date: 0,
                        offset_id: cur.offset_id,
                        add_offset: 0,
                        limit: PAGE,
                        max_id: 0,
                        min_id: 0,
                        hash: 0,
                    })
                    .await?
            }
            None => {
                let offset_peer = if cur.offset_peer.is_empty() {
                    tl::enums::InputPeer::Empty
                } else {
                    match peer_ref_of_key(&self.session, &cur.offset_peer).await {
                        Some(r) => r.into(),
                        None => tl::enums::InputPeer::Empty,
                    }
                };
                self.client
                    .invoke(&tl::functions::messages::SearchGlobal {
                        folder_id: None,
                        q: String::new(),
                        filter,
                        min_date: 0,
                        max_date: 0,
                        offset_rate: cur.offset_rate,
                        offset_peer,
                        offset_id: cur.offset_id,
                        limit: PAGE,
                        broadcasts_only: false,
                        groups_only: false,
                        users_only: false,
                        community: None,
                    })
                    .await?
            }
        };
        let (msgs, users, chats, rate, total, all) = match resp {
            Messages::Messages(m) => (m.messages, m.users, m.chats, None, None, true),
            Messages::Slice(m) => (m.messages, m.users, m.chats, m.next_rate, Some(m.count), false),
            Messages::ChannelMessages(m) => (m.messages, m.users, m.chats, None, Some(m.count), false),
            Messages::NotModified(_) => (Vec::new(), Vec::new(), Vec::new(), None, None, true),
        };
        let peers = self.client.build_peer_map(users, chats).await;
        let out = msgs.into_iter().map(|m| Message::from_raw(&self.client, m, peer.cloned(), peers.clone())).collect();
        Ok((out, rate, total, all))
    }

    /// Gộp bài của một trang vào thư viện. Trả (số bài mới, các bài theo thứ tự: đã có hay chưa).
    fn absorb(&self, msgs: &[Message]) -> Result<(usize, Vec<bool>), CoreError> {
        let mut groups: HashMap<String, (Vec<Song>, usize)> = HashMap::new();
        let mut known = Vec::new();
        let mut lib = lock(&self.lib);
        for (i, m) in msgs.iter().enumerate() {
            let key = peer_key(m.peer_id());
            if let Some(s) = media::song_from(m, &key) {
                known.push(lib.index(&key).is_some_and(|ix| ix.has(s.id)));
                groups.entry(key).or_insert_with(|| (Vec::new(), i)).0.push(s);
            }
        }
        let mut added = 0;
        for (key, (songs, i)) in groups {
            let m = &msgs[i];
            added += lib.add_songs(&key, songs, || self.source_for(m, &key))?;
        }
        Ok((added, known))
    }

    fn set_sync_info(&self, v: Value) {
        *lock(&self.sync_info) = v.clone();
        let mut ev = v;
        ev["e"] = json!("sync.state");
        (self.emit)(ev);
    }

    fn library_changed(&self) {
        let (gen, total) = {
            let mut lib = lock(&self.lib);
            let t = lib.total();
            (lib.generation, t)
        };
        (self.emit)(json!({"e": "library.changed", "gen": gen, "total": total}));
    }

    /// Chạy một lượt tìm (toàn tài khoản hoặc một chat) cho tới hết / bắt kịp. `full`: quét hết, không dừng sớm. Ghi con
    /// trỏ qua `checkpoint` (chỉ khi quét toàn tài khoản). Trả số bài mới.
    async fn scan_phase(
        self: &Arc<Self>,
        peer: Option<&PeerRef>,
        phase: &str,
        mut cur: Cursor,
        full: bool,
        since: i64,
        st: &mut SyncState,
        found: &mut usize,
    ) -> Result<(), CoreError> {
        // `since` (lấy bài mới): tới tin cũ hơn mốc này là đủ — lượt «tài liệu» có thể toàn tệp không phải nhạc, đếm bài đã có
        // không dừng được
        let since = if full { 0 } else { since };
        let mut overlap = Overlap::default();
        let mut fetched = 0usize;
        let mut pages_since_flush = 0;
        let mut flood_tries = 0;
        let label = if peer.is_none() { "all" } else { "chat" };
        loop {
            if self.sync_stop.load(Ordering::Relaxed) {
                return Err(CoreError::new("STOPPED", "Đã dừng quét."));
            }
            let (msgs, rate, total, all) = match self.search_page(peer, phase, &cur).await {
                Ok(r) => {
                    flood_tries = 0;
                    r
                }
                Err(e) if e.code == "FLOOD_WAIT" && flood_tries < 3 => {
                    flood_tries += 1;
                    let wait = e.extra.get("value").and_then(Value::as_u64).unwrap_or(3).max(1);
                    if wait > 600 {
                        return Err(e);
                    }
                    self.set_sync_info(json!({"running": true, "scope": label, "phase": phase, "pages": st.pages, "found": *found,
                                              "chat": st.chat_i, "chats": st.chat_n, "wait": wait, "try": flood_tries}));
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if msgs.is_empty() {
                break;
            }
            let (added, known) = self.absorb(&msgs)?;
            *found += added;
            st.pages += 1;
            fetched += msgs.len();
            let songs = known.len();
            self.log(&format!(
                "quét {phase} ({label}): trang {} — {} tin, {songs} bài, {added} bài mới, máy chủ báo tổng {:?}{}",
                st.pages,
                msgs.len(),
                total,
                if all { ", trả hết một lần" } else { "" }
            ));
            let mut caught_up = false;
            if !full {
                for k in known {
                    if overlap.see(k) {
                        caught_up = true;
                        break;
                    }
                }
            }
            let last = msgs.last().expect("trang có tin");
            if since > 0 && last.date().as_second() < since {
                caught_up = true;
            }
            let next = Cursor {
                offset_rate: rate.unwrap_or(0),
                offset_peer: if peer.is_none() { peer_key(last.peer_id()) } else { String::new() },
                offset_id: last.id(),
            };
            if next == cur {
                self.log(&format!("quét {phase}: con trỏ đứng yên ở {}", cur.offset_id));
                break;
            }
            cur = next;
            pages_since_flush += 1;
            if pages_since_flush >= 5 {
                pages_since_flush = 0;
                lock(&self.lib).flush()?;
                if peer.is_none() && full {
                    st.phase = phase.to_string();
                    st.cursor = cur.clone();
                    let _ = st.save(&self.cfg.data_dir);
                }
                self.library_changed();
            }
            self.set_sync_info(json!({"running": true, "scope": label, "phase": phase, "pages": st.pages, "found": *found,
                                      "chat": st.chat_i, "chats": st.chat_n}));
            let exhausted = all || total.is_some_and(|t| fetched >= t.max(0) as usize);
            if caught_up || exhausted || (peer.is_some() && cur.offset_id <= 1) {
                break;
            }
            // Nhịp hỏi nhẹ tay (tránh FLOOD_WAIT)
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
        lock(&self.lib).flush()?;
        Ok(())
    }

    /// Quét toàn tài khoản (`full` = quét toàn bộ / tiếp lần dở; không thì lấy bài mới), rồi Tin nhắn đã lưu và các nguồn thêm
    /// tay. Chạy nền, báo `sync.state`; xong báo `library.changed`.
    fn start_sync(self: &Arc<Self>, full: bool, only: Option<String>) -> bool {
        if self.syncing.swap(true, Ordering::AcqRel) {
            if let Some(p) = only {
                let mut q = lock(&self.sync_pending);
                if !q.contains(&p) {
                    q.push(p);
                }
                return true;
            }
            return false;
        }
        self.sync_stop.store(false, Ordering::Relaxed);
        let c = Arc::clone(self);
        tokio::spawn(async move {
            let t = Instant::now();
            let mut st = SyncState::load(&c.cfg.data_dir);
            let mut found = 0usize;
            let r = Box::pin(c.sync_run(full, only.as_deref(), &mut st, &mut found)).await;
            let err = r.as_ref().err().map(|e| e.to_json());
            if let Err(e) = &r {
                c.log(&format!("quét lỗi {} sau {} ms", e.code, t.elapsed().as_millis()));
                st.last_error = Some(e.msg.clone());
            } else {
                c.log(&format!("quét xong: {found} bài mới, {} trang, {} ms", st.pages, t.elapsed().as_millis()));
                st.last_error = None;
            }
            let _ = st.save(&c.cfg.data_dir);
            c.syncing.store(false, Ordering::Release);
            c.set_sync_info(json!({"running": false, "found": found, "error": err, "last_sync": st.last_sync, "full_done": st.full_done}));
            c.library_changed();
            let next = {
                let mut q = lock(&c.sync_pending);
                (!q.is_empty()).then(|| q.remove(0))
            };
            if let Some(p) = next {
                if !c.sync_stop.load(Ordering::Relaxed) {
                    c.start_sync(true, Some(p));
                }
            }
        });
        true
    }

    /// Các chat của tài khoản (danh sách chat của Telegram, cả Lưu trữ): (khoá peer, ngày tin mới nhất). grammers lưu luôn
    /// access_hash của từng chat vào phiên → quét được từng chat.
    async fn dialog_list(&self) -> Result<Vec<(String, i64)>, CoreError> {
        let mut it = self.client.iter_dialogs();
        let mut out = Vec::new();
        let mut fresh = Vec::new();
        while let Some(d) = it.next().await? {
            let date = d.last_message.as_ref().map(|m| m.date().as_second()).unwrap_or(0);
            let key = peer_key(d.peer().id());
            out.push((key.clone(), date));
            fresh.push((key, source_from_peer(d.peer())));
            if out.len() >= 3000 {
                break;
            }
        }
        self.save_session();
        // Cập nhật tên, ảnh, cờ «có chủ đề» của các nguồn đã có (nhóm đổi tên / bật chủ đề sau khi đã quét)
        let mut lib = lock(&self.lib);
        for (key, src) in fresh {
            if let Some(old) = lib.source(&key) {
                if old.name != src.name || old.photo != src.photo || old.forum != src.forum || old.username != src.username {
                    let mut s = src;
                    s.kind = if old.kind == "self" { old.kind.clone() } else { s.kind };
                    s.name = if old.kind == "self" { old.name.clone() } else { s.name };
                    let _ = lib.upsert_source(s);
                }
            }
        }
        Ok(out)
    }

    /// Quét thư viện: lần đầu (hoặc `full`, hoặc lần quét toàn bộ trước còn dở) quét hết từng chat của tài khoản; các lần sau chỉ
    /// quét chat có tin mới từ lần trước (và chat chưa quét bao giờ). Rồi các nguồn thêm tay (kênh công khai chưa vào). Tìm toàn
    /// cục (`messages.searchGlobal`) không dùng được: với từ khoá rỗng Telegram trả gần như không có gì (thử trên iPhone 5,
    /// 17/11/2026 — DECISIONS M7).
    async fn sync_run(self: &Arc<Self>, full: bool, only: Option<&str>, st: &mut SyncState, found: &mut usize) -> Result<(), CoreError> {
        if let Some(peer) = only {
            return Box::pin(self.sync_chat(peer, full, st, found)).await;
        }
        let t = Instant::now();
        let dialogs = self.dialog_list().await?;
        self.log(&format!("quét: {} chat (đọc danh sách {} ms)", dialogs.len(), t.elapsed().as_millis()));
        let discovery = full || !st.full_done || st.phase == "chats" || st.version < crate::sync::VERSION;
        if discovery {
            if st.phase != "chats" || full {
                st.phase = "chats".into();
                st.done.clear();
                st.pages = 0;
            }
            let done: HashSet<String> = st.done.iter().cloned().collect();
            st.chat_n = dialogs.len();
            for (i, (peer, _)) in dialogs.iter().enumerate() {
                st.chat_i = i + 1;
                if done.contains(peer) {
                    continue;
                }
                if let Err(e) = Box::pin(self.sync_chat(peer, true, st, found)).await {
                    if matches!(e.code.as_str(), "STOPPED" | "NETWORK" | "TIMEOUT" | "FLOOD_WAIT") {
                        return Err(e);
                    }
                    self.log(&format!("quét chat {}: lỗi {} — bỏ qua", i + 1, e.code));
                }
                st.done.push(peer.clone());
                if i % 5 == 4 {
                    let _ = st.save(&self.cfg.data_dir);
                    self.library_changed();
                }
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            st.phase.clear();
            st.done.clear();
            st.full_done = true;
            st.version = crate::sync::VERSION;
        } else {
            let since = st.last_sync - 86_400;
            let todo: Vec<String> = dialogs
                .iter()
                .filter(|(p, date)| *date >= since || !st.chats.contains_key(p.as_str()))
                .map(|(p, _)| p.clone())
                .collect();
            st.chat_n = todo.len();
            for (i, peer) in todo.iter().enumerate() {
                st.chat_i = i + 1;
                if let Err(e) = Box::pin(self.sync_chat(peer, false, st, found)).await {
                    if matches!(e.code.as_str(), "STOPPED" | "NETWORK" | "TIMEOUT") {
                        return Err(e);
                    }
                    self.log(&format!("quét chat: lỗi {} — bỏ qua", e.code));
                }
            }
        }
        // Nguồn thêm tay (kênh công khai chưa vào — không có trong danh sách chat)
        let in_dialogs: HashSet<&String> = dialogs.iter().map(|(p, _)| p).collect();
        let manual: Vec<String> =
            lock(&self.lib).sources().iter().filter(|s| s.manual && !in_dialogs.contains(&s.peer)).map(|s| s.peer.clone()).collect();
        for p in manual {
            if let Err(e) = Box::pin(self.sync_chat(&p, false, st, found)).await {
                if e.code == "STOPPED" {
                    return Err(e);
                }
                self.log(&format!("quét nguồn thêm tay: lỗi {}", e.code));
            }
        }
        st.chat_i = 0;
        st.chat_n = 0;
        st.last_sync = now();
        Ok(())
    }

    /// Quét riêng một chat (lượt âm thanh, lượt tài liệu). Chưa quét bao giờ (hoặc `full`) thì quét hết; không thì chỉ lấy tin
    /// mới hơn lần quét trước (lùi 1 ngày cho chắc).
    async fn sync_chat(self: &Arc<Self>, peer: &str, full: bool, st: &mut SyncState, found: &mut usize) -> Result<(), CoreError> {
        let pr = peer_ref_of_key(&self.session, peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat này."))?;
        let last = st.chats.get(peer).copied();
        let deep = full || last.is_none();
        let since = last.map(|t| t - 86_400).unwrap_or(0);
        for phase in PHASES {
            Box::pin(self.scan_phase(Some(&pr), phase, Cursor::default(), deep, since, st, found)).await?;
        }
        st.chats.insert(peer.to_string(), now());
        let mut lib = lock(&self.lib);
        lib.set_complete(peer, true);
        let _ = lib.update_source(peer, |s| s.synced = now());
        lib.flush()?;
        Ok(())
    }

    /// Thêm nguồn bằng @tên / link t.me (kênh / nhóm công khai, kể cả chưa vào) rồi quét riêng.
    async fn add_source(self: &Arc<Self>, link: &str) -> Res {
        let name = username_of(link).ok_or_else(|| CoreError::new("BAD_LINK", "Nhập @tên hoặc link t.me/… của kênh / nhóm."))?;
        let p = self.client.resolve_username(&name).await?.ok_or_else(|| CoreError::new("USERNAME_NOT_OCCUPIED", "Không có kênh / nhóm nào dùng tên này."))?;
        let key = peer_key(p.id());
        let mut s = source_from_peer(&p);
        s.peer = key.clone();
        s.manual = true;
        s.added = now();
        lock(&self.lib).upsert_source(s)?;
        self.save_session();
        let started = self.start_sync(true, Some(key.clone()));
        let src = lock(&self.lib).source(&key).cloned();
        Ok(json!({"source": src.map(|s| self.source_json(&s)), "syncing": started}))
    }

    // ───────────────────────── Bài hát → JSON ─────────────────────────

    fn source_json(&self, s: &Source) -> Value {
        let n = lock(&self.lib).indexed(&s.peer);
        json!({"peer": s.peer, "name": s.name, "kind": s.kind, "username": s.username, "photo": s.photo, "photo_dc": s.photo_dc,
               "enabled": s.enabled, "manual": s.manual, "count": n, "synced": s.synced, "forum": s.forum})
    }

    /// Trạng thái tải: "offline" (đã tải về máy), "queued" (chờ / đang tải về máy), "cached" (nghe trọn — có trong đệm), null.
    fn dl_state(&self, s: &Song, offline: &HashSet<String>) -> Value {
        let done = self.streams.complete_path(s).is_some();
        match (offline.contains(&s.key()), done) {
            (true, true) => json!("offline"),
            (true, false) => json!("queued"),
            (false, true) => json!("cached"),
            _ => Value::Null,
        }
    }

    fn songs_json(&self, songs: &[Song]) -> Vec<Value> {
        let offline = lock(&self.offline).clone();
        let names: HashMap<String, String> = lock(&self.lib).sources().iter().map(|s| (s.peer.clone(), s.name.clone())).collect();
        let pls = lock(&self.pls);
        songs
            .iter()
            .map(|s| {
                json!({
                    "key": s.key(), "peer": s.peer, "id": s.id, "title": s.title, "artist": s.artist, "dur": s.dur,
                    "size": s.size, "ext": s.ext, "date": s.date, "thumb": s.thumb, "ok": s.playable(),
                    "src": names.get(&s.peer), "fav": pls.is_fav(&s.key()), "dl": self.dl_state(s, &offline), "topic": s.topic,
                })
            })
            .collect()
    }

    /// Bộ lọc của `library.query` / `library.keys`: all | offline (đã tải về máy) | cached (có trên máy, kể cả nghe trọn).
    /// + `topic` (chủ đề của nhóm diễn đàn; 1 = General — gồm cả bài không thuộc chủ đề nào).
    fn keep_fn(&self, a: &Value) -> Box<dyn Fn(&Song) -> bool + '_> {
        let topic = a.get("topic").and_then(Value::as_i64).map(|t| t as i32);
        let in_topic = move |s: &Song| topic.is_none_or(|t| s.topic == t || (t == 1 && s.topic == 0));
        match a.get("filter").and_then(Value::as_str).unwrap_or("all") {
            "offline" => {
                let off = lock(&self.offline).clone();
                Box::new(move |s: &Song| in_topic(s) && off.contains(&s.key()))
            }
            "cached" => Box::new(move |s: &Song| in_topic(s) && self.streams.complete_path(s).is_some()),
            _ => Box::new(in_topic),
        }
    }

    /// Chủ đề của nhóm diễn đàn (`messages.getForumTopics`, tối đa 500) — lưu vào nguồn; mất mạng thì dùng bản đã lưu. Trả các
    /// chủ đề có bài (kèm số bài), theo thứ tự của Telegram.
    async fn topics(&self, peer: &str) -> Res {
        let pr = peer_ref_of_key(&self.session, peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết nhóm này."))?;
        let mut fetched: Vec<crate::library::Topic> = Vec::new();
        let (mut od, mut oid, mut otop) = (0, 0, 0);
        let mut err = None;
        while fetched.len() < 500 {
            let r = self
                .client
                .invoke(&tl::functions::messages::GetForumTopics {
                    peer: pr.into(),
                    q: None,
                    offset_date: od,
                    offset_id: oid,
                    offset_topic: otop,
                    limit: 100,
                })
                .await;
            let tl::enums::messages::ForumTopics::Topics(r) = match r {
                Ok(r) => r,
                Err(e) => {
                    err = Some(CoreError::from(e));
                    break;
                }
            };
            let n = r.topics.len();
            let total = r.count as usize;
            for t in r.topics {
                if let tl::enums::ForumTopic::Topic(t) = t {
                    od = t.date;
                    oid = t.top_message;
                    otop = t.id;
                    fetched.push(crate::library::Topic { id: t.id, title: t.title, color: t.icon_color });
                }
            }
            if n < 100 || fetched.len() >= total {
                break;
            }
        }
        let mut lib = lock(&self.lib);
        if !fetched.is_empty() {
            let _ = lib.update_source(peer, |s| {
                s.forum = true;
                s.topics = fetched.clone();
            });
        } else if let Some(e) = err.filter(|_| lib.source(peer).is_none_or(|s| s.topics.is_empty())) {
            return Err(e);
        }
        let counts = lib.topic_counts(peer);
        let src = lib.source(peer).cloned().unwrap_or_default();
        let mut out: Vec<Value> = src
            .topics
            .iter()
            .filter_map(|t| counts.get(&t.id).map(|n| json!({"id": t.id, "title": t.title, "color": t.color, "count": n})))
            .collect();
        // Bài thuộc chủ đề đã xoá / chưa có tên
        let named: HashSet<i32> = src.topics.iter().map(|t| t.id).collect();
        for (id, n) in &counts {
            if !named.contains(id) {
                let title = if *id == 1 { "General".to_string() } else { format!("Chủ đề {id}") };
                out.push(json!({"id": id, "title": title, "color": 0, "count": n}));
            }
        }
        let total: usize = counts.values().sum();
        Ok(json!({"forum": src.forum, "name": src.name, "total": total, "topics": out}))
    }

    /// Bài theo khoá (bài không còn trong thư viện thì dùng bản chép của playlist, `missing: true`).
    fn items_json(&self, items: &[Item]) -> Vec<Value> {
        let (found, missing): (Vec<Option<Song>>, Vec<&Item>) = {
            let lib = lock(&self.lib);
            let f: Vec<Option<Song>> = items.iter().map(|i| lib.song(&i.key).cloned()).collect();
            let m = items.iter().collect();
            (f, m)
        };
        let mut out = Vec::with_capacity(items.len());
        for (s, it) in found.iter().zip(missing) {
            match s {
                Some(s) => out.extend(self.songs_json(std::slice::from_ref(s))),
                None => out.push(json!({"key": it.key, "title": it.title, "artist": it.artist, "dur": it.dur, "missing": true, "ok": false})),
            }
        }
        out
    }

    fn save_offline(&self) {
        let v: Vec<String> = lock(&self.offline).iter().cloned().collect();
        if let Err(e) = crate::library::write_atomic(&self.cfg.data_dir.join("offline.json"), &serde_json::to_vec(&v).unwrap_or_default()) {
            self.log(&format!("ghi offline.json: {e}"));
        }
    }

    /// Hàng tải về máy: lần lượt từng bài (một bài tối đa 2 khúc song song), báo `dl.progress` / `dl.done` / `dl.error`.
    fn run_downloads(self: &Arc<Self>) {
        if self.dl_running.swap(true, Ordering::AcqRel) {
            return;
        }
        let c = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let Some(key) = lock(&c.dlq).pop_front() else { break };
                if !lock(&c.offline).contains(&key) {
                    continue; // đã huỷ
                }
                let Some(song) = lock(&c.lib).song(&key).cloned() else { continue };
                if c.streams.complete_path(&song).is_some() {
                    (c.emit)(json!({"e": "dl.done", "key": key}));
                    continue;
                }
                let t = match c.streams.set_full(&song, true) {
                    Ok(t) => t,
                    Err(e) => {
                        (c.emit)(json!({"e": "dl.error", "key": key, "code": "IO", "msg": e.to_string()}));
                        continue;
                    }
                };
                loop {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let (h, n) = t.progress();
                    if t.is_done() {
                        (c.emit)(json!({"e": "dl.done", "key": key}));
                        break;
                    }
                    if !lock(&c.offline).contains(&key) {
                        let _ = c.streams.set_full(&song, false);
                        break;
                    }
                    if let Some(e) = t.failed() {
                        (c.emit)(json!({"e": "dl.error", "key": key, "code": e.code, "msg": e.msg}));
                        break;
                    }
                    if !t.is_full() {
                        // lỗi tạm đã bỏ cờ tải trọn → bật lại
                        let _ = c.streams.set_full(&song, true);
                    }
                    (c.emit)(json!({"e": "dl.progress", "key": key, "have": h, "total": n}));
                }
            }
            c.dl_running.store(false, Ordering::Release);
            // Bài thêm vào hàng đúng lúc vòng vừa thoát
            if !lock(&c.dlq).is_empty() {
                c.run_downloads();
            }
        });
    }

    // ───────────────────────── Bộ nhớ đệm, mạng, cài đặt (như GramMini) ─────────────────────────

    fn trim_soon(self: &Arc<Self>, force: bool) {
        if !self.cache.begin(force) {
            return;
        }
        let c = Arc::clone(self);
        let offline = lock(&self.offline).clone();
        let busy = self.streams.busy();
        tokio::task::spawn_blocking(move || {
            let t = Instant::now();
            let r = crate::cache::trim(&c.cfg.data_dir, c.cache.policy(), &offline, &busy);
            c.cache.end();
            if r.files > 0 {
                c.log(&format!("dọn đệm: xoá {} tệp ({} KB), còn {} KB, {} ms", r.files, r.freed >> 10, r.total >> 10, t.elapsed().as_millis()));
                (c.emit)(json!({"e": "cache.cleared"}));
            }
        });
    }

    fn set_net(&self, ok: bool) {
        let new = if ok { 1 } else { 2 };
        if self.net.swap(new, Ordering::Relaxed) != new {
            self.log(if ok { "mạng: kết nối được" } else { "mạng: mất kết nối" });
            (self.emit)(json!({"e": "net.state", "ok": ok}));
        }
    }

    async fn check_net(&self) -> bool {
        let r = tokio::time::timeout(Duration::from_secs(8), self.client.invoke(&tl::functions::updates::GetState {})).await;
        let ok = matches!(r, Ok(Ok(_)) | Ok(Err(InvocationError::Rpc(_))));
        self.set_net(ok);
        ok
    }

    /// Ghi nhật ký lõi (không ghi tên bài / số điện thoại).
    pub fn log(&self, line: &str) {
        if let Some(f) = lock(&self.log).as_mut() {
            let _ = writeln!(f, "{} {line}", now());
        }
    }

    fn settings(dir: &std::path::Path) -> serde_json::Map<String, Value> {
        std::fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn save_settings(&self, kv: &[(&str, Value)]) -> Result<(), CoreError> {
        let mut m = Self::settings(&self.cfg.data_dir);
        for (k, v) in kv {
            m.insert(k.to_string(), v.clone());
        }
        crate::library::write_atomic(&self.cfg.data_dir.join("settings.json"), Value::Object(m).to_string().as_bytes())
            .map_err(|e| CoreError::new("IO", format!("ghi cài đặt: {e}")))
    }

    fn save_session(&self) {
        if let Err(e) = self.session.save() {
            self.log(&format!("ghi phiên: {e}"));
        }
    }

    /// Nghe cập nhật (một lần): bài mới gửi vào chat nào cũng vào thư viện; tin bị xoá thì bỏ khỏi thư viện.
    fn start_updates(self: &Arc<Self>) {
        let Some(rx) = lock(&self.updates).take() else { return };
        let c = Arc::clone(self);
        tokio::spawn(async move {
            let mut stream = match c.client.stream_updates(rx, UpdatesConfiguration { catch_up: true }).await {
                Ok(s) => s,
                Err(e) => {
                    c.log(&format!("stream_updates: {e}"));
                    return;
                }
            };
            let mut last_save = Instant::now();
            loop {
                match tokio::time::timeout(Duration::from_secs(30), stream.next()).await {
                    Ok(Ok(Update::NewMessage(m))) | Ok(Ok(Update::MessageEdited(m))) => {
                        let key = peer_key(m.peer_id());
                        if let Some(song) = media::song_from(&m, &key) {
                            let n = {
                                let mut lib = lock(&c.lib);
                                let n = lib.add_songs(&key, vec![song], || c.source_for(&m, &key)).unwrap_or(0);
                                let _ = lib.flush();
                                n
                            };
                            if n > 0 {
                                c.library_changed();
                            }
                        }
                    }
                    Ok(Ok(Update::MessageDeleted(d))) => {
                        let ch = d.channel_id().and_then(PeerId::channel).map(peer_key);
                        let n = lock(&c.lib).remove_songs(ch.as_deref(), d.messages());
                        if n > 0 {
                            c.library_changed();
                        }
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        c.log(&format!("cập nhật lỗi: {e}"));
                        if c.check_dead(&CoreError::from(e).code) {
                            break;
                        }
                        c.set_net(false);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Err(_) => {}
                }
                if last_save.elapsed() > Duration::from_secs(30) {
                    let _ = stream.sync_update_state().await;
                    c.save_session();
                    last_save = Instant::now();
                }
            }
        });
    }

    // ───────────────────────── Lệnh ─────────────────────────

    /// Gọi một lệnh. Xem docs/FFI.md.
    pub async fn call(self: &Arc<Self>, method: &str, a: Value) -> Res {
        let t = Instant::now();
        // Trạng thái của `dispatch` gộp mọi nhánh (kiểu TL to) — để trên heap (GramMini D20)
        let r = Box::pin(self.dispatch(method, &a)).await;
        let ms = t.elapsed().as_millis();
        match &r {
            Ok(_) => self.set_net(true),
            Err(e) if e.code == "NETWORK" || e.code == "TIMEOUT" => self.set_net(false),
            Err(_) => {}
        }
        match &r {
            Ok(_) => self.log(&format!("{method}: xong {ms} ms")),
            Err(e) => self.log(&format!("{method}: lỗi {} sau {ms} ms", e.code)),
        }
        r
    }

    /// Tệp trên máy (khoá `file:<đường dẫn>`, local.rs): None = không phải khoá tệp.
    fn local_arg(a: &Value) -> Option<Result<(String, Song), CoreError>> {
        let path = a.get("key").and_then(Value::as_str)?.strip_prefix(crate::local::PREFIX)?;
        Some(
            crate::local::song(path)
                .map(|s| (path.to_string(), s))
                .ok_or_else(|| CoreError::new("NO_SONG", "Không còn tệp này trên máy.")),
        )
    }

    fn song_arg(&self, a: &Value) -> Result<Song, CoreError> {
        let key = arg_str(a, "key")?;
        lock(&self.lib).song(key).cloned().ok_or_else(|| CoreError::new("NO_SONG", "Bài này không còn trong thư viện."))
    }

    /// Lời bài hát (LRCLIB): `key` (bài trong thư viện) hoặc `key` + `title`, `artist`, `dur` (nhạc trên máy); `refresh` = hỏi lại.
    /// Bản đã lưu trả ngay (cả khi đã tắt); hỏi LRCLIB chỉ khi anh đã bật trong Cài đặt.
    async fn lyrics(self: &Arc<Self>, a: &Value) -> Res {
        let key = arg_str(a, "key")?.to_string();
        let song = match a.get("title").and_then(Value::as_str) {
            Some(t) => Song {
                title: t.to_string(),
                artist: a.get("artist").and_then(Value::as_str).unwrap_or("").to_string(),
                dur: a.get("dur").and_then(Value::as_u64).unwrap_or(0) as u32,
                ..Default::default()
            },
            None => self.song_arg(a)?,
        };
        let name: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let name = if name.len() > 80 || name != key {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut h);
            format!("x{:016x}", h.finish())
        } else {
            name
        };
        let dir = self.cfg.data_dir.join("lyrics");
        let path = dir.join(format!("{name}.json"));
        // Một bài chỉ hỏi LRCLIB một lần: khung lời trong app và màn hình khoá (GMLockState) cùng hỏi lúc đổi bài → bên sau chờ bên
        // trước xong rồi đọc bản đã lưu
        static GATES: std::sync::OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = std::sync::OnceLock::new();
        let gate = {
            let mut g = lock(GATES.get_or_init(Default::default));
            g.retain(|_, m| Arc::strong_count(m) > 1);
            Arc::clone(g.entry(name.clone()).or_default())
        };
        let _turn = gate.lock().await;
        if !a.get("refresh").and_then(Value::as_bool).unwrap_or(false) {
            if let Some(v) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
                // Lời chạy / bài không lời: giữ mãi; lời thường / không thấy: 7 ngày hỏi lại (LRCLIB có thể thêm lời chạy); bản lưu
                // trước khi biết tìm thêm bản có lời chạy (không có "v") thì hỏi lại ngay
                let keep = v["synced"] == true
                    || v["instrumental"] == true
                    || (v["v"].as_u64() >= Some(crate::lyrics::CACHE_V) && age.is_some_and(|e| e < crate::lyrics::MISS_TTL));
                if keep {
                    return Ok(v);
                }
            }
        }
        if !Self::settings(&self.cfg.data_dir).get("lyrics").and_then(Value::as_bool).unwrap_or(false) {
            return Err(CoreError::new("LYRICS_OFF", "Chưa bật lời bài hát trong Cài đặt."));
        }
        let t = Instant::now();
        let rec = crate::lyrics::fetch(&song).await.map_err(|e| CoreError::new("NETWORK", e))?;
        let v = crate::lyrics::to_result(rec.as_ref());
        let _ = std::fs::create_dir_all(&dir);
        let _ = crate::library::write_atomic(&path, v.to_string().as_bytes());
        self.log(&format!(
            "lời bài hát {key}: {} ({} ms)",
            if v["synced"] == true {
                "lời chạy"
            } else if v["instrumental"] == true {
                "bài không lời"
            } else if v["found"] == true {
                "lời thường (không có mốc thời gian)"
            } else {
                "không có"
            },
            t.elapsed().as_millis()
        ));
        Ok(v)
    }

    async fn dispatch(self: &Arc<Self>, method: &str, a: &Value) -> Res {
        match method {
            "auth.state" => {
                if self.dead.load(Ordering::Relaxed) {
                    return Ok(json!({"authorized": false, "dead": true}));
                }
                let ok = match self.client.is_authorized().await {
                    Ok(v) => v,
                    Err(e) => {
                        let ce = CoreError::from(e);
                        if self.check_dead(&ce.code) {
                            return Ok(json!({"authorized": false, "dead": true}));
                        }
                        return Err(ce);
                    }
                };
                if !ok {
                    return Ok(json!({"authorized": false}));
                }
                let me = self.client.get_me().await?;
                Ok(json!({"authorized": true, "me": user_json(&me)}))
            }
            "auth.sendCode" => {
                let phone = arg_str(a, "phone")?;
                let token = self.client.request_login_code(phone, &self.cfg.api_hash).await?;
                *lock(&self.login) = Some(token);
                Ok(json!({}))
            }
            "auth.signIn" => {
                let code = arg_str(a, "code")?;
                let token = lock(&self.login).take().ok_or_else(|| CoreError::new("NO_CODE", "Chưa gửi mã."))?;
                match self.client.sign_in(&token, code).await {
                    Ok(me) => self.signed_in(&me).await,
                    Err(SignInError::PasswordRequired(p)) => {
                        let hint = p.hint().unwrap_or("").to_string();
                        *lock(&self.password) = Some(p);
                        let mut e = CoreError::new("PASSWORD_NEEDED", "Tài khoản có mật khẩu 2 lớp.");
                        e.extra = json!({"hint": hint});
                        Err(e)
                    }
                    Err(SignInError::InvalidCode) => {
                        *lock(&self.login) = Some(token);
                        Err(CoreError::new("PHONE_CODE_INVALID", "Mã không đúng."))
                    }
                    Err(SignInError::SignUpRequired) => Err(CoreError::new(
                        "SIGN_UP_REQUIRED",
                        "Số này chưa có tài khoản Telegram — đăng ký bằng app Telegram chính thức trước.",
                    )),
                    Err(SignInError::Other(e)) => Err(e.into()),
                    Err(e) => Err(CoreError::new("SIGN_IN", e.to_string())),
                }
            }
            "auth.checkPassword" => {
                let pw = arg_str(a, "password")?;
                let token = lock(&self.password).take().ok_or_else(|| CoreError::new("NO_PASSWORD", "Không cần mật khẩu."))?;
                // Thông số SRP chỉ dùng được một lần (GramMini): sai / SRP_ID_INVALID → xin thông số mới
                let mut r = self.client.check_password(token, pw).await;
                if matches!(&r, Err(SignInError::Other(e)) if e.is("SRP_ID_INVALID")) {
                    let fresh = self.password_token().await?;
                    r = self.client.check_password(fresh, pw).await;
                }
                match r {
                    Ok(me) => self.signed_in(&me).await,
                    Err(SignInError::InvalidPassword(t)) => {
                        *lock(&self.password) = Some(self.password_token().await.unwrap_or(t));
                        Err(CoreError::new("PASSWORD_INVALID", "Mật khẩu không đúng."))
                    }
                    Err(SignInError::Other(e)) => {
                        if let Ok(t) = self.password_token().await {
                            *lock(&self.password) = Some(t);
                        }
                        Err(e.into())
                    }
                    Err(e) => Err(CoreError::new("SIGN_IN", e.to_string())),
                }
            }
            "auth.logout" => {
                let _ = self.client.sign_out().await;
                self.save_session();
                (self.emit)(json!({"e": "auth.state", "authorized": false}));
                Ok(json!({}))
            }
            "proxy.get" => {
                let link = Self::settings(&self.cfg.data_dir).get("proxy").cloned().unwrap_or(Value::Null);
                Ok(json!({"link": link}))
            }
            "proxy.set" => {
                let link = a.get("link").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
                let cfg = match link {
                    Some(l) => gm_net::parse_link(l).map_err(|e| CoreError::new("PROXY_LINK", e))?,
                    None => ProxyConfig::Direct,
                };
                self.save_settings(&[("proxy", json!(link))])?;
                self.conn.set(cfg);
                for dc in 1..=5 {
                    self.handle.disconnect_from_dc(dc);
                }
                Ok(json!({}))
            }
            "proxy.test" => {
                let cfg = match a.get("link").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                    Some(l) => gm_net::parse_link(l).map_err(|e| CoreError::new("PROXY_LINK", e))?,
                    None => ProxyConfig::Direct,
                };
                let conn = GmConnector::new(cfg);
                let t = Instant::now();
                let addr = ServerAddr::Custom { dc_id: 2, address: "149.154.167.41:443".parse().unwrap(), connector: conn.clone() };
                tokio::time::timeout(Duration::from_secs(25), grammers_mtsender::connect(conn.transport(), addr))
                    .await
                    .map_err(|_| CoreError::new("TIMEOUT", "Quá 25 giây không kết nối được."))?
                    .map_err(|e| CoreError::new("PROXY_FAIL", e.to_string()))?;
                Ok(json!({"ms": t.elapsed().as_millis() as u64}))
            }
            "net.state" => Ok(json!({"ok": self.net.load(Ordering::Relaxed) != 2})),
            "net.check" => Ok(json!({"ok": self.check_net().await})),
            "app.flush" => {
                self.save_session();
                lock(&self.lib).flush()?;
                Ok(json!({}))
            }

            // ── Thư viện ──
            "library.query" => {
                let q = a.get("q").and_then(Value::as_str).unwrap_or("");
                let source = a.get("source").and_then(Value::as_str).unwrap_or("");
                let sort = Sort::parse(a.get("sort").and_then(Value::as_str).unwrap_or("date"));
                let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(100).min(500) as usize;
                let keep = self.keep_fn(a);
                let (total, page, sections, gen) = {
                    let mut lib = lock(&self.lib);
                    let (t, p, s) = lib.query(q, source, sort, offset, limit, &*keep);
                    (t, p, s, lib.generation)
                };
                Ok(json!({"total": total, "gen": gen, "offset": offset, "items": self.songs_json(&page), "sections": sections}))
            }
            "library.keys" => {
                let q = a.get("q").and_then(Value::as_str).unwrap_or("");
                let source = a.get("source").and_then(Value::as_str).unwrap_or("");
                let sort = Sort::parse(a.get("sort").and_then(Value::as_str).unwrap_or("date"));
                let keep = self.keep_fn(a);
                let mut keys = lock(&self.lib).keys(q, source, sort, &*keep);
                keys.truncate(5000);
                Ok(json!(keys))
            }
            "library.songs" => {
                let keys = arg_keys(a);
                if keys.iter().any(|k| k.starts_with(crate::local::PREFIX)) {
                    // Có tệp trên máy: bài thư viện hỏi một lượt, tệp đọc tại chỗ — giữ đúng thứ tự
                    let items: Vec<Item> = keys
                        .iter()
                        .filter(|k| !k.starts_with(crate::local::PREFIX))
                        .map(|k| Item { key: k.clone(), ..Default::default() })
                        .collect();
                    let mut lib = self.items_json(&items).into_iter();
                    let out: Vec<Value> = keys
                        .iter()
                        .map(|k| match k.strip_prefix(crate::local::PREFIX) {
                            Some(p) => crate::local::song(p).map(|s| crate::local::to_json(p, &s)).unwrap_or_else(|| {
                                let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned());
                                json!({"key": k, "title": name.unwrap_or_default(), "missing": true, "ok": false})
                            }),
                            None => lib.next().unwrap_or(Value::Null),
                        })
                        .collect();
                    return Ok(Value::Array(out));
                }
                let items: Vec<Item> = keys.iter().map(|k| Item { key: k.clone(), ..Default::default() }).collect();
                Ok(Value::Array(self.items_json(&items)))
            }
            "library.stats" => {
                let (total, sources, gen) = {
                    let mut lib = lock(&self.lib);
                    let t = lib.total();
                    (t, lib.sources().len(), lib.generation)
                };
                let st = SyncState::load(&self.cfg.data_dir);
                Ok(json!({"total": total, "sources": sources, "gen": gen, "offline": lock(&self.offline).len(),
                          "full_done": st.full_done, "last_sync": st.last_sync, "partial": !st.phase.is_empty(),
                          "sync": lock(&self.sync_info).clone()}))
            }
            "sources.list" => {
                let me = self.me_key();
                let mut v: Vec<Source> = lock(&self.lib).sources().to_vec();
                let counts: HashMap<String, usize> = {
                    let lib = lock(&self.lib);
                    v.iter().map(|s| (s.peer.clone(), lib.indexed(&s.peer))).collect()
                };
                v.sort_by(|x, y| {
                    let sx = me.as_deref() == Some(&x.peer);
                    let sy = me.as_deref() == Some(&y.peer);
                    sy.cmp(&sx).then(counts[&y.peer].cmp(&counts[&x.peer])).then(x.name.cmp(&y.name))
                });
                Ok(Value::Array(v.iter().map(|s| self.source_json(s)).collect()))
            }
            "sources.set" => {
                let peer = arg_str(a, "peer")?;
                let on = a.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                lock(&self.lib).update_source(peer, |s| s.enabled = on)?;
                self.library_changed();
                Ok(json!({"enabled": on}))
            }
            "sources.add" => Box::pin(self.add_source(arg_str(a, "link")?)).await,
            "sources.topics" => Box::pin(self.topics(arg_str(a, "peer")?)).await,
            "sources.remove" => {
                let peer = arg_str(a, "peer")?;
                lock(&self.lib).remove_source(peer)?;
                self.library_changed();
                Ok(json!({}))
            }
            "sync.start" => {
                let full = a.get("full").and_then(Value::as_bool).unwrap_or(false);
                let peer = a.get("peer").and_then(Value::as_str).map(str::to_string);
                Ok(json!({"started": self.start_sync(full, peer)}))
            }
            "sync.stop" => {
                self.sync_stop.store(true, Ordering::Relaxed);
                Ok(json!({}))
            }
            "sync.state" => Ok(lock(&self.sync_info).clone()),

            // ── Phát ──
            "play.url" => {
                if let Some(r) = Self::local_arg(a) {
                    // Tệp trên máy: định dạng AVPlayer đọc được → file:// ; FLAC / OGG / Opus → lõi giải qua cổng phát
                    let (path, song) = r?;
                    if !song.playable() {
                        return Err(CoreError::new("FORMAT", format!("Định dạng {} chưa phát được.", song.ext.to_uppercase())));
                    }
                    if !crate::decode::needs_decode(&song.ext) {
                        return Ok(json!({"url": crate::local::file_url(&path), "cached": 1.0}));
                    }
                    self.streams.add_local(song.clone(), path.into());
                    return Ok(json!({"url": self.streams.url(&song).await?, "cached": 1.0}));
                }
                let song = self.song_arg(a)?;
                if !song.playable() {
                    return Err(CoreError::new(
                        "FORMAT",
                        format!("Định dạng {} chưa phát được trên iOS 5 / 6.", song.ext.to_uppercase()),
                    ));
                }
                let url = self.streams.url(&song).await?;
                if let Some(p) = self.streams.complete_path(&song) {
                    crate::cache::touch(&p);
                }
                Ok(json!({"url": url, "cached": self.streams.cached_fraction(&song)}))
            }
            "play.prefetch" => {
                if Self::local_arg(a).is_some() {
                    return Ok(json!({}));
                }
                let song = self.song_arg(a)?;
                if song.playable() && self.streams.complete_path(&song).is_none() {
                    // 2 khúc đầu (512 KB ≈ 15–30 giây nhạc): bấm «Tiếp» là có tiếng ngay
                    self.streams.prefetch(&song, 2)?;
                }
                Ok(json!({}))
            }
            "play.played" => {
                if Self::local_arg(a).is_some() {
                    return Ok(json!({})); // tệp trên máy không vào «Nghe gần đây» (danh sách của lõi chỉ giữ bài Telegram)
                }
                let song = self.song_arg(a)?;
                lock(&self.pls).played(Item::from(&song))?;
                Ok(json!({}))
            }
            "play.pos" => {
                // Nhịp nghe: vị trí đang phát của bài (app báo mỗi giây / lúc dừng, tua, đổi bài)
                let key = arg_str(a, "key")?;
                let pos = a.get("pos").and_then(Value::as_f64).unwrap_or(0.0);
                let playing = a.get("playing").and_then(Value::as_bool).unwrap_or(true);
                // Tệp trên máy giải mã: nhịp theo khoá nội bộ của bản WAV
                let inner = key.strip_prefix(crate::local::PREFIX).and_then(crate::local::inner_key);
                self.streams.set_pace(inner.as_deref().unwrap_or(key), pos, playing);
                Ok(json!({}))
            }
            "play.info" => {
                // Thông số tệp (định dạng, tần số, bit, bitrate, cỡ) — đọc phần đầu tệp đã có; chưa có thì chỉ định dạng + cỡ
                if let Some(r) = Self::local_arg(a) {
                    let (path, song) = r?;
                    let mut head = Vec::new();
                    if let Ok(f) = std::fs::File::open(&path) {
                        use std::io::Read;
                        let _ = f.take(64 * 1024).read_to_end(&mut head);
                    }
                    let mut v = crate::info::to_json(&song, &crate::info::parse(&song.ext, &head));
                    v["ready"] = json!(true);
                    return Ok(v);
                }
                let song = self.song_arg(a)?;
                let head = self.streams.head_bytes(&song, 64 * 1024);
                let info = crate::info::parse(&song.ext, head.as_deref().unwrap_or(&[]));
                let mut v = crate::info::to_json(&song, &info);
                v["ready"] = json!(head.is_some());
                Ok(v)
            }
            "play.cached" => {
                if Self::local_arg(a).is_some() {
                    return Ok(json!({"cached": 1.0}));
                }
                let song = self.song_arg(a)?;
                Ok(json!({"cached": self.streams.cached_fraction(&song)}))
            }

            // ── Tải về máy ──
            "dl.add" => {
                let keys = arg_keys(a);
                let mut n = 0;
                {
                    let mut off = lock(&self.offline);
                    let mut q = lock(&self.dlq);
                    for k in keys {
                        if lock(&self.lib).song(&k).is_some() && off.insert(k.clone()) {
                            q.push_back(k);
                            n += 1;
                        }
                    }
                }
                self.save_offline();
                self.run_downloads();
                Ok(json!({"added": n}))
            }
            "dl.remove" => {
                // Bỏ khỏi «Đã tải» và xoá tệp (cả phần đệm của bài)
                let keys: HashSet<String> = arg_keys(a).into_iter().collect();
                {
                    let mut off = lock(&self.offline);
                    for k in &keys {
                        off.remove(k);
                    }
                }
                self.save_offline();
                lock(&self.dlq).retain(|k| !keys.contains(k));
                for k in &keys {
                    self.streams.forget(k);
                }
                let dir = self.cfg.data_dir.clone();
                let busy = self.streams.busy();
                let off = lock(&self.offline).clone();
                let kinds = vec!["offline".to_string(), "music".to_string()];
                let (freed, files) =
                    tokio::task::spawn_blocking(move || crate::cache::clear(&dir, &off, Some(&kinds), None, Some(&keys), &busy))
                        .await
                        .map_err(|e| CoreError::new("IO", e.to_string()))?;
                Ok(json!({"freed": freed, "files": files}))
            }
            "dl.state" => {
                let q: Vec<String> = lock(&self.dlq).iter().cloned().collect();
                Ok(json!({"queue": q, "running": self.dl_running.load(Ordering::Relaxed)}))
            }

            // ── Ảnh ──
            "media.cover" => {
                let song = self.song_arg(a)?;
                let path = self.cfg.data_dir.join("covers").join(format!("{}.jpg", song.key()));
                if self.cache.hit(&path) {
                    return Ok(json!({"path": path}));
                }
                if !song.thumb {
                    return Err(CoreError::new("NO_COVER", "Bài không có ảnh bìa."));
                }
                let _permit = self.dl.acquire().await.map_err(|_| CoreError::new("BUSY", "hàng tải đã đóng"))?;
                let pr = peer_ref_of_key(&self.session, &song.peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat chứa bài."))?;
                let msg = self.client.get_messages_by_id(pr, &[song.id]).await?.into_iter().flatten().next();
                let Some(Media::Document(d)) = msg.as_ref().and_then(|m| m.media()) else {
                    return Err(CoreError::new("NO_MEDIA", "Tin chứa bài này đã bị xoá."));
                };
                let loc = media::doc_thumb_loc(&d).ok_or_else(|| CoreError::new("NO_COVER", "Bài không có ảnh bìa."))?;
                self.download(&loc, &path).await?;
                Ok(json!({"path": path}))
            }
            "media.avatar" => {
                let key = arg_str(a, "peer")?;
                let peer = peer_ref_of_key(&self.session, key).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat này."))?;
                let photo: i64 = arg_str(a, "photo")?.parse().map_err(|_| CoreError::new("BAD_ARGS", "«photo» sai"))?;
                let dc = a.get("dc").and_then(Value::as_i64).unwrap_or(0) as i32;
                let path = self.cfg.data_dir.join("avatars").join(format!("{key}_{photo}.jpg"));
                if self.cache.hit(&path) {
                    return Ok(json!({"path": path}));
                }
                let _permit = self.dl.acquire().await.map_err(|_| CoreError::new("BUSY", "hàng tải đã đóng"))?;
                self.download(&media::avatar_loc(&peer, photo, dc), &path).await?;
                Ok(json!({"path": path}))
            }

            // ── Playlist, Yêu thích, Nghe gần đây ──
            "pl.list" => {
                let pls = lock(&self.pls);
                let lists: Vec<Value> = pls
                    .lists()
                    .iter()
                    .map(|p| json!({"id": p.id, "name": p.name, "count": p.items.len(), "updated": p.updated,
                                    "cover": p.items.first().map(|i| i.key.clone())}))
                    .collect();
                Ok(json!({"lists": lists, "fav": pls.favs().len(), "recent": pls.history().len()}))
            }
            "pl.create" => {
                let name = arg_str(a, "name")?.trim().to_string();
                if name.is_empty() {
                    return Err(CoreError::new("BAD_ARGS", "Chưa đặt tên playlist."));
                }
                let items = self.items_of(&arg_keys(a));
                let mut pls = lock(&self.pls);
                let id = pls.create(&name)?;
                if !items.is_empty() {
                    pls.add(id, items)?;
                }
                Ok(json!({"id": id}))
            }
            "pl.fromQuery" => {
                let name = arg_str(a, "name")?.trim().to_string();
                if name.is_empty() {
                    return Err(CoreError::new("BAD_ARGS", "Chưa đặt tên playlist."));
                }
                let q = a.get("q").and_then(Value::as_str).unwrap_or("");
                let source = a.get("source").and_then(Value::as_str).unwrap_or("");
                let sort = Sort::parse(a.get("sort").and_then(Value::as_str).unwrap_or("date"));
                let keep = self.keep_fn(a);
                let keys = lock(&self.lib).keys(q, source, sort, &*keep);
                drop(keep);
                if keys.is_empty() {
                    return Err(CoreError::new("EMPTY", "Danh sách không có bài nào."));
                }
                let items = self.items_of(&keys);
                let mut pls = lock(&self.pls);
                // Cùng tên đã có (bấm lại «mỗi chủ đề một playlist») → thêm bài mới vào playlist cũ thay vì tạo bản trùng
                let id = match pls.lists().iter().find(|p| p.name == name).map(|p| p.id) {
                    Some(id) => id,
                    None => pls.create(&name)?,
                };
                let added = pls.add(id, items)?;
                Ok(json!({"id": id, "count": keys.len(), "added": added}))
            }
            "pl.rename" => {
                let id = pl_id(a)?;
                lock(&self.pls).rename(id, arg_str(a, "name")?)?;
                Ok(json!({}))
            }
            "pl.delete" => {
                lock(&self.pls).delete(pl_id(a)?)?;
                Ok(json!({}))
            }
            "pl.items" => {
                let items: Vec<Item> = {
                    let pls = lock(&self.pls);
                    match a.get("id") {
                        Some(Value::String(s)) if s == "fav" => pls.favs().to_vec(),
                        Some(Value::String(s)) if s == "recent" => pls.history().to_vec(),
                        _ => pls.get(pl_id(a)?).map(|p| p.items.clone()).ok_or_else(|| CoreError::new("NO_PLAYLIST", "Playlist đã bị xoá."))?,
                    }
                };
                Ok(Value::Array(self.items_json(&items)))
            }
            "pl.add" => {
                let id = pl_id(a)?;
                let items = self.items_of(&arg_keys(a));
                let n = lock(&self.pls).add(id, items)?;
                Ok(json!({"added": n}))
            }
            "pl.remove" => {
                let keys = arg_keys(a);
                match a.get("id") {
                    Some(Value::String(s)) if s == "recent" => lock(&self.pls).clear_history()?,
                    Some(Value::String(s)) if s == "fav" => {
                        let mut pls = lock(&self.pls);
                        for k in keys {
                            pls.set_fav(Item { key: k, ..Default::default() }, false)?;
                        }
                    }
                    _ => {
                        lock(&self.pls).remove(pl_id(a)?, &keys)?;
                    }
                }
                Ok(json!({}))
            }
            "pl.move" => {
                let id = pl_id(a)?;
                let from = a.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to = a.get("to").and_then(Value::as_u64).unwrap_or(0) as usize;
                lock(&self.pls).move_item(id, from, to)?;
                Ok(json!({}))
            }
            "fav.set" => {
                let song = self.song_arg(a)?;
                let on = a.get("on").and_then(Value::as_bool).unwrap_or(true);
                let fav = lock(&self.pls).set_fav(Item::from(&song), on)?;
                Ok(json!({"fav": fav}))
            }

            // ── Bộ nhớ đệm ──
            "cache.stats" => {
                let dir = self.cfg.data_dir.clone();
                let off = lock(&self.offline).clone();
                let entries = tokio::task::spawn_blocking(move || crate::cache::scan(&dir, &off))
                    .await
                    .map_err(|e| CoreError::new("IO", e.to_string()))?;
                let mut kinds: std::collections::BTreeMap<&str, u64> = crate::cache::KINDS.iter().map(|k| (*k, 0)).collect();
                let mut by_src: HashMap<String, u64> = HashMap::new();
                let mut total = 0u64;
                for e in &entries {
                    total += e.size;
                    *kinds.entry(e.kind).or_default() += e.size;
                    if let (Some(p), true) = (&e.peer, e.kind == "music" || e.kind == "offline") {
                        *by_src.entry(p.clone()).or_default() += e.size;
                    }
                }
                let mut srcs: Vec<(String, u64)> = by_src.into_iter().collect();
                srcs.sort_by(|a, b| b.1.cmp(&a.1));
                srcs.truncate(50);
                let names: HashMap<String, String> = lock(&self.lib).sources().iter().map(|s| (s.peer.clone(), s.name.clone())).collect();
                let p = self.cache.policy();
                let disk = crate::cache::disk_space(&self.cfg.data_dir);
                Ok(json!({
                    "total": total, "files": entries.len(), "kinds": kinds,
                    "sources": srcs.iter().map(|(p, b)| json!({"peer": p, "name": names.get(p), "bytes": b})).collect::<Vec<_>>(),
                    "limit_mb": p.limit_mb, "keep_days": p.keep_days,
                    "free": disk.map(|d| d.0), "disk": disk.map(|d| d.1),
                }))
            }
            "cache.clear" => {
                let kinds: Option<Vec<String>> =
                    a.get("kinds").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect());
                let peer = a.get("peer").and_then(Value::as_str).map(str::to_string);
                let dir = self.cfg.data_dir.clone();
                let busy = self.streams.busy();
                let off = lock(&self.offline).clone();
                let clears_offline = kinds.as_ref().is_some_and(|k| k.iter().any(|x| x == "offline"));
                let (freed, n) = tokio::task::spawn_blocking(move || {
                    crate::cache::clear(&dir, &off, kinds.as_deref(), peer.as_deref(), None, &busy)
                })
                .await
                .map_err(|e| CoreError::new("IO", e.to_string()))?;
                if clears_offline {
                    // Bài đã tải bị xoá tệp → bỏ khỏi «Đã tải» (trừ bài đang phát — tệp còn)
                    let lib = lock(&self.lib);
                    lock(&self.offline).retain(|k| lib.song(k).is_some_and(|s| self.streams.complete_path(s).is_some()));
                    drop(lib);
                    self.save_offline();
                }
                self.log(&format!("xoá đệm: {n} tệp ({} KB)", freed >> 10));
                (self.emit)(json!({"e": "cache.cleared"}));
                Ok(json!({"freed": freed, "files": n}))
            }
            "lyrics.get" => self.lyrics(a).await,
            "net.dns" => {
                // Thử tra tên miền (M17): `host`, `fail` (nhãn máy chủ coi như bị chặn — thử chuỗi dự phòng), `fresh` (quên đã nhớ)
                let host = arg_str(a, "host")?.to_string();
                let fail: std::collections::HashSet<String> = a
                    .get("fail")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                if !fail.is_empty() || a.get("fresh").and_then(Value::as_bool).unwrap_or(false) {
                    gm_net::dns::clear_cache();
                }
                gm_net::dns::set_debug_fail(fail);
                let t = Instant::now();
                let r = gm_net::dns::for_connect(&host).await;
                gm_net::dns::set_debug_fail(Default::default());
                let ms = t.elapsed().as_millis() as u64;
                match r {
                    Ok(Some(ip)) => Ok(json!({"ip": ip.to_string(), "via": gm_net::dns::via(&host), "ms": ms, "secure": true})),
                    Ok(None) => {
                        // Tắt / chuỗi không trả lời → DNS của máy
                        let sys = tokio::time::timeout(Duration::from_secs(10), tokio::net::lookup_host((host.as_str(), 0))).await;
                        let ip = sys.ok().and_then(|r| r.ok()).and_then(|mut it| it.next()).map(|s| s.ip().to_string());
                        Ok(json!({"ip": ip, "via": "DNS của máy", "ms": t.elapsed().as_millis() as u64,
                                  "secure": gm_net::dns::enabled()}))
                    }
                    Err(e) => Err(CoreError::new("NO_DOMAIN", e.to_string())),
                }
            }
            "settings.get" => {
                let s = Self::settings(&self.cfg.data_dir);
                Ok(json!({
                    "ahead_secs": self.streams.ahead(),
                    "lyrics": s.get("lyrics").and_then(Value::as_bool).unwrap_or(false),
                    "secure_dns": gm_net::dns::enabled(),
                    "dns_providers": gm_net::dns::providers(),
                }))
            }
            "settings.set" => {
                let mut kv: Vec<(&str, Value)> = Vec::new();
                if let Some(v) = a.get("ahead_secs").and_then(Value::as_u64) {
                    let v = v.min(3600);
                    self.streams.set_ahead(v);
                    self.log(&format!("tải trước: {}", if v == 0 { "cả bài".to_string() } else { format!("{v} giây") }));
                    kv.push(("ahead_secs", json!(v)));
                }
                if let Some(v) = a.get("lyrics").and_then(Value::as_bool) {
                    kv.push(("lyrics", json!(v)));
                }
                if let Some(v) = a.get("secure_dns").and_then(Value::as_bool) {
                    gm_net::dns::set_enabled(v);
                    self.log(&format!("DNS mã hoá: {}", if v { "bật" } else { "tắt" }));
                    kv.push(("secure_dns", json!(v)));
                }
                if !kv.is_empty() {
                    self.save_settings(&kv)?;
                }
                Ok(json!({}))
            }
            "cache.set" => {
                let mut p = self.cache.policy();
                if let Some(v) = a.get("limit_mb").and_then(Value::as_u64) {
                    p.limit_mb = v;
                }
                if let Some(v) = a.get("keep_days").and_then(Value::as_u64) {
                    p.keep_days = v;
                }
                self.save_settings(&[("cache_limit_mb", json!(p.limit_mb)), ("cache_keep_days", json!(p.keep_days))])?;
                self.cache.set_policy(p);
                self.trim_soon(true);
                Ok(json!({}))
            }
            _ => Err(CoreError::new("NO_METHOD", format!("không có lệnh {method}"))),
        }
    }

    /// Mục playlist (bản chép tên…) cho các khoá còn trong thư viện.
    fn items_of(&self, keys: &[String]) -> Vec<Item> {
        let lib = lock(&self.lib);
        keys.iter().filter_map(|k| lib.song(k)).map(Item::from).collect()
    }

    async fn signed_in(self: &Arc<Self>, me: &grammers_client::peer::User) -> Res {
        self.save_session();
        let key = peer_key(me.id());
        let _ = self.save_settings(&[("me", json!(key))]);
        *lock(&self.me) = Some(key);
        (self.emit)(json!({"e": "auth.state", "authorized": true}));
        self.after_login().await;
        // Lần đầu: quét toàn bộ tài khoản
        self.start_sync(true, None);
        Ok(json!({"me": user_json(me)}))
    }
}

fn pl_id(a: &Value) -> Result<u32, CoreError> {
    a.get("id")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .map(|v| v as u32)
        .ok_or_else(|| CoreError::new("BAD_ARGS", "thiếu «id»"))
}

/// Tên người dùng từ «@tên», «tên», «t.me/tên», «https://t.me/tên/123», «tg://resolve?domain=tên».
pub fn username_of(link: &str) -> Option<String> {
    let l = link.trim();
    let rest = if let Some(i) = l.find("domain=") {
        l[i + 7..].split('&').next().unwrap_or("")
    } else if let Some(i) = l.find("t.me/").or_else(|| l.find("telegram.me/")) {
        let start = i + if l[i..].starts_with("t.me/") { 5 } else { 12 };
        l[start..].trim_start_matches("s/").split(['/', '?']).next().unwrap_or("")
    } else {
        l.trim_start_matches('@')
    };
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (name.len() >= 4 && name.len() <= 32 && !name.starts_with('+') && name.chars().next()?.is_ascii_alphabetic()).then_some(name)
}

fn peer_key(id: PeerId) -> String {
    id.bot_api_dialog_id().map(|i| i.to_string()).unwrap_or_default()
}

fn peer_name(p: &Peer) -> String {
    match p {
        Peer::User(u) => {
            let n = u.full_name();
            if n.trim().is_empty() { "Tài khoản đã xoá".into() } else { n }
        }
        _ => p.name().unwrap_or("").to_string(),
    }
}

fn peer_kind(p: &Peer) -> &'static str {
    match p {
        Peer::User(u) if u.is_bot() => "bot",
        Peer::User(_) => "user",
        Peer::Group(_) | Peer::Community(_) => "group",
        Peer::Channel(_) => "channel",
    }
}

/// Ảnh đại diện của peer: (photo_id dạng chuỗi, DC).
fn peer_photo(p: &Peer) -> (Option<String>, Option<i32>) {
    let x = match p {
        Peer::User(u) => u.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Group(g) => g.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Channel(c) => c.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Community(c) => c.photo().map(|x| (x.photo_id, x.dc_id)),
    };
    match x {
        Some((id, dc)) => (Some(id.to_string()), Some(dc)),
        None => (None, None),
    }
}

fn source_from_peer(p: &Peer) -> Source {
    let (photo, photo_dc) = peer_photo(p);
    Source {
        peer: peer_key(p.id()),
        name: peer_name(p),
        kind: peer_kind(p).into(),
        username: p.username().map(str::to_string),
        photo,
        photo_dc,
        enabled: true,
        forum: matches!(p, Peer::Group(g) if matches!(&g.raw, tl::enums::Chat::Channel(c) if c.forum)),
        ..Default::default()
    }
}

fn user_json(u: &grammers_client::peer::User) -> Value {
    let (photo, photo_dc) = peer_photo(&Peer::User(u.clone()));
    json!({"id": peer_key(u.id()), "name": u.full_name(), "username": u.username(), "phone": u.phone(), "photo": photo, "photo_dc": photo_dc})
}

/// Tệp tạm riêng cho mỗi lần tải (`<tên>.<số>.part`) — hai lệnh cùng tải một tệp không ghi chồng (GramMini D27).
fn part_path(path: &std::path::Path) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    path.with_extension(format!("{}.part", N.fetch_add(1, Ordering::Relaxed)))
}

/// Tách khoá bài cho app (khỏi phải tự ghép chuỗi).
pub fn key_parts(key: &str) -> Option<(String, i32)> {
    split_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(username_of("@nhacviet_hay").as_deref(), Some("nhacviet_hay"));
        assert_eq!(username_of("https://t.me/nhacviet_hay/123").as_deref(), Some("nhacviet_hay"));
        assert_eq!(username_of("t.me/s/lofimusic").as_deref(), Some("lofimusic"));
        assert_eq!(username_of("tg://resolve?domain=lofimusic&post=5").as_deref(), Some("lofimusic"));
        assert_eq!(username_of("https://t.me/+AbCdEf"), None); // link mời riêng tư: chưa hỗ trợ
        assert_eq!(username_of("ab"), None);
    }
}
