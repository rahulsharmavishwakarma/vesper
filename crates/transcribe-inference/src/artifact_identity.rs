//! Observed file identity shared by resident native sessions. Hash verification
//! still occurs before every new load; metadata only controls resident reuse.
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ArtifactIdentity {
    relative_path: String,
    resolved_path: PathBuf,
    revision: String,
    sha256: String,
    size_bytes: u64,
    precision: Option<String>,
    stamp: Option<FileStamp>,
}

impl ArtifactIdentity {
    pub(crate) fn observe(root: &Path, a: transcribe_models::Artifact) -> Self {
        let path = root.join(&a.relative_path);
        let stamp = fs::metadata(&path).ok().and_then(|m| {
            if !m.is_file() {
                return None;
            }
            Some(FileStamp {
                len: m.len(),
                modified: m.modified().ok()?,
                #[cfg(unix)]
                identity: {
                    use std::os::unix::fs::MetadataExt;
                    (m.dev(), m.ino(), m.ctime(), m.ctime_nsec())
                },
            })
        });
        ArtifactIdentity {
            resolved_path: path.canonicalize().unwrap_or(path),
            relative_path: a.relative_path,
            revision: a.revision,
            sha256: a.sha256,
            size_bytes: a.size_bytes,
            precision: a.quantization,
            stamp,
        }
    }
}
