//! Giải mã nhạc AVPlayer của iOS 5 / 6 không đọc được — **FLAC** (lossless, phần lớn nhạc trên các kênh lossless), **OGG
//! Vorbis**, **Opus** — thành **WAV 16 bit** để cổng phát nội bộ trả cho AVPlayer (DECISIONS M8, M13). FLAC / Vorbis giải bằng
//! symphonia (Rust thuần), Opus bằng libopus (`oggopus.rs`).
//!
//! - Tệp gốc đọc qua bộ đệm khúc của `stream.rs` (`TrackReader`: cần khúc nào thì chờ tải khúc đó) → nghe ngay khi mới tải vài
//!   trăm KB, bài nghe trọn thì tệp gốc nằm sẵn trên máy (nghe lại không cần mạng).
//! - WAV ghi vào `music/<khoá>.pcm.wav.part` (tệp thưa, đủ cỡ) + `.map` (khúc 256 KB nào đã giải) như tệp gốc; đủ thì đổi tên
//!   `<khoá>.pcm.wav`. Chỉ giữ 3 bài đã giải gần nhất (WAV to gấp ~1,7 lần FLAC).
//! - **Tua**: AVPlayer hỏi byte ở giữa → tính ra mẫu → symphonia tìm khung FLAC chứa mẫu đó (tìm nhị phân theo đầu khung, không
//!   cần SEEKTABLE), giải từ đó, bỏ các mẫu thừa ở đầu khung → đúng tới từng mẫu; chỉ tải phần tệp gốc quanh chỗ cần.
//! - Hi-res: 88,2 / 96 kHz → 44,1 / 48 kHz, 176,4 / 192 kHz → chia 4 (trung bình các mẫu); 24 / 32 bit → 16 bit; nhiều hơn 2 kênh →
//!   lấy 2 kênh đầu. Nhẹ cho A4 và chắc AVPlayer iOS 5 / 6 phát được.

use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::Notify;

use crate::library::Song;
use crate::stream::{FetchError, Streams, Track, BLOCK, IDLE};

/// Cỡ phần đầu WAV (RIFF + fmt + data).
pub const HEADER: u64 = 44;
/// Giữ tối đa chừng này bài đã giải trên đĩa.
const KEEP: usize = 3;

/// Đuôi cần giải mã thành WAV trước khi phát.
pub fn needs_decode(ext: &str) -> bool {
    matches!(ext, "flac" | "ogg" | "oga" | "opus")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Đọc tệp gốc qua bộ đệm khúc của bài (chạy ở luồng blocking: chờ khúc bằng `block_on`).
pub struct TrackReader {
    streams: Arc<Streams>,
    track: Arc<Track>,
    rt: tokio::runtime::Handle,
    pos: u64,
    size: u64,
}

impl TrackReader {
    pub fn new(streams: Arc<Streams>, track: Arc<Track>, rt: tokio::runtime::Handle) -> TrackReader {
        let size = track.song.size;
        TrackReader { streams, track, rt, pos: 0, size }
    }
}

impl Read for TrackReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let b = (self.pos / BLOCK) as u32;
        let (s, t) = (Arc::clone(&self.streams), Arc::clone(&self.track));
        self.rt
            .block_on(async move { s.wait_block(&t, b).await })
            .map_err(|e: FetchError| io::Error::other(format!("{}: {}", e.code, e.msg)))?;
        let end = ((b as u64 + 1) * BLOCK).min(self.size);
        let n = ((end - self.pos) as usize).min(buf.len());
        let data = self.streams.read_range(&self.track, self.pos, n)?;
        buf[..n].copy_from_slice(&data);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for TrackReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(x) => x as i64,
            SeekFrom::End(x) => self.size as i64 + x,
            SeekFrom::Current(x) => self.pos as i64 + x,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "tua trước đầu tệp"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

impl MediaSource for TrackReader {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.size)
    }
}

/// Thông số giải: tần số vào, hệ số chia (hi-res), số kênh vào / ra, số khung ra.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub rate_in: u32,
    pub k: u32,
    pub ch_in: usize,
    pub ch_out: usize,
    pub frames_out: u64,
}

impl Params {
    fn rate_out(&self) -> u32 {
        self.rate_in / self.k
    }

    fn bpf(&self) -> u64 {
        self.ch_out as u64 * 2
    }

    pub fn wav_size(&self) -> u64 {
        HEADER + self.frames_out * self.bpf()
    }
}

/// Hệ số chia tần số: ≤ 48 kHz giữ nguyên; 88,2 / 96 kHz chia 2; 176,4 / 192 kHz (và hơn) chia 4.
fn factor(rate: u32) -> u32 {
    match rate {
        r if r > 96_000 && r % 4 == 0 && r / 4 >= 44_100 => 4,
        r if r > 48_000 && r % 2 == 0 && r / 2 >= 44_100 => 2,
        _ => 1,
    }
}

/// Nguồn mẫu PCM của tệp gốc (symphonia hoặc Opus).
trait Src {
    /// Tua tới mẫu vào `ts`: gói kế tiếp bắt đầu ở / trước `ts` (hàm gọi bỏ phần thừa theo mốc của từng gói).
    fn seek(&mut self, ts: u64) -> Result<(), String>;
    /// Gói kế tiếp → mẫu xen kẽ (`ch_in` kênh) vào `out`; trả mốc (mẫu vào) của mẫu đầu — có thể âm (Opus pre-skip). None = hết tệp.
    fn next(&mut self, out: &mut Vec<i16>) -> Result<Option<i64>, String>;
}

struct SymSrc {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    tid: u32,
    sbuf: Option<SampleBuffer<i16>>,
}

impl Src for SymSrc {
    fn seek(&mut self, ts: u64) -> Result<(), String> {
        let r = self
            .format
            .seek(SeekMode::Accurate, SeekTo::TimeStamp { ts, track_id: self.tid })
            .map_err(|e| format!("tua lỗi: {e}"))?;
        self.decoder.reset();
        crate::dlog(&format!("giải mã: tua tới mẫu {} (khung bắt đầu ở {})", r.required_ts, r.actual_ts));
        Ok(())
    }

    fn next(&mut self, out: &mut Vec<i16>) -> Result<Option<i64>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(pk) => pk,
                Err(SymError::IoError(e)) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(format!("đọc tệp lỗi: {e}")),
            };
            if packet.track_id() != self.tid {
                continue;
            }
            let ts = packet.ts();
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(_)) => continue, // khung hỏng: bỏ qua
                Err(e) => return Err(format!("giải mã lỗi: {e}")),
            };
            let need = decoded.capacity() * decoded.spec().channels.count();
            if self.sbuf.as_ref().is_none_or(|b| b.capacity() < need) {
                self.sbuf = Some(SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec()));
            }
            let sb = self.sbuf.as_mut().expect("có bộ đệm");
            sb.copy_interleaved_ref(decoded);
            out.clear();
            out.extend_from_slice(sb.samples());
            return Ok(Some(ts as i64));
        }
    }
}

impl Src for crate::oggopus::OpusSrc {
    fn seek(&mut self, ts: u64) -> Result<(), String> {
        crate::oggopus::OpusSrc::seek(self, ts)
    }

    fn next(&mut self, out: &mut Vec<i16>) -> Result<Option<i64>, String> {
        crate::oggopus::OpusSrc::next(self, out)
    }
}

/// Tệp Ogg chứa Opus? (đọc trang đầu, trả con trỏ về đầu tệp)
fn is_ogg_opus(src: &mut dyn MediaSource) -> bool {
    let mut head = [0u8; 128];
    let mut n = 0;
    while n < head.len() {
        match src.read(&mut head[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    let _ = src.seek(SeekFrom::Start(0));
    head[..n].windows(8).any(|w| w == b"OpusHead")
}

/// Mở tệp gốc: đọc phần đầu (STREAMINFO, OpusHead…), tạo bộ giải mã. `dur` (giây) dùng khi tệp không ghi tổng số mẫu.
fn open(mut src: Box<dyn MediaSource>, ext: &str, dur: u32) -> Result<(Box<dyn Src>, Params), String> {
    if ext == "opus" || (matches!(ext, "ogg" | "oga") && is_ogg_opus(&mut *src)) {
        let o = crate::oggopus::OpusSrc::open(src)?;
        let frames = if o.total > 0 { o.total } else { dur as u64 * 48_000 };
        let p = Params { rate_in: 48_000, k: 1, ch_in: o.channels, ch_out: o.channels.min(2), frames_out: frames };
        return Ok((Box::new(o), p));
    }
    let mss = MediaSourceStream::new(src, MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("không đọc được tệp {}: {e}", ext.to_uppercase()))?;
    let format = probed.format;
    let track = format.default_track().ok_or("tệp không có luồng âm thanh")?;
    let cp = track.codec_params.clone();
    let tid = track.id;
    let decoder = symphonia::default::get_codecs()
        .make(&cp, &DecoderOptions::default())
        .map_err(|e| format!("không giải được định dạng này: {e}"))?;
    let rate_in = cp.sample_rate.ok_or("tệp không ghi tần số lấy mẫu")?;
    let ch_in = cp.channels.map(|c| c.count()).unwrap_or(2).max(1);
    let k = factor(rate_in);
    let frames_in = cp.n_frames.unwrap_or(dur as u64 * rate_in as u64);
    let p = Params { rate_in, k, ch_in, ch_out: ch_in.min(2), frames_out: frames_in.div_ceil(k as u64) };
    Ok((Box::new(SymSrc { format, decoder, tid, sbuf: None }), p))
}

/// Phần đầu WAV PCM 16 bit.
pub fn wav_header(p: &Params) -> [u8; 44] {
    let mut h = [0u8; 44];
    let data = (p.frames_out * p.bpf()).min(u32::MAX as u64 - 36) as u32;
    let rate = p.rate_out();
    let ba = p.ch_out as u16 * 2;
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&(p.ch_out as u16).to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * ba as u32).to_le_bytes());
    h[32..34].copy_from_slice(&ba.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

struct PState {
    have: Vec<bool>,
    /// Khúc WAV trình phát đang cần.
    want: u32,
    readers: u32,
    last_read: Instant,
    running: bool,
    err: Option<String>,
    done: bool,
    /// Đang có bài khác chờ giải (tự dọn bản đã giải cũ).
    used: Instant,
}

enum Target {
    Block(u32),
    /// Chưa tới nhịp nghe — chờ.
    Wait,
    /// Đủ, hoặc không còn ai nghe.
    Stop,
}

/// WAV của một bài đang / đã giải.
pub struct Pcm {
    pub song: Song,
    pub size: u64,
    p: Params,
    n: u32,
    part: PathBuf,
    map: PathBuf,
    fin: PathBuf,
    st: Mutex<PState>,
    notify: Notify,
}

impl Pcm {
    pub fn final_path(dir: &Path, song: &Song) -> PathBuf {
        dir.join(format!("{}.pcm.wav", song.key()))
    }

    /// Mở (blocking): đã giải đủ thì dùng ngay; không thì đọc phần đầu tệp gốc (tải khúc đầu nếu chưa có) để biết cỡ WAV.
    pub fn open(dir: &Path, song: Song, reader: Box<dyn MediaSource>) -> io::Result<Pcm> {
        let fin = Self::final_path(dir, &song);
        let part = dir.join(format!("{}.pcm.wav.part", song.key()));
        let map = dir.join(format!("{}.pcm.wav.map", song.key()));
        let now = Instant::now();
        let state = |have: Vec<bool>, done: bool| PState {
            have,
            want: 0,
            readers: 0,
            last_read: now - IDLE * 2,
            running: false,
            err: None,
            done,
            used: now,
        };
        if let Ok(md) = std::fs::metadata(&fin) {
            let size = md.len();
            let n = size.div_ceil(BLOCK).max(1) as u32;
            let p = Params { rate_in: 0, k: 1, ch_in: 0, ch_out: 0, frames_out: 0 };
            crate::cache::touch(&fin);
            return Ok(Pcm { song, size, p, n, part, map, fin, st: Mutex::new(state(vec![true; n as usize], true)), notify: Notify::new() });
        }
        let (_, p) = open(reader, &song.ext, song.dur).map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?; // tệp trên máy: chưa có khúc tải nào tạo thư mục
        let size = p.wav_size();
        let n = size.div_ceil(BLOCK).max(1) as u32;
        crate::dlog(&format!(
            "giải mã: mở {} — {} Hz / {} kênh, chia {}, {} khung, WAV {} KB",
            song.key(),
            p.rate_in,
            p.ch_in,
            p.k,
            p.frames_out,
            size >> 10
        ));
        let mut have = vec![false; n as usize];
        let ok = std::fs::metadata(&part).map(|m| m.len() == size).unwrap_or(false);
        match std::fs::read(&map) {
            Ok(m) if ok && m.len() == n as usize => {
                for (h, b) in have.iter_mut().zip(m) {
                    *h = b == 1;
                }
            }
            _ => {
                let f = std::fs::File::create(&part)?;
                f.set_len(size)?;
                crate::stream::write_all_at(&f, &wav_header(&p), 0)?;
                std::fs::write(&map, vec![0u8; n as usize])?;
            }
        }
        Ok(Pcm { song, size, p, n, part, map, fin, st: Mutex::new(state(have, false)), notify: Notify::new() })
    }

    pub fn key(&self) -> String {
        self.song.key()
    }

    /// Byte WAV mỗi giây (nhịp nghe).
    pub fn bytes_per_sec(&self) -> f64 {
        if self.p.ch_out == 0 {
            // bản đã giải đủ (mở từ tệp có sẵn): ước theo cỡ / thời lượng
            return if self.song.dur > 0 { self.size as f64 / self.song.dur as f64 } else { 176_400.0 };
        }
        (self.p.rate_out() as u64 * self.p.bpf()) as f64
    }

    pub fn is_done(&self) -> bool {
        lock(&self.st).done
    }

    fn active(st: &PState) -> bool {
        st.readers > 0 || st.last_read.elapsed() < IDLE
    }

    pub fn busy(&self) -> bool {
        let st = lock(&self.st);
        st.running || Self::active(&st)
    }

    pub fn last_used(&self) -> Instant {
        lock(&self.st).used
    }

    /// Chờ khúc WAV `b` (đặt vị trí cần cho luồng giải; `kick` chạy luồng giải nếu đang nghỉ).
    pub async fn wait_block(&self, b: u32, kick: &(dyn Fn() + Send + Sync)) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut st = lock(&self.st);
                if st.have[b as usize] {
                    return Ok(());
                }
                if let Some(e) = st.err.take() {
                    if !st.running {
                        return Err(e);
                    }
                }
                st.want = b;
                st.last_read = Instant::now();
                st.used = Instant::now();
            }
            kick();
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("Giải mã quá lâu.".into());
            }
            let _ = tokio::time::timeout(left.min(Duration::from_secs(1)), notified).await;
        }
    }

    pub fn read_range(&self, from: u64, len: usize) -> io::Result<Vec<u8>> {
        let f = match std::fs::File::open(&self.part) {
            Ok(f) if !lock(&self.st).done => f,
            _ => std::fs::File::open(&self.fin).or_else(|_| std::fs::File::open(&self.part))?,
        };
        let mut buf = vec![0u8; len];
        let mut got = 0;
        while got < len {
            let n = crate::stream::read_at(&f, &mut buf[got..], from + got as u64)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "tệp WAV ngắn hơn dự kiến"));
            }
            got += n;
        }
        Ok(buf)
    }

    pub fn reader_enter(&self) {
        lock(&self.st).readers += 1;
    }

    pub fn reader_leave(&self) {
        let mut st = lock(&self.st);
        st.readers = st.readers.saturating_sub(1);
        st.last_read = Instant::now();
    }

    pub fn touch_read(&self, next_block: u32) {
        let mut st = lock(&self.st);
        st.last_read = Instant::now();
        st.want = next_block.min(self.n - 1);
    }

    /// Giành quyền chạy luồng giải (false nếu đang chạy / đã xong).
    pub fn begin_run(&self) -> bool {
        let mut st = lock(&self.st);
        if st.running || st.done {
            return false;
        }
        st.running = true;
        true
    }

    /// Khúc WAV nên giải tiếp: khúc trình phát đang chờ, rồi các khúc sau nó trong nhịp nghe (`lim` khúc), rồi phần đầu bài.
    fn next_target(&self, lim: u32) -> Target {
        let st = lock(&self.st);
        if st.done || !Self::active(&st) {
            return Target::Stop;
        }
        let w = st.want.min(self.n - 1);
        if !st.have[w as usize] {
            return Target::Block(w);
        }
        match (w..lim.min(self.n)).chain(0..w).find(|&b| !st.have[b as usize]) {
            Some(b) => Target::Block(b),
            None => Target::Wait,
        }
    }

    fn block_done(&self, b: u32) -> bool {
        lock(&self.st).have.get(b as usize).copied().unwrap_or(true)
    }

    /// Đánh dấu các khúc nằm trọn trong [from, to) đã giải; ghi `.map`; đủ thì đổi tên.
    fn mark(&self, from: u64, to: u64) -> io::Result<()> {
        let first = if from == 0 { 0 } else { from.div_ceil(BLOCK) };
        let last_full = if to >= self.size { self.n as u64 } else { to / BLOCK };
        if first >= last_full {
            return Ok(());
        }
        let all = {
            let mut st = lock(&self.st);
            let mut changed = false;
            for b in first..last_full {
                if !st.have[b as usize] {
                    st.have[b as usize] = true;
                    changed = true;
                }
            }
            if !changed {
                return Ok(());
            }
            let m: Vec<u8> = st.have.iter().map(|h| *h as u8).collect();
            std::fs::write(&self.map, m)?;
            st.have.iter().all(|h| *h)
        };
        if all {
            std::fs::rename(&self.part, &self.fin)?;
            let _ = std::fs::remove_file(&self.map);
            lock(&self.st).done = true;
        }
        self.notify.notify_waiters();
        Ok(())
    }

    fn fail(&self, e: String) {
        lock(&self.st).err = Some(e);
        self.notify.notify_waiters();
    }

    /// Luồng giải (blocking): mở tệp gốc qua `reader`, giải các khúc còn thiếu theo thứ tự trình phát cần, tới khi đủ hoặc không
    /// còn ai nghe. Gọi sau `begin_run`.
    pub fn run(&self, reader: Box<dyn MediaSource>, limit: &dyn Fn() -> u64) {
        let t = Instant::now();
        let r = self.run_inner(reader, limit);
        let have = lock(&self.st).have.iter().filter(|h| **h).count();
        match &r {
            Ok(()) => crate::dlog(&format!("giải mã: {} nghỉ sau {} ms, có {have}/{} khúc", self.key(), t.elapsed().as_millis(), self.n)),
            Err(e) => crate::dlog(&format!("giải mã: {} lỗi sau {} ms: {e}", self.key(), t.elapsed().as_millis())),
        }
        if let Err(e) = r {
            self.fail(e);
        }
        lock(&self.st).running = false;
        self.notify.notify_waiters();
    }

    fn run_inner(&self, reader: Box<dyn MediaSource>, limit: &dyn Fn() -> u64) -> Result<(), String> {
        let (mut src, p) = open(reader, &self.song.ext, self.song.dur)?;
        if p != self.p {
            return Err("tệp gốc đã đổi — mở lại bài".into());
        }
        let file = std::fs::OpenOptions::new().write(true).open(&self.part).map_err(|e| e.to_string())?;
        let bpf = p.bpf();
        let k = p.k as u64;
        let ch = p.ch_in;
        // Vị trí khung ra mà gói kế tiếp sẽ cho (giải tuần tự không cần tua lại); None = phải tua
        let mut cursor: Option<u64> = Some(0);
        let mut pcm: Vec<i16> = Vec::new();
        let mut out: Vec<i16> = Vec::new();
        loop {
            let lim = (limit().div_ceil(BLOCK) as u32).max(1);
            let target = match self.next_target(lim) {
                Target::Block(b) => b,
                Target::Stop => return Ok(()),
                Target::Wait => {
                    // Đã giải đủ tới nhịp nghe (tạm dừng / còn nhiều phía trước) — chờ nhịp đi tiếp
                    std::thread::sleep(Duration::from_millis(400));
                    continue;
                }
            };
            let start_byte = target as u64 * BLOCK;
            let start = if start_byte <= HEADER { 0 } else { (start_byte - HEADER) / bpf };
            // Bỏ các mẫu vào trước `skip_to` (gói chứa điểm tua bắt đầu sớm hơn; Opus: pre-skip, đoạn làm nóng)
            let skip_to = (start * k) as i64;
            if cursor != Some(start) {
                src.seek(start * k)?;
                crate::dlog(&format!("giải mã: tua tới khúc {target}"));
            }
            let run_from = if start == 0 { 0 } else { HEADER + start * bpf };
            let mut pos = HEADER + start * bpf; // byte WAV kế tiếp sẽ ghi
            let mut frame = start; // khung ra kế tiếp
            // Gom mẫu để chia tần số (hi-res): tổng từng kênh + số mẫu đã gom
            let mut acc = [0i32; 2];
            let mut accn = 0u64;
            'run: loop {
                let ts = match src.next(&mut pcm)? {
                    Some(ts) => ts,
                    None => {
                        // Hết tệp: tệp ghi thiếu tổng số mẫu / ít mẫu hơn → phần còn lại là im lặng (tệp đã tạo đủ cỡ, toàn 0)
                        self.mark(run_from, self.size).map_err(|e| e.to_string())?;
                        cursor = None;
                        break 'run;
                    }
                };
                let frames = pcm.len() / ch;
                let i0 = if ts < skip_to { ((skip_to - ts) as u64).min(frames as u64) as usize } else { 0 };
                out.clear();
                for i in i0..frames {
                    let l = pcm[i * ch] as i32;
                    let r = if ch > 1 { pcm[i * ch + 1] as i32 } else { l };
                    acc[0] += l;
                    acc[1] += r;
                    accn += 1;
                    if accn == k {
                        out.push((acc[0] / k as i32) as i16);
                        if p.ch_out > 1 {
                            out.push((acc[1] / k as i32) as i16);
                        }
                        acc = [0, 0];
                        accn = 0;
                    }
                }
                // Không ghi quá số khung đã báo trong phần đầu WAV
                let room = p.frames_out.saturating_sub(frame) as usize * p.ch_out;
                out.truncate(room);
                if !out.is_empty() {
                    let bytes: Vec<u8> = out.iter().flat_map(|v| v.to_le_bytes()).collect();
                    crate::stream::write_all_at(&file, &bytes, pos).map_err(|e| e.to_string())?;
                    pos += bytes.len() as u64;
                    frame += (out.len() / p.ch_out) as u64;
                }
                let full = frame >= p.frames_out;
                self.mark(run_from, if full { self.size } else { pos }).map_err(|e| e.to_string())?;
                cursor = Some(frame);
                if full {
                    cursor = None;
                    break 'run;
                }
                // Dừng lượt: khúc đang ghi đã có từ lượt trước (giải tới đây là nối), trình phát nhảy chỗ khác, hoặc không
                // còn ai nghe → chọn lại khúc cần giải
                let cur_block = (pos / BLOCK) as u32;
                let (want, active) = {
                    let st = lock(&self.st);
                    (st.want, Self::active(&st))
                };
                if !active {
                    return Ok(());
                }
                if !self.block_done(want) && (want < cur_block || want > cur_block + 2) {
                    break 'run;
                }
                if cur_block < self.n && self.block_done(cur_block) {
                    break 'run;
                }
                // Nhịp nghe: giải trước tới mép thì đứng chờ tại chỗ (giữ trạng thái — khỏi tua lại); trình phát nhảy chỗ khác hoặc
                // thôi nghe thì ra chọn lại
                while pos >= limit() && pos + crate::stream::TAIL_ALLOW < self.size {
                    let (want, active) = {
                        let st = lock(&self.st);
                        (st.want, Self::active(&st))
                    };
                    if !active {
                        return Ok(());
                    }
                    if !self.block_done(want) && (want < cur_block || want > cur_block + 2) {
                        break 'run;
                    }
                    std::thread::sleep(Duration::from_millis(400));
                }
            }
        }
    }
}

/// Dọn bản đã giải cũ: giữ `KEEP` bản gần nhất (và mọi bản đang dùng). Trả các khoá đã xoá.
pub fn prune(dir: &Path, keep_keys: &HashSet<String>) -> Vec<String> {
    let mut files: Vec<(std::time::SystemTime, String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.ends_with(".pcm.wav") || name.ends_with(".pcm.wav.part")) {
                continue;
            }
            let key = name.split('.').next().unwrap_or("").to_string();
            let t = std::fs::metadata(e.path()).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            files.push((t, key, e.path()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let mut kept = 0;
    let mut gone = Vec::new();
    for (_, key, path) in files {
        if keep_keys.contains(&key) {
            continue;
        }
        if kept < KEEP {
            kept += 1;
            continue;
        }
        let _ = std::fs::remove_file(&path);
        if path.extension().is_some_and(|x| x == "part") {
            let _ = std::fs::remove_file(path.with_extension("map"));
        }
        gone.push(key);
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factors_and_header() {
        assert_eq!(factor(44_100), 1);
        assert_eq!(factor(48_000), 1);
        assert_eq!(factor(88_200), 2);
        assert_eq!(factor(96_000), 2);
        assert_eq!(factor(176_400), 4);
        assert_eq!(factor(192_000), 4);
        assert_eq!(factor(64_000), 1);
        let p = Params { rate_in: 96_000, k: 2, ch_in: 2, ch_out: 2, frames_out: 1000 };
        let h = wav_header(&p);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 48_000);
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 4000);
        assert_eq!(p.wav_size(), 4044);
        assert!(needs_decode("flac") && !needs_decode("mp3"));
    }
}
