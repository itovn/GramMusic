//! Thư viện nhạc: các nguồn (Tin nhắn đã lưu, nhóm, kênh, chat riêng) và chỉ mục bài hát của từng nguồn.
//!
//! - `sources.json`: danh sách nguồn (bật / tắt trong thư viện, số bài máy chủ báo, lần quét cuối).
//! - `library/<peer>.json`: chỉ mục một nguồn — các bài (mới → cũ), tin mới nhất / cũ nhất đã quét, đã quét hết chưa. Quét dở
//!   (tắt app giữa chừng) thì lần sau quét tiếp từ tin cũ nhất đã có.
//!
//! Trong bộ nhớ: danh sách phẳng các bài của nguồn đang bật (lọc trùng theo mã tài liệu — cùng một tệp chuyển tiếp sang nhiều
//! chat chỉ hiện một lần), kèm chữ đã bỏ dấu để tìm / xếp. Thứ tự xếp được đệm theo (nguồn, cách xếp) tới khi thư viện đổi.
//! App lấy từng trang (`query`) chứ không lấy cả thư viện — máy A4 256 MB, kênh có hàng chục nghìn bài.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::text;

/// Một bài hát = một tin có tệp âm thanh.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub peer: String,
    pub id: i32,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    /// Thời lượng, giây.
    #[serde(default)]
    pub dur: u32,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mime: String,
    /// Tên tệp gốc.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ext: String,
    /// Ngày gửi (Unix giây).
    #[serde(default)]
    pub date: i64,
    /// Mã tài liệu Telegram (lọc trùng).
    #[serde(default)]
    pub doc: i64,
    /// Có ảnh bìa (ảnh thu nhỏ đính kèm tệp).
    #[serde(default)]
    pub thumb: bool,
    /// Chủ đề (nhóm có chủ đề — diễn đàn): id tin tạo chủ đề; 0 = không thuộc chủ đề nào (nhóm thường, hoặc chủ đề General).
    #[serde(default)]
    pub topic: i32,
}

impl Song {
    pub fn key(&self) -> String {
        format!("{}_{}", self.peer, self.id)
    }

    /// Tên tệp trong bộ nhớ đệm: `<khoá>.<đuôi>`.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.key(), if self.ext.is_empty() { "bin" } else { &self.ext })
    }

    pub fn playable(&self) -> bool {
        text::playable_ext(&self.ext)
    }
}

/// Tách khoá bài `<peer>_<id>`.
pub fn split_key(key: &str) -> Option<(String, i32)> {
    let (p, i) = key.rsplit_once('_')?;
    p.parse::<i64>().ok()?;
    Some((p.to_string(), i.parse().ok()?))
}

/// Một nguồn nhạc.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Source {
    pub peer: String,
    pub name: String,
    /// self (Tin nhắn đã lưu) | user | group | channel
    pub kind: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
    #[serde(default)]
    pub photo_dc: Option<i32>,
    /// Hiện trong «Thư viện» (người dùng có thể ẩn một nguồn — vd. nhóm chat nhiều tệp ghi âm).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Nguồn quét riêng (Tin nhắn đã lưu, kênh công khai thêm bằng @tên / link — chưa vào kênh thì quét toàn tài khoản không thấy).
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub added: i64,
    /// Lần quét riêng xong gần nhất (Unix giây, 0 = chưa).
    #[serde(default)]
    pub synced: i64,
    /// Nhóm có chủ đề (diễn đàn).
    #[serde(default)]
    pub forum: bool,
    /// Các chủ đề (lấy lần gần nhất) — tên hiện ở danh sách chủ đề.
    #[serde(default)]
    pub topics: Vec<Topic>,
}

/// Một chủ đề của nhóm diễn đàn.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Topic {
    pub id: i32,
    pub title: String,
    /// Màu biểu tượng (RGB) Telegram gán cho chủ đề.
    #[serde(default)]
    pub color: i32,
}

fn yes() -> bool {
    true
}

/// Chỉ mục một nguồn: các bài (tin mới → cũ).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SourceIndex {
    pub peer: String,
    /// Đã quét riêng nguồn này tới tin đầu tiên (Tin nhắn đã lưu, kênh công khai thêm tay). Nguồn có từ quét toàn tài khoản
    /// thì theo trạng thái quét chung (`sync.json`).
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub songs: Vec<Song>,
}

impl SourceIndex {
    /// Gộp bài (bài đã có thì cập nhật — tin có thể đã bị sửa tên); giữ thứ tự tin mới → cũ. Trả số bài mới.
    pub fn merge(&mut self, songs: Vec<Song>) -> usize {
        let mut pos: HashMap<i32, usize> = self.songs.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut n = 0;
        let mut sorted = true;
        for s in songs {
            match pos.get(&s.id) {
                Some(&i) => self.songs[i] = s,
                None => {
                    if self.songs.last().is_some_and(|l| l.id < s.id) {
                        sorted = false;
                    }
                    pos.insert(s.id, self.songs.len());
                    self.songs.push(s);
                    n += 1;
                }
            }
        }
        if !sorted {
            self.songs.sort_by(|a, b| b.id.cmp(&a.id));
        }
        n
    }

    pub fn has(&self, id: i32) -> bool {
        self.songs.iter().any(|s| s.id == id)
    }
}

/// Cách xếp.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sort {
    /// Mới nhất trước.
    Date,
    /// Tên A–Z.
    Title,
    /// Ca sĩ A–Z (rồi tên).
    Artist,
}

impl Sort {
    pub fn parse(s: &str) -> Sort {
        match s {
            "title" => Sort::Title,
            "artist" => Sort::Artist,
            _ => Sort::Date,
        }
    }
}

/// Bài trong bộ nhớ: bài + chữ bỏ dấu để tìm, khoá xếp.
struct Entry {
    song: Song,
    /// Nguồn đang bật và không trùng tệp với bài mới hơn của nguồn đang bật khác → có trong «mọi nguồn».
    shown: bool,
    /// tên + ca sĩ + tên tệp, bỏ dấu (tìm).
    hay: String,
    tkey: String,
    akey: String,
}

fn rank(c: char) -> u8 {
    if c == '#' { 27 } else { c as u8 - b'A' + 1 }
}

pub struct Library {
    dir: PathBuf,
    sources: Vec<Source>,
    idx: HashMap<String, SourceIndex>,
    /// Bài của mọi nguồn (mới → cũ) — dựng lại khi `dirty`.
    all: Vec<Entry>,
    dirty: bool,
    /// Thứ tự xếp đã tính: (nguồn hoặc "", cách xếp) → chỉ số trong `all`.
    orders: HashMap<(String, Sort), Vec<u32>>,
    /// Đổi mỗi khi thư viện đổi (app so để biết trang đã lấy còn đúng không).
    pub generation: u64,
    /// Nguồn có bài mới chưa ghi xuống đĩa (quét gộp nhiều trang rồi mới ghi — kênh lớn ghi lại cả tệp mỗi lần).
    unsaved: HashSet<String>,
}

impl Library {
    /// Đọc `sources.json` + `library/*.json` (hỏng thì bỏ qua — quét lại được).
    pub fn open(data_dir: &Path) -> Library {
        let dir = data_dir.join("library");
        let _ = std::fs::create_dir_all(&dir);
        let sources: Vec<Source> = std::fs::read(data_dir.join("sources.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut idx = HashMap::new();
        for s in &sources {
            if let Some(i) = std::fs::read(dir.join(format!("{}.json", s.peer))).ok().and_then(|b| serde_json::from_slice::<SourceIndex>(&b).ok()) {
                idx.insert(s.peer.clone(), i);
            }
        }
        Library { dir, sources, idx, all: Vec::new(), dirty: true, orders: HashMap::new(), generation: 1, unsaved: HashSet::new() }
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.orders.clear();
        self.generation += 1;
    }

    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    pub fn source(&self, peer: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.peer == peer)
    }

    pub fn index(&self, peer: &str) -> Option<&SourceIndex> {
        self.idx.get(peer)
    }

    /// Số bài đã quét của một nguồn.
    pub fn indexed(&self, peer: &str) -> usize {
        self.idx.get(peer).map(|i| i.songs.len()).unwrap_or(0)
    }

    fn save_sources(&self) -> std::io::Result<()> {
        let path = self.dir.parent().unwrap_or(&self.dir).join("sources.json");
        write_atomic(&path, &serde_json::to_vec(&self.sources).unwrap_or_default())
    }

    /// Thêm / cập nhật nguồn (tên, ảnh mới; giữ `enabled`, `manual`, `added`, `synced` cũ nếu đã có).
    pub fn upsert_source(&mut self, mut s: Source) -> std::io::Result<()> {
        if let Some(old) = self.sources.iter_mut().find(|x| x.peer == s.peer) {
            s.enabled = old.enabled;
            s.manual = old.manual || s.manual;
            s.added = old.added;
            s.synced = old.synced;
            if s.topics.is_empty() {
                s.topics = std::mem::take(&mut old.topics);
            }
            *old = s;
        } else {
            self.sources.push(s);
        }
        self.changed();
        self.save_sources()
    }

    /// Gộp bài vào chỉ mục của nguồn (tạo nguồn nếu chưa có, bằng `make`). Chỉ mục chưa ghi xuống đĩa — gọi `flush`. Trả số
    /// bài mới.
    pub fn add_songs(&mut self, peer: &str, songs: Vec<Song>, make: impl FnOnce() -> Source) -> std::io::Result<usize> {
        if songs.is_empty() {
            return Ok(0);
        }
        if self.source(peer).is_none() {
            let mut s = make();
            s.peer = peer.to_string();
            self.sources.push(s);
            self.save_sources()?;
        }
        let i = self.idx.entry(peer.to_string()).or_insert_with(|| SourceIndex { peer: peer.to_string(), ..Default::default() });
        let n = i.merge(songs);
        self.unsaved.insert(peer.to_string());
        self.changed();
        Ok(n)
    }

    /// Ghi các chỉ mục còn chưa ghi.
    pub fn flush(&mut self) -> std::io::Result<()> {
        let mut todo: Vec<String> = std::mem::take(&mut self.unsaved).into_iter().collect();
        while let Some(p) = todo.pop() {
            if let Some(i) = self.idx.get(&p) {
                if let Err(e) = write_atomic(&self.dir.join(format!("{p}.json")), &serde_json::to_vec(i).unwrap_or_default()) {
                    // Lỗi (đầy đĩa…): nguồn này và các nguồn chưa kịp ghi để lần sau ghi lại — trước đây bị bỏ khỏi danh sách cần ghi
                    self.unsaved.insert(p);
                    self.unsaved.extend(todo);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Lấy ra các nguồn có bài chưa ghi: (khoá nguồn, đường dẫn, nội dung) — để ghi ở luồng blocking (`Core::flush_lib`), không
    /// giữ luồng duy nhất của lõi (cổng phát, mọi lệnh) trong lúc ghi + sync cả tệp chỉ mục vài MB của kênh lớn. Ghi lỗi thì
    /// `mark_unsaved`.
    pub fn take_unsaved(&mut self) -> Vec<(String, PathBuf, Vec<u8>)> {
        let peers = std::mem::take(&mut self.unsaved);
        peers
            .into_iter()
            .filter_map(|p| {
                let i = self.idx.get(&p)?;
                let bytes = serde_json::to_vec(i).unwrap_or_default();
                Some((p.clone(), self.dir.join(format!("{p}.json")), bytes))
            })
            .collect()
    }

    /// Ghi lại sau (lần ghi trước lỗi).
    pub fn mark_unsaved(&mut self, peers: impl IntoIterator<Item = String>) {
        self.unsaved.extend(peers);
    }

    /// Đánh dấu nguồn đã quét riêng xong.
    pub fn set_complete(&mut self, peer: &str, complete: bool) {
        if let Some(i) = self.idx.get_mut(peer) {
            i.complete = complete;
            self.unsaved.insert(peer.to_string());
        }
    }

    pub fn update_source(&mut self, peer: &str, f: impl FnOnce(&mut Source)) -> std::io::Result<()> {
        if let Some(s) = self.sources.iter_mut().find(|x| x.peer == peer) {
            let was = s.enabled;
            f(s);
            if was != s.enabled {
                self.changed();
            }
            return self.save_sources();
        }
        Ok(())
    }

    /// Gỡ nguồn khỏi thư viện (xoá cả chỉ mục).
    pub fn remove_source(&mut self, peer: &str) -> std::io::Result<()> {
        self.sources.retain(|s| s.peer != peer);
        self.idx.remove(peer);
        let _ = std::fs::remove_file(self.dir.join(format!("{peer}.json")));
        self.changed();
        self.save_sources()
    }

    /// Thay chỉ mục của nguồn và ghi xuống đĩa.
    pub fn put_index(&mut self, i: SourceIndex) -> std::io::Result<()> {
        self.unsaved.remove(&i.peer);
        let path = self.dir.join(format!("{}.json", i.peer));
        let bytes = serde_json::to_vec(&i).unwrap_or_default();
        self.idx.insert(i.peer.clone(), i);
        self.changed();
        write_atomic(&path, &bytes)
    }

    /// Xoá bài khỏi chỉ mục (tin đã bị xoá trên Telegram). `peer` None = tìm ở mọi nguồn không phải kênh (Telegram không báo
    /// chat của tin bị xoá ở chat riêng / nhóm thường). Trả số bài đã xoá.
    pub fn remove_songs(&mut self, peer: Option<&str>, ids: &[i32]) -> usize {
        let ids: HashSet<i32> = ids.iter().copied().collect();
        let mut n = 0;
        let mut touched = Vec::new();
        for (p, i) in self.idx.iter_mut() {
            let hit = match peer {
                Some(x) => x == p,
                // Kênh / siêu nhóm theo cách đánh số của Bot API: −(10¹² + id). So theo chuỗi «-100…» thì nhóm thường có id bắt
                // đầu bằng 100 (khoá «-1001234567») bị coi là kênh → tin bị xoá ở đó không bao giờ được bỏ khỏi thư viện
                None => !p.parse::<i64>().is_ok_and(|v| v <= -1_000_000_000_000),
            };
            if !hit {
                continue;
            }
            let before = i.songs.len();
            i.songs.retain(|s| !ids.contains(&s.id));
            if i.songs.len() != before {
                n += before - i.songs.len();
                touched.push(p.clone());
            }
        }
        // Ghi qua flush (bên gọi): ghi thẳng ở đây có thể bị một lần ghi cũ hơn đang chạy ở luồng khác đè lên
        self.unsaved.extend(touched);
        if n > 0 {
            self.changed();
        }
        n
    }

    fn rebuild(&mut self) {
        if !self.dirty {
            return;
        }
        let on: HashSet<&str> = self.sources.iter().filter(|s| s.enabled).map(|s| s.peer.as_str()).collect();
        let mut songs: Vec<&Song> = Vec::new();
        for s in &self.sources {
            if let Some(i) = self.idx.get(&s.peer) {
                songs.extend(i.songs.iter());
            }
        }
        // Mới trước: bài trùng tệp (chuyển tiếp sang nhiều chat) chỉ hiện bản mới nhất trong «mọi nguồn»
        songs.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let mut seen: HashSet<i64> = HashSet::new();
        let mut all = Vec::with_capacity(songs.len());
        for s in songs {
            let shown = on.contains(s.peer.as_str()) && (s.doc == 0 || seen.insert(s.doc));
            let hay = text::fold(&format!("{} {} {}", s.title, s.artist, s.name));
            all.push(Entry { song: s.clone(), shown, hay, tkey: text::sort_key(&s.title), akey: text::sort_key(&s.artist) });
        }
        self.all = all;
        self.dirty = false;
    }

    /// Thứ tự xếp (đệm) của các bài thuộc `source` ("" = mọi nguồn đang bật).
    fn order(&mut self, source: &str, sort: Sort) -> &Vec<u32> {
        self.rebuild();
        let k = (source.to_string(), sort);
        if !self.orders.contains_key(&k) {
            let all = &self.all;
            // «Mọi nguồn»: nguồn đang bật, lọc trùng; một nguồn: mọi bài của nguồn đó
            let mut v: Vec<u32> = (0..all.len() as u32)
                .filter(|&i| if source.is_empty() { all[i as usize].shown } else { all[i as usize].song.peer == source })
                .collect();
            match sort {
                Sort::Date => {} // `all` đã xếp mới → cũ
                Sort::Title => v.sort_by(|&a, &b| {
                    let (x, y) = (&all[a as usize], &all[b as usize]);
                    (rank(text::letter(&x.tkey)), &x.tkey, &x.akey).cmp(&(rank(text::letter(&y.tkey)), &y.tkey, &y.akey))
                }),
                Sort::Artist => v.sort_by(|&a, &b| {
                    let (x, y) = (&all[a as usize], &all[b as usize]);
                    (rank(text::letter(&x.akey)), &x.akey, &x.tkey).cmp(&(rank(text::letter(&y.akey)), &y.akey, &y.tkey))
                }),
            }
            self.orders.insert(k.clone(), v);
        }
        &self.orders[&k]
    }

    /// Tìm / lọc / xếp rồi lấy một trang. `keep`: chỉ giữ bài thoả (bài đã tải…). Trả (tổng, trang, mục chữ cái
    /// [{t, i (vị trí đầu), n}] — chỉ khi xếp theo tên / ca sĩ).
    pub fn query(
        &mut self,
        q: &str,
        source: &str,
        sort: Sort,
        offset: usize,
        limit: usize,
        keep: &dyn Fn(&Song) -> bool,
    ) -> (usize, Vec<Song>, Vec<Value>) {
        let words = text::query_words(q);
        let order = self.order(source, sort).clone();
        let all = &self.all;
        let hits: Vec<u32> =
            order.into_iter().filter(|&i| text::matches(&all[i as usize].hay, &words) && keep(&all[i as usize].song)).collect();
        let page: Vec<Song> = hits.iter().skip(offset).take(limit).map(|&i| all[i as usize].song.clone()).collect();
        let mut sections = Vec::new();
        if sort != Sort::Date {
            let mut cur: Option<(char, usize, usize)> = None;
            for (pos, &i) in hits.iter().enumerate() {
                let e = &all[i as usize];
                let l = text::letter(if sort == Sort::Title { &e.tkey } else { &e.akey });
                match &mut cur {
                    Some((c, _, n)) if *c == l => *n += 1,
                    _ => {
                        if let Some((c, start, n)) = cur.take() {
                            sections.push(json!({"t": c.to_string(), "i": start, "n": n}));
                        }
                        cur = Some((l, pos, 1));
                    }
                }
            }
            if let Some((c, start, n)) = cur {
                sections.push(json!({"t": c.to_string(), "i": start, "n": n}));
            }
        }
        (hits.len(), page, sections)
    }

    /// Khoá của mọi bài thoả điều kiện (hàng chờ phát cả danh sách).
    pub fn keys(&mut self, q: &str, source: &str, sort: Sort, keep: &dyn Fn(&Song) -> bool) -> Vec<String> {
        let words = text::query_words(q);
        let order = self.order(source, sort).clone();
        let all = &self.all;
        order
            .into_iter()
            .map(|i| &all[i as usize])
            .filter(|e| text::matches(&e.hay, &words) && keep(&e.song))
            .map(|e| e.song.key())
            .collect()
    }

    /// Số bài theo chủ đề của một nguồn (0 — không thuộc chủ đề nào — tính vào General, id 1).
    pub fn topic_counts(&self, peer: &str) -> HashMap<i32, usize> {
        let mut m = HashMap::new();
        if let Some(i) = self.idx.get(peer) {
            for s in &i.songs {
                *m.entry(if s.topic == 0 { 1 } else { s.topic }).or_insert(0) += 1;
            }
        }
        m
    }

    /// Tìm bài theo khoá (ở mọi nguồn, kể cả nguồn đang tắt).
    pub fn song(&self, key: &str) -> Option<&Song> {
        let (peer, id) = split_key(key)?;
        self.idx.get(&peer)?.songs.iter().find(|s| s.id == id)
    }

    /// Tổng số bài (đã lọc trùng) của các nguồn đang bật.
    pub fn total(&mut self) -> usize {
        self.rebuild();
        self.all.iter().filter(|e| e.shown).count()
    }
}

/// Ghi tệp tạm rồi đổi tên (không để tệp dở khi tắt app giữa chừng).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.part");
    {
        // sync trước khi đổi tên: máy treo / mất điện ngay sau đó (iOS cũ + tweak) thì còn tệp cũ hoặc tệp mới đủ, không phải tệp
        // rỗng (sources.json rỗng = mất cả thư viện)
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(peer: &str, id: i32, title: &str, artist: &str, doc: i64) -> Song {
        Song {
            peer: peer.into(),
            id,
            title: title.into(),
            artist: artist.into(),
            dur: 200,
            size: 5 << 20,
            mime: "audio/mpeg".into(),
            name: format!("{title}.mp3"),
            ext: "mp3".into(),
            date: 1_700_000_000 + id as i64,
            doc,
            thumb: false,
            // bài id chẵn thuộc chủ đề 7, lẻ: không thuộc chủ đề nào (General)
            topic: if id % 2 == 0 { 7 } else { 0 },
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gmu-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lib_with_two_sources(d: &Path) -> Library {
        let mut lib = Library::open(d);
        for p in ["1", "-1001"] {
            lib.upsert_source(Source { peer: p.into(), name: format!("N{p}"), kind: "channel".into(), enabled: true, ..Default::default() })
                .unwrap();
        }
        let mut a = SourceIndex { peer: "1".into(), ..Default::default() };
        a.merge(vec![song("1", 1, "Lạc Trôi", "Sơn Tùng M-TP", 11), song("1", 2, "Đi Về Nhà", "Đen", 12)]);
        let mut b = SourceIndex { peer: "-1001".into(), ..Default::default() };
        // bài 5 trùng tệp với bài 1 của nguồn kia (chuyển tiếp)
        b.merge(vec![song("-1001", 5, "Lạc Trôi", "Sơn Tùng M-TP", 11), song("-1001", 6, "Anh Ơi Ở Lại", "Chi Pu", 13),
                         song("-1001", 7, "2 Phút Hơn", "Pháo", 14)]);
        lib.put_index(a).unwrap();
        lib.put_index(b).unwrap();
        lib
    }

    #[test]
    fn index_merge() {
        let mut i = SourceIndex { peer: "1".into(), ..Default::default() };
        assert_eq!(i.merge(vec![song("1", 12, "b", "", 2), song("1", 10, "a", "", 1)]), 2);
        assert_eq!(i.merge(vec![song("1", 8, "c", "", 3), song("1", 10, "a2", "", 1)]), 1);
        assert_eq!(i.merge(vec![song("1", 15, "d", "", 4)]), 1);
        assert_eq!(i.songs.iter().map(|s| s.id).collect::<Vec<_>>(), vec![15, 12, 10, 8]);
        assert_eq!(i.songs[2].title, "a2"); // tin sửa tên: cập nhật
        assert!(i.has(8) && !i.has(9));
    }

    #[test]
    fn query_dedupe_sort_sections_search() {
        let d = tmpdir("q");
        let mut lib = lib_with_two_sources(&d);
        assert_eq!(lib.total(), 4); // trùng tệp bị lọc
        let all = |_: &Song| true;
        let (n, page, _) = lib.query("", "", Sort::Date, 0, 10, &all);
        assert_eq!(n, 4);
        assert_eq!(page[0].id, 7); // mới nhất trước
        assert!(page.iter().any(|s| s.key() == "-1001_5") && !page.iter().any(|s| s.key() == "1_1"));
        let (_, page, secs) = lib.query("", "", Sort::Title, 0, 10, &all);
        let titles: Vec<&str> = page.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Anh Ơi Ở Lại", "Đi Về Nhà", "Lạc Trôi", "2 Phút Hơn"]);
        assert_eq!(secs, vec![json!({"t": "A", "i": 0, "n": 1}), json!({"t": "D", "i": 1, "n": 1}), json!({"t": "L", "i": 2, "n": 1}),
                              json!({"t": "#", "i": 3, "n": 1})]);
        let (n, page, _) = lib.query("son tung", "", Sort::Date, 0, 10, &all);
        assert_eq!((n, page[0].title.as_str()), (1, "Lạc Trôi"));
        let (n, _, _) = lib.query("", "1", Sort::Date, 0, 10, &all);
        assert_eq!(n, 2); // xem riêng một nguồn: đủ bài của nguồn, kể cả bài trùng tệp với nguồn khác
        // Tắt nguồn → bài của nguồn biến mất; mở lại từ đĩa vẫn còn
        lib.update_source("-1001", |s| s.enabled = false).unwrap();
        assert_eq!(lib.total(), 2);
        let mut again = Library::open(&d);
        assert_eq!(again.total(), 2);
        assert_eq!(again.song("-1001_6").map(|s| s.title.clone()), Some("Anh Ơi Ở Lại".into()));
        assert_eq!(again.keys("", "", Sort::Date, &all), vec!["1_2", "1_1"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn remove_songs_by_peer_or_any() {
        let d = tmpdir("rm");
        let mut lib = lib_with_two_sources(&d);
        assert_eq!(lib.remove_songs(Some("-1001"), &[6]), 1);
        // Tin bị xoá không kèm chat: chỉ nguồn không phải kênh. Kênh = −(10¹² + id); «-1001» là nhóm thường (id 1001 bắt đầu
        // bằng 100 — trước đây bị coi là kênh)
        let ch = "-1001234567890";
        lib.upsert_source(Source { peer: ch.into(), name: "K".into(), kind: "channel".into(), enabled: true, ..Default::default() })
            .unwrap();
        let mut c = SourceIndex { peer: ch.into(), ..Default::default() };
        c.merge(vec![song(ch, 7, "Kênh", "", 21)]);
        lib.put_index(c).unwrap();
        assert_eq!(lib.remove_songs(None, &[2, 7]), 2);
        assert!(lib.song("1_2").is_none() && lib.song("-1001_7").is_none());
        assert!(lib.song("-1001234567890_7").is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn topic_counts_general() {
        let d = tmpdir("topic");
        let lib = lib_with_two_sources(&d);
        let c = lib.topic_counts("-1001");
        assert_eq!((c.get(&1), c.get(&7)), (Some(&2), Some(&1))); // 5, 7 → General; 6 → chủ đề 7
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn keys_split() {
        assert_eq!(split_key("-1001_5"), Some(("-1001".into(), 5)));
        assert_eq!(split_key("x_5"), None);
    }
}
