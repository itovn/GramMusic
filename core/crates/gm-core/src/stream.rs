//! Phát nhạc qua mạng: cổng HTTP nội bộ (`127.0.0.1`, có `Range`) cho AVPlayer, dữ liệu lấy từng khúc 256 KB qua
//! `upload.getFile` rồi ghi xuống đĩa.
//!
//! - Mỗi bài một tệp trong `music/`: đang dở là `<khoá>.<đuôi>.part` (tệp thưa, đủ cỡ) + `.map` (khúc nào đã có); đủ khúc thì
//!   đổi tên thành `<khoá>.<đuôi>` — lần sau nghe không cần mạng (bài nghe trọn = đã nằm trong bộ nhớ đệm).
//! - Khúc 256 KB: `upload.getFile` đòi offset / limit chia hết cho 4 KB, 1 MB chia hết cho limit và một lần đọc không vắt qua
//!   ranh giới 1 MB — khúc 256 KB bắt đầu ở bội 256 KB luôn thoả.
//! - Trình phát đọc tới đâu thì tải tới đó (ưu tiên khúc ở vị trí đang đọc — tua là tải ngay chỗ mới), rồi tải tiếp phần còn
//!   lại của bài khi bài còn đang được nghe; người dùng chuyển bài (không còn ai đọc quá vài giây) thì thôi, trừ khi bài được
//!   «Tải về» (`full`). Mỗi bài tối đa 2 khúc tải song song, cả lõi tối đa 4.
//! - URL có mã ngẫu nhiên theo lần mở lõi (`/s/<mã>/<khoá>.<đuôi>`): app khác trên máy không đọc ké được.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering as AtOrd};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore};

use crate::decode::{self, Pcm, TrackReader};
use crate::library::Song;
use crate::text;

/// Cỡ một khúc tải.
pub const BLOCK: u64 = 256 * 1024;
/// Khúc tải song song của một bài.
const TRACK_WORKERS: u32 = 2;
/// Không còn ai đọc quá chừng này thì thôi tải (trừ bài «Tải về»).
pub(crate) const IDLE: Duration = Duration::from_secs(6);
/// Chờ một khúc quá chừng này thì báo lỗi cho trình phát.
const WAIT_BLOCK: Duration = Duration::from_secs(45);

/// Tải theo nhịp nghe: mặc định chỉ tải / trả dữ liệu tới chừng này giây trước chỗ đang phát (tạm dừng → đứng yên ở đó) — tệp 100
/// MB nghe 1 phút rồi dừng không tốn cả 100 MB (anh yêu cầu 17/11/2026, DECISIONS M10). Đổi được trong Cài đặt («Tải trước», M12).
pub const AHEAD_SECS: u64 = 60;
/// Bài chưa tới lượt phát (AVQueuePlayer nạp sẵn bài kế): chỉ chừng này byte đầu.
pub const HEAD_ALLOW: u64 = 2 << 20;
/// Phần cuối tệp luôn được đọc: AVPlayer đọc thử cuối tệp lúc mở (moov của m4a, 5 KB cuối WAV).
pub const TAIL_ALLOW: u64 = 1 << 20;

/// Vị trí đang phát app báo lên (`play.pos`).
#[derive(Clone)]
struct Pace {
    key: String,
    pos: f64,
    playing: bool,
    at: Instant,
}

/// Byte / giây ước của tệp gốc (cỡ / thời lượng; không rõ thời lượng: 64 KB/s ≈ 512 kbit/s).
pub fn raw_bps(song: &Song) -> f64 {
    if song.dur > 0 { song.size as f64 / song.dur as f64 } else { 64_000.0 }
}

/// Lỗi tải tạm thời (mạng chậm / mất, Telegram bắt chờ): đáng chờ rồi thử lại thay vì bỏ.
fn transient_code(code: &str) -> bool {
    matches!(code, "NETWORK" | "TIMEOUT" | "FLOOD_WAIT")
}

/// Như `transient_code` cho lỗi của luồng giải (chuỗi, mang mã lỗi tải gốc — vd. «đọc tệp lỗi: TIMEOUT: …»).
fn transient_msg(msg: &str) -> bool {
    msg.contains("NETWORK") || msg.contains("TIMEOUT") || msg.contains("FLOOD_WAIT") || msg.contains("quá lâu")
}

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Lỗi lấy dữ liệu: `retry` = đáng thử lại (mạng, FLOOD_WAIT…), kèm thời gian chờ gợi ý.
#[derive(Debug, Clone)]
pub struct FetchError {
    pub code: String,
    pub msg: String,
    pub retry: Option<Duration>,
}

/// Nơi lấy dữ liệu của bài: lõi thật dùng `upload.getFile` (service.rs); phép thử dùng dữ liệu trong bộ nhớ.
pub trait Fetch: Send + Sync + 'static {
    /// Đọc khúc bắt đầu ở `offset` (bội của BLOCK), tối đa BLOCK byte (khúc cuối ngắn hơn).
    fn fetch<'a>(&'a self, song: &'a Song, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>>;
}

pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

struct TState {
    have: Vec<bool>,
    inflight: HashSet<u32>,
    /// Khúc trình phát đang cần / sắp cần.
    want: u32,
    /// Tải trọn bài kể cả khi không ai nghe («Tải về»).
    full: bool,
    /// Chỉ tải trước tới khúc này (bài kế tiếp trong hàng chờ) khi chưa ai đọc.
    prefetch: u32,
    workers: u32,
    readers: u32,
    last_read: Instant,
    err: Option<FetchError>,
    done: bool,
    last_emit: Instant,
}

/// Một bài đang phát / đang tải.
pub struct Track {
    pub song: Song,
    n: u32,
    part: PathBuf,
    map: PathBuf,
    fin: PathBuf,
    st: Mutex<TState>,
    notify: Notify,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(unix)]
pub(crate) fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(f, buf, off)
}
#[cfg(windows)]
pub(crate) fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(f, buf, off)
}
#[cfg(unix)]
pub(crate) fn write_all_at(f: &std::fs::File, buf: &[u8], off: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::write_all_at(f, buf, off)
}
#[cfg(windows)]
pub(crate) fn write_all_at(f: &std::fs::File, mut buf: &[u8], mut off: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_write(f, buf, off)?;
        buf = &buf[n..];
        off += n as u64;
    }
    Ok(())
}

impl Track {
    fn open(dir: &Path, song: Song) -> io::Result<Track> {
        let name = song.file_name();
        let fin = dir.join(&name);
        let part = dir.join(format!("{name}.part"));
        let map = dir.join(format!("{name}.map"));
        let n = song.size.div_ceil(BLOCK).max(1) as u32;
        let mut have = vec![false; n as usize];
        let mut done = false;
        if std::fs::metadata(&fin).map(|m| m.len() == song.size).unwrap_or(false) {
            have.iter_mut().for_each(|h| *h = true);
            done = true;
        } else {
            let ok = std::fs::metadata(&part).map(|m| m.len() == song.size).unwrap_or(false);
            match std::fs::read(&map) {
                Ok(m) if ok && m.len() == n as usize => {
                    for (h, b) in have.iter_mut().zip(m) {
                        *h = b == 1;
                    }
                }
                _ => {
                    std::fs::create_dir_all(dir)?;
                    let f = std::fs::File::create(&part)?;
                    f.set_len(song.size)?; // tệp thưa: chưa tốn chỗ thật
                    std::fs::write(&map, vec![0u8; n as usize])?;
                }
            }
        }
        let now = Instant::now();
        Ok(Track {
            song,
            n,
            part,
            map,
            fin,
            st: Mutex::new(TState {
                have,
                inflight: HashSet::new(),
                want: 0,
                full: false,
                prefetch: 0,
                workers: 0,
                readers: 0,
                last_read: now - IDLE * 2,
                err: None,
                done,
                last_emit: now,
            }),
            notify: Notify::new(),
        })
    }

    pub fn key(&self) -> String {
        self.song.key()
    }

    /// (khúc đã có, tổng số khúc).
    pub fn progress(&self) -> (u32, u32) {
        let st = lock(&self.st);
        (st.have.iter().filter(|h| **h).count() as u32, self.n)
    }

    pub fn is_done(&self) -> bool {
        lock(&self.st).done
    }

    /// Lỗi hẳn (không tự thử lại nữa) của lần tải gần nhất, khi không còn lượt tải nào chạy.
    pub fn failed(&self) -> Option<FetchError> {
        let st = lock(&self.st);
        if st.done || st.workers > 0 {
            return None;
        }
        st.err.clone().filter(|e| e.retry.is_none())
    }

    /// Còn đang tải trọn bài («Tải về»).
    pub fn is_full(&self) -> bool {
        lock(&self.st).full
    }

    /// Còn việc (đang đọc, vừa đọc, đang tải trọn / tải trước, đang có khúc bay).
    fn active(st: &TState) -> bool {
        st.readers > 0 || st.last_read.elapsed() < IDLE || st.full || !st.inflight.is_empty()
    }

    /// Khúc nên tải tiếp. Tải trọn («Tải về máy»): từ chỗ đang đọc tới hết rồi quay lại phần đầu. Đang nghe: khúc trình phát đang
    /// chờ, rồi các khúc sau nó nhưng chỉ tới `limit` (byte — nhịp nghe). Không ai nghe: tải trước vài khúc đầu.
    fn next_block(&self, st: &TState, limit: u64) -> Option<u32> {
        if st.done || st.err.as_ref().is_some_and(|e| e.retry.is_none()) {
            return None;
        }
        let listening = st.readers > 0 || st.last_read.elapsed() < IDLE;
        let free = |b: u32| !st.have[b as usize] && !st.inflight.contains(&b);
        let want = st.want.min(self.n);
        if st.full {
            if let Some(b) = (want..self.n).find(|&b| free(b)) {
                return Some(b);
            }
            return (0..want).find(|&b| free(b));
        }
        if listening {
            if want < self.n && free(want) {
                return Some(want);
            }
            let lim = (limit.div_ceil(BLOCK) as u32).min(self.n);
            return (want..lim).find(|&b| free(b));
        }
        (0..st.prefetch.min(self.n)).find(|&b| free(b))
    }
}

/// Bộ phát luồng của lõi.
pub struct Streams {
    dir: PathBuf,
    fetch: Arc<dyn Fetch>,
    emit: Emit,
    tracks: Mutex<HashMap<String, Arc<Track>>>,
    /// Bài đang / đã giải mã thành WAV (FLAC, OGG Vorbis, Opus).
    pcms: Mutex<HashMap<String, Arc<Pcm>>>,
    /// Tệp trên máy cần giải mã (local.rs): khoá nội bộ `local_<băm>` → (bài, đường dẫn).
    locals: Mutex<HashMap<String, (Song, PathBuf)>>,
    /// Vị trí đang phát (nhịp nghe) — đổi thì đánh thức kết nối đang chờ.
    pace: Mutex<Option<Pace>>,
    pace_notify: Notify,
    /// Số giây tải trước chỗ đang phát; 0 = cả bài (Cài đặt → «Tải trước»).
    ahead: AtomicU64,
    sem: Arc<Semaphore>,
    token: String,
    /// Cổng phát đang mở: số cổng + việc nhận kết nối (bỏ được khi mở lại — M23)
    port: Mutex<Option<(u16, tokio::task::AbortHandle)>>,
    /// Một lượt mở / kiểm tra cổng một lúc
    port_gate: tokio::sync::Mutex<()>,
    /// Tìm bài theo khoá (thư viện của lõi).
    resolve: Box<dyn Fn(&str) -> Option<Song> + Send + Sync>,
}

impl Streams {
    pub fn new(dir: PathBuf, fetch: Arc<dyn Fetch>, emit: Emit, resolve: Box<dyn Fn(&str) -> Option<Song> + Send + Sync>) -> Arc<Streams> {
        let mut seed = [0u8; 8];
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let mix = t.as_nanos() as u64 ^ (std::process::id() as u64).rotate_left(32) ^ (&seed as *const _ as u64);
        seed.copy_from_slice(&mix.to_le_bytes());
        let token: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        Arc::new(Streams {
            dir,
            fetch,
            emit,
            tracks: Mutex::new(HashMap::new()),
            pcms: Mutex::new(HashMap::new()),
            locals: Mutex::new(HashMap::new()),
            pace: Mutex::new(None),
            pace_notify: Notify::new(),
            ahead: AtomicU64::new(AHEAD_SECS),
            sem: Arc::new(Semaphore::new(4)),
            token,
            port: Mutex::new(None),
            port_gate: tokio::sync::Mutex::new(()),
            resolve,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Mở (hoặc lấy lại) bài đang theo dõi; dọn các bài đã thôi dùng.
    pub fn track(&self, song: &Song) -> io::Result<Arc<Track>> {
        let key = song.key();
        let mut m = lock(&self.tracks);
        m.retain(|k, t| *k == key || Track::active(&lock(&t.st)));
        if let Some(t) = m.get(&key) {
            // Bài đổi cỡ (tin bị sửa thay tệp): mở lại
            if t.song.size == song.size {
                return Ok(Arc::clone(t));
            }
        }
        let t = Arc::new(Track::open(&self.dir, song.clone())?);
        m.insert(key, Arc::clone(&t));
        Ok(t)
    }

    /// Khoá các bài đang dùng (bộ nhớ đệm không xoá).
    pub fn busy(&self) -> HashSet<String> {
        let mut v: HashSet<String> =
            lock(&self.tracks).iter().filter(|(_, t)| Track::active(&lock(&t.st))).map(|(k, _)| k.clone()).collect();
        v.extend(lock(&self.pcms).iter().filter(|(_, p)| p.busy()).map(|(k, _)| k.clone()));
        v
    }

    /// App báo vị trí đang phát của bài (mỗi giây khi đang phát; lúc dừng, tua, đổi bài).
    pub fn set_pace(self: &Arc<Self>, key: &str, pos: f64, playing: bool) {
        *lock(&self.pace) = Some(Pace { key: key.to_string(), pos: pos.max(0.0), playing, at: Instant::now() });
        self.pace_notify.notify_waiters();
        let t = lock(&self.tracks).get(key).cloned();
        if let Some(t) = t {
            self.pump(&t); // nhịp đi tiếp → tải thêm cho đủ khoảng phía trước
        }
    }

    /// Đặt số giây tải trước (0 = cả bài) — có hiệu lực ngay với bài đang phát.
    pub fn set_ahead(self: &Arc<Self>, secs: u64) {
        self.ahead.store(secs, AtOrd::Relaxed);
        self.pace_notify.notify_waiters();
        let key = lock(&self.pace).as_ref().map(|p| p.key.clone());
        let t = key.and_then(|k| lock(&self.tracks).get(&k).cloned());
        if let Some(t) = t {
            self.pump(&t);
        }
    }

    pub fn ahead(&self) -> u64 {
        self.ahead.load(AtOrd::Relaxed)
    }

    /// Giới hạn byte (không tính) được tải / trả của bài: bài đang phát → tới `ahead` giây sau vị trí đang phát (đang phát thì
    /// tính cả thời gian từ lần báo gần nhất; `ahead` = 0 → cả bài); bài khác → `HEAD_ALLOW` byte đầu. `head` = byte trước dữ liệu
    /// âm thanh (đầu WAV).
    pub fn limit(&self, key: &str, size: u64, head: u64, bps: f64) -> u64 {
        let p = lock(&self.pace).clone();
        let ahead = self.ahead();
        match p {
            Some(p) if p.key == key && ahead == 0 => size,
            Some(p) if p.key == key => {
                let t = p.pos + if p.playing { p.at.elapsed().as_secs_f64().min(30.0) } else { 0.0 };
                let lim = head as f64 + (t + ahead as f64) * bps;
                (lim as u64).max(head + (1 << 20)).min(size)
            }
            _ => HEAD_ALLOW.min(size),
        }
    }

    /// Được trả đoạn bắt đầu ở `off` chưa (trong nhịp nghe, phần đầu, hoặc phần cuối tệp).
    fn may_serve(&self, key: &str, off: u64, size: u64, head: u64, bps: f64) -> bool {
        off < HEAD_ALLOW || off + TAIL_ALLOW >= size || off < self.limit(key, size, head, bps)
    }

    /// Ghi nhận tệp trên máy cần giải mã (FLAC / OGG / Opus ở «Tệp trên máy») — cổng phát trả WAV theo khoá `song.key()`.
    pub fn add_local(&self, song: Song, path: PathBuf) {
        lock(&self.locals).insert(song.key(), (song, path));
    }

    /// Bài theo khoá: tệp trên máy đã ghi nhận, rồi thư viện.
    fn find_song(&self, key: &str) -> Option<Song> {
        if let Some((s, _)) = lock(&self.locals).get(key) {
            return Some(s.clone());
        }
        (self.resolve)(key)
    }

    /// Nguồn đọc tệp gốc để giải mã: tệp trên máy, hoặc bộ đệm khúc tải từ Telegram.
    fn source(self: &Arc<Self>, song: &Song) -> io::Result<Box<dyn symphonia::core::io::MediaSource>> {
        let local = lock(&self.locals).get(&song.key()).map(|(_, p)| p.clone());
        if let Some(path) = local {
            return Ok(Box::new(std::fs::File::open(path)?));
        }
        let track = self.track(song)?;
        Ok(Box::new(TrackReader::new(Arc::clone(self), track, tokio::runtime::Handle::current())))
    }

    /// WAV giải mã của bài (FLAC / OGG / Opus): mở (đọc phần đầu tệp gốc — tải khúc đầu nếu chưa có), dọn bản đã giải cũ.
    pub async fn pcm(self: &Arc<Self>, song: &Song) -> io::Result<Arc<Pcm>> {
        let key = song.key();
        if let Some(p) = lock(&self.pcms).get(&key) {
            if p.song.size == song.size {
                return Ok(Arc::clone(p));
            }
        }
        let reader = self.source(song)?;
        let (dir, s2) = (self.dir.clone(), song.clone());
        let pcm = tokio::task::spawn_blocking(move || Pcm::open(&dir, s2, reader))
            .await
            .map_err(|e| io::Error::other(e.to_string()))??;
        let pcm = Arc::new(pcm);
        let mut m = lock(&self.pcms);
        m.insert(key.clone(), Arc::clone(&pcm));
        // Giữ bài đang dùng + vài bài gần nhất; bỏ bản ghi của bài đã xoá khỏi đĩa
        let mut keep: HashSet<String> = m.iter().filter(|(_, p)| p.busy()).map(|(k, _)| k.clone()).collect();
        keep.insert(key);
        for gone in decode::prune(&self.dir, &keep) {
            m.remove(&gone);
        }
        Ok(pcm)
    }

    /// Chạy luồng giải (blocking) cho bài nếu đang nghỉ.
    fn kick(self: &Arc<Self>, pcm: &Arc<Pcm>) {
        if pcm.is_done() {
            return;
        }
        // Mở nguồn trước khi giành quyền chạy: lỗi mở thì không kẹt cờ «đang chạy»
        let Ok(reader) = self.source(&pcm.song) else { return };
        if !pcm.begin_run() {
            return;
        }
        let p = Arc::clone(pcm);
        let me = Arc::clone(self);
        let (key, size, bps) = (pcm.key(), pcm.size, pcm.bytes_per_sec());
        let limit = move || if me.may_serve(&key, 0, size, decode::HEADER, bps) { me.limit(&key, size, decode::HEADER, bps) } else { 0 };
        tokio::task::spawn_blocking(move || p.run(reader, &limit));
    }

    /// Bỏ theo dõi (bài vừa bị xoá khỏi bộ nhớ đệm).
    pub fn forget(&self, key: &str) {
        lock(&self.tracks).remove(key);
    }

    /// Đường dẫn tệp đầy đủ của bài nếu đã tải đủ.
    pub fn complete_path(&self, song: &Song) -> Option<PathBuf> {
        let p = self.dir.join(song.file_name());
        std::fs::metadata(&p).ok().filter(|m| m.len() == song.size).map(|_| p)
    }

    /// Tiến độ đã có trên đĩa: 1 = đủ, 0…1 = dở, 0 = chưa có.
    pub fn cached_fraction(&self, song: &Song) -> f64 {
        if let Some(t) = lock(&self.tracks).get(&song.key()) {
            let (h, n) = t.progress();
            return h as f64 / n.max(1) as f64;
        }
        if self.complete_path(song).is_some() {
            return 1.0;
        }
        let name = song.file_name();
        match std::fs::read(self.dir.join(format!("{name}.map"))) {
            Ok(m) if !m.is_empty() => m.iter().filter(|b| **b == 1).count() as f64 / m.len() as f64,
            _ => 0.0,
        }
    }

    /// Phần đầu tệp gốc (tối đa `n` byte) nếu khúc đầu đã có trên máy — không tải (thông số tệp cho màn Đang phát).
    pub fn head_bytes(&self, song: &Song, n: usize) -> Option<Vec<u8>> {
        let n = n.min(song.size as usize).min(BLOCK as usize);
        let read = |path: &Path| -> Option<Vec<u8>> {
            let f = std::fs::File::open(path).ok()?;
            let mut buf = vec![0u8; n];
            let got = read_at(&f, &mut buf, 0).ok()?;
            buf.truncate(got);
            Some(buf)
        };
        let name = song.file_name();
        if let Some(p) = self.complete_path(song) {
            return read(&p);
        }
        let first = match lock(&self.tracks).get(&song.key()) {
            Some(t) => lock(&t.st).have.first().copied().unwrap_or(false),
            None => std::fs::read(self.dir.join(format!("{name}.map"))).ok().is_some_and(|m| m.first() == Some(&1)),
        };
        if first { read(&self.dir.join(format!("{name}.part"))) } else { None }
    }

    /// Tải trọn bài (`on`) hoặc thôi tải trọn (phần đã có giữ làm đệm).
    pub fn set_full(self: &Arc<Self>, song: &Song, on: bool) -> io::Result<Arc<Track>> {
        let t = self.track(song)?;
        {
            let mut st = lock(&t.st);
            st.full = on;
            if on {
                st.err = None;
            }
        }
        if on {
            self.pump(&t);
        }
        Ok(t)
    }

    /// Tải trước `blocks` khúc đầu (bài kế tiếp trong hàng chờ: bấm «Tiếp» là phát ngay).
    pub fn prefetch(self: &Arc<Self>, song: &Song, blocks: u32) -> io::Result<()> {
        let t = self.track(song)?;
        lock(&t.st).prefetch = blocks;
        self.pump(&t);
        Ok(())
    }

    /// Chờ tới khi dữ liệu quanh giây `pos` của bài đã có (WAV giải sẵn / khúc tệp gốc đã tải), tối đa `max`. `Ok(false)` = chưa có
    /// (hết giờ — hỏi lại). App chỉ ra lệnh tua cho AVPlayer khi có: tua tới chỗ chưa có dữ liệu thì mediaserverd chặn lúc mở đoạn đọc
    /// mới tới khi có, mạng chậm quá 10 giây là iOS giết nó (M22).
    pub async fn ready_at(self: &Arc<Self>, song: &Song, pos: f64, max: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + max;
        let pos = if pos.is_finite() { pos.max(0.0) } else { 0.0 };
        if decode::needs_decode(&song.ext) {
            let pcm = match tokio::time::timeout(max, self.pcm(song)).await {
                Ok(p) => p?,
                Err(_) => return Ok(false),
            };
            let off = decode::HEADER + (pos * pcm.bytes_per_sec()) as u64;
            let b0 = (off.min(pcm.size.saturating_sub(1)) / BLOCK) as u32;
            let me = Arc::clone(self);
            let pk = Arc::clone(&pcm);
            let kick = move || me.kick(&pk);
            // Khúc chứa vị trí + khúc sau (≈ 1,5 giây WAV mỗi khúc): tua xong phát được ngay
            for b in b0..=(b0 + 1).min(pcm.blocks() - 1) {
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(false);
                    }
                    match tokio::time::timeout(left, pcm.wait_block(b, &kick)).await {
                        Err(_) => return Ok(false),
                        Ok(Ok(())) => break,
                        Ok(Err(e)) if transient_msg(&e) => tokio::time::sleep(Duration::from_secs(2).min(left)).await,
                        Ok(Err(e)) => return Err(io::Error::other(e)),
                    }
                }
            }
            return Ok(true);
        }
        if song.size == 0 || self.complete_path(song).is_some() {
            return Ok(true);
        }
        let t = self.track(song)?;
        let n = song.size.div_ceil(BLOCK) as u32;
        // Tệp gốc (mp3 / m4a…): AVPlayer tự đổi giây ra byte — ước theo bitrate trung bình, lấy cả khúc trước / sau cho chắc
        let b = ((pos * raw_bps(song)) as u64).min(song.size - 1) / BLOCK;
        let b = b as u32;
        for b in b.saturating_sub(1)..=(b + 1).min(n - 1) {
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Ok(false);
                }
                match tokio::time::timeout(left, self.wait_block(&t, b)).await {
                    Err(_) => return Ok(false),
                    Ok(Ok(())) => break,
                    Ok(Err(e)) if transient_code(&e.code) => tokio::time::sleep(Duration::from_secs(2).min(left)).await,
                    Ok(Err(e)) => return Err(io::Error::other(format!("{}: {}", e.code, e.msg))),
                }
            }
        }
        Ok(true)
    }

    /// Chạy thêm lượt tải cho bài nếu còn khúc cần và chưa đủ lượt.
    fn pump(self: &Arc<Self>, t: &Arc<Track>) {
        let limit = self.limit(&t.song.key(), t.song.size, 0, raw_bps(&t.song));
        loop {
            let b = {
                let mut st = lock(&t.st);
                if st.workers >= TRACK_WORKERS {
                    return;
                }
                let Some(b) = t.next_block(&st, limit) else { return };
                st.inflight.insert(b);
                st.workers += 1;
                b
            };
            let (me, t) = (Arc::clone(self), Arc::clone(t));
            tokio::spawn(async move { me.worker(t, b).await });
        }
    }

    async fn worker(self: Arc<Self>, t: Arc<Track>, mut b: u32) {
        let mut fails = 0u32;
        loop {
            let permit = Arc::clone(&self.sem).acquire_owned().await;
            let r = self.fetch.fetch(&t.song, b as u64 * BLOCK).await;
            drop(permit);
            match r {
                Ok(data) => {
                    fails = 0;
                    let want = ((t.song.size - b as u64 * BLOCK).min(BLOCK)) as usize;
                    if data.len() < want {
                        // Máy chủ trả thiếu (tệp đổi?) → lỗi hẳn
                        self.fail(&t, b, FetchError { code: "SHORT".into(), msg: "Máy chủ trả thiếu dữ liệu.".into(), retry: None });
                        break;
                    }
                    if let Err(e) = self.store(&t, b, &data[..want]) {
                        self.fail(&t, b, FetchError { code: "IO".into(), msg: format!("Ghi tệp nhạc: {e}"), retry: None });
                        break;
                    }
                }
                Err(e) => {
                    fails += 1;
                    match e.retry {
                        Some(wait) if fails <= 6 => {
                            {
                                let mut st = lock(&t.st);
                                st.err = Some(e.clone());
                            }
                            t.notify.notify_waiters();
                            tokio::time::sleep(wait.min(Duration::from_secs(30))).await;
                            let st = lock(&t.st);
                            if !Track::active(&st) {
                                drop(st);
                                self.release(&t, b);
                                break;
                            }
                            continue;
                        }
                        _ => {
                            self.fail(&t, b, e);
                            break;
                        }
                    }
                }
            }
            // Khúc tiếp theo cho lượt này
            let limit = self.limit(&t.song.key(), t.song.size, 0, raw_bps(&t.song));
            let next = {
                let mut st = lock(&t.st);
                st.inflight.remove(&b);
                st.err = None;
                match t.next_block(&st, limit) {
                    Some(n) => {
                        st.inflight.insert(n);
                        Some(n)
                    }
                    None => {
                        st.workers -= 1;
                        None
                    }
                }
            };
            match next {
                Some(n) => b = n,
                None => return,
            }
        }
    }

    fn release(&self, t: &Track, b: u32) {
        let mut st = lock(&t.st);
        st.inflight.remove(&b);
        st.workers -= 1;
    }

    fn fail(&self, t: &Track, b: u32, e: FetchError) {
        {
            let mut st = lock(&t.st);
            st.inflight.remove(&b);
            st.workers -= 1;
            st.full = false;
            st.err = Some(e.clone());
        }
        t.notify.notify_waiters();
        (self.emit)(json!({"e": "stream.error", "key": t.key(), "code": e.code, "msg": e.msg}));
    }

    /// Ghi khúc `b`, đánh dấu đã có; đủ khúc thì đổi tên thành tệp đầy đủ.
    fn store(&self, t: &Track, b: u32, data: &[u8]) -> io::Result<()> {
        {
            let f = std::fs::OpenOptions::new().write(true).open(&t.part)?;
            write_all_at(&f, data, b as u64 * BLOCK)?;
        }
        let (all, emit) = {
            let mut st = lock(&t.st);
            st.have[b as usize] = true;
            let map: Vec<u8> = st.have.iter().map(|h| *h as u8).collect();
            std::fs::write(&t.map, map)?;
            let all = st.have.iter().all(|h| *h);
            let emit = all || st.last_emit.elapsed() > Duration::from_millis(800);
            if emit {
                st.last_emit = Instant::now();
            }
            (all, emit)
        };
        if all {
            std::fs::rename(&t.part, &t.fin)?;
            let _ = std::fs::remove_file(&t.map);
            lock(&t.st).done = true;
            (self.emit)(json!({"e": "stream.done", "key": t.key()}));
        } else if emit {
            let (h, n) = t.progress();
            (self.emit)(json!({"e": "stream.progress", "key": t.key(), "have": h, "total": n}));
        }
        t.notify.notify_waiters();
        Ok(())
    }

    /// Chờ tới khi khúc `b` có trên đĩa (đặt vị trí cần cho bộ tải).
    pub(crate) async fn wait_block(self: &Arc<Self>, t: &Arc<Track>, b: u32) -> Result<(), FetchError> {
        let deadline = Instant::now() + WAIT_BLOCK;
        loop {
            let notified = t.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut st = lock(&t.st);
                if st.have[b as usize] {
                    return Ok(());
                }
                if let Some(e) = st.err.as_ref().filter(|e| e.retry.is_none()) {
                    // Lỗi hẳn của lần trước: cho thử lại một lần khi trình phát hỏi lại
                    let e = e.clone();
                    st.err = None;
                    if st.workers > 0 {
                        return Err(e);
                    }
                }
                st.want = b;
                st.last_read = Instant::now();
            }
            self.pump(t);
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let e = lock(&t.st).err.clone();
                return Err(e.unwrap_or(FetchError { code: "TIMEOUT".into(), msg: "Tải nhạc quá lâu.".into(), retry: None }));
            }
            let _ = tokio::time::timeout(left.min(Duration::from_secs(1)), notified).await;
        }
    }

    /// Đọc [from, from + len) của tệp bài (đã chắc có trên đĩa).
    pub(crate) fn read_range(&self, t: &Track, from: u64, len: usize) -> io::Result<Vec<u8>> {
        let f = match std::fs::File::open(&t.part) {
            Ok(f) if !lock(&t.st).done => f,
            _ => std::fs::File::open(&t.fin).or_else(|_| std::fs::File::open(&t.part))?,
        };
        let mut buf = vec![0u8; len];
        let mut got = 0;
        while got < len {
            let n = read_at(&f, &mut buf[got..], from + got as u64)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "tệp nhạc ngắn hơn dự kiến"));
            }
            got += n;
        }
        Ok(buf)
    }

    /// Mở cổng phát nếu chưa; trả số cổng. Cổng đã mở mà không còn nhận kết nối thì mở lại (`revive`).
    pub async fn listen(self: &Arc<Self>) -> io::Result<u16> {
        Ok(self.ensure_port(true).await?.map(|(p, _)| p).unwrap_or(0))
    }

    /// Quay lại app: iOS thu hồi cổng nghe của app nằm nền lâu (tạm dừng → app bị treo) — kết nối vào cổng bị từ chối, AVPlayer báo
    /// -1004 «không kết nối được máy chủ» với mọi bài. Cổng chết thì mở lại, giữ số cổng cũ nếu được (item đang mở dùng tiếp địa
    /// chỉ cũ — M23). Trả (cổng, vừa mở lại?); chưa mở cổng lần nào thì None.
    pub async fn revive(self: &Arc<Self>) -> io::Result<Option<(u16, bool)>> {
        self.ensure_port(false).await
    }

    /// Thử (lệnh `stream.drop`): giả iOS thu hồi cổng phát — bỏ việc nhận, cổng đóng, kết nối bị từ chối tới khi `revive`.
    pub fn drop_port(&self) -> Option<u16> {
        let mut g = lock(&self.port);
        let (port, task) = g.take()?;
        task.abort();
        *g = Some((port, tokio::spawn(async {}).abort_handle()));
        crate::dlog(&format!("phát: (thử) đóng cổng phát {port}"));
        Some(port)
    }

    async fn ensure_port(self: &Arc<Self>, open: bool) -> io::Result<Option<(u16, bool)>> {
        let _gate = self.port_gate.lock().await;
        let old = lock(&self.port).as_ref().map(|(p, _)| *p);
        match old {
            Some(p) if port_alive(p).await => return Ok(Some((p, false))),
            None if !open => return Ok(None),
            _ => {}
        }
        if let Some((_, task)) = lock(&self.port).take() {
            task.abort(); // bỏ việc nhận trên cổng chết (thả cổng)
        }
        let (l, port) = bind_local(old.unwrap_or(0)).await?;
        let me = Arc::clone(self);
        let task = tokio::spawn(async move {
            let mut errs = 0u32;
            loop {
                match l.accept().await {
                    Ok((s, _)) => {
                        errs = 0;
                        let me = Arc::clone(&me);
                        tokio::spawn(async move {
                            let _ = me.handle(s).await;
                        });
                    }
                    Err(e) => {
                        errs += 1;
                        if errs == 1 {
                            crate::dlog(&format!("phát: cổng phát {port} nhận kết nối lỗi: {e}"));
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });
        *lock(&self.port) = Some((port, task.abort_handle()));
        if let Some(o) = old {
            crate::dlog(&format!("phát: cổng phát {o} không nhận kết nối (iOS thu hồi lúc app nằm nền) — mở lại cổng {port}"));
        }
        Ok(Some((port, old.is_some())))
    }

    /// URL phát của bài (mở cổng nếu chưa).
    pub async fn url(self: &Arc<Self>, song: &Song) -> io::Result<String> {
        let port = self.listen().await?;
        let ext = if decode::needs_decode(&song.ext) { "wav" } else if song.ext.is_empty() { "bin" } else { &song.ext };
        Ok(format!("http://127.0.0.1:{port}/s/{}/{}.{}", self.token, song.key(), ext))
    }

    async fn handle(self: Arc<Self>, mut s: TcpStream) -> io::Result<()> {
        let _ = s.set_nodelay(true);
        let head = match tokio::time::timeout(Duration::from_secs(15), read_head(&mut s)).await {
            Ok(Ok(h)) => h,
            _ => return Ok(()),
        };
        let req = match Request::parse(&head) {
            Some(r) => r,
            None => return respond(&mut s, "400 Bad Request", &[], b"").await,
        };
        let song = req
            .path
            .strip_prefix("/s/")
            .and_then(|p| p.split_once('/'))
            .filter(|(tok, _)| *tok == self.token)
            .and_then(|(_, f)| f.split('.').next())
            .and_then(|k| self.find_song(k));
        let Some(song) = song else { return respond(&mut s, "404 Not Found", &[], b"").await };
        let pcm = if req.path.ends_with(".wav") && decode::needs_decode(&song.ext) {
            // Mở hỏng có thể chỉ vì một lượt tải khúc khác hỏng cùng lúc (lỗi chung của bài) — thử lại vài lần trước khi báo lỗi
            let mut tries = 0;
            loop {
                match self.pcm(&song).await {
                    Ok(p) => break Some(p),
                    Err(e) if tries < 3 => {
                        tries += 1;
                        crate::dlog(&format!("phát: {} mở WAV lỗi (lần {tries}): {e} — thử lại", song.key()));
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                            _ = closed(&s) => return Ok(()),
                        }
                    }
                    Err(_) => return respond(&mut s, "500 Internal Server Error", &[], b"").await,
                }
            }
        } else {
            None
        };
        let size = pcm.as_ref().map(|p| p.size).unwrap_or(song.size);
        let ctype = if pcm.is_some() { "audio/x-wav" } else { text::mime_for_ext(&song.ext) };
        let (start, end, partial) = match req.range {
            None => (0, size.saturating_sub(1), false),
            Some(r) => match r.resolve(size) {
                Some((a, b)) => (a, b, true),
                None => {
                    let cr = format!("bytes */{size}");
                    return respond(&mut s, "416 Range Not Satisfiable", &[("Content-Range", &cr)], b"").await;
                }
            },
        };
        let len = if size == 0 { 0 } else { end - start + 1 };
        let clen = len.to_string();
        let crange = format!("bytes {start}-{end}/{size}");
        let mut hdrs: Vec<(&str, &str)> = vec![("Content-Type", ctype), ("Content-Length", &clen), ("Accept-Ranges", "bytes")];
        if partial {
            hdrs.push(("Content-Range", &crange));
        }
        let status = if partial { "206 Partial Content" } else { "200 OK" };
        crate::dlog(&format!(
            "phát: GET {} {}{}-{}/{size}",
            song.key(),
            if pcm.is_some() { "wav " } else { "" },
            start,
            end
        ));
        if let Some(pcm) = pcm {
            write_head(&mut s, status, &hdrs).await?;
            if req.head || len == 0 {
                return s.shutdown().await;
            }
            struct PReader(Arc<Pcm>);
            impl Drop for PReader {
                fn drop(&mut self) {
                    self.0.reader_leave();
                }
            }
            pcm.reader_enter();
            let _guard = PReader(Arc::clone(&pcm));
            let me = Arc::clone(&self);
            let pk = Arc::clone(&pcm);
            let kick = move || me.kick(&pk);
            let mut pos = start;
            let (wkey, wbps) = (song.key(), pcm.bytes_per_sec());
            let mut waiting = false;
            while pos <= end {
                let b = (pos / BLOCK) as u32;
                // Nhịp nghe: chưa tới thì chờ (tạm dừng → đứng yên), vẫn theo dõi trình phát ngắt kết nối
                while !self.may_serve(&wkey, b as u64 * BLOCK, size, decode::HEADER, wbps) {
                    tokio::select! {
                        _ = self.pace_notify.notified() => {}
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                        _ = closed(&s) => return Ok(()),
                    }
                }
                let r = tokio::select! {
                    r = pcm.wait_block(b, &kick) => r,
                    _ = closed(&s) => return Ok(()), // trình phát đã ngắt (chuyển bài, tua) — thôi chờ
                };
                if let Err(e) = r {
                    if transient_msg(&e) {
                        // Mạng chậm / mất: giữ kết nối, chờ rồi giải tiếp. Đóng thì AVPlayer phải mở lại đoạn đọc — lúc mở,
                        // mediaserverd chặn tới khi có dữ liệu, quá 10 giây iOS giết nó (mất tiếng — M22)
                        if !waiting {
                            crate::dlog(&format!("phát: {} khúc WAV {b}: {e} — giữ kết nối, chờ mạng", song.key()));
                            waiting = true;
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                            _ = closed(&s) => return Ok(()),
                        }
                        continue;
                    }
                    crate::dlog(&format!("phát: {} khúc WAV {b}: {e}", song.key()));
                    return Ok(());
                }
                if waiting {
                    crate::dlog(&format!("phát: {} khúc WAV {b}: có lại dữ liệu", song.key()));
                    waiting = false;
                }
                let block_end = ((b as u64 + 1) * BLOCK).min(size) - 1;
                let upto = block_end.min(end);
                let data = pcm.read_range(pos, (upto - pos + 1) as usize)?;
                s.write_all(&data).await?;
                pos = upto + 1;
                pcm.touch_read((pos / BLOCK) as u32);
            }
            s.flush().await?;
            return s.shutdown().await;
        }
        let t = match self.track(&song) {
            Ok(t) => t,
            Err(_) => return respond(&mut s, "500 Internal Server Error", &[], b"").await,
        };
        write_head(&mut s, status, &hdrs).await?;
        if req.head || len == 0 {
            return s.shutdown().await;
        }
        struct Reader(Arc<Track>);
        impl Drop for Reader {
            fn drop(&mut self) {
                let mut st = lock(&self.0.st);
                st.readers = st.readers.saturating_sub(1);
                st.last_read = Instant::now();
            }
        }
        lock(&t.st).readers += 1;
        let _guard = Reader(Arc::clone(&t));
        let mut pos = start;
        let (rkey, rbps) = (song.key(), raw_bps(&song));
        let mut waiting = false;
        while pos <= end {
            let b = (pos / BLOCK) as u32;
            // Nhịp nghe: chưa tới thì chờ (tạm dừng → đứng yên), vẫn theo dõi trình phát ngắt kết nối
            while !self.may_serve(&rkey, b as u64 * BLOCK, size, 0, rbps) {
                tokio::select! {
                    _ = self.pace_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    _ = closed(&s) => return Ok(()),
                }
            }
            let r = tokio::select! {
                r = self.wait_block(&t, b) => r,
                _ = closed(&s) => return Ok(()), // trình phát đã ngắt (chuyển bài, tua) — thôi chờ
            };
            if let Err(e) = r {
                if transient_code(&e.code) {
                    // Như WAV ở trên: giữ kết nối, chờ mạng rồi tải tiếp (M22)
                    if !waiting {
                        crate::dlog(&format!("phát: {} khúc {b}: {} {} — giữ kết nối, chờ mạng", song.key(), e.code, e.msg));
                        waiting = true;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                        _ = closed(&s) => return Ok(()),
                    }
                    continue;
                }
                crate::dlog(&format!("phát: {} khúc {b}: {} {}", song.key(), e.code, e.msg));
                return Ok(()); // lỗi hẳn: đóng kết nối — AVPlayer báo lỗi
            }
            if waiting {
                crate::dlog(&format!("phát: {} khúc {b}: có lại dữ liệu", song.key()));
                waiting = false;
            }
            let block_end = ((b as u64 + 1) * BLOCK).min(size) - 1;
            let upto = block_end.min(end);
            let data = self.read_range(&t, pos, (upto - pos + 1) as usize)?;
            s.write_all(&data).await?;
            pos = upto + 1;
            {
                let mut st = lock(&t.st);
                st.last_read = Instant::now();
                st.want = (pos / BLOCK) as u32;
            }
            self.pump(&t);
        }
        s.flush().await?;
        s.shutdown().await
    }
}

/// Cổng nghe 127.0.0.1:port còn nhận kết nối không (thử kết nối vào chính nó).
async fn port_alive(port: u16) -> bool {
    matches!(tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(("127.0.0.1", port))).await, Ok(Ok(_)))
}

/// Mở cổng nghe 127.0.0.1:want (0 = cổng bất kỳ). Cổng cũ vừa bỏ có thể chưa nhả ngay — thử lại một lúc rồi mới lấy cổng khác.
async fn bind_local(want: u16) -> io::Result<(TcpListener, u16)> {
    if want != 0 {
        for _ in 0..10 {
            match TcpListener::bind(("127.0.0.1", want)).await {
                Ok(l) => return Ok((l, want)),
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    }
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    Ok((l, port))
}

/// Xong khi bên kia đóng kết nối (đọc được 0 byte / lỗi). Byte thừa bên kia gửi (yêu cầu kế tiếp trên cùng kết nối) bỏ qua.
async fn closed(s: &TcpStream) {
    let mut b = [0u8; 256];
    loop {
        if s.readable().await.is_err() {
            return;
        }
        match s.try_read(&mut b) {
            Ok(0) => return,
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(_) => return,
        }
    }
}

/// Đọc phần đầu yêu cầu HTTP (tới dòng trống), tối đa 16 KB.
async fn read_head(s: &mut TcpStream) -> io::Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    loop {
        let n = s.read(&mut tmp).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "đóng giữa chừng"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.windows(2).any(|w| w == b"\n\n") {
            return Ok(String::from_utf8_lossy(&buf).into_owned());
        }
        if buf.len() > 16 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "đầu yêu cầu quá dài"));
        }
    }
}

async fn write_head(s: &mut TcpStream, status: &str, hdrs: &[(&str, &str)]) -> io::Result<()> {
    let mut h = format!("HTTP/1.1 {status}\r\n");
    for (k, v) in hdrs {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("Connection: close\r\nCache-Control: no-cache\r\n\r\n");
    s.write_all(h.as_bytes()).await
}

async fn respond(s: &mut TcpStream, status: &str, hdrs: &[(&str, &str)], body: &[u8]) -> io::Result<()> {
    let len = body.len().to_string();
    let mut all: Vec<(&str, &str)> = hdrs.to_vec();
    all.push(("Content-Length", &len));
    write_head(s, status, &all).await?;
    s.write_all(body).await?;
    s.shutdown().await
}

/// `Range: bytes=…` (một đoạn).
#[derive(Debug, PartialEq, Clone, Copy)]
enum Range {
    /// a-b hoặc a- (b = None).
    From(u64, Option<u64>),
    /// -n: n byte cuối.
    Suffix(u64),
}

impl Range {
    fn parse(v: &str) -> Option<Range> {
        let v = v.trim().strip_prefix("bytes=")?;
        let first = v.split(',').next()?.trim();
        let (a, b) = first.split_once('-')?;
        let (a, b) = (a.trim(), b.trim());
        if a.is_empty() {
            return Some(Range::Suffix(b.parse().ok()?));
        }
        Some(Range::From(a.parse().ok()?, if b.is_empty() { None } else { Some(b.parse().ok()?) }))
    }

    /// Đoạn [đầu, cuối] (cuối tính cả) trong tệp cỡ `size`; None = không thoả (416).
    fn resolve(self, size: u64) -> Option<(u64, u64)> {
        if size == 0 {
            return None;
        }
        match self {
            Range::From(a, b) => {
                if a >= size {
                    return None;
                }
                let e = b.unwrap_or(size - 1).min(size - 1);
                (e >= a).then_some((a, e))
            }
            Range::Suffix(n) => {
                if n == 0 {
                    return None;
                }
                Some((size.saturating_sub(n), size - 1))
            }
        }
    }
}

struct Request {
    head: bool,
    path: String,
    range: Option<Range>,
}

impl Request {
    fn parse(h: &str) -> Option<Request> {
        let mut lines = h.lines();
        let mut first = lines.next()?.split_whitespace();
        let method = first.next()?;
        let path = first.next()?.to_string();
        if method != "GET" && method != "HEAD" {
            return None;
        }
        let mut range = None;
        for l in lines {
            if let Some((k, v)) = l.split_once(':') {
                if k.trim().eq_ignore_ascii_case("range") {
                    range = Range::parse(v);
                }
            }
        }
        Some(Request { head: method == "HEAD", path, range })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn ranges() {
        assert_eq!(Range::parse("bytes=0-1"), Some(Range::From(0, Some(1))));
        assert_eq!(Range::parse(" bytes=500-"), Some(Range::From(500, None)));
        assert_eq!(Range::parse("bytes=-100"), Some(Range::Suffix(100)));
        assert_eq!(Range::parse("items=0-1"), None);
        assert_eq!(Range::From(0, Some(1)).resolve(1000), Some((0, 1)));
        assert_eq!(Range::From(10, None).resolve(1000), Some((10, 999)));
        assert_eq!(Range::From(10, Some(5000)).resolve(1000), Some((10, 999)));
        assert_eq!(Range::From(1000, None).resolve(1000), None);
        assert_eq!(Range::Suffix(100).resolve(1000), Some((900, 999)));
        assert_eq!(Range::Suffix(5000).resolve(1000), Some((0, 999)));
    }

    /// Dữ liệu giả trong bộ nhớ, chậm một chút, đếm số lần lấy.
    struct Fake {
        data: Vec<u8>,
        calls: AtomicUsize,
        fail_first: AtomicUsize,
        /// «Mất mạng» với khúc bắt đầu trong [down_from, down_to): lỗi TIMEOUT không gợi ý thử lại (tới tận luồng giải / trình
        /// phát); down_from = u64::MAX = có mạng
        down_from: std::sync::atomic::AtomicU64,
        down_to: std::sync::atomic::AtomicU64,
    }

    fn net_up() -> std::sync::atomic::AtomicU64 {
        std::sync::atomic::AtomicU64::new(u64::MAX)
    }

    impl Fetch for Fake {
        fn fetch<'a>(&'a self, _song: &'a Song, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.calls.fetch_add(1, Ordering::SeqCst);
                // Trừ một lần lỗi giả chỉ khi còn (hai lượt tải song song không được cùng trừ — trước đây tràn số → lỗi mãi)
                if offset >= self.down_from.load(Ordering::SeqCst) && offset < self.down_to.load(Ordering::SeqCst) {
                    return Err(FetchError { code: "TIMEOUT".into(), msg: "thử".into(), retry: None });
                }
                if self.fail_first.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
                    return Err(FetchError { code: "NETWORK".into(), msg: "thử".into(), retry: Some(Duration::from_millis(20)) });
                }
                let a = offset as usize;
                let b = (a + BLOCK as usize).min(self.data.len());
                Ok(self.data[a..b].to_vec())
            })
        }
    }

    fn setup(tag: &str, size: usize, fail_first: usize) -> (Arc<Streams>, Arc<Fake>, Song, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gmu-stream-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data: Vec<u8> = (0..size).map(|i| (i * 7 % 251) as u8).collect();
        let fake = Arc::new(Fake { data, calls: AtomicUsize::new(0), fail_first: AtomicUsize::new(fail_first), down_from: net_up(), down_to: net_up() });
        let song = Song { peer: "-1001".into(), id: 42, title: "t".into(), size: size as u64, ext: "mp3".into(), ..Default::default() };
        let s2 = song.clone();
        let events: Emit = Arc::new(|_| {});
        let st = Streams::new(dir.join("music"), fake.clone(), events, Box::new(move |k| (k == s2.key()).then(|| s2.clone())));
        (st, fake, song, dir)
    }

    async fn get(url: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut s = TcpStream::connect(host).await.unwrap();
        let mut req = format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n");
        if let Some(r) = range {
            req.push_str(&format!("Range: {r}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let mut all = Vec::new();
        s.read_to_end(&mut all).await.unwrap();
        let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&all[..split]).into_owned(), all[split + 4..].to_vec())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serves_ranges_and_completes_file() {
        let size = (BLOCK * 5 + 1234) as usize;
        let (st, fake, song, dir) = setup("serve", size, 1);
        let url = st.url(&song).await.unwrap();
        // Thăm dò kiểu AVPlayer: 2 byte đầu
        let (h, body) = get(&url, Some("bytes=0-1")).await;
        assert!(h.starts_with("HTTP/1.1 206"), "{h}");
        assert!(h.contains(&format!("Content-Range: bytes 0-1/{size}")), "{h}");
        assert!(h.contains("Content-Type: audio/mpeg"));
        assert_eq!(body, fake.data[..2]);
        // Tua giữa bài, vắt qua ranh giới khúc
        let a = BLOCK as usize * 3 - 10;
        let (h, body) = get(&url, Some(&format!("bytes={a}-"))).await;
        assert!(h.contains(&format!("Content-Length: {}", size - a)), "{h}");
        assert_eq!(body, fake.data[a..]);
        // Cả bài
        let (h, body) = get(&url, None).await;
        assert!(h.starts_with("HTTP/1.1 200"));
        assert_eq!(body.len(), size);
        assert_eq!(body, fake.data);
        // Đọc xong: bộ tải điền nốt khúc còn thiếu → tệp đầy đủ, hết .part / .map
        for _ in 0..200 {
            if st.complete_path(&song).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let fin = st.complete_path(&song).expect("tệp đầy đủ");
        assert_eq!(std::fs::read(&fin).unwrap(), fake.data);
        assert!(!dir.join("music").join("-1001_42.mp3.part").exists());
        assert_eq!(st.cached_fraction(&song), 1.0);
        // Mỗi khúc chỉ tải một lần (+1 lần lỗi mạng thử lại)
        assert_eq!(fake.calls.load(Ordering::SeqCst), 6 + 1);
        // URL sai mã → 404; ngoài cỡ → 416
        let bad = url.replace("/s/", "/s/x");
        assert!(get(&bad, None).await.0.starts_with("HTTP/1.1 404"));
        assert!(get(&url, Some(&format!("bytes={size}-"))).await.0.starts_with("HTTP/1.1 416"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bài từ tệp mẫu (tests/data): FLAC / OGG thật để thử giải mã.
    fn setup_file(tag: &str, file: &str, ext: &str, dur: u32) -> (Arc<Streams>, Arc<Fake>, Song, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gmu-dec-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = std::fs::read(format!("{}/tests/data/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let fake = Arc::new(Fake { data, calls: AtomicUsize::new(0), fail_first: AtomicUsize::new(0), down_from: net_up(), down_to: net_up() });
        let song = Song { peer: "-1002".into(), id: 7, title: "t".into(), size: fake.data.len() as u64, ext: ext.into(), dur,
                          ..Default::default() };
        let s2 = song.clone();
        let st = Streams::new(dir.join("music"), fake.clone(), Arc::new(|_| {}), Box::new(move |k| (k == s2.key()).then(|| s2.clone())));
        (st, fake, song, dir)
    }

    fn wav_fmt(h: &[u8]) -> (u16, u32, u16, u32) {
        let ch = u16::from_le_bytes([h[22], h[23]]);
        let rate = u32::from_le_bytes(h[24..28].try_into().unwrap());
        let bits = u16::from_le_bytes([h[34], h[35]]);
        let data = u32::from_le_bytes(h[40..44].try_into().unwrap());
        (ch, rate, bits, data)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn flac_to_wav_bit_exact_and_seek_without_seektable() {
        use sha2::Digest;
        let (st, _fake, song, dir) = setup_file("flac", "noise44.flac", "flac", 4);
        let url = st.url(&song).await.unwrap();
        assert!(url.ends_with(".wav"), "{url}");
        let size = 44 + 705_600u64;
        // Tua trước khi giải gì: đoạn giữa bài (khúc WAV thứ 3) — tệp không có SEEKTABLE
        let (h, mid) = get(&url, Some("bytes=600000-650000")).await;
        assert!(h.starts_with("HTTP/1.1 206"), "{h}");
        assert!(h.contains("Content-Type: audio/x-wav"), "{h}");
        assert!(h.contains(&format!("Content-Range: bytes 600000-650000/{size}")), "{h}");
        assert_eq!(mid.len(), 50_001);
        // Cả bài
        let (h, full) = get(&url, None).await;
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        assert_eq!(full.len() as u64, size);
        assert_eq!(&full[0..4], b"RIFF");
        assert_eq!(wav_fmt(&full), (2, 44_100, 16, 705_600));
        // Khớp từng bit với ffmpeg (ffmpeg -i noise44.flac -f s16le -ac 2 -)
        let sha: String = sha2::Sha256::digest(&full[44..]).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(sha, "d21ec0481170a29313a66a0f5fc576861582da20d4fae8ac4f5a78fdeaed3946");
        // Đoạn giải sau khi tua = đúng đoạn đó của bản giải tuần tự
        assert_eq!(mid, full[600_000..=650_000]);
        // Đủ khúc → tệp WAV đầy đủ trên đĩa, tệp FLAC gốc cũng đủ (nghe lại không cần mạng)
        for _ in 0..100 {
            if Pcm::final_path(st.dir(), &song).exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(std::fs::read(Pcm::final_path(st.dir(), &song)).unwrap(), full);
        assert!(st.complete_path(&song).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hires_flac_downsampled_and_ogg_vorbis() {
        // 24 bit / 96 kHz → 16 bit / 48 kHz
        let (st, _f, song, dir) = setup_file("hires", "hires96.flac", "flac", 2);
        let url = st.url(&song).await.unwrap();
        let (_, mid) = get(&url, Some("bytes=200000-210000")).await;
        let (_, full) = get(&url, None).await;
        assert_eq!(wav_fmt(&full), (2, 48_000, 16, 96_000 * 4));
        assert_eq!(mid, full[200_000..=210_000]);
        // Sóng sin 1 kHz biên độ 1/8 (lavfi sine mặc định), ffmpeg nhân 1/√2 khi đổi 1 kênh → 2 kênh (-ac 2) → đỉnh ≈ 32768 / 8 / √2 ≈ 2896
        let peak = full[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        assert!((2850..2950).contains(&peak), "đỉnh {peak}");
        let _ = std::fs::remove_dir_all(&dir);
        // OGG Vorbis 2 giây
        let (st, _f, song, dir) = setup_file("ogg", "tone.ogg", "ogg", 2);
        let url = st.url(&song).await.unwrap();
        let (h, full) = get(&url, None).await;
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        let (ch, rate, bits, data) = wav_fmt(&full);
        assert_eq!((ch, rate, bits), (2, 44_100, 16));
        assert!((80_000..=90_000).contains(&(data / 4)), "{data}");
        let peak = full[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        assert!(peak > 1000, "đỉnh {peak}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sai lệch so với sóng sin lý tưởng (lavfi sine: biên độ 1/8, pha 0 ở mẫu 0) — tỉ số tín hiệu / nhiễu (dB) của kênh `c`.
    fn sine_snr(pcm: &[u8], c: usize, freq: f64, first: usize, from: usize, n: usize) -> f64 {
        let (mut sig, mut err) = (0f64, 0f64);
        for i in from..from + n {
            let j = (i * 2 + c) * 2;
            let v = i16::from_le_bytes([pcm[j], pcm[j + 1]]) as f64;
            let t = (first + i) as f64 / 48_000.0;
            let want = 4096.0 * (2.0 * std::f64::consts::PI * freq * t).sin();
            sig += want * want;
            err += (v - want) * (v - want);
        }
        10.0 * (sig / err.max(1e-9)).log10()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ogg_opus_exact_length_aligned_and_seek() {
        // 20 giây, trái 440 Hz, phải 1 kHz, 48 kbps (ffmpeg -c:a libopus) → 960 000 mẫu đúng như ffmpeg giải (bỏ pre-skip 312)
        for ext in ["opus", "ogg"] {
            let (st, _f, song, dir) = setup_file(&format!("opus-{ext}"), "tone.opus", ext, 20);
            let url = st.url(&song).await.unwrap();
            assert!(url.ends_with(".wav"), "{url}");
            // WAV 3,8 MB > phần đầu luôn cho đọc (2 MB): báo đang nghe để lõi giải / trả theo nhịp nghe
            st.set_pace(&song.key(), 10.0, true);
            let size = 44 + 960_000u64 * 4;
            // Tua trước: đoạn ở giây ~12 (khúc WAV thứ 9)
            let at = 2_304_000usize;
            let (h, mid) = get(&url, Some(&format!("bytes={at}-{}", at + 96_000 - 1))).await;
            assert!(h.starts_with("HTTP/1.1 206"), "{h}");
            assert_eq!(mid.len(), 96_000);
            let (h, full) = get(&url, None).await;
            assert!(h.starts_with("HTTP/1.1 200"), "{h}");
            assert_eq!(full.len() as u64, size);
            assert_eq!(wav_fmt(&full), (2, 48_000, 16, 960_000 * 4));
            let pcm = &full[44..];
            // Đúng mốc: bỏ pre-skip đúng → khớp sóng sin pha 0 (lệch 312 mẫu sẽ ra nhiễu lớn)
            let l = sine_snr(pcm, 0, 440.0, 0, 48_000, 48_000);
            let r = sine_snr(pcm, 1, 1000.0, 0, 48_000, 48_000);
            assert!(l > 20.0 && r > 20.0, "SNR trái {l:.1} dB, phải {r:.1} dB");
            // Đoạn giải sau khi tua (bộ giải làm nóng 80 ms) ≈ đoạn đó của bản giải tuần tự
            let f0 = (at - 44) / 4;
            let ml = sine_snr(&mid, 0, 440.0, f0, 0, 24_000 - 1);
            assert!(ml > 20.0, "SNR đoạn tua {ml:.1} dB");
            let diff = mid.chunks(2).zip(full[at..at + 96_000].chunks(2))
                .map(|(a, b)| (i16::from_le_bytes([a[0], a[1]]) as i32 - i16::from_le_bytes([b[0], b[1]]) as i32).unsigned_abs())
                .max()
                .unwrap();
            assert!(diff < 600, "lệch tối đa {diff}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn paced_by_listening_position() {
        // 40 khúc (10 MB), 100 giây → ~105 KB/s; nhịp nghe 60 giây ≈ 6,3 MB ≈ 24 khúc
        let size = (BLOCK * 40) as usize;
        let (st, fake, mut song, dir) = setup("pace", size, 0);
        song.dur = 100;
        let s2 = song.clone();
        let st2 = Streams::new(dir.join("music2"), fake.clone(), Arc::new(|_| {}), Box::new(move |k| (k == s2.key()).then(|| s2.clone())));
        drop(st);
        let st = st2;
        let url = st.url(&song).await.unwrap();
        st.set_pace(&song.key(), 0.0, true);
        let (_, body) = get(&url, Some("bytes=0-1")).await;
        assert_eq!(body, fake.data[..2]);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let fetched = fake.calls.load(Ordering::SeqCst);
        assert!((20..=27).contains(&fetched), "chỉ tải tới mép nhịp nghe, đã tải {fetched} khúc");
        // Đoạn ở 8 MB (ngoài nhịp, không phải cuối tệp) lúc đang dừng ở giây 0: chưa trả
        st.set_pace(&song.key(), 0.0, false);
        let far = BLOCK as usize * 32;
        let req = format!("bytes={far}-{}", far + 99);
        let u2 = url.clone();
        let r2 = req.clone();
        let pending = tokio::spawn(async move { get(&u2, Some(&r2)).await });
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(!pending.is_finished(), "chưa tới nhịp thì chưa trả");
        assert!(fake.calls.load(Ordering::SeqCst) <= 27);
        // Nghe tới giây 40 → nhịp tới ~10,5 MB → trả
        st.set_pace(&song.key(), 40.0, true);
        let (h, body) = tokio::time::timeout(Duration::from_secs(5), pending).await.expect("trả sau khi tới nhịp").unwrap();
        assert!(h.starts_with("HTTP/1.1 206"), "{h}");
        assert_eq!(body, fake.data[far..far + 100]);
        // Phần cuối tệp luôn được đọc (AVPlayer đọc thử lúc mở)
        let tail = size - 5000;
        let (_, body) = tokio::time::timeout(Duration::from_secs(5), get(&url, Some(&format!("bytes={tail}-")))).await.unwrap();
        assert_eq!(body, fake.data[tail..]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_file_decoded_without_fetch() {
        use sha2::Digest;
        // Tệp trên máy (local.rs): giải thẳng từ đĩa, không gọi tải khúc nào
        let (st, fake, _song, dir) = setup("local", BLOCK as usize, 0);
        let path = format!("{}/tests/data/noise44.flac", env!("CARGO_MANIFEST_DIR"));
        let song = crate::local::song(&path).unwrap();
        st.add_local(song.clone(), path.into());
        let url = st.url(&song).await.unwrap();
        assert!(url.ends_with(&format!("{}.wav", song.key())), "{url}");
        let (h, full) = get(&url, None).await;
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        let sha: String = sha2::Sha256::digest(&full[44..]).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(sha, "d21ec0481170a29313a66a0f5fc576861582da20d4fae8ac4f5a78fdeaed3946");
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ahead_setting_moves_limit() {
        let (st, _fake, song, dir) = setup("ahead", BLOCK as usize, 0);
        let key = song.key();
        let size = 100_000_000u64;
        st.set_pace(&key, 10.0, false);
        assert_eq!(st.limit(&key, size, 0, 100_000.0), 7_000_000); // mặc định 60 giây
        st.set_ahead(30);
        assert_eq!(st.limit(&key, size, 0, 100_000.0), 4_000_000);
        st.set_ahead(0); // cả bài
        assert_eq!(st.limit(&key, size, 0, 100_000.0), size);
        assert_eq!(st.limit("khac_1", size, 0, 100_000.0), HEAD_ALLOW); // bài chưa tới lượt: vẫn chỉ phần đầu
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_download_and_resume_from_map() {
        let size = (BLOCK * 3) as usize;
        let (st, fake, song, dir) = setup("full", size, 0);
        // Tải trước 1 khúc rồi «tắt app» (bỏ Streams) → .map giữ khúc đã có
        st.prefetch(&song, 1).unwrap();
        for _ in 0..200 {
            if st.cached_fraction(&song) > 0.0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
        drop(st);
        let fake2 =
            Arc::new(Fake { data: fake.data.clone(), calls: AtomicUsize::new(0), fail_first: AtomicUsize::new(0), down_from: net_up(), down_to: net_up() });
        let s2 = song.clone();
        let st = Streams::new(dir.join("music"), fake2.clone(), Arc::new(|_| {}), Box::new(move |k| (k == s2.key()).then(|| s2.clone())));
        assert!((st.cached_fraction(&song) - 1.0 / 3.0).abs() < 1e-9);
        st.set_full(&song, true).unwrap();
        for _ in 0..300 {
            if st.complete_path(&song).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(std::fs::read(st.complete_path(&song).unwrap()).unwrap(), fake.data);
        assert_eq!(fake2.calls.load(Ordering::SeqCst), 2); // chỉ tải 2 khúc còn thiếu
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// Mạng chập chờn giữa bài (lỗi tải tới tận luồng giải): cổng phát giữ kết nối, chờ rồi giải tiếp — trả đủ WAV, không cắt
    /// ngang (cắt thì AVPlayer phải mở lại đoạn đọc → mediaserverd chặn → iOS giết nó — M22).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transient_errors_keep_connection_open() {
        let (st, fake, song, dir) = setup_file("keep", "noise44_8s.flac", "flac", 8);
        let url = st.url(&song).await.unwrap();
        let size = 44 + 8 * 176_400usize;
        assert!(fake.data.len() as u64 > 5 * BLOCK, "tệp mẫu phải dài hơn 5 khúc");
        // Khúc đầu + cuối (mở WAV) tải được; các khúc giữa «mất mạng» 1,5 giây
        fake.down_to.store(4 * BLOCK, Ordering::SeqCst);
        fake.down_from.store(BLOCK, Ordering::SeqCst);
        // Mở WAV (chỉ cần khúc đầu + cuối; lượt tải khúc giữa hỏng song song có thể làm lần mở đầu hỏng — thử lại)
        for i in 0.. {
            match st.pcm(&song).await {
                Ok(_) => break,
                Err(e) if i < 50 => {
                    let _ = e;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => panic!("không mở được WAV: {e}"),
            }
        }
        let f2 = Arc::clone(&fake);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            f2.down_from.store(u64::MAX, Ordering::SeqCst); // mạng có lại
        });
        let t0 = Instant::now();
        let (h, full) = get(&url, None).await;
        assert!(h.starts_with("HTTP/1.1 200"), "{h} — sau {} ms", t0.elapsed().as_millis());
        assert_eq!(full.len(), size, "WAV bị cắt ngang sau {} ms", t0.elapsed().as_millis());
        assert_eq!(wav_fmt(&full), (2, 44_100, 16, 8 * 176_400));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ready_at`: chờ dữ liệu ở giữa bài (WAV giải từ FLAC, tệp gốc mp3); mạng mất thì trả «chưa» khi hết giờ chờ.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ready_at_waits_for_data_at_position() {
        let (st, _f, song, dir) = setup_file("ready", "noise44.flac", "flac", 4);
        st.set_pace(&song.key(), 3.0, false);
        assert!(st.ready_at(&song, 3.0, Duration::from_secs(10)).await.unwrap());
        // Đoạn ở giây 3 đã giải: trả ngay, đúng bằng bản giải cả bài
        let url = st.url(&song).await.unwrap();
        let a = 44 + 3 * 176_400;
        let (_, mid) = get(&url, Some(&format!("bytes={a}-{}", a + 9_999))).await;
        let (_, full) = get(&url, None).await;
        assert_eq!(mid, full[a..a + 10_000]);
        let _ = std::fs::remove_dir_all(&dir);

        // Tệp gốc: khúc quanh vị trí (ước theo bitrate) đã tải
        let size = (BLOCK * 40) as usize;
        let (st, _f, mut song, dir) = setup("ready-raw", size, 0);
        song.dur = 400;
        st.set_pace(&song.key(), 300.0, false);
        assert!(st.ready_at(&song, 300.0, Duration::from_secs(10)).await.unwrap());
        let b = (300.0 * size as f64 / 400.0) as u64 / BLOCK;
        let t = st.track(&song).unwrap();
        assert!(lock(&t.st).have[b as usize]);
        let _ = std::fs::remove_dir_all(&dir);
        // Mạng mất ngay từ đầu → «chưa» khi hết giờ chờ (không treo)
        let (st, fake, mut song, dir) = setup("ready-off", size, 0);
        fake.down_from.store(0, Ordering::SeqCst);
        fake.down_to.store(u64::MAX, Ordering::SeqCst);
        song.dur = 400;
        let t0 = Instant::now();
        assert!(!st.ready_at(&song, 100.0, Duration::from_secs(2)).await.unwrap());
        assert!(t0.elapsed() < Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// iOS thu hồi cổng nghe của app nằm nền lâu (giả: bỏ việc nhận → cổng đóng): `revive` mở lại đúng số cổng, URL cũ phát tiếp
    /// được; cổng còn sống thì không đụng (M23).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn revive_reopens_reclaimed_port() {
        let (st, fake, song, dir) = setup("revive", (BLOCK * 2) as usize, 0);
        assert!(st.revive().await.unwrap().is_none(), "chưa mở cổng thì không mở");
        let url = st.url(&song).await.unwrap();
        let port = st.listen().await.unwrap();
        assert_eq!(st.revive().await.unwrap(), Some((port, false)));
        // «Thu hồi»: bỏ việc nhận kết nối → cổng đóng, kết nối bị từ chối
        assert_eq!(st.drop_port(), Some(port));
        for _ in 0..50 {
            if !port_alive(port).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!port_alive(port).await, "cổng giả thu hồi vẫn nhận kết nối");
        assert_eq!(st.revive().await.unwrap(), Some((port, true)), "mở lại đúng số cổng cũ");
        let (h, body) = get(&url, Some("bytes=0-9")).await;
        assert!(h.starts_with("HTTP/1.1 206"), "{h}");
        assert_eq!(body, fake.data[..10]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
