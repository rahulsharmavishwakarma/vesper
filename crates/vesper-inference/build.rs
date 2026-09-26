use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn main() {
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    let patch = Path::new("../../patches/sherpa-onnx/decode-report.patch");
    println!("cargo:rerun-if-changed={}", patch.display());
    let directory = env::var("SHERPA_ONNX_LIB_DIR").expect(
        "Build the patched runtime with `make sherpa-runtime`, then source scripts/dev-env.sh before cargo commands",
    );
    let manifest_path = Path::new(&directory).join("vesper-runtime.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .expect("Missing patched-runtime provenance; run make sherpa-runtime"),
    )
    .expect("Invalid native runtime manifest");
    let patch_hash = sha256(&fs::read(patch).expect("Missing native reporting patch"));
    assert_eq!(
        manifest["patchSha256"].as_str(),
        Some(patch_hash.as_str()),
        "Native patch changed; run make sherpa-runtime"
    );
    assert_eq!(
        manifest["revision"],
        "11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf"
    );
    let libraries = manifest["libraries"]
        .as_object()
        .expect("Missing library hashes");
    assert_eq!(libraries.len(), 13, "Incomplete native runtime build");
    for (filename, expected) in libraries {
        assert_eq!(Path::new(filename).components().count(), 1);
        assert!(filename.starts_with("lib") && filename.ends_with(".a"));
        let file = Path::new(&directory).join(filename);
        println!("cargo:rerun-if-changed={}", file.display());
        assert_eq!(
            sha256(&fs::read(&file).expect("Missing native library")),
            expected.as_str().expect("Invalid hash"),
            "Native library integrity failed: {filename}"
        );
    }
    for (key, name) in [
        ("buildId", "VESPER_SHERPA_BUILD_ID"),
        ("patchSha256", "VESPER_SHERPA_PATCH_SHA256"),
    ] {
        let value = manifest[key]
            .as_str()
            .expect("Missing native build identity");
        assert!(value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
        println!("cargo:rustc-env={name}={value}");
    }
}
