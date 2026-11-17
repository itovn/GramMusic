//! Tin thoại của Telegram là OGG/Opus — iOS 5 / 6 không phát được. Ở đây: libopus (C, dựng sẵn trong crate) + đọc / ghi
//! OGG tự viết → đổi sang WAV 16 bit đơn kênh để AVAudioPlayer phát, và chiều ngược lại (ghi âm → OGG/Opus để gửi).

pub mod ogg;

use std::io;
use std::os::raw::{c_int, c_uchar};

#[repr(C)]
struct OpusDecoder {
    _p: [u8; 0],
}
#[repr(C)]
struct OpusEncoder {
    _p: [u8; 0],
}

unsafe extern "C" {
    fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;
    fn opus_decode(st: *mut OpusDecoder, data: *const c_uchar, len: i32, pcm: *mut i16, frame_size: c_int, fec: c_int) -> c_int;
    fn opus_decoder_destroy(st: *mut OpusDecoder);
    fn opus_encoder_create(fs: i32, channels: c_int, application: c_int, error: *mut c_int) -> *mut OpusEncoder;
    fn opus_encode(st: *mut OpusEncoder, pcm: *const i16, frame_size: c_int, data: *mut c_uchar, max: i32) -> i32;
    fn opus_encoder_ctl(st: *mut OpusEncoder, request: c_int, ...) -> c_int;
    fn opus_encoder_destroy(st: *mut OpusEncoder);
}

const OPUS_APPLICATION_VOIP: c_int = 2048;
const OPUS_SET_BITRATE_REQUEST: c_int = 4002;

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Bộ giải mã Opus (đơn kênh / hai kênh), tự huỷ.
pub struct Decoder {
    st: *mut OpusDecoder,
    channels: usize,
}

unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(rate: i32, channels: usize) -> io::Result<Self> {
        let mut e = 0;
        let st = unsafe { opus_decoder_create(rate, channels as c_int, &mut e) };
        if st.is_null() || e != 0 {
            return Err(err(format!("opus_decoder_create lỗi {e}")));
        }
        Ok(Self { st, channels })
    }

    /// Giải một gói vào `out` (xen kênh); trả số mẫu mỗi kênh.
    pub fn decode(&mut self, packet: &[u8], out: &mut [i16]) -> io::Result<usize> {
        let frame = (out.len() / self.channels) as c_int;
        let n = unsafe { opus_decode(self.st, packet.as_ptr(), packet.len() as i32, out.as_mut_ptr(), frame, 0) };
        if n < 0 {
            return Err(err(format!("opus_decode lỗi {n}")));
        }
        Ok(n as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.st) }
    }
}

/// Bộ mã hoá Opus đơn kênh (thoại).
pub struct Encoder {
    st: *mut OpusEncoder,
}

unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(rate: i32, bitrate: i32) -> io::Result<Self> {
        let mut e = 0;
        let st = unsafe { opus_encoder_create(rate, 1, OPUS_APPLICATION_VOIP, &mut e) };
        if st.is_null() || e != 0 {
            return Err(err(format!("opus_encoder_create lỗi {e}")));
        }
        unsafe { opus_encoder_ctl(st, OPUS_SET_BITRATE_REQUEST, bitrate as c_int) };
        Ok(Self { st })
    }

    /// Mã một khung (vd 20 ms = rate / 50 mẫu) → gói Opus.
    pub fn encode(&mut self, pcm: &[i16]) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; 4000];
        let n = unsafe { opus_encode(self.st, pcm.as_ptr(), pcm.len() as c_int, buf.as_mut_ptr(), buf.len() as i32) };
        if n < 0 {
            return Err(err(format!("opus_encode lỗi {n}")));
        }
        buf.truncate(n as usize);
        Ok(buf)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.st) }
    }
}

/// Phần đầu WAV PCM 16 bit.
fn wav_header(rate: u32, channels: u16, data_len: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&channels.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    h.extend_from_slice(&(channels * 2).to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    h
}

/// OGG/Opus → WAV đơn kênh ở `rate` Hz (8000 / 12000 / 16000 / 24000 / 48000). Trả (số mẫu, thời lượng giây).
pub fn ogg_to_wav(ogg_data: &[u8], rate: u32) -> io::Result<(Vec<u8>, f64)> {
    let packets = ogg::packets(ogg_data)?;
    let head = packets.first().ok_or_else(|| err("tệp OGG rỗng"))?;
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return Err(err("không phải OGG/Opus"));
    }
    let channels = head[9].clamp(1, 2) as usize;
    let pre_skip = u16::from_le_bytes([head[10], head[11]]) as usize * rate as usize / 48000;
    let mut dec = Decoder::new(rate as i32, channels)?;
    let max = rate as usize * 120 / 1000 * channels; // khung dài nhất 120 ms
    let mut frame = vec![0i16; max];
    let mut pcm: Vec<i16> = Vec::new();
    for p in packets.iter().skip(2) {
        // gói 0 = OpusHead, gói 1 = OpusTags
        let n = dec.decode(p, &mut frame)?;
        if channels == 1 {
            pcm.extend_from_slice(&frame[..n]);
        } else {
            pcm.extend((0..n).map(|i| ((frame[2 * i] as i32 + frame[2 * i + 1] as i32) / 2) as i16));
        }
    }
    let pcm = if pcm.len() > pre_skip { &pcm[pre_skip..] } else { &pcm[..0] };
    let mut out = wav_header(rate, 1, (pcm.len() * 2) as u32);
    out.reserve(pcm.len() * 2);
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    Ok((out, pcm.len() as f64 / rate as f64))
}

/// Đọc WAV PCM 16 bit (đơn kênh) — tệp ghi âm của app. Trả (mẫu, tần số).
pub fn read_wav(data: &[u8]) -> io::Result<(Vec<i16>, u32)> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(err("không phải WAV"));
    }
    let (mut i, mut rate, mut channels, mut bits) = (12usize, 0u32, 1u16, 16u16);
    while i + 8 <= data.len() {
        let id = &data[i..i + 4];
        let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        let body = &data[i + 8..(i + 8 + len).min(data.len())];
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if bits != 16 {
                return Err(err("WAV không phải 16 bit"));
            }
            let samples: Vec<i16> = body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            let mono = if channels <= 1 {
                samples
            } else {
                samples.chunks(channels as usize).map(|c| (c.iter().map(|x| *x as i32).sum::<i32>() / c.len() as i32) as i16).collect()
            };
            return Ok((mono, rate));
        }
        i += 8 + len + (len & 1);
    }
    Err(err("WAV thiếu phần data"))
}

/// Dạng sóng kiểu Telegram: 100 giá trị 0–31 (đỉnh mỗi đoạn, chuẩn hoá), xếp 5 bit.
pub fn waveform(pcm: &[i16]) -> Vec<u8> {
    const N: usize = 100;
    let mut peaks = vec![0u16; N];
    if !pcm.is_empty() {
        for (i, s) in pcm.iter().enumerate() {
            let k = i * N / pcm.len();
            peaks[k] = peaks[k].max(s.unsigned_abs());
        }
    }
    let top = (*peaks.iter().max().unwrap_or(&1)).max(1) as u32;
    let vals: Vec<u8> = peaks.iter().map(|p| (*p as u32 * 31 / top) as u8).collect();
    let mut out = vec![0u8; (N * 5).div_ceil(8)];
    for (i, v) in vals.iter().enumerate() {
        let bit = i * 5;
        let (byte, shift) = (bit / 8, bit % 8);
        let w = (*v as u16 & 31) << shift;
        out[byte] |= w as u8;
        if byte + 1 < out.len() {
            out[byte + 1] |= (w >> 8) as u8;
        }
    }
    out
}

/// PCM đơn kênh → OGG/Opus (khung 20 ms, 24 kbit/s). Trả (tệp OGG, thời lượng giây).
pub fn encode_ogg(pcm: &[i16], rate: u32) -> io::Result<(Vec<u8>, f64)> {
    let rate_ok = [8000u32, 12000, 16000, 24000, 48000];
    if !rate_ok.contains(&rate) {
        return Err(err(format!("tần số {rate} Hz không hỗ trợ")));
    }
    let mut enc = Encoder::new(rate as i32, 24000)?;
    let frame = rate as usize / 50;
    let mut w = ogg::Writer::new(0x4752_4d4d); // serial «GRMM»
    // OpusHead: phiên bản 1, 1 kênh, pre-skip 312 (mặc định libopus ở 48 kHz), tần số gốc, gain 0, mapping 0
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(1);
    head.extend_from_slice(&312u16.to_le_bytes());
    head.extend_from_slice(&rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    w.packet(&head, 0, true);
    let mut tags = b"OpusTags".to_vec();
    let vendor = b"GramMini libopus 1.4";
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    w.packet(&tags, 0, true);
    let mut granule: u64 = 312;
    let chunks: Vec<&[i16]> = pcm.chunks(frame).collect();
    for (i, c) in chunks.iter().enumerate() {
        let mut buf = c.to_vec();
        buf.resize(frame, 0);
        let pkt = enc.encode(&buf)?;
        granule += 960; // granule tính theo 48 kHz: 20 ms = 960
        // Gom ~1 giây mỗi trang OGG
        w.packet(&pkt, granule, i % 50 == 49 || i + 1 == chunks.len());
    }
    Ok((w.finish(), pcm.len() as f64 / rate as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_tone() {
        // 1 giây sóng sin 440 Hz ở 16 kHz → OGG/Opus → WAV 24 kHz
        let rate = 16000;
        let pcm: Vec<i16> = (0..rate).map(|i| ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / rate as f64).sin() * 12000.0) as i16).collect();
        let (ogg_data, dur) = encode_ogg(&pcm, rate as u32).unwrap();
        assert!((dur - 1.0).abs() < 0.01);
        assert!(ogg_data.len() > 1000 && ogg_data.len() < 8000, "cỡ OGG {}", ogg_data.len());
        let (wav, d2) = ogg_to_wav(&ogg_data, 24000).unwrap();
        assert!((d2 - 1.0).abs() < 0.05, "thời lượng {d2}");
        let (back, r) = read_wav(&wav).unwrap();
        assert_eq!(r, 24000);
        // Có tiếng (năng lượng đủ lớn ở giữa đoạn)
        let mid = &back[back.len() / 4..back.len() * 3 / 4];
        let rms = (mid.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / mid.len() as f64).sqrt();
        assert!(rms > 3000.0, "rms {rms}");
    }

    #[test]
    fn waveform_shape() {
        let pcm: Vec<i16> = (0..1000).map(|i| if i < 500 { 1000 } else { 10000 }).collect();
        let w = waveform(&pcm);
        assert_eq!(w.len(), 63);
    }
}
