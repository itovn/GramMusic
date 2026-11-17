//! Giờ đúng cho bắt tay fake-TLS. MTProxy kiểm giờ Unix khách ký trong ClientHello và chỉ cho lệch vài giây (proxy của anh:
//! iPhone 5 lệch −2,2 s qua, iPad 1 lệch +5 s bị từ chối — D11). Máy iOS cũ hay trôi giờ, nên lõi tự hỏi giờ qua SNTP (UDP
//! 123) một lần rồi ký bằng giờ đã chỉnh; giờ hệ thống của máy giữ nguyên.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::Mutex;

/// Máy chủ giờ thử lần lượt.
const SERVERS: [&str; 4] = ["time.google.com", "time.cloudflare.com", "pool.ntp.org", "time.apple.com"];
const PER_SERVER: Duration = Duration::from_millis(1500);
/// Giây giữa mốc NTP (1900) và mốc Unix (1970).
const NTP_UNIX_DELTA: f64 = 2_208_988_800.0;
/// Đo lại sau ngần này (máy trôi giờ dần); lần đo hỏng thì chờ ngắn hơn rồi mới thử lại (khỏi chậm mọi lần kết nối khi
/// mạng chặn UDP 123).
const MAX_AGE: Duration = Duration::from_secs(6 * 3600);
const RETRY_AFTER_FAIL: Duration = Duration::from_secs(10 * 60);

static OFFSET_MS: AtomicI64 = AtomicI64::new(0);
static SYNCED: AtomicBool = AtomicBool::new(false);
/// Lần đo gần nhất: (lúc nào, có được không).
static LAST: Mutex<Option<(Instant, bool)>> = Mutex::const_new(None);

fn system_unix() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Giờ Unix đúng (giờ máy + độ lệch đã đo).
pub fn now_unix() -> u32 {
    (system_unix() + OFFSET_MS.load(Ordering::Relaxed) as f64 / 1000.0) as u32
}

/// Độ lệch đã đo (giờ đúng − giờ máy, ms); None = chưa đo được.
pub fn offset_ms() -> Option<i64> {
    SYNCED.load(Ordering::Relaxed).then(|| OFFSET_MS.load(Ordering::Relaxed))
}

/// Đặt độ lệch từ nguồn khác (vd giờ máy chủ Telegram).
pub fn set_offset_ms(ms: i64) {
    OFFSET_MS.store(ms, Ordering::Relaxed);
    SYNCED.store(true, Ordering::Relaxed);
}

/// Hỏi một máy chủ SNTP. Trả (độ lệch ms, máy chủ).
async fn query(server: &str) -> std::io::Result<i64> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect((server, 123)).await?;
    let mut req = [0u8; 48];
    req[0] = 0x1b; // LI 0, phiên bản 3, chế độ 3 (khách)
    let t1 = system_unix();
    sock.send(&req).await?;
    let mut resp = [0u8; 48];
    let n = sock.recv(&mut resp).await?;
    let t4 = system_unix();
    let mode = resp[0] & 7;
    let secs = u32::from_be_bytes(resp[40..44].try_into().unwrap()) as f64;
    if n < 48 || mode != 4 || secs == 0.0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "câu trả lời SNTP sai"));
    }
    let frac = u32::from_be_bytes(resp[44..48].try_into().unwrap()) as f64 / 4_294_967_296.0;
    let server_time = secs + frac - NTP_UNIX_DELTA;
    // Giờ máy chủ lúc gửi ≈ giữa lượt hỏi
    Ok(((server_time + (t4 - t1) / 2.0 - t4) * 1000.0).round() as i64)
}

/// Đo độ lệch nếu chưa đo hoặc đã cũ. Lỗi (không hỏi được máy chủ nào) thì giữ độ lệch cũ (mặc định 0). Trả độ lệch nếu có.
pub async fn ensure_synced() -> Option<i64> {
    let mut last = LAST.lock().await;
    if let Some((t, ok)) = *last {
        if t.elapsed() < if ok { MAX_AGE } else { RETRY_AFTER_FAIL } {
            return offset_ms();
        }
    }
    for s in SERVERS {
        if let Ok(Ok(ms)) = tokio::time::timeout(PER_SERVER, query(s)).await {
            set_offset_ms(ms);
            *last = Some((Instant::now(), true));
            return Some(ms);
        }
    }
    *last = Some((Instant::now(), false));
    offset_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_applies() {
        let base = now_unix();
        set_offset_ms(10_000);
        let shifted = now_unix();
        set_offset_ms(0);
        assert!((shifted as i64 - base as i64 - 10).abs() <= 1);
    }
}
