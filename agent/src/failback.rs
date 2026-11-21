//! Penanda mode darurat (failback) yang tahan restart.
//!
//! Saat operator mengaktifkan mode darurat, agent berhenti mereplikasi
//! master -> backup dan menunggu master hidup lagi. Setelah master terdeteksi
//! hidup, agent membandingkan data backup dengan master lalu menyalin balik
//! tabel yang berbeda. Setelah itu agent masuk status **pending**: replikasi
//! maju tetap dijeda sampai operator memverifikasi data dan memilih
//! "Mulai replikasi maju".
//!
//! Status "armed" dan "pending" disimpan sebagai file di folder config supaya
//! tetap berlaku walau agent atau komputer di-restart.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FailbackState {
    state_file: PathBuf,
    request_file: PathBuf,
    pending_file: PathBuf,
}

impl FailbackState {
    pub fn new(state_file: PathBuf) -> FailbackState {
        let request_file = sibling_path(&state_file, "now");
        let pending_file = sibling_path(&state_file, "pending");
        FailbackState {
            state_file,
            request_file,
            pending_file,
        }
    }

    pub fn state_file(&self) -> &Path {
        &self.state_file
    }

    pub fn pending_file(&self) -> &Path {
        &self.pending_file
    }

    pub fn is_armed(&self) -> bool {
        self.state_file.is_file()
    }

    /// Replikasi balik selesai dan menunggu konfirmasi operator sebelum
    /// replikasi maju dijalankan lagi.
    pub fn is_pending(&self) -> bool {
        self.pending_file.is_file()
    }

    /// Aktifkan mode darurat. Mengembalikan `Ok(false)` bila sudah aktif.
    pub fn arm(&self) -> Result<bool, String> {
        if self.is_pending() {
            return Err(
                "replikasi balik masih berstatus pending; mulai replikasi maju terlebih dahulu"
                    .to_string(),
            );
        }
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

    /// Matikan mode darurat (termasuk status pending). Mengembalikan `Ok(true)`
    /// bila sebelumnya ada penanda yang dihapus.
    pub fn disarm(&self) -> Result<bool, String> {
        let existed = self.is_armed() || self.is_pending();
        let mut first_error: Option<String> = None;
        for (path, label) in [
            (&self.state_file, "penanda failback"),
            (&self.pending_file, "penanda pending"),
        ] {
            if path.is_file() {
                if let Err(error) = fs::remove_file(path) {
                    first_error.get_or_insert_with(|| {
                        format!(
                            "tidak bisa menghapus {} {}: {}",
                            label,
                            path.display(),
                            error
                        )
                    });
                }
            }
        }
        // Permintaan sinkronisasi balik yang belum sempat diproses ikut dibatalkan.
        let _ = fs::remove_file(&self.request_file);
        match first_error {
            Some(error) => Err(error),
            None => Ok(existed),
        }
    }

    /// Replikasi balik selesai: tulis penanda pending lalu lepas penanda armed.
    ///
    /// Penanda pending ditulis lebih dahulu supaya restart pada saat yang tidak
    /// tepat tidak membuat replikasi maju berjalan otomatis.
    pub fn mark_pending(&self, message: &str) -> Result<(), String> {
        if let Some(parent) = self.pending_file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "tidak bisa membuat folder penanda pending {}: {}",
                    parent.display(),
                    error
                )
            })?;
        }
        let message = message.replace(['\r', '\n'], " ");
        let content = format!(
            "pending_at={}\nmessage={}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            message
        );
        fs::write(&self.pending_file, content).map_err(|error| {
            format!(
                "tidak bisa menulis penanda pending {}: {}",
                self.pending_file.display(),
                error
            )
        })?;
        let _ = fs::remove_file(&self.request_file);
        fs::remove_file(&self.state_file).map_err(|error| {
            format!(
                "penanda pending sudah ditulis tetapi penanda darurat {} gagal dihapus: {}",
                self.state_file.display(),
                error
            )
        })?;
        Ok(())
    }

    /// Operator menyetujui data hasil replikasi balik: hapus status pending
    /// agar replikasi maju master -> backup berjalan lagi.
    pub fn resume_forward(&self) -> Result<bool, String> {
        let was_pending = self.is_pending();
        let mut first_error: Option<String> = None;
        for path in [&self.pending_file, &self.state_file] {
            if path.is_file() {
                if let Err(error) = fs::remove_file(path) {
                    first_error.get_or_insert_with(|| {
                        format!("tidak bisa menghapus {}: {}", path.display(), error)
                    });
                }
            }
        }
        let _ = fs::remove_file(&self.request_file);
        match first_error {
            Some(error) => Err(error),
            None => Ok(was_pending),
        }
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

fn sibling_path(state_file: &Path, extension: &str) -> PathBuf {
    let parent = state_file.parent().unwrap_or_else(|| Path::new("."));
    let stem = state_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("failback");
    parent.join(format!("{}.{}", stem, extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_for_test(name: &str) -> (FailbackState, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "replicator-failback-test-{}-{}-{:?}",
            std::process::id(),
            name,
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        (FailbackState::new(dir.join("failback.armed")), dir)
    }

    #[test]
    fn arm_request_and_disarm_roundtrip() {
        let (state, dir) = state_for_test("arm");
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

    #[test]
    fn pending_blocks_forward_until_resume() {
        let (state, dir) = state_for_test("pending");
        state.arm().unwrap();
        state.mark_pending("replikasi balik selesai").unwrap();

        assert!(!state.is_armed());
        assert!(state.is_pending());
        assert!(state.arm().is_err(), "pending harus diselesaikan dulu");

        assert!(state.resume_forward().unwrap());
        assert!(!state.is_pending());
        assert!(!state.resume_forward().unwrap());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disarm_also_clears_pending() {
        let (state, dir) = state_for_test("disarm-pending");
        state.arm().unwrap();
        state.mark_pending("selesai").unwrap();
        assert!(state.disarm().unwrap());
        assert!(!state.is_pending());

        let _ = fs::remove_dir_all(&dir);
    }
}
