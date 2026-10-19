use super::*;
use std::{
    cell::RefCell,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

const REVISION: &str = "1234567890abcdef1234567890abcdef12345678";
const DATA: &[u8] = b"public synthetic model fixture";

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
        quantization: None,
        license: "fixture".into(),
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

struct Server {
    endpoint: String,
    calls: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(bytes: Vec<u8>, etag: String, revision: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (observed, stopped) = (calls.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = match stream.read(&mut buffer) {
                        Ok(n) => n,
                        Err(_) => break, // A cancelled client can close before sending headers.
                    };
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                }
                if !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    continue;
                }
                let request = String::from_utf8_lossy(&request);
                let method = request.split_whitespace().next().unwrap().to_string();
                observed.lock().unwrap().push(method.clone());
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"{etag}\"\r\nx-repo-commit: {revision}\r\nConnection: close\r\n\r\n", bytes.len());
                if method != "HEAD" {
                    let _ = stream.write_all(&bytes);
                }
            }
        });
        Self {
            endpoint,
            calls,
            stop,
            worker: Some(worker),
        }
    }
    fn http(&self) -> ArtifactHttp {
        let mut http = ArtifactHttp::new().unwrap();
        http.endpoint = Some(self.endpoint.clone());
        http
    }
    fn count(&self, method: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.as_str() == method)
            .count()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let joined = self.worker.take().unwrap().join();
        if !thread::panicking() {
            joined.unwrap();
        }
    }
}
fn server() -> Server {
    Server::new(DATA.into(), fixture().sha256, REVISION.into())
}

fn install(
    root: &Path,
    cache: &HubCache,
    artifact: &Artifact,
    http: &ArtifactHttp,
) -> Result<Vec<InstallProgress>> {
    let progress = RefCell::new(Vec::new());
    install_artifacts_with_cache(
        root,
        std::slice::from_ref(artifact),
        &AtomicBool::new(false),
        &|p| progress.borrow_mut().push(p.clone()),
        Some(cache),
        http,
    )?;
    Ok(progress.into_inner())
}

#[test]
fn standard_cache_environment_precedence_and_tilde() {
    let all = [
        ("HF_HUB_CACHE", "/explicit"),
        ("HUGGINGFACE_HUB_CACHE", "/legacy"),
        ("HF_HOME", "/hf"),
        ("XDG_CACHE_HOME", "/xdg"),
        ("HOME", "/home/test"),
    ];
    for (start, expected) in [
        (0, "/explicit"),
        (1, "/legacy"),
        (2, "/hf/hub"),
        (3, "/xdg/huggingface/hub"),
        (4, "/home/test/.cache/huggingface/hub"),
    ] {
        assert_eq!(
            cache_directory(|key| all[start..]
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.into())),
            Some(expected.into())
        );
    }
    assert_eq!(
        cache_directory(|key| match key {
            "HF_HUB_CACHE" => Some("~/models/hub".into()),
            "HOME" => Some("/home/test".into()),
            _ => None,
        }),
        Some("/home/test/models/hub".into())
    );
    assert_eq!(cache_directory(|_| None), None);
}

#[test]
fn identity_requires_pinned_repo_revision_and_safe_filename() {
    let a = fixture();
    assert!(HubArtifact::parse(&a).is_some());
    for url in [
        a.url.replace("huggingface.co", "example.org"),
        a.url.replace(REVISION, "main"),
        a.url.replace("weights.gguf", "%2E%2E%2Fevil"),
        a.url.replace("weights.gguf", "weights.gguf.incomplete"),
    ] {
        assert!(HubArtifact::parse(&Artifact { url, ..a.clone() }).is_none());
    }
    assert!(HubArtifact::parse(&Artifact {
        revision: "a".repeat(40),
        ..a
    })
    .is_none());
}

#[test]
fn plain_snapshot_hit_uses_zero_requests_and_a_shared_inode() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    let snapshot = cache.root.join(HubArtifact::parse(&a).unwrap().snapshot);
    write(&snapshot, DATA);
    let server = server();
    let root = temp.path().join("app");
    let progress = install(&root, &cache, &a, &server.http()).unwrap();
    assert!(server.calls.lock().unwrap().is_empty());
    assert_eq!(progress.last().unwrap().phase, InstallPhase::Installed);
    assert_eq!(progress.last().unwrap().completed_bytes, DATA.len() as u64);
    assert!(progress
        .iter()
        .all(|p| p.phase != InstallPhase::Downloading));
    assert!(artifact_file_installed(&root, &a));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            snapshot.metadata().unwrap().ino(),
            root.join(&a.relative_path).metadata().unwrap().ino()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_snapshot_and_cache_wide_blob_are_reused_without_network() {
    for wide in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let cache = HubCache {
            root: temp.path().join("hub"),
        };
        let a = fixture();
        let entry = HubArtifact::parse(&a).unwrap();
        let blob = cache.root.join(if wide {
            "blobs/ab/xet-file".into()
        } else {
            format!("{}/blobs/{}", entry.repo, a.sha256)
        });
        write(&blob, DATA);
        let snapshot = cache.root.join(entry.snapshot);
        std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&blob, &snapshot).unwrap();
        let server = server();
        install(&temp.path().join("app"), &cache, &a, &server.http()).unwrap();
        assert!(server.calls.lock().unwrap().is_empty());
    }
}

#[test]
fn miss_downloads_once_and_next_install_is_offline() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    let server = server();
    let progress = install(&temp.path().join("app1"), &cache, &a, &server.http()).unwrap();
    assert_eq!(server.count("HEAD"), 1);
    assert_eq!(server.count("GET"), 1);
    assert!(progress
        .windows(2)
        .all(|p| p[0].completed_bytes <= p[1].completed_bytes));
    assert_eq!(
        std::fs::read(cache.root.join(HubArtifact::parse(&a).unwrap().snapshot)).unwrap(),
        DATA
    );
    install(&temp.path().join("app2"), &cache, &a, &server.http()).unwrap();
    assert_eq!(server.count("HEAD"), 1);
    assert_eq!(server.count("GET"), 1);
}

#[test]
fn git_etag_uses_git_blob_hash_and_rejects_incorrect_metadata() {
    for valid in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let cache = HubCache {
            root: temp.path().join("hub"),
        };
        let a = fixture();
        // sha1 0.11 implements digest 0.11; sha2 0.10 keeps digest 0.10.
        use sha1::Digest as _;
        let mut git = sha1::Sha1::new();
        git.update(format!("blob {}\0", DATA.len()));
        git.update(DATA);
        let etag = if valid {
            git.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        } else {
            "0".repeat(40)
        };
        let server = Server::new(DATA.into(), etag.clone(), REVISION.into());
        assert_eq!(
            install(&temp.path().join("app"), &cache, &a, &server.http()).is_ok(),
            valid
        );
        assert_eq!(
            cache
                .root
                .join(format!("models--test--model/blobs/{etag}"))
                .exists(),
            valid
        );
    }
}

#[test]
fn wrong_revision_export_corrupt_and_incomplete_entries_are_not_hits() {
    for case in ["revision", "export", "corrupt", "incomplete"] {
        let temp = tempfile::tempdir().unwrap();
        let cache = HubCache {
            root: temp.path().join("hub"),
        };
        let a = fixture();
        let entry = HubArtifact::parse(&a).unwrap();
        let relative = match case {
            "revision" => entry.snapshot.replace(REVISION, &"a".repeat(40)),
            "export" => entry
                .snapshot
                .replace("weights.gguf", "weights.safetensors"),
            "incomplete" => format!("{}/blobs/{}.incomplete", entry.repo, a.sha256),
            _ => entry.snapshot,
        };
        let original = cache.root.join(relative);
        write(
            &original,
            if case == "corrupt" {
                b"wrong bytes"
            } else {
                DATA
            },
        );
        let server = server();
        install(&temp.path().join("app"), &cache, &a, &server.http()).unwrap();
        assert_eq!(server.count("GET"), 1, "{case}");
        if case != "corrupt" {
            assert_eq!(std::fs::read(original).unwrap(), DATA);
        }
    }
}

#[test]
fn existing_blob_repairs_snapshot_without_weight_download() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    write(
        &cache
            .root
            .join(format!("models--test--model/blobs/{}", a.sha256)),
        DATA,
    );
    let server = server();
    install(&temp.path().join("app"), &cache, &a, &server.http()).unwrap();
    assert_eq!(server.count("GET"), 0);
}

#[test]
fn cancellation_and_corrupt_downloads_do_not_publish_partial_files() {
    for mode in ["cancel", "corrupt", "metadata"] {
        let temp = tempfile::tempdir().unwrap();
        let cache = HubCache {
            root: temp.path().join("hub"),
        };
        let a = fixture();
        let server = Server::new(
            if mode == "corrupt" {
                b"corrupt".to_vec()
            } else {
                DATA.to_vec()
            },
            a.sha256.clone(),
            if mode == "metadata" {
                "0".repeat(40)
            } else {
                REVISION.into()
            },
        );
        let root = temp.path().join("app");
        let cancelled = AtomicBool::new(false);
        let result = install_artifacts_with_cache(
            &root,
            std::slice::from_ref(&a),
            &cancelled,
            &|p| {
                if mode == "cancel" && p.phase == InstallPhase::Downloading && p.completed_bytes > 0
                {
                    cancelled.store(true, Ordering::Relaxed);
                }
            },
            Some(&cache),
            &server.http(),
        );
        assert!(result.is_err(), "{mode}");
        assert!(!root.join(&a.relative_path).exists());
        assert!(!cache
            .root
            .join(HubArtifact::parse(&a).unwrap().snapshot)
            .exists());
        assert!(!cache
            .root
            .join(format!("models--test--model/blobs/{}", a.sha256))
            .exists());
    }
}

#[test]
fn concurrent_installers_share_one_download_and_lock_wait_can_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    let server = server();
    let http = server.http();
    thread::scope(|scope| {
        let one = scope.spawn(|| install(&temp.path().join("app1"), &cache, &a, &http));
        let two = scope.spawn(|| install(&temp.path().join("app2"), &cache, &a, &http));
        one.join().unwrap().unwrap();
        two.join().unwrap().unwrap();
    });
    assert_eq!(server.count("GET"), 1);
    let held = cache
        .lock("models--test--model", &a.sha256, &AtomicBool::new(false))
        .unwrap();
    let cancelled = AtomicBool::new(false);
    thread::scope(|scope| {
        let waiter = scope.spawn(|| cache.lock("models--test--model", &a.sha256, &cancelled));
        thread::sleep(Duration::from_millis(50));
        cancelled.store(true, Ordering::Relaxed);
        assert!(matches!(waiter.join().unwrap(), Err(ModelError::Cancelled)));
    });
    drop(held);
}

#[test]
fn copy_fallback_and_failed_target_preserve_source_and_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let a = fixture();
    let source = temp.path().join("source");
    write(&source, DATA);
    let target = temp.path().join("app/model");
    materialize(&source, &target, &a, &AtomicBool::new(false), true).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            source.metadata().unwrap().ino(),
            target.metadata().unwrap().ino()
        );
    }
    std::fs::remove_file(target).unwrap();
    let bad = temp.path().join("existing-directory");
    std::fs::create_dir(&bad).unwrap();
    assert!(materialize(&source, &bad, &a, &AtomicBool::new(false), false).is_err());
    assert_eq!(std::fs::read(source).unwrap(), DATA);
    assert!(bad.is_dir());
}

#[cfg(unix)]
#[test]
fn outside_and_incomplete_snapshot_links_are_ignored_and_unsafe_parents_rejected() {
    for incomplete in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let cache = HubCache {
            root: temp.path().join("hub"),
        };
        let a = fixture();
        let entry = HubArtifact::parse(&a).unwrap();
        let external = if incomplete {
            cache.root.join("blobs/file.incomplete")
        } else {
            temp.path().join("external")
        };
        write(&external, DATA);
        let snapshot = cache.root.join(&entry.snapshot);
        std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&external, &snapshot).unwrap();
        let server = server();
        install(&temp.path().join("app"), &cache, &a, &server.http()).unwrap();
        assert_eq!(server.count("GET"), 1);
        assert_eq!(std::fs::read(external).unwrap(), DATA);
    }
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(&cache.root).unwrap();
    std::os::unix::fs::symlink(&outside, cache.root.join("models--test--model")).unwrap();
    assert!(cache.destination("models--test--model/blobs/hash").is_err());
    assert!(std::fs::read_dir(outside).unwrap().next().is_none());
}

#[test]
fn existing_app_install_is_preserved_without_migrating_or_network() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    let root = temp.path().join("app");
    write(&root.join(&a.relative_path), DATA);
    let server = server();
    install(&root, &cache, &a, &server.http()).unwrap();
    assert!(server.calls.lock().unwrap().is_empty());
    assert!(!cache.root.exists());
}

#[test]
#[ignore = "explicit public-network smoke; creates only an isolated temporary HF cache"]
fn public_hf_cache_smoke() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let http = ArtifactHttp::new().unwrap();
    // Exercise both Git-file and large-file ETags with pinned public metadata/data.
    let all = catalog();
    let small = all
        .iter()
        .find(|a| {
            HubArtifact::parse(a).is_some()
                && a.name.ends_with("config.json")
                && a.size_bytes < 100_000
        })
        .unwrap();
    let model = all
        .iter()
        .filter(|a| HubArtifact::parse(a).is_some() && a.name.ends_with(".onnx"))
        .min_by_key(|a| a.size_bytes)
        .unwrap();
    for a in [small, model] {
        install(&temp.path().join("app1"), &cache, a, &http).unwrap();
        // A local server records zero requests on the second install.
        let server = server();
        let second = install(&temp.path().join("app2"), &cache, a, &server.http()).unwrap();
        assert!(server.calls.lock().unwrap().is_empty());
        assert!(second.iter().all(|p| p.phase != InstallPhase::Downloading));
        eprintln!(
            "public cache smoke: {} revision={} sha256={} bytes={}",
            a.name, a.revision, a.sha256, a.size_bytes
        );
    }
}

#[cfg(unix)]
#[test]
fn repairing_shared_corruption_replaces_inodes_without_writing_other_installations() {
    let temp = tempfile::tempdir().unwrap();
    let cache = HubCache {
        root: temp.path().join("hub"),
    };
    let a = fixture();
    let server = server();
    let http = server.http();
    let one = temp.path().join("app1");
    let two = temp.path().join("app2");
    install(&one, &cache, &a, &http).unwrap();
    install(&two, &cache, &a, &http).unwrap();
    let blob = cache
        .root
        .join(format!("models--test--model/blobs/{}", a.sha256));
    write(&blob, b"external corruption");
    assert!(validate_reader(
        File::open(two.join(&a.relative_path)).unwrap(),
        &a,
        &AtomicBool::new(false)
    )
    .is_err());
    install(&one, &cache, &a, &http).unwrap();
    assert_eq!(std::fs::read(&blob).unwrap(), DATA);
    assert_eq!(std::fs::read(one.join(&a.relative_path)).unwrap(), DATA);
    assert_eq!(
        std::fs::read(two.join(&a.relative_path)).unwrap(),
        b"external corruption"
    );
    assert_eq!(server.count("GET"), 2);
    install(&two, &cache, &a, &http).unwrap();
    assert_eq!(std::fs::read(two.join(&a.relative_path)).unwrap(), DATA);
    assert_eq!(server.count("GET"), 2);
}
