//! Playlist, Yêu thích, Nghe gần đây (`playlists.json`). Mỗi mục giữ khoá bài + bản chép tên / ca sĩ / thời lượng, nên vẫn
//! hiện được khi nguồn chứa bài đã bị gỡ khỏi thư viện (bài khi đó không phát được cho tới khi thêm lại nguồn).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{write_atomic, Song};

/// Giữ tối đa chừng này bài «Nghe gần đây».
const HISTORY_MAX: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub dur: u32,
}

impl From<&Song> for Item {
    fn from(s: &Song) -> Item {
        Item { key: s.key(), title: s.title.clone(), artist: s.artist.clone(), dur: s.dur }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Playlist {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    #[serde(default)]
    next_id: u32,
    #[serde(default)]
    lists: Vec<Playlist>,
    #[serde(default)]
    fav: Vec<Item>,
    #[serde(default)]
    history: Vec<Item>,
}

pub struct Playlists {
    path: PathBuf,
    d: Data,
    favs: HashSet<String>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Playlists {
    pub fn open(data_dir: &Path) -> Playlists {
        let path = data_dir.join("playlists.json");
        let d: Data = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let favs = d.fav.iter().map(|i| i.key.clone()).collect();
        Playlists { path, d, favs }
    }

    fn save(&self) -> std::io::Result<()> {
        write_atomic(&self.path, &serde_json::to_vec(&self.d).unwrap_or_default())
    }

    pub fn lists(&self) -> &[Playlist] {
        &self.d.lists
    }

    pub fn get(&self, id: u32) -> Option<&Playlist> {
        self.d.lists.iter().find(|p| p.id == id)
    }

    pub fn create(&mut self, name: &str) -> std::io::Result<u32> {
        self.d.next_id = self.d.next_id.max(1);
        let id = self.d.next_id;
        self.d.next_id += 1;
        let t = now();
        self.d.lists.push(Playlist { id, name: name.trim().to_string(), items: Vec::new(), created: t, updated: t });
        self.save()?;
        Ok(id)
    }

    fn with(&mut self, id: u32, f: impl FnOnce(&mut Playlist)) -> std::io::Result<bool> {
        let Some(p) = self.d.lists.iter_mut().find(|p| p.id == id) else { return Ok(false) };
        f(p);
        p.updated = now();
        self.save()?;
        Ok(true)
    }

    pub fn rename(&mut self, id: u32, name: &str) -> std::io::Result<bool> {
        let name = name.trim().to_string();
        self.with(id, |p| p.name = name)
    }

    pub fn delete(&mut self, id: u32) -> std::io::Result<bool> {
        let before = self.d.lists.len();
        self.d.lists.retain(|p| p.id != id);
        self.save()?;
        Ok(before != self.d.lists.len())
    }

    /// Thêm bài vào cuối (bài đã có trong playlist thì bỏ qua). Trả số bài đã thêm.
    pub fn add(&mut self, id: u32, items: Vec<Item>) -> std::io::Result<usize> {
        let mut n = 0;
        self.with(id, |p| {
            let mut have: HashSet<String> = p.items.iter().map(|i| i.key.clone()).collect();
            for it in items {
                if have.insert(it.key.clone()) {
                    p.items.push(it);
                    n += 1;
                }
            }
        })?;
        Ok(n)
    }

    pub fn remove(&mut self, id: u32, keys: &[String]) -> std::io::Result<bool> {
        let ks: HashSet<&String> = keys.iter().collect();
        self.with(id, |p| p.items.retain(|i| !ks.contains(&i.key)))
    }

    /// Đổi chỗ một bài (kéo thả trong chế độ Sửa).
    pub fn move_item(&mut self, id: u32, from: usize, to: usize) -> std::io::Result<bool> {
        self.with(id, |p| {
            if from < p.items.len() {
                let it = p.items.remove(from);
                p.items.insert(to.min(p.items.len()), it);
            }
        })
    }

    pub fn is_fav(&self, key: &str) -> bool {
        self.favs.contains(key)
    }

    pub fn favs(&self) -> &[Item] {
        &self.d.fav
    }

    /// Bật / tắt Yêu thích; trả trạng thái mới. Bài mới thích lên đầu.
    pub fn set_fav(&mut self, item: Item, on: bool) -> std::io::Result<bool> {
        self.d.fav.retain(|i| i.key != item.key);
        self.favs.remove(&item.key);
        if on {
            self.favs.insert(item.key.clone());
            self.d.fav.insert(0, item);
        }
        self.save()?;
        Ok(on)
    }

    pub fn history(&self) -> &[Item] {
        &self.d.history
    }

    /// Ghi bài vừa nghe lên đầu «Nghe gần đây».
    pub fn played(&mut self, item: Item) -> std::io::Result<()> {
        self.d.history.retain(|i| i.key != item.key);
        self.d.history.insert(0, item);
        self.d.history.truncate(HISTORY_MAX);
        self.save()
    }

    pub fn clear_history(&mut self) -> std::io::Result<()> {
        self.d.history.clear();
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(k: &str) -> Item {
        Item { key: k.into(), title: format!("T{k}"), artist: String::new(), dur: 1 }
    }

    #[test]
    fn playlists_roundtrip() {
        let d = std::env::temp_dir().join(format!("gmu-pl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut p = Playlists::open(&d);
        let a = p.create(" Nhạc buồn ").unwrap();
        let b = p.create("Chạy bộ").unwrap();
        assert_ne!(a, b);
        assert_eq!(p.add(a, vec![it("1_1"), it("1_2"), it("1_1")]).unwrap(), 2);
        assert_eq!(p.add(a, vec![it("1_2"), it("1_3")]).unwrap(), 1);
        p.move_item(a, 2, 0).unwrap();
        p.remove(a, &["1_2".into()]).unwrap();
        p.set_fav(it("1_9"), true).unwrap();
        p.played(it("1_1")).unwrap();
        p.played(it("1_3")).unwrap();
        p.played(it("1_1")).unwrap();
        p.delete(b).unwrap();
        let q = Playlists::open(&d);
        assert_eq!(q.lists().len(), 1);
        assert_eq!(q.get(a).unwrap().name, "Nhạc buồn");
        assert_eq!(q.get(a).unwrap().items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["1_3", "1_1"]);
        assert!(q.is_fav("1_9") && !q.is_fav("1_1"));
        assert_eq!(q.history().iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["1_1", "1_3"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
