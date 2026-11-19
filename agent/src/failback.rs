//! Penanda mode darurat (failback) yang tahan restart.
//!
//! Saat operator mengaktifkan mode darurat, agent berhenti mereplikasi
//! master -> backup dan menunggu master hidup lagi. Setelah master terdeteksi
//! hidup, agent menjalankan full load backup -> master lalu kembali normal.
//!
//! Status "armed" disimpan sebagai file di folder config supaya tetap berlaku
//! walau agent atau komputer di-restart.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FailbackState {
    state_file: PathBuf,
    request_file: PathBuf,
}

impl FailbackState {
    pub fn new(state_file: PathBuf) -> FailbackState {
        let request_file = request_path(&state_file);
        FailbackState {
            state_file,
            request_file,
        }
    }

    pub fn state_file(&self) -> &Path {
        &self.state_file
    }

    pub fn is_armed(&self) -> bool {
        self.state_file.is_file()
    }

    /// Aktifkan mode darurat. Mengembalikan `Ok(false)` bila sudah aktif.
    pub fn arm(&self) -> Result<bool, String> {
        if self.is_armed() {
            return Ok(false);
        }
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "tidak bisa membuat folder penanda failback {}: {}",
                    parent.display(),
                    error
                )
            })?;
        }
        let content = format!(
            "armed_at={}\naction=wait_for_master\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        fs::write(&self.state_file, content).map_err(|error| {
            format!(
                "tidak bisa menulis penanda failback {}: {}",
                self.state_file.display(),
                error
            )
        })?;
        Ok(true)
    }

    /// Matikan mode darurat. Mengembalikan `Ok(true)` bila sebelumnya aktif.
    pub fn disarm(&self) -> Result<bool, String> {
        let existed = self.is_armed();
        if existed {
            fs::remove_file(&self.state_file).map_err(|error| {
                format!(
                    "tidak bisa menghapus penanda failback {}: {}",
                    self.state_file.display(),
                    error
                )
            })?;
        }
        // Permintaan sinkronisasi balik yang belum sempat diproses ikut dibatalkan.
        let _ = fs::remove_file(&self.request_file);
        Ok(existed)
    }

    /// Minta agent menjalankan satu siklus replikasi balik secepatnya.
    pub fn request_now(&self) -> Result<(), String> {
        if let Some(parent) = self.request_file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "tidak bisa membuat folder permintaan failback {}: {}",
                    parent.display(),
                    error
                )
            })?;
        }
        fs::write(
            &self.request_file,
            format!(
                "requested_at={}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            ),
        )
        .map_err(|error| {
            format!(
                "tidak bisa menulis permintaan failback {}: {}",
                self.request_file.display(),
                error
            )
        })
    }

    /// Ambil permintaan yang tertunda. File dihapus agar tidak dijalankan dua kali.
    pub fn take_request(&self) -> bool {
        if self.request_file.is_file() {
            let _ = fs::remove_file(&self.request_file);
            true
        } else {
            false
        }
    }
}

fn request_path(state_file: &Path) -> PathBuf {
    let parent = state_file.parent().unwrap_or_else(|| Path::new("."));
    let stem = state_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("failback");
    parent.join(format!("{}.now", stem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_request_and_disarm_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "replicator-failback-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let state = FailbackState::new(dir.join("failback.armed"));

        assert!(state.arm().unwrap());
        assert!(state.is_armed());
        assert!(!state.arm().unwrap(), "arm kedua tidak mengubah apa pun");
        assert!(!state.take_request());

        state.request_now().unwrap();
        assert!(state.take_request());
        assert!(!state.take_request(), "permintaan hanya diproses sekali");

        assert!(state.disarm().unwrap());
        assert!(!state.is_armed());
        assert!(!state.disarm().unwrap());

        let _ = fs::remove_dir_all(&dir);
    }
}
