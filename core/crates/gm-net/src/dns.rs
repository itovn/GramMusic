//! **DNS mã hoá** (chép từ ChimToTV `linux/core/src/doh.rs` — bản Côn Bằng TV / ChimToTV đã chạy trên Android TV, iPad, Windows;
//! GramMusic DECISIONS M17): hỏi địa chỉ tên miền qua **DoH** (RFC 8484) rồi **DoT** (RFC 7858), mỗi giao thức lần lượt NextDNS
//! (hồ sơ riêng của app — mã trong `keys/nextdns.txt`, build.rs đóng vào tệp chạy, không có trên git) → Cloudflare → Google; lỗi
//! hết thì bên gọi dùng DNS của máy — app không bao giờ hỏng vì DNS mã hoá bị chặn.
//!
//! - Dùng cho mọi tên miền lõi tự phân giải: MTProxy / SOCKS5 ghi bằng tên miền, lrclib.net (lời bài hát), OTA sau này. Telegram
//!   đi thẳng nối bằng IP nên không qua đây.
//! - Máy chủ DNS gọi bằng IP cố định (khỏi cần DNS để tìm máy chủ DNS), TLS kiểm chứng chỉ theo tên máy chủ bằng **bộ chứng chỉ gốc
//!   Mozilla đóng sẵn** (webpki-roots — kho chứng chỉ của iOS 5 / 6 đã cũ; ChimToTV dùng kho của máy), đi thẳng (không qua proxy).
//! - NextDNS dùng máy chủ gần nhất: lần đầu hỏi chính NextDNS tên «dns.nextdns.io» (trả máy chủ gần mạng này — anycast từ Viettel
//!   bị đưa sang Hồng Kông), nhớ 6 giờ; anycast đứng sau.
//! - Giữ kết nối TLS tới máy chủ DNS để dùng lại (DoT; DoH bằng HTTP/1.1 keep-alive). Máy chủ lỗi thì nghỉ 2 phút, lần sau thử IP kế.
//! - NXDOMAIN là trả lời thật (không hỏi máy chủ sau); hồ sơ NextDNS chặn bằng 0.0.0.0 thì hỏi máy chủ sau.
//! - Không ghi mã NextDNS vào nhật ký (`scrub`).

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DoH,
    DoT,
}

/// Một máy chủ DNS mã hoá. `steer` = NextDNS: IP lấy theo máy chủ gần nhất (`ips_for`) rồi mới tới `ips` cố định (anycast).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub label: String,
    pub kind: Kind,
    pub host: String,
    pub path: String,
    pub ips: Vec<String>,
    pub steer: bool,
}

impl Server {
    pub fn new(label: &str, kind: Kind, host: &str, path: &str, ips: &[&str], steer: bool) -> Server {
        Server {
            label: label.into(),
            kind,
            host: host.into(),
            path: path.into(),
            ips: ips.iter().map(|s| s.to_string()).collect(),
            steer,
        }
    }

    pub fn port(&self) -> u16 {
        if self.kind == Kind::DoT {
            853
        } else {
            443
        }
    }
}

const NEXT_IPS: [&str; 2] = ["45.90.28.0", "45.90.30.0"];
const GOOGLE_IPS: [&str; 2] = ["8.8.8.8", "8.8.4.4"];

/// Thứ tự hỏi: DoH NextDNS → Cloudflare → Google, rồi DoT cũng vậy (không có mã NextDNS thì bỏ NextDNS).
pub fn chain(next_id: &str) -> Vec<Server> {
    let id = next_id.trim();
    let mut l = Vec::new();
    if !id.is_empty() {
        l.push(Server::new("DoH NextDNS", Kind::DoH, "dns.nextdns.io", &format!("/{id}"), &NEXT_IPS, true));
    }
    l.push(Server::new("DoH Cloudflare", Kind::DoH, "dns.cloudflare.com", "/dns-query", &["104.16.248.249", "104.16.249.249"], false));
    l.push(Server::new("DoH Google", Kind::DoH, "dns.google", "/dns-query", &GOOGLE_IPS, false));
    if !id.is_empty() {
        l.push(Server::new("DoT NextDNS", Kind::DoT, &format!("{id}.dns.nextdns.io"), "", &NEXT_IPS, true));
    }
    l.push(Server::new("DoT Cloudflare", Kind::DoT, "1dot1dot1dot1.cloudflare-dns.com", "", &["1.1.1.1", "1.0.0.1"], false));
    l.push(Server::new("DoT Google", Kind::DoT, "dns.google", "", &GOOGLE_IPS, false));
    l
}

/// Chuỗi máy chủ của app (theo mã NextDNS đóng lúc dựng).
pub fn servers() -> Arc<Vec<Server>> {
    static S: OnceLock<Arc<Vec<Server>>> = OnceLock::new();
    S.get_or_init(|| Arc::new(chain(&next_dns_id()))).clone()
}

/// Tên các nhà cung cấp theo thứ tự, để hiện trong Cài đặt: "NextDNS → Cloudflare → Google".
pub fn providers() -> String {
    let mut names: Vec<String> = Vec::new();
    for s in servers().iter() {
        let n = s.label.split_once(' ').map(|(_, b)| b).unwrap_or(&s.label).to_string();
        if !names.contains(&n) {
            names.push(n);
        }
    }
    names.join(" → ")
}

/// Mã NextDNS đóng vào tệp chạy lúc dựng (build.rs đọc keys/nextdns.txt); không có thì "".
fn next_dns_id() -> String {
    let id = option_env!("GM_NEXTDNS_ID").unwrap_or("").trim();
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
        id.to_string()
    } else {
        String::new()
    }
}

/// Bỏ mã NextDNS khỏi chữ sắp ghi nhật ký (vd lỗi chứng chỉ có tên «<mã>.dns.nextdns.io»).
fn scrub(text: &str) -> String {
    let id = next_dns_id();
    if id.is_empty() {
        text.to_string()
    } else {
        text.replace(&id, "<mã>")
    }
}

// ---- Bật / tắt, nhật ký ----

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Bật / tắt DNS mã hoá (Cài đặt → Proxy). Đổi thì quên địa chỉ đã hỏi.
pub fn set_enabled(on: bool) {
    if ENABLED.swap(on, Ordering::Relaxed) != on {
        clear_cache();
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

type LogFn = Arc<dyn Fn(&str) + Send + Sync>;
static LOG: RwLock<Option<LogFn>> = RwLock::new(None);

/// Nơi ghi nhật ký (lõi: core.log).
pub fn set_log(f: LogFn) {
    *LOG.write().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

fn log(msg: &str) {
    let f = LOG.read().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(f) = f {
        f(&scrub(msg));
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

const CONNECT_MS: u64 = 2_500;
const READ_MS: u64 = 3_000;
const DOWN_MS: i64 = 120_000;
const IDLE_MS: i64 = 90_000;
const MAX_IDLE: usize = 4;
const POPS_MS: i64 = 6 * 3_600_000;

/// Khoá không «nhiễm độc» làm sập app: luồng nào lỡ panic lúc giữ khoá thì vẫn dùng tiếp dữ liệu.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Đã hỏi: tên miền → (địa chỉ; None = tên miền không có (NXDOMAIN), hạn nhớ (ms), máy chủ đã trả lời).
type AddrCache = HashMap<String, (Option<Ipv4Addr>, i64, String)>;

static CACHE: LazyLock<Mutex<AddrCache>> = LazyLock::new(Default::default);
static DOWN_UNTIL: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);
static NEXT_IP: LazyLock<Mutex<HashMap<String, usize>>> = LazyLock::new(Default::default);

/// Chỉ để thử (lệnh `net.dns` của lõi): coi các máy chủ này (theo nhãn) như bị chặn, để thử chuỗi dự phòng.
static DEBUG_FAIL: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(Default::default);

pub fn set_debug_fail(labels: HashSet<String>) {
    *DEBUG_FAIL.write().unwrap_or_else(|e| e.into_inner()) = labels;
}

/// Kết quả hỏi một máy chủ (máy chủ lỗi thì `ask` trả Err).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// `all` = mọi bản ghi A (theo thứ tự máy chủ trả); `addr` = cái đầu.
    Found { addr: Ipv4Addr, ttl: u32, all: Vec<Ipv4Addr> },
    /// NXDOMAIN: tên miền không có (hoặc hồ sơ DNS chặn) — không hỏi máy chủ khác.
    NoDomain,
    /// Có tên miền nhưng không có bản ghi A (vd chỉ có IPv6) — để DNS của máy thử.
    NoIpv4,
    /// SERVFAIL / REFUSED / gói hỏng — hỏi máy chủ sau.
    Failed,
}

fn no_domain(name: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("không tìm thấy tên miền {name}"))
}

/// Tên miền chuẩn để hỏi / nhớ: bỏ [ ] của IPv6, dấu chấm cuối, chữ thường.
fn clean(host: &str) -> String {
    host.trim_matches(|c| c == '[' || c == ']').trim_end_matches('.').to_lowercase()
}

/// Địa chỉ IPv4 qua chuỗi DNS mã hoá (nhớ theo TTL, 30 giây – 10 phút). Ok(None) = IP / localhost, hoặc mọi máy chủ lỗi /
/// không có bản ghi A → bên gọi dùng DNS của máy. Tên miền không có → Err(NotFound).
pub async fn resolve(host: &str) -> io::Result<Option<IpAddr>> {
    resolve_with(host, servers()).await
}

/// Kết quả một lần tra (dùng chung cho các bên cùng chờ).
#[derive(Debug, Clone)]
enum Out {
    Addr(Option<Ipv4Addr>),
    NoDomain,
}

/// Đang hỏi: tên miền → ô kết quả. Nhiều kết nối cùng lúc tới một tên miền (MTProxy mở kết nối tới vài DC một lượt): chỉ hỏi
/// một lần, các kết nối kia chờ kết quả đó (bên đang hỏi bỏ giữa chừng thì bên chờ kế tiếp hỏi thay — OnceCell).
type Cell = Arc<tokio::sync::OnceCell<Out>>;
static INFLIGHT: LazyLock<Mutex<HashMap<String, Cell>>> = LazyLock::new(Default::default);

/// Như `resolve`, với chuỗi máy chủ riêng (test).
pub async fn resolve_with(host: &str, servers: Arc<Vec<Server>>) -> io::Result<Option<IpAddr>> {
    let name = clean(host);
    if name.parse::<IpAddr>().is_ok() || name == "localhost" {
        return Ok(None);
    }
    if let Some(hit) = cached(&name) {
        return hit;
    }
    let cell = lock(&INFLIGHT).entry(name.clone()).or_default().clone();
    let out = cell.get_or_init(|| lookup(&name, &servers)).await.clone();
    {
        let mut m = lock(&INFLIGHT);
        if m.get(&name).is_some_and(|c| Arc::ptr_eq(c, &cell)) {
            m.remove(&name);
        }
    }
    match out {
        Out::Addr(a) => Ok(a.map(IpAddr::V4)),
        Out::NoDomain => Err(no_domain(&name)),
    }
}

fn cached(name: &str) -> Option<io::Result<Option<IpAddr>>> {
    let c = lock(&CACHE);
    let (addr, until, _) = c.get(name)?;
    if *until <= now_ms() {
        return None;
    }
    Some(match addr {
        Some(a) => Ok(Some(IpAddr::V4(*a))),
        None => Err(no_domain(name)),
    })
}

/// Máy chủ đã trả lời tên miền này (đang nhớ) — để thử / hiện.
pub fn via(host: &str) -> Option<String> {
    let c = lock(&CACHE);
    c.get(&clean(host)).filter(|(_, until, _)| *until > now_ms()).map(|(_, _, v)| v.clone())
}

async fn lookup(name: &str, servers: &[Server]) -> Out {
    // Tên miền sai (nhãn rỗng / dài quá 63 byte…): khỏi hỏi, để DNS của máy báo lỗi — không coi máy chủ nào là lỗi
    if dns_wire::query(name, 0).is_err() {
        return Out::Addr(None);
    }
    for s in servers {
        let down = lock(&DOWN_UNTIL).get(&s.label).copied().unwrap_or(0);
        if down > now_ms() {
            continue;
        }
        let ips = ips_for(s).await;
        if ips.is_empty() {
            continue;
        }
        let k = lock(&NEXT_IP).get(&s.label).copied().unwrap_or(0);
        let ip = ips[k % ips.len()].clone();
        let t0 = now_ms();
        let answer = match ask(s, &ip, name).await {
            Ok(a) => a,
            Err(e) => {
                // Máy chủ bị chặn / không trả lời: nghỉ 2 phút, lần sau thử IP kế của nó
                lock(&DOWN_UNTIL).insert(s.label.clone(), now_ms() + DOWN_MS);
                *lock(&NEXT_IP).entry(s.label.clone()).or_insert(0) += 1;
                log(&format!("dns: {} · {ip} lỗi ({} ms): {e}", s.label, now_ms() - t0));
                continue;
            }
        };
        match answer {
            // Hồ sơ NextDNS chặn tên miền bằng 0.0.0.0 → câu trả lời không dùng được: hỏi máy chủ sau (Cloudflare / Google không
            // lọc). NXDOMAIN vẫn là trả lời thật.
            Answer::Found { ref all, .. } if all.iter().any(|a| blocked(&IpAddr::V4(*a))) => {
                log(&format!("dns: {name} bị {} chặn ({all:?}), hỏi máy chủ sau", s.label));
            }
            Answer::Found { addr, ttl, .. } => {
                let keep = (ttl as i64).clamp(30, 600) * 1000;
                lock(&CACHE).insert(name.to_string(), (Some(addr), now_ms() + keep, s.label.clone()));
                log(&format!("dns: {name} → {addr} ({} · {ip}, {} ms)", s.label, now_ms() - t0));
                return Out::Addr(Some(addr));
            }
            Answer::NoDomain => {
                // nhớ 30 giây: kết nối khác khỏi hỏi lại
                lock(&CACHE).insert(name.to_string(), (None, now_ms() + 30_000, s.label.clone()));
                log(&format!("dns: {name} không có ({})", s.label));
                return Out::NoDomain;
            }
            Answer::NoIpv4 => return Out::Addr(None),
            Answer::Failed => {}
        }
    }
    log(&format!("dns: {name} — chuỗi DNS mã hoá không trả lời, dùng DNS của máy"));
    Out::Addr(None)
}

// ---- NextDNS: máy chủ gần nhất ----

static POPS: Mutex<(Vec<String>, i64)> = Mutex::new((Vec::new(), 0));
static POPS_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// IP để gọi máy chủ s: NextDNS = máy chủ gần nhất rồi anycast; còn lại = IP cố định.
pub async fn ips_for(s: &Server) -> Vec<String> {
    if !s.steer {
        return s.ips.clone();
    }
    let mut l = next_pops().await;
    for ip in &s.ips {
        if !l.contains(ip) {
            l.push(ip.clone());
        }
    }
    l
}

fn fresh_pops() -> Option<Vec<String>> {
    let p = lock(&POPS);
    if now_ms() < p.1 {
        Some(p.0.clone())
    } else {
        None
    }
}

async fn next_pops() -> Vec<String> {
    if let Some(p) = fresh_pops() {
        return p;
    }
    let _g = POPS_LOCK.lock().await;
    if let Some(p) = fresh_pops() {
        return p;
    }
    let found = discover_pops().await;
    *lock(&POPS) = (found.clone(), now_ms() + if found.is_empty() { 600_000 } else { POPS_MS });
    let list = if found.is_empty() { "(không tìm được — dùng anycast)".to_string() } else { found.join(", ") };
    log(&format!("dns: NextDNS gần nhất: {list}"));
    found
}

/// Hỏi chính NextDNS (anycast, đã mã hoá) tên «dns.nextdns.io» — NextDNS trả máy chủ gần IP của mạng này; không được thì
/// DNS máy.
async fn discover_pops() -> Vec<String> {
    let all = servers();
    let any = all.iter().find(|x| x.steer && x.kind == Kind::DoH).or_else(|| all.iter().find(|x| x.steer));
    let Some(any) = any else {
        return Vec::new();
    };
    for ip in &any.ips {
        // lỗi thì thử IP anycast kế
        if let Ok(Answer::Found { all, .. }) = ask(any, ip, "dns.nextdns.io").await {
            return distinct(all.into_iter().map(IpAddr::V4).filter(|a| !blocked(a)).map(|a| a.to_string()));
        }
    }
    match tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host(("dns.nextdns.io", 0))).await {
        Ok(Ok(it)) => distinct(it.map(|a| a.ip()).filter(|a| a.is_ipv4() && !blocked(a)).map(|a| a.to_string())),
        _ => Vec::new(),
    }
}

fn distinct(it: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for x in it {
        if !v.contains(&x) {
            v.push(x);
        }
    }
    v
}

/// Hỏi máy chủ s ở địa chỉ ip.
pub async fn ask(s: &Server, ip: &str, name: &str) -> io::Result<Answer> {
    if DEBUG_FAIL.read().unwrap_or_else(|e| e.into_inner()).contains(&s.label) {
        return Err(io::Error::other("giả bị chặn"));
    }
    // DoH khuyên mã câu hỏi 0 (dễ lưu đệm); DoT: mã ngẫu nhiên, trả lời khác mã thì bỏ
    let id: u16 = if s.kind == Kind::DoT {
        let mut b = [0u8; 2];
        let _ = getrandom::fill(&mut b);
        u16::from_be_bytes(b)
    } else {
        0
    };
    let query = dns_wire::query(name, id)?;
    Ok(match exchange(s, ip, &query).await? {
        None => Answer::Failed,
        Some(reply) => dns_wire::answer(&reply, id),
    })
}

// ---- Kết nối dùng lại ----

struct DnsConn {
    key: String,
    io: BufReader<tokio_rustls::client::TlsStream<TcpStream>>,
    last_used: i64,
}

static IDLE: LazyLock<Mutex<HashMap<String, Vec<DnsConn>>>> = LazyLock::new(Default::default);

fn take(key: &str) -> Option<DnsConn> {
    let mut idle = lock(&IDLE);
    let q = idle.get_mut(key)?;
    // nghỉ quá lâu thì bỏ (đóng khi rơi khỏi phạm vi)
    while let Some(c) = q.pop() {
        if now_ms() - c.last_used < IDLE_MS {
            return Some(c);
        }
    }
    None
}

fn give(mut c: DnsConn) {
    c.last_used = now_ms();
    let mut idle = lock(&IDLE);
    let q = idle.entry(c.key.clone()).or_default();
    if q.len() < MAX_IDLE {
        q.push(c);
    }
}

/// Một lần hỏi: dùng lại kết nối đang nghỉ (hỏng thì mở mới ngay), không có thì mở kết nối mới.
async fn exchange(s: &Server, ip: &str, query: &[u8]) -> io::Result<Option<Vec<u8>>> {
    let key = format!("{}|{ip}", s.label);
    if let Some(old) = take(&key) {
        // lỗi: máy chủ đã đóng kết nối cũ — không phải máy chủ lỗi, mở kết nối mới bên dưới
        if let Ok(r) = round_trip(s, old, query).await {
            return Ok(r);
        }
    }
    let c = open(s, ip, key).await?;
    round_trip(s, c, query).await
}

/// Cấu hình TLS chung: bộ chứng chỉ gốc Mozilla đóng sẵn (iOS 5 / 6 có kho chứng chỉ cũ), bộ mã hoá ring (như reqwest của lõi).
fn tls_config() -> Option<Arc<rustls::ClientConfig>> {
    static C: OnceLock<Option<Arc<rustls::ClientConfig>>> = OnceLock::new();
    C.get_or_init(|| {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .ok()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Some(Arc::new(cfg))
    })
    .clone()
}

fn timed_out(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, format!("{what} quá thời gian"))
}

async fn open(s: &Server, ip: &str, key: String) -> io::Result<DnsConn> {
    let addr: IpAddr = ip.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "IP máy chủ DNS sai"))?;
    let tcp = tokio::time::timeout(Duration::from_millis(CONNECT_MS), TcpStream::connect(SocketAddr::new(addr, s.port())))
        .await
        .map_err(|_| timed_out("kết nối"))??;
    tcp.set_nodelay(true)?;
    let cfg = tls_config().ok_or_else(|| io::Error::other("không tạo được TLS"))?;
    // Kiểm chứng chỉ theo tên máy chủ
    let name = rustls::pki_types::ServerName::try_from(s.host.clone())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "tên máy chủ DNS sai"))?;
    let tls = tokio::time::timeout(Duration::from_millis(READ_MS), tokio_rustls::TlsConnector::from(cfg).connect(name, tcp))
        .await
        .map_err(|_| timed_out("bắt tay TLS"))??;
    Ok(DnsConn { key, io: BufReader::with_capacity(4096, tls), last_used: now_ms() })
}

/// Gửi một câu hỏi trên kết nối c, đọc đúng một trả lời rồi trả kết nối về chỗ nghỉ (lỗi / máy chủ báo đóng thì đóng).
async fn round_trip(s: &Server, mut c: DnsConn, query: &[u8]) -> io::Result<Option<Vec<u8>>> {
    match tokio::time::timeout(Duration::from_millis(READ_MS), talk(s, &mut c, query)).await {
        Ok(Ok((reply, keep))) => {
            if keep {
                give(c);
            }
            Ok(reply)
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(timed_out("máy chủ DNS trả lời")),
    }
}

async fn talk(s: &Server, c: &mut DnsConn, query: &[u8]) -> io::Result<(Option<Vec<u8>>, bool)> {
    if s.kind == Kind::DoT {
        let mut msg = vec![(query.len() >> 8) as u8, query.len() as u8];
        msg.extend_from_slice(query);
        c.io.get_mut().write_all(&msg).await?;
        c.io.get_mut().flush().await?;
        let mut len = [0u8; 2];
        c.io.read_exact(&mut len).await?;
        let mut reply = vec![0u8; ((len[0] as usize) << 8) | len[1] as usize];
        c.io.read_exact(&mut reply).await?;
        return Ok((Some(reply), true));
    }
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/dns-message\r\nAccept: application/dns-message\r\nContent-Length: {}\r\n\r\n",
        s.path,
        s.host,
        query.len()
    );
    let mut msg = head.into_bytes();
    msg.extend_from_slice(query);
    c.io.get_mut().write_all(&msg).await?;
    c.io.get_mut().flush().await?;
    let r = doh_http::read(&mut c.io).await?;
    Ok((if r.status == 200 { Some(r.body) } else { None }, r.keep_alive))
}

/// Đổi cài đặt mạng: quên địa chỉ đã hỏi, máy chủ đang nghỉ, kết nối đang nghỉ.
pub fn clear_cache() {
    lock(&CACHE).clear();
    lock(&DOWN_UNTIL).clear();
    lock(&IDLE).clear();
}

/// Địa chỉ DNS chặn trả: trỏ về chính máy chứ không phải trang thật (0.0.0.0, ::, 127.x, ::1).
pub fn blocked(a: &IpAddr) -> bool {
    match a {
        IpAddr::V4(v) => v.is_unspecified() || v.is_loopback(),
        IpAddr::V6(v) => {
            v.is_unspecified()
                || v.is_loopback()
                || v.to_ipv4_mapped().map(|m| m.is_unspecified() || m.is_loopback()).unwrap_or(false)
        }
    }
}

// ---- Dùng cho kết nối ----

/// Địa chỉ cho một kết nối lõi tự phân giải: qua DNS mã hoá nếu bật; Ok(None) = dùng DNS của máy như thường. Tên miền không có
/// (DNS mã hoá trả NXDOMAIN) → Err(NotFound).
pub async fn for_connect(host: &str) -> io::Result<Option<IpAddr>> {
    if enabled() {
        resolve(host).await
    } else {
        Ok(None)
    }
}

/// Mở TCP tới `host:port` (tên miền qua DNS mã hoá nếu bật, không được thì DNS của máy).
pub async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    match for_connect(host).await? {
        Some(ip) => TcpStream::connect(SocketAddr::new(ip, port)).await,
        None => TcpStream::connect((host, port)).await,
    }
}

/// Mọi địa chỉ của `host` (cho reqwest): DNS mã hoá nếu bật, không được thì DNS của máy.
pub async fn lookup_host(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    if let Some(ip) = for_connect(host).await? {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    Ok(tokio::net::lookup_host((host, port)).await?.collect())
}

/// Gói DNS (RFC 1035): hỏi bản ghi A, đọc trả lời.
pub mod dns_wire {
    use super::Answer;
    use std::io;
    use std::net::Ipv4Addr;

    pub fn query(name: &str, id: u16) -> io::Result<Vec<u8>> {
        let mut q = vec![(id >> 8) as u8, id as u8, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]; // RD, 1 câu hỏi
        for label in name.trim_end_matches('.').split('.') {
            let b = label.as_bytes();
            if b.is_empty() || b.len() > 63 || !label.is_ascii() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("tên miền sai: {name}")));
            }
            q.push(b.len() as u8);
            q.extend_from_slice(b);
        }
        q.extend_from_slice(&[0, 0, 1, 0, 1]); // hết tên, kiểu A, lớp IN
        Ok(q)
    }

    /// Trả lời → Answer; id khác (DoT) hay gói hỏng → Failed.
    pub fn answer(b: &[u8], id: u16) -> Answer {
        if b.len() < 12 {
            return Answer::Failed;
        }
        // chỉ gọi khi i + 1 < b.len()
        let u16_at = |i: usize| ((b[i] as usize) << 8) | b[i + 1] as usize;
        if u16_at(0) != id as usize {
            return Answer::Failed;
        }
        let rcode = b[3] & 0x0F;
        if rcode == 3 {
            return Answer::NoDomain;
        }
        if rcode != 0 {
            return Answer::Failed;
        }
        let mut i = 12;
        for _ in 0..u16_at(4) {
            i = skip_name(b, i) + 4;
        }
        let mut all = Vec::new();
        const NONE: i64 = i32::MAX as i64;
        let mut ttl = NONE;
        for _ in 0..u16_at(6) {
            i = skip_name(b, i);
            if i + 10 > b.len() {
                return Answer::Failed;
            }
            let kind = u16_at(i);
            let t = u32::from_be_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as i64;
            let len = u16_at(i + 8);
            i += 10;
            if i + len > b.len() {
                return Answer::Failed;
            }
            if kind == 1 && len == 4 {
                all.push(Ipv4Addr::new(b[i], b[i + 1], b[i + 2], b[i + 3]));
                ttl = ttl.min(t.min(NONE));
            }
            i += len;
        }
        if all.is_empty() {
            return Answer::NoIpv4;
        }
        Answer::Found { addr: all[0], ttl: if ttl == NONE { 60 } else { ttl as u32 }, all }
    }

    /// Bỏ qua một tên (có thể nén bằng con trỏ 0xC0).
    fn skip_name(b: &[u8], mut i: usize) -> usize {
        while i < b.len() {
            let l = b[i] as usize;
            if l == 0 {
                return i + 1;
            }
            if l & 0xC0 == 0xC0 {
                return i + 2;
            }
            i += 1 + l;
        }
        b.len()
    }
}

/// Trả lời HTTP/1.1 của máy chủ DoH: đọc đúng một trả lời trên kết nối (để dùng lại kết nối).
pub mod doh_http {
    use std::io;
    use tokio::io::{AsyncRead, AsyncReadExt};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reply {
        pub status: u16,
        pub body: Vec<u8>,
        pub keep_alive: bool,
    }

    const MAX_BODY: usize = 70_000;

    fn broken(msg: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
    }

    async fn byte<R: AsyncRead + Unpin + ?Sized>(s: &mut R) -> io::Result<u8> {
        let mut one = [0u8; 1];
        if s.read(&mut one).await? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "máy chủ ngắt giữa chừng"));
        }
        Ok(one[0])
    }

    /// Giá trị (chữ thường, đã cắt) của dòng đầu `name:` trong phần đầu trả lời.
    fn header(h: &str, name: &str) -> Option<String> {
        h.split("\r\n").skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_ascii_lowercase())
        })
    }

    /// Đọc đầu trả lời rồi thân theo Content-Length / chunked; không có độ dài thì đọc tới khi máy chủ đóng.
    pub async fn read<R: AsyncRead + Unpin + ?Sized>(s: &mut R) -> io::Result<Reply> {
        let mut head = Vec::new();
        let mut last: u32 = 0;
        while last != 0x0D0A_0D0A {
            let b = byte(s).await?;
            head.push(b);
            last = (last << 8) | b as u32;
            if head.len() > 16_384 {
                return Err(broken("đầu trả lời quá dài"));
            }
        }
        let h: String = head.iter().map(|&b| b as char).collect(); // Latin-1
        let status = h
            .split("\r\n")
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|x| x.parse::<u16>().ok())
            .ok_or_else(|| broken("trả lời HTTP sai"))?;
        let close = header(&h, "connection").is_some_and(|v| v.contains("close"));
        if header(&h, "transfer-encoding").is_some_and(|v| v.contains("chunked")) {
            return Ok(Reply { status, body: dechunk(s).await?, keep_alive: !close });
        }
        let Some(len) = header(&h, "content-length") else {
            return Ok(Reply { status, body: read_to_end(s).await?, keep_alive: false });
        };
        let len: usize = len.parse().map_err(|_| broken("trả lời DNS quá dài"))?;
        if len > MAX_BODY {
            return Err(broken("trả lời DNS quá dài"));
        }
        let mut body = vec![0u8; len];
        s.read_exact(&mut body).await?;
        Ok(Reply { status, body, keep_alive: !close })
    }

    async fn line<R: AsyncRead + Unpin + ?Sized>(s: &mut R) -> io::Result<String> {
        let mut b = String::new();
        loop {
            let c = byte(s).await?;
            if c == b'\n' {
                return Ok(b.trim_end_matches('\r').to_string());
            }
            b.push(c as char);
            if b.len() > 1024 {
                return Err(broken("dòng quá dài"));
            }
        }
    }

    async fn dechunk<R: AsyncRead + Unpin + ?Sized>(s: &mut R) -> io::Result<Vec<u8>> {
        let mut o = Vec::new();
        loop {
            let l = line(s).await?;
            let size = l.split(';').next().unwrap_or("").trim();
            let n = usize::from_str_radix(size, 16).map_err(|_| broken("chunk sai"))?;
            if n == 0 {
                // bỏ phần đuôi (trailer)
                while !line(s).await?.is_empty() {}
                return Ok(o);
            }
            if o.len().saturating_add(n) > MAX_BODY {
                return Err(broken("trả lời DNS quá dài"));
            }
            let at = o.len();
            o.resize(at + n, 0);
            s.read_exact(&mut o[at..]).await?;
            line(s).await?; // CRLF sau mỗi khúc
        }
    }

    async fn read_to_end<R: AsyncRead + Unpin + ?Sized>(s: &mut R) -> io::Result<Vec<u8>> {
        let mut o = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = s.read(&mut buf).await?;
            if n == 0 {
                return Ok(o);
            }
            o.extend_from_slice(&buf[..n]);
            if o.len() > MAX_BODY {
                return Err(broken("trả lời DNS quá dài"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..t.len()).step_by(2).map(|i| u8::from_str_radix(&t[i..i + 2], 16).unwrap()).collect()
    }

    fn to_hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    async fn body(resp: &[u8]) -> Option<Vec<u8>> {
        let mut r = resp;
        match doh_http::read(&mut r).await {
            Ok(x) if x.status == 200 => Some(x.body),
            _ => None,
        }
    }

    #[test]
    fn blocked_answers() {
        for a in ["0.0.0.0", "::", "127.0.0.1", "::1"] {
            assert!(blocked(&ip(a)), "{a}");
        }
        for a in ["104.21.35.220", "2606:4700:3035::6815:23dc", "10.0.0.1"] {
            assert!(!blocked(&ip(a)), "{a}");
        }
    }

    #[test]
    fn query_for_a() {
        assert_eq!(to_hex(&dns_wire::query("a.bc", 0).unwrap()), "000001000001000000000000016102626300".to_string() + "00010001");
        assert!(dns_wire::query("a..b", 0).is_err());
        assert!(dns_wire::query(&"x".repeat(64), 0).is_err());
    }

    #[test]
    fn answer_with_cname_and_compression() {
        let reply = hex(
            "0000 8180 0001 0003 0000 0000\
             03 777777 01 78 02 766e 00 0001 0001\
             c00c 0005 0001 00000e10 0004 c010 c010\
             c010 0001 0001 0000012c 0004 68 15 23 dc\
             c010 0001 0001 0000003c 0004 ac 43 b4 3b",
        );
        match dns_wire::answer(&reply, 0) {
            Answer::Found { addr, ttl, all } => {
                assert_eq!(addr, "104.21.35.220".parse::<Ipv4Addr>().unwrap());
                assert_eq!(ttl, 60);
                assert_eq!(all.iter().map(|a| a.to_string()).collect::<Vec<_>>(), vec!["104.21.35.220", "172.67.180.59"]);
            }
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn no_domain_no_ipv4_and_wrong_id() {
        assert_eq!(dns_wire::answer(&hex("0000 8183 0001 0000 0000 0000 01 78 00 0001 0001"), 0), Answer::NoDomain);
        let empty = hex("0000 8180 0001 0000 0000 0000 01 78 00 0001 0001");
        assert_eq!(dns_wire::answer(&empty, 0), Answer::NoIpv4);
        assert_eq!(dns_wire::answer(&empty, 0x1234), Answer::Failed); // DoT: id khác → bỏ
        assert_eq!(dns_wire::answer(&hex("0000 8182 0001 0000 0000 0000 01 78 00 0001 0001"), 0), Answer::Failed);
        // gói cụt / rác: không panic
        assert_eq!(dns_wire::answer(&hex("0000 8180 0001 0001 0000 0000 01 78 00 0001 0001 c00c 0001"), 0), Answer::Failed);
        assert_eq!(dns_wire::answer(&[0u8; 5], 0), Answer::Failed);
        assert_eq!(dns_wire::answer(&hex("0000 8180 ffff ffff 0000 0000 3f"), 0), Answer::Failed);
    }

    #[tokio::test]
    async fn http_body_plain_and_chunked() {
        assert_eq!(
            body(b"HTTP/1.1 200 OK\r\nContent-Type: application/dns-message\r\nContent-Length: 3\r\n\r\nabc").await.as_deref(),
            Some(&b"abc"[..])
        );
        assert_eq!(
            body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab\r\n3;x=1\r\ncde\r\n0\r\n\r\n").await.as_deref(),
            Some(&b"abcde"[..])
        );
        assert_eq!(body(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n").await, None);
        assert_eq!(body(b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999999\r\n\r\n").await, None);
        assert_eq!(body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n").await, None);
    }

    #[test]
    fn chain_order() {
        let labels = |l: Vec<Server>| l.into_iter().map(|s| s.label).collect::<Vec<_>>();
        assert_eq!(
            labels(chain("abc123")),
            vec!["DoH NextDNS", "DoH Cloudflare", "DoH Google", "DoT NextDNS", "DoT Cloudflare", "DoT Google"]
        );
        assert_eq!(chain("abc123")[0].path, "/abc123");
        assert_eq!(chain("abc123")[3].host, "abc123.dns.nextdns.io");
        assert_eq!(labels(chain("")), vec!["DoH Cloudflare", "DoH Google", "DoT Cloudflare", "DoT Google"]);
        assert!(providers().ends_with("Cloudflare → Google"));
    }

    #[tokio::test]
    async fn dead_servers_fall_back_to_device() {
        // Mọi máy chủ không tới được (cổng đóng trên máy) → None: bên gọi dùng DNS của máy, không báo lỗi
        let dead = Arc::new(vec![Server::new("Chết", Kind::DoT, "dns.test", "", &["127.0.0.1"], false)]);
        assert_eq!(resolve_with("nhac.test", dead.clone()).await.unwrap(), None);
        assert_eq!(resolve_with("localhost", dead.clone()).await.unwrap(), None); // IP / localhost: không hỏi
        assert_eq!(resolve_with("[::1]", dead).await.unwrap(), None);
    }

    /// Máy chủ lỗi → nghỉ 2 phút, lần sau thử IP kế của nó; đổi cài đặt (clear_cache) thì hỏi lại ngay.
    #[tokio::test]
    async fn failed_server_rests_then_clear_cache() {
        let s = Arc::new(vec![Server::new("Giả nghỉ", Kind::DoT, "dns.test", "", &["127.0.0.1", "127.0.0.2"], false)]);
        assert_eq!(resolve_with("nghi.test", s.clone()).await.unwrap(), None);
        assert!(lock(&DOWN_UNTIL).get("Giả nghỉ").copied().unwrap_or(0) > now_ms());
        assert_eq!(lock(&NEXT_IP).get("Giả nghỉ").copied(), Some(1));
        // đang nghỉ: không hỏi (IP kế vẫn là 1)
        assert_eq!(resolve_with("nghi2.test", s.clone()).await.unwrap(), None);
        assert_eq!(lock(&NEXT_IP).get("Giả nghỉ").copied(), Some(1));
        clear_cache();
        assert!(!lock(&DOWN_UNTIL).contains_key("Giả nghỉ"));
    }

    /// Tên miền không có (NXDOMAIN) nhớ 30 giây: hỏi lại trả lỗi ngay, không hỏi máy chủ.
    #[tokio::test]
    async fn no_domain_is_remembered() {
        lock(&CACHE).insert("khong-co.test".into(), (None, now_ms() + 30_000, "thử".into()));
        let e = resolve("khong-co.test").await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
        assert!(e.to_string().contains("khong-co.test"));
        lock(&CACHE).insert("co.test".into(), (Some(Ipv4Addr::new(203, 0, 113, 5)), now_ms() + 30_000, "thử".into()));
        assert_eq!(resolve("CO.test.").await.unwrap(), Some(ip("203.0.113.5")));
        assert_eq!(via("co.test").as_deref(), Some("thử"));
    }

    const RN: &str = "\r\n";

    #[tokio::test]
    async fn keep_alive_reads_one_reply_at_a_time() {
        // Ba trả lời liền nhau trên một kết nối (keep-alive): đọc đúng từng cái; máy chủ báo đóng thì không dùng lại
        let all = [
            "HTTP/1.1 200 OK", RN, "Content-Length: 3", RN, RN, "abc", "HTTP/1.1 200 OK", RN, "Transfer-Encoding: chunked", RN, RN,
            "2", RN, "de", RN, "0", RN, RN, "HTTP/1.1 200 OK", RN, "Content-Length: 1", RN, "Connection: close", RN, RN, "f",
        ]
        .concat();
        let mut st = BufReader::new(all.as_bytes());
        let r1 = doh_http::read(&mut st).await.unwrap();
        let r2 = doh_http::read(&mut st).await.unwrap();
        let r3 = doh_http::read(&mut st).await.unwrap();
        assert_eq!((r1.body.as_slice(), r1.keep_alive), (&b"abc"[..], true));
        assert_eq!((r2.body.as_slice(), r2.keep_alive), (&b"de"[..], true));
        assert_eq!((r3.body.as_slice(), r3.keep_alive), (&b"f"[..], false));
    }

    #[tokio::test]
    async fn only_next_dns_is_steered() {
        assert_eq!(chain("abc123").iter().map(|s| s.steer).collect::<Vec<_>>(), vec![true, false, false, true, false, false]);
        assert_eq!(ips_for(&chain("abc123")[4]).await, vec!["1.1.1.1", "1.0.0.1"]);
    }

    /// Hỏi thật từng máy chủ trong chuỗi (cần mạng): `cargo test -p gm-net live_chain -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_chain() {
        for s in servers().iter() {
            let ips = ips_for(s).await;
            let t = std::time::Instant::now();
            let r = ask(s, &ips[0], "lrclib.net").await;
            println!("{} · {}: {:?} ({} ms)", s.label.replace(&next_dns_id(), "<mã>"), ips[0], r, t.elapsed().as_millis());
            assert!(matches!(r, Ok(Answer::Found { .. })), "{}", s.label);
        }
    }
}
