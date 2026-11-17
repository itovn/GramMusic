//! Bộ nhớ đệm trên đĩa (trong thư mục dữ liệu): `music/` (bài đã nghe — đủ hoặc dở), `covers/` (ảnh bìa), `avatars/` (ảnh
//! nguồn nhạc), `tmp/`. Chép từ GramMini (`gm-core/src/cache.rs`), đổi cho nhạc:
//!
//! - Bài **đã tải về máy** (người dùng bấm «Tải về» — danh sách ở `offline.json`) không bao giờ tự xoá; chỉ xoá khi người dùng
//!   gỡ hoặc «Xoá bài đã tải».
//! - Bài nghe qua (luồng phát ghi xuống đĩa: `<khoá>.<đuôi>` đủ, hoặc `<khoá>.<đuôi>.part` + `.map` dở dang) là đệm thường:
//!   quá hạn giữ thì xoá, tổng vượt giới hạn thì xoá bài lâu không nghe nhất trước, máy sắp đầy thì xoá tiếp.
//! - «Lần dùng cuối» = thời điểm sửa của tệp ([`touch`] mỗi lần phát lại). Tệp vừa dùng / đang ghi trong 10 phút được giữ.
//!
//! Phiên, cài đặt, chỉ mục thư viện, playlist, nhật ký nằm ngoài các thư mục trên → không bao giờ bị xoá.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const DIRS: [&str; 4] = ["music", "covers", "avatars", "tmp"];
/// Tệp vừa dùng / đang ghi trong khoảng này không tự xoá (đang nghe, đang tải).
const PROTECT: Duration = Duration::from_secs(10 * 60);
/// Tệp tạm quá chừng này thì xoá.
const TEMP_AGE: Duration = Duration::from_secs(3600);
/// Máy còn ít hơn chừng này thì dọn đệm…
const LOW_FREE: u64 = 300 << 20;
/// …tới khi còn chừng này.
const FREE_TARGET: u64 = 500 << 20;
/// Hai lượt tự dọn cách nhau ít nhất chừng này (mỗi lượt quét thư mục).
const TRIM_GAP: Duration = Duration::from_secs(60);

/// Các loại (thứ tự hiện trên màn «Dữ liệu và bộ nhớ»): bài đã tải về máy, bài nghe qua (đệm), ảnh bìa, ảnh nguồn, khác.
pub const KINDS: [&str; 5] = ["offline", "music", "cover", "avatar", "other"];

/// Chính sách tự dọn (không áp cho bài đã tải về máy).
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Giới hạn tổng dung lượng đệm, MB (0 = không giới hạn).
    pub limit_mb: u64,
    /// Hạn giữ tệp không dùng lại, ngày (0 = mãi mãi).
    pub keep_days: u64,
}

impl Default for Policy {
    fn default() -> Self {
        // Máy 16 GB đời cũ: 1 GB đệm ≈ 200 bài mp3 320 kbit/s
        Policy { limit_mb: 1024, keep_days: 30 }
    }
}

/// Một tệp trong bộ nhớ đệm.
pub struct Entry {
    pub path: PathBuf,
    pub size: u64,
    /// Lần dùng cuối (thời điểm sửa).
    pub used: SystemTime,
    pub kind: &'static str,
    /// Khoá bài (`<peer>_<id>`) với tệp nhạc / ảnh bìa; khoá peer với ảnh nguồn.
    pub key: Option<String>,
    /// Khoá peer của nguồn chứa bài (thống kê theo nguồn).
    pub peer: Option<String>,
    /// Tệp tạm (tệp trong `tmp/`, tệp `.<số>.part` của lần tải ảnh).
    pub temp: bool,
}

/// Khoá bài từ tên tệp nhạc / ảnh bìa: `-100123_45.mp3` → `-100123_45`.
pub fn key_of(name: &str) -> Option<String> {
    let stem = name.split('.').next()?;
    let (peer, id) = stem.split_once('_')?;
    (peer.parse::<i64>().is_ok() && id.parse::<i64>().is_ok()).then(|| stem.to_string())
}

/// Loại + khoá theo thư mục và tên tệp. `offline`: khoá các bài đã tải về máy.
fn classify(dir: &str, name: &str, offline: &HashSet<String>) -> (&'static str, Option<String>, bool) {
    match dir {
        "music" => {
            let key = key_of(name);
            let kind = if key.as_ref().is_some_and(|k| offline.contains(k)) { "offline" } else { "music" };
            (kind, key, false)
        }
        "covers" => ("cover", key_of(name), name.ends_with(".part")),
        "avatars" => ("avatar", name.split_once('_').map(|(p, _)| p.to_string()), name.ends_with(".part")),
        _ => ("other", None, true),
    }
}

/// Quét toàn bộ tệp đệm. Tệp `.map` (bảng khúc đã tải của bài dở — vài trăm byte) đi kèm `.part`, không tính riêng.
pub fn scan(data_dir: &Path, offline: &HashSet<String>) -> Vec<Entry> {
    let mut v = Vec::new();
    for dir in DIRS {
        let Ok(rd) = std::fs::read_dir(data_dir.join(dir)) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".map") {
                // .map mồ côi (tệp .part đã mất) → bỏ luôn
                let part = e.path().with_extension("part");
                if !part.exists() {
                    let _ = std::fs::remove_file(e.path());
                }
                continue;
            }
            // std::fs::metadata (stat), không dùng DirEntry::metadata: std gọi fstatat — chỉ có từ iOS 8 (GramMini D15)
            let Ok(md) = std::fs::metadata(e.path()) else { continue };
            if !md.is_file() {
                continue;
            }
            let (kind, key, temp) = classify(dir, &name, offline);
            let peer = key.as_deref().and_then(|k| k.split_once('_')).map(|(p, _)| p.to_string()).or_else(|| key.clone());
            let used = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            v.push(Entry { path: e.path(), size: md.len(), used, kind, key, peer, temp });
        }
    }
    v
}

/// Xoá một tệp đệm (tệp `.part` thì xoá cả `.map` đi kèm). Trả có xoá được không.
fn remove(e: &Entry) -> bool {
    let ok = std::fs::remove_file(&e.path).is_ok();
    if ok && e.path.extension().is_some_and(|x| x == "part") {
        let _ = std::fs::remove_file(e.path.with_extension("map"));
    }
    ok
}

/// Tệp của bài đang dùng? Theo khoá bài, hoặc theo tên tệp — bản giải của tệp trên máy (`local_<băm>.pcm.wav`) không có khoá
/// `<peer>_<id>` nên trước đây «Xoá đệm» xoá luôn tệp đang phát.
fn in_use(e: &Entry, busy: &HashSet<String>) -> bool {
    e.key.as_ref().is_some_and(|k| busy.contains(k))
        || e.path.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('.').next()).is_some_and(|s| busy.contains(s))
}

fn age(now: SystemTime, e: &Entry) -> Duration {
    now.duration_since(e.used).unwrap_or_default()
}

/// Xoá tệp đệm khớp điều kiện (`kinds` rỗng / None = mọi loại **trừ** bài đã tải về máy; muốn xoá bài đã tải thì ghi rõ
/// «offline»), `peer` = chỉ nguồn đó, `keys` = chỉ các bài đó. Bỏ qua tệp đang dùng (`busy`) và tệp tạm vừa ghi chưa tới
/// 2 phút. Trả (byte đã xoá, số tệp).
pub fn clear(
    data_dir: &Path,
    offline: &HashSet<String>,
    kinds: Option<&[String]>,
    peer: Option<&str>,
    keys: Option<&HashSet<String>>,
    busy: &HashSet<String>,
) -> (u64, usize) {
    let now = SystemTime::now();
    let (mut freed, mut n) = (0, 0);
    for e in scan(data_dir, offline) {
        match kinds.filter(|k| !k.is_empty()) {
            Some(k) if !k.iter().any(|x| x == e.kind) => continue,
            None if e.kind == "offline" => continue,
            _ => {}
        }
        if peer.is_some() && e.peer.as_deref() != peer {
            continue;
        }
        if let Some(ks) = keys {
            if !e.key.as_ref().is_some_and(|k| ks.contains(k)) {
                continue;
            }
        }
        if in_use(&e, busy) || (e.temp && age(now, &e) < Duration::from_secs(120)) {
            continue;
        }
        if remove(&e) {
            freed += e.size;
            n += 1;
        }
    }
    (freed, n)
}

/// Kết quả một lượt tự dọn.
#[derive(Debug, Default)]
pub struct Trimmed {
    pub freed: u64,
    pub files: usize,
    /// Tổng đệm còn lại (không tính bài đã tải về máy).
    pub total: u64,
}

/// Tự dọn theo chính sách (xem đầu tệp). Chạy ở luồng blocking. Bài đã tải về máy (`offline`) và bài đang phát / tải
/// (`busy`) không đụng tới.
pub fn trim(data_dir: &Path, p: Policy, offline: &HashSet<String>, busy: &HashSet<String>) -> Trimmed {
    let now = SystemTime::now();
    let keep = (p.keep_days > 0).then(|| Duration::from_secs(p.keep_days * 86400));
    let mut r = Trimmed::default();
    let mut rest = Vec::new();
    for e in scan(data_dir, offline) {
        if e.kind == "offline" || in_use(&e, busy) {
            continue;
        }
        let a = age(now, &e);
        let stale = (e.temp && a > TEMP_AGE) || (keep.is_some_and(|k| a > k) && a > PROTECT);
        if stale && remove(&e) {
            r.freed += e.size;
            r.files += 1;
        } else {
            rest.push(e);
        }
    }
    r.total = rest.iter().map(|e| e.size).sum();
    let limit = p.limit_mb << 20;
    let mut target = if limit > 0 && r.total > limit { limit / 10 * 9 } else { r.total };
    if let Some((free, _)) = disk_space(data_dir) {
        if free < LOW_FREE {
            target = target.min(r.total.saturating_sub(FREE_TARGET - free));
        }
    }
    if r.total > target {
        rest.sort_by_key(|e| e.used);
        for e in &rest {
            if r.total <= target {
                break;
            }
            if age(now, e) < PROTECT {
                continue;
            }
            if remove(e) {
                r.total -= e.size;
                r.freed += e.size;
                r.files += 1;
            }
        }
    }
    r
}

/// Đánh dấu vừa dùng (đổi thời điểm sửa thành bây giờ) — chỉ khi lần đánh dấu trước đã quá 1 giờ, đỡ ghi đĩa.
pub fn touch(path: &Path) {
    let Ok(md) = std::fs::metadata(path) else { return };
    let old = md
        .modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_none_or(|a| a > Duration::from_secs(3600));
    if !old {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        if let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) {
            // utimes(…, NULL) = bây giờ; có từ thuở đầu iOS (File::set_modified của std dùng fsetattrlist)
            unsafe { libc::utimes(c.as_ptr(), std::ptr::null()) };
        }
    }
    #[cfg(not(unix))]
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// (còn trống, tổng) của phân vùng chứa `dir`, byte.
pub fn disk_space(dir: &Path) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        let bs = st.f_frsize as u64;
        Some((st.f_bavail as u64 * bs, st.f_blocks as u64 * bs))
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        None
    }
}

/// Trạng thái tự dọn của lõi: chính sách hiện hành, đang dọn hay không, lần dọn trước.
pub struct Cache {
    policy: Mutex<Policy>,
    busy: AtomicBool,
    last: Mutex<Option<Instant>>,
}

impl Cache {
    pub fn new(policy: Policy) -> Cache {
        Cache { policy: Mutex::new(policy), busy: AtomicBool::new(false), last: Mutex::new(None) }
    }

    pub fn policy(&self) -> Policy {
        *self.policy.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_policy(&self, p: Policy) {
        *self.policy.lock().unwrap_or_else(|e| e.into_inner()) = p;
    }

    /// Có tệp đã tải ở `path` không (có thì đánh dấu vừa dùng).
    pub fn hit(&self, path: &Path) -> bool {
        if path.exists() {
            touch(path);
            true
        } else {
            false
        }
    }

    /// Giành lượt dọn: false nếu đang dọn, hoặc (khi không `force`) vừa dọn chưa tới 1 phút.
    pub fn begin(&self, force: bool) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if !force && last.is_some_and(|t| t.elapsed() < TRIM_GAP) {
            return false;
        }
        if self.busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    pub fn end(&self) {
        self.busy.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(dir: &Path, sub: &str, name: &str, size: usize, age_secs: u64) -> PathBuf {
        let d = dir.join(sub);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, vec![0u8; size]).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(age_secs)).unwrap();
        p
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gmu-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn classify_names() {
        let off = set(&["-1001_7"]);
        assert_eq!(classify("music", "-1001_7.mp3", &off), ("offline", Some("-1001_7".into()), false));
        assert_eq!(classify("music", "-1001_8.m4a.part", &off), ("music", Some("-1001_8".into()), false));
        assert_eq!(classify("covers", "5_9.jpg", &off), ("cover", Some("5_9".into()), false));
        assert_eq!(classify("avatars", "42_777.jpg", &off), ("avatar", Some("42".into()), false));
        assert_eq!(classify("tmp", "x.bin", &off), ("other", None, true));
        assert_eq!(key_of("abc.mp3"), None);
    }

    #[test]
    fn trim_keeps_offline_and_busy() {
        let d = tmpdir("trim");
        let off = set(&["1_1"]);
        let pinned = put(&d, "music", "1_1.mp3", 700 << 10, 90 * 86400); // đã tải về máy: giữ dù quá hạn
        let old = put(&d, "music", "1_2.mp3", 600 << 10, 40 * 86400); // quá hạn giữ
        let a = put(&d, "music", "1_3.mp3", 600 << 10, 5 * 86400); // cũ nhất còn hạn
        let b = put(&d, "music", "1_4.mp3.part", 600 << 10, 3 * 86400);
        let bmap = put(&d, "music", "1_4.mp3.map", 10, 3 * 86400);
        let busy = put(&d, "music", "1_5.mp3", 600 << 10, 4 * 86400); // đang phát
        let c = put(&d, "covers", "1_6.jpg", 600 << 10, 60); // vừa dùng: được bảo vệ
        let r = trim(&d, Policy { limit_mb: 1, keep_days: 30 }, &off, &set(&["1_5"]));
        assert!(pinned.exists() && busy.exists() && c.exists());
        assert!(!old.exists() && !a.exists() && !b.exists() && !bmap.exists(), "{r:?}");
        assert_eq!(r.total, 600 << 10);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn clear_by_kind_peer_keys() {
        let d = tmpdir("clear");
        let off = set(&["2_1"]);
        let m = put(&d, "music", "1_1.mp3", 10, 100);
        let o = put(&d, "music", "2_1.mp3", 20, 100);
        let cv = put(&d, "covers", "1_1.jpg", 30, 100);
        // Không ghi loại: không xoá bài đã tải về máy
        assert_eq!(clear(&d, &off, None, Some("1"), None, &HashSet::new()), (40, 2));
        assert!(!m.exists() && !cv.exists() && o.exists());
        assert_eq!(clear(&d, &off, Some(&["offline".into()]), None, Some(&set(&["2_1"])), &HashSet::new()), (20, 1));
        assert!(!o.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn clear_keeps_busy_local_decode() {
        // Bản giải của tệp trên máy (`local_<băm>`) không có khoá `<peer>_<id>` — vẫn phải nhận ra là đang dùng
        let d = tmpdir("clear-local");
        let playing = put(&d, "music", "local_123.pcm.wav.part", 10, 100);
        let pmap = put(&d, "music", "local_123.pcm.wav.map", 1, 100);
        let idle = put(&d, "music", "local_456.pcm.wav", 20, 100);
        clear(&d, &HashSet::new(), None, None, None, &set(&["local_123"]));
        assert!(playing.exists() && pmap.exists() && !idle.exists());
        trim(&d, Policy { limit_mb: 0, keep_days: 0 }, &HashSet::new(), &set(&["local_123"]));
        assert!(playing.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
