use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_MLX");
    if env::var_os("CARGO_FEATURE_MLX").is_none() {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "macos",
        "MLX requires macOS"
    );
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ARCH").unwrap(),
        "aarch64",
        "MLX requires Apple Silicon"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let runtime = root.join(".tooling/mlx-runtime");
    let provenance = runtime.join("build-provenance.json");
    println!("cargo:rerun-if-changed={}", provenance.display());
    let bytes = fs::read(&provenance)
        .expect("Run node scripts/mlx-runtime.mjs before building with --features mlx");
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (source, field) in [
        ("native/mlx/Package.resolved", "packageLockSha256"),
        (
            "patches/mlx-audio-swift/completion-report.patch",
            "patchSha256",
        ),
        ("native/mlx/Sources/VesperMLX/Bridge.swift", "bridgeSha256"),
        (
            "native/mlx/Sources/VesperMLX/Live.swift",
            "liveBridgeSha256",
        ),
        (
            "native/mlx/Sources/VesperMLX/GraniteSpeech5CTC.swift",
            "graniteBridgeSha256",
        ),
        (
            "native/mlx/Sources/VesperMLX/Phonon2.swift",
            "phonon2BridgeSha256",
        ),
        (
            "patches/mlx-audio-swift/live-session.patch",
            "livePatchSha256",
        ),
        (
            "patches/mlx-audio-swift/parakeet-load-hook.patch",
            "loadHookPatchSha256",
        ),
        (
            "native/mlx/Sources/PrepareTokenizer/main.swift",
            "tokenizerSourceSha256",
        ),
    ] {
        let source = root.join(source);
        println!("cargo:rerun-if-changed={}", source.display());
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(source).unwrap())),
            metadata[field].as_str().unwrap(),
            "MLX source changed: rebuild native runtime"
        );
    }
    let mut artifacts = Vec::new();
    for (name, field) in [
        ("libVesperMLX.dylib", "librarySha256"),
        ("mlx.metallib", "metalSha256"),
        ("prepare-qwen-tokenizer", "tokenizerToolSha256"),
    ] {
        let path = runtime.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(path).expect("missing packaged MLX runtime file");
        let hash = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            hash,
            metadata[field].as_str().unwrap(),
            "MLX build output changed"
        );
        artifacts.push(serde_json::json!({"name":name,"relativePath":name,"url":"builtin:mlx-runtime","revision":metadata["runtimeRevision"],"sha256":hash,"sizeBytes":bytes.len(),"role":"asr_runtime","license":"Application license unselected; dependencies retain upstream licenses"}));
    }
    println!(
        "cargo:rustc-env=VESPER_MLX_BUILD_DIRECTORY={}",
        runtime.canonicalize().unwrap().display()
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        out.join("mlx-runtime.json"),
        serde_json::to_vec(&artifacts).unwrap(),
    )
    .unwrap();
}
