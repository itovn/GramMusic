//! Phiên đăng nhập lưu ra một tệp JSON (bản thử, trước khi có SQLite): DC chính, auth key từng DC, trạng thái cập nhật,
//! danh bạ peer (id + access_hash). Bọc `MemorySession` của grammers; tệp ghi kiểu «tệp tạm rồi đổi tên», quyền 0600.
//!
//! Auth key và DC chính ghi ngay khi đổi (mất là phải đăng nhập lại); peer và trạng thái cập nhật ghi khi gọi `save`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use grammers_session::storages::MemorySession;
use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use grammers_session::{BoxFuture, Session, SessionData};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Snapshot {
    home_dc: i32,
    dc_options: Vec<DcOption>,
    updates_state: UpdatesState,
    peers: Vec<PeerInfo>,
}

struct State {
    home_dc: i32,
    dc_options: HashMap<i32, DcOption>,
    updates_state: UpdatesState,
    peers: HashMap<PeerId, PeerInfo>,
    dirty: bool,
}

pub struct FileSession {
    mem: MemorySession,
    path: PathBuf,
    state: Mutex<State>,
}

impl FileSession {
    /// Mở tệp phiên (chưa có thì phiên trống — chưa đăng nhập).
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut data = SessionData::default();
        if path.exists() {
            let snap: Snapshot = serde_json::from_slice(&std::fs::read(&path)?)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("tệp phiên hỏng: {e}")))?;
            data.home_dc = snap.home_dc;
            for o in snap.dc_options {
                data.dc_options.insert(o.id, o);
            }
            data.updates_state = snap.updates_state;
            for p in snap.peers {
                data.peer_infos.insert(p.id(), p);
            }
        }
        let state = State {
            home_dc: data.home_dc,
            dc_options: data.dc_options.clone(),
            updates_state: data.updates_state.clone(),
            peers: data.peer_infos.clone(),
            dirty: false,
        };
        Ok(Self { mem: MemorySession::from(data), path, state: Mutex::new(state) })
    }

    /// Ghi tệp nếu có thay đổi chưa ghi.
    pub fn save(&self) -> io::Result<()> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.dirty {
            self.write(&st)?;
            st.dirty = false;
        }
        Ok(())
    }

    fn write(&self, st: &State) -> io::Result<()> {
        let snap = Snapshot {
            home_dc: st.home_dc,
            dc_options: st.dc_options.values().cloned().collect(),
            updates_state: st.updates_state.clone(),
            peers: st.peers.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec(&snap).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)
    }

    /// Đổi trạng thái; `now` = ghi tệp ngay.
    fn update(&self, now: bool, f: impl FnOnce(&mut State)) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut st);
        st.dirty = true;
        if now {
            match self.write(&st) {
                Ok(()) => st.dirty = false,
                Err(e) => eprintln!("không ghi được tệp phiên {}: {e}", self.path.display()),
            }
        }
    }
}

type Error = <MemorySession as Session>::Error;

impl Session for FileSession {
    type Error = Error;

    fn home_dc_id(&self) -> Result<i32, Error> {
        self.mem.home_dc_id()
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.mem.set_home_dc_id(dc_id).await?;
            self.update(true, |st| st.home_dc = dc_id);
            Ok(())
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Error> {
        self.mem.dc_option(dc_id)
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Error>> {
        let o = dc_option.clone();
        Box::pin(async move {
            self.mem.set_dc_option(&o).await?;
            self.update(true, |st| {
                st.dc_options.insert(o.id, o);
            });
            Ok(())
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Error>> {
        self.mem.peer(peer)
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let id = peer.id();
            self.mem.cache_peer(peer).await?;
            if let Some(merged) = self.mem.peer(id).await? {
                self.update(false, |st| {
                    st.peers.insert(id, merged);
                });
            }
            Ok(())
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Error>> {
        self.mem.updates_state()
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.mem.set_update_state(update).await?;
            let s = self.mem.updates_state().await?;
            self.update(false, |st| st.updates_state = s);
            Ok(())
        })
    }
}
