//! Interoperate with the HF Hub snapshot/blob cache without changing adapter paths.
//! Format/lock contract: huggingface_hub v1.7.2 file_download.py and Hub local-cache docs.
use super::*;
use fs2::FileExt;
use std::{ffi::OsString, fs::OpenOptions};

pub(super) struct HubCache {
    root: PathBuf,
}

struct HubArtifact {
    repo: String,
    snapshot: String,
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}

impl HubArtifact {
    fn parse(artifact: &Artifact) -> Option<Self> {
        let url = reqwest::Url::parse(&artifact.url).ok()?;
        if url.scheme() != "https"
            || url.host_str() != Some("huggingface.co")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return None;
        }
        let parts: Vec<String> = url
            .path_segments()?
            .map(|p| {
                percent_encoding::percent_decode_str(p)
                    .decode_utf8()
                    .ok()
                    .map(String::from)
            })
            .collect::<Option<_>>()?;
        if parts.len() < 5
            || parts[2] != "resolve"
            || !hex(&parts[3], 40)
            || parts[3] != artifact.revision
            || !hex(&artifact.sha256, 64)
            || parts
                .iter()
                .any(|p| !component(p) || p.ends_with(".incomplete"))
        {
            return None;
        }
        let repo = format!("models--{}--{}", parts[0], parts[1]);
        Some(Self {
            snapshot: format!("{repo}/snapshots/{}/{}", parts[3], parts[4..].join("/")),
            repo,
        })
    }
}

// Inject environment lookup for tests: never mutate the process environment in parallel tests.
fn cache_directory(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let get = |key| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = get("HOME").or_else(|| get("USERPROFILE"));
    let path = get("HF_HUB_CACHE")
        .or_else(|| get("HUGGINGFACE_HUB_CACHE"))
        .or_else(|| get("HF_HOME").map(|p| p.join("hub")))
        .or_else(|| {
            get("XDG_CACHE_HOME")
                .or_else(|| home.as_ref().map(|p| p.join(".cache")))
                .map(|p| p.join("huggingface/hub"))
        })?;
    if let Ok(rest) = path.strip_prefix("~") {
        Some(home?.join(rest))
    } else {
        Some(path)
    }
}

/// Separate metadata requests stop at CDN redirects to retain Hub ETag/commit headers.
/// A test-only endpoint reroutes the same production installer through a local HTTP server.
pub(super) struct ArtifactHttp {
    download: reqwest::blocking::Client,
    metadata: reqwest::blocking::Client,
    #[cfg(test)]
    pub endpoint: Option<String>,
}

impl ArtifactHttp {
    pub fn new() -> Result<Self> {
        let builder = || {
            reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .user_agent("vesper-desktop/0.1")
        };
        Ok(Self {
            download: builder().timeout(Duration::from_secs(1800)).build()?,
            metadata: builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            #[cfg(test)]
            endpoint: None,
        })
    }

    fn url(&self, artifact: &Artifact) -> String {
        #[cfg(test)]
        if let Some(endpoint) = &self.endpoint {
            if let Some(path) = artifact.url.strip_prefix("https://huggingface.co") {
                return format!("{endpoint}{path}");
            }
        }
        artifact.url.clone()
    }

    pub fn get(&self, artifact: &Artifact) -> Result<reqwest::blocking::Response> {
        Ok(self
            .download
            .get(self.url(artifact))
            .send()?
            .error_for_status()?)
    }

    fn etag(&self, artifact: &Artifact, cancelled: &AtomicBool) -> Result<String> {
        let mut url = reqwest::Url::parse(&self.url(artifact))
            .map_err(|e| ModelError::InvalidRequest(e.to_string()))?;
        for _ in 0..6 {
            check_cancel(cancelled)?;
            let response = self.metadata.head(url.clone()).send()?.error_for_status()?;
            let headers = response.headers();
            let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
            if let (Some(commit), Some(etag)) = (
                header("x-repo-commit"),
                header("x-linked-etag").or_else(|| header("etag")),
            ) {
                let etag = etag.strip_prefix("W/").unwrap_or(etag).trim_matches('"');
                if commit != artifact.revision
                    || !(hex(etag, 40) || hex(etag, 64))
                    || (etag.len() == 64 && etag != artifact.sha256)
                {
                    return Err(ModelError::Integrity(artifact.name.clone()));
                }
                return Ok(etag.into());
            }
            if response.status().is_redirection() {
                if let Some(next) = header("location").and_then(|location| url.join(location).ok())
                {
                    if next.origin() == url.origin() {
                        url = next;
                        continue;
                    }
                }
            }
            break;
        }
        Err(ModelError::InvalidRequest(format!(
            "Hugging Face did not provide pinned cache metadata for {}",
            artifact.name
        )))
    }
}

impl HubCache {
    pub fn from_env() -> Option<Self> {
        cache_directory(|key| std::env::var_os(key)).map(|root| Self { root })
    }

    /// Read existing snapshot symlinks/plain files, including cache-wide Xet blob links.
    /// Never accept links outside the configured cache or incomplete download files.
    fn verified(
        &self,
        relative: &str,
        artifact: &Artifact,
        cancelled: &AtomicBool,
    ) -> Result<Option<PathBuf>> {
        check_cancel(cancelled)?;
        let Ok(root) = self.root.canonicalize() else {
            return Ok(None);
        };
        let Ok(path) = self.root.join(relative).canonicalize() else {
            return Ok(None);
        };
        if !path.starts_with(root)
            || path
                .components()
                .any(|p| p.as_os_str().to_string_lossy().ends_with(".incomplete"))
        {
            return Ok(None);
        }
        let Ok(file) = File::open(&path) else {
            return Ok(None);
        };
        if !file.metadata()?.is_file() || file.metadata()?.len() != artifact.size_bytes {
            return Ok(None);
        }
        match validate_reader(file, artifact, cancelled) {
            Ok(()) => Ok(Some(path)),
            Err(ModelError::Cancelled) => Err(ModelError::Cancelled),
            Err(_) => Ok(None),
        }
    }

    // Mutations reject descendant directory symlinks. A snapshot's final symlink
    // is replaced atomically, never opened for writing or followed for deletion.
    fn destination(&self, relative: &str) -> Result<PathBuf> {
        let relative = Path::new(relative);
        let parent = artifact_path(&self.root, &relative.parent().unwrap().to_string_lossy())?;
        std::fs::create_dir_all(&parent)?;
        artifact_path(&self.root, &relative.parent().unwrap().to_string_lossy())?;
        Ok(parent.join(relative.file_name().unwrap()))
    }

    fn lock(&self, repo: &str, etag: &str, cancelled: &AtomicBool) -> Result<File> {
        let relative = format!(".locks/{repo}/{etag}.lock");
        self.destination(&relative)?;
        let path = artifact_path(&self.root, &relative)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        loop {
            check_cancel(cancelled)?;
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(file), // Closing releases the OS lock; retain the lock file.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn publish_snapshot(
        &self,
        entry: &HubArtifact,
        source: &Path,
        artifact: &Artifact,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let target = self.destination(&entry.snapshot)?;
        let staging = tempfile::tempdir_in(target.parent().unwrap())?;
        let pointer = staging.path().join("pointer");
        #[cfg(unix)]
        {
            // Relative to the final snapshot parent, not the temporary directory.
            let snapshot_parent = Path::new(&entry.snapshot).parent().unwrap();
            let root = self.root.canonicalize()?;
            let blob = source
                .strip_prefix(&root)
                .map_err(|_| ModelError::UnsafePath(source.into()))?;
            let mut relative = PathBuf::new();
            for _ in 0..snapshot_parent.components().count() {
                relative.push("..");
            }
            relative.push(blob);
            if std::os::unix::fs::symlink(relative, &pointer).is_err() {
                materialize(source, &pointer, artifact, cancelled, false)?;
            }
        }
        #[cfg(not(unix))]
        materialize(source, &pointer, artifact, cancelled, false)?;
        check_cancel(cancelled)?;
        // Persist a temporary symlink without following an existing destination.
        tempfile::TempPath::try_from_path(pointer)?
            .persist(&target)
            .map_err(|e| ModelError::Io(e.error))?;
        Ok(())
    }

    pub fn install(
        &self,
        artifact: &Artifact,
        target: &Path,
        http: &ArtifactHttp,
        cancelled: &AtomicBool,
        report: &impl Fn(InstallPhase, u64),
    ) -> Result<bool> {
        let Some(entry) = HubArtifact::parse(artifact) else {
            return Ok(false);
        };
        if let Some(source) = self.verified(&entry.snapshot, artifact, cancelled)? {
            materialize(&source, target, artifact, cancelled, false)?;
            return Ok(true);
        }
        let etag = http.etag(artifact, cancelled)?;
        let _lock = self.lock(&entry.repo, &etag, cancelled)?;
        // Recheck after the lock: another Vesper/HF process may have completed it.
        if let Some(source) = self.verified(&entry.snapshot, artifact, cancelled)? {
            materialize(&source, target, artifact, cancelled, false)?;
            return Ok(true);
        }
        let relative_blob = format!("{}/blobs/{etag}", entry.repo);
        let source = match self.verified(&relative_blob, artifact, cancelled)? {
            Some(path) => path,
            None => {
                let blob = self.destination(&relative_blob)?;
                let staging =
                    download_artifact(blob.parent().unwrap(), artifact, http, cancelled, report)?;
                if etag.len() == 40 {
                    let mut file = File::open(staging.path())?;
                    // sha1 0.11 implements digest 0.11; sha2 0.10 keeps digest 0.10.
                    use sha1::Digest as _;
                    let mut hash = sha1::Sha1::new();
                    hash.update(format!("blob {}\0", artifact.size_bytes));
                    let mut buffer = [0; 128 * 1024];
                    loop {
                        check_cancel(cancelled)?;
                        let count = file.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        hash.update(&buffer[..count]);
                    }
                    if hash
                        .finalize()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                        != etag
                    {
                        return Err(ModelError::Integrity(artifact.name.clone()));
                    }
                }
                check_cancel(cancelled)?;
                staging
                    .persist(&blob)
                    .map_err(|e| ModelError::Io(e.error))?;
                blob.canonicalize()?
            }
        };
        self.publish_snapshot(&entry, &source, artifact, cancelled)?;
        materialize(&source, target, artifact, cancelled, false)?;
        Ok(true)
    }
}

/// Install a managed hard link, or copy across filesystems. Always replace rather
/// than modifying an existing inode: it may be shared with HF or another app.
fn materialize(
    source: &Path,
    target: &Path,
    artifact: &Artifact,
    cancelled: &AtomicBool,
    force_copy: bool,
) -> Result<()> {
    check_cancel(cancelled)?;
    let parent = target.parent().unwrap();
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    let path = staging.path().join("model");
    if force_copy || std::fs::hard_link(source, &path).is_err() {
        let mut reader = File::open(source)?;
        let mut writer = File::create(&path)?;
        let mut buffer = [0; 128 * 1024];
        let mut copied = 0;
        loop {
            check_cancel(cancelled)?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied += count as u64;
            if copied > artifact.size_bytes {
                return Err(ModelError::Integrity(artifact.name.clone()));
            }
            writer.write_all(&buffer[..count])?;
        }
        writer.sync_all()?;
    }
    // Verify the actual new reference, protecting against a replaced source path.
    validate_reader(File::open(&path)?, artifact, cancelled)?;
    check_cancel(cancelled)?;
    tempfile::TempPath::try_from_path(path)?
        .persist(target)
        .map_err(|e| ModelError::Io(e.error))?;
    Ok(())
}

#[cfg(test)]
#[path = "hub_cache_tests.rs"]
mod tests;
