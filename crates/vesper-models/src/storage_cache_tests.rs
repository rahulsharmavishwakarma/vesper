use super::*;

const DATA: &[u8] = b"public synthetic model fixture";
const REVISION: &str = "1234567890abcdef1234567890abcdef12345678";

fn fixture() -> Artifact {
    Artifact {
        name: "weights.gguf".into(),
        relative_path: "model/weights.gguf".into(),
        url: format!("https://huggingface.co/test/model/resolve/{REVISION}/nested/weights.gguf"),
        revision: REVISION.into(),
        sha256: format!("{:x}", Sha256::digest(DATA)),
        size_bytes: DATA.len() as u64,
        role: "rewriter".into(),
        model_id: None,
        platform: None,
        backend: None,
        archive: None,
        engine: None,
        quantization: None,
        license: "fixture".into(),
    }
}
fn snapshot(cache: &Path) -> PathBuf {
    cache.join(format!(
        "models--test--model/snapshots/{REVISION}/nested/weights.gguf"
    ))
}

#[test]
fn configured_cache_reaches_catalog_installer_and_safe_removal() {
    // Exercise the public entry points with a real environment in a child process,
    // without changing process-global environment for concurrent unit tests.
    let Some(root) = std::env::var_os("VESPER_TEST_HF_CHILD_ROOT") else {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("hub");
        std::fs::create_dir_all(snapshot(&cache).parent().unwrap()).unwrap();
        std::fs::write(snapshot(&cache), DATA).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::cache_tests::configured_cache_reaches_catalog_installer_and_safe_removal",
                "--nocapture",
            ])
            .env("VESPER_TEST_HF_CHILD_ROOT", temp.path().join("app"))
            .env("HF_HUB_CACHE", &cache)
            // Any accidental network access fails instead of downloading weights.
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(snapshot(&cache)).unwrap(), DATA);
        return;
    };
    let root = PathBuf::from(root);
    let a = fixture();
    let profile: HuggingFaceProfile = serde_json::from_value(serde_json::json!({
        "id": "cache-fixture", "name": "Cache fixture", "repository": "test/model",
        "revision": REVISION, "runtime": "llama_cpp", "license": "fixture",
        "files": [{ "role": "weights", "path": "nested/weights.gguf", "sha256": a.sha256, "sizeBytes": a.size_bytes }]
    })).unwrap();
    profile.validate().unwrap();
    let path = root.join(profile.relative_path(&profile.files[0]));
    let mut config = AppConfig::default();
    config.models.custom_rewriter = Some(profile);
    let before = config.clone();
    let result = install_model(
        &config,
        &root,
        "cache-fixture",
        None,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(result.ready);
    assert_eq!(result.local_file_count, 1);
    assert_eq!(result.downloaded_bytes, DATA.len() as u64);
    assert_eq!(result.removable_bytes, 0); // Shared hard link: no false reclaimed bytes.
    assert!(list_model_installations(&config, &root)
        .iter()
        .any(|s| s.model_id == "cache-fixture" && s.ready));
    let removed = remove_model(
        &config,
        &root,
        &root,
        "cache-fixture",
        ModelRole::TextReconstruction,
        None,
    )
    .unwrap();
    assert_eq!(removed.removed_files, 1);
    assert_eq!(removed.removed_bytes, 0);
    assert!(!path.exists());
    assert_eq!(config, before);
    assert!(
        install_model(
            &config,
            &root,
            "cache-fixture",
            None,
            &AtomicBool::new(false),
            |_| {}
        )
        .unwrap()
        .ready
    );
}
