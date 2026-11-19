use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .expect("folder installer harus berada di dalam folder proyek")
        .to_path_buf();

    let candidates = [
        root.join("dist").join("replicator-agent.exe"),
        root.join("agent")
            .join("target")
            .join("release")
            .join("replicator-agent.exe"),
        root.join("agent")
            .join("target")
            .join("x86_64-pc-windows-gnu")
            .join("release")
            .join("replicator-agent.exe"),
        root.join("agent")
            .join("target")
            .join("x86_64-pc-windows-msvc")
            .join("release")
            .join("replicator-agent.exe"),
    ];

    let agent = candidates
        .iter()
        .filter(|path| path.is_file())
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
        })
        .unwrap_or_else(|| {
            panic!(
                "replicator-agent.exe belum ada. Jalankan scripts\\build-agent.ps1 dulu.\n\
                 Dicari di: {}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target = out_dir.join("replicator-agent.exe");
    fs::copy(agent, &target).expect("gagal menyalin agent ke folder build installer");

    // Versi agent dibaca dari agent/Cargo.toml agar installer dan readme selalu
    // menampilkan versi yang benar-benar disematkan.
    let agent_manifest = root.join("agent").join("Cargo.toml");
    let agent_version = fs::read_to_string(&agent_manifest)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("version")?.trim();
                let value = rest.strip_prefix('=')?.trim();
                Some(value.trim_matches('"').to_string())
            })
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=AGENT_VERSION={}", agent_version);

    println!("cargo:rerun-if-changed={}", agent.display());
    println!("cargo:rerun-if-changed={}", agent_manifest.display());
    println!("cargo:rerun-if-changed=build.rs");
}
