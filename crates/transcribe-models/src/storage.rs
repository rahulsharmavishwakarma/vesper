//! Inventory and removal of declared model weights. Runtime files are shared and retained.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRemoval {
    pub removed_bytes: u64,
    pub removed_files: usize,
}

fn weight_paths(spec: &ModelInstallSpec) -> BTreeSet<&str> {
    spec.artifacts
        .iter()
        .filter(|a| a.role != "asr_runtime")
        .map(|a| a.relative_path.as_str())
        .collect()
}

fn local_weights(spec: &ModelInstallSpec, root: &Path) -> Result<Vec<(PathBuf, u64)>> {
    weight_paths(spec)
        .into_iter()
        .filter_map(|relative| {
            let result = (|| {
                let path = artifact_path(root, relative)?;
                match path.symlink_metadata() {
                    Ok(metadata) if metadata.is_file() => Ok(Some((path, metadata.len()))),
                    Ok(_) => Err(ModelError::UnsafePath(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(error.into()),
                }
            })();
            result.transpose()
        })
        .collect()
}

fn removal_plan(
    spec: &ModelInstallSpec,
    specs: &[ModelInstallSpec],
    root: &Path,
) -> Result<Vec<(PathBuf, u64)>> {
    // Preflight every declared path before touching any file. Never recurse or follow
    // descendant symlinks. Keep files referenced by another catalog package/profile.
    let shared: BTreeSet<_> = specs
        .iter()
        .filter(|other| !std::ptr::eq(*other, spec))
        .flat_map(weight_paths)
        .map(|relative| root.join(relative))
        .collect();
    Ok(local_weights(spec, root)?
        .into_iter()
        .filter(|(path, _)| !shared.contains(path))
        .collect())
}

pub(super) fn inspect_storage(
    status: &mut ModelInstallation,
    spec: &ModelInstallSpec,
    specs: &[ModelInstallSpec],
    root: &Path,
) {
    match local_weights(spec, root) {
        Ok(files) => {
            status.local_file_count = files.len();
            status.downloaded_bytes = files.iter().map(|(_, bytes)| bytes).sum();
            match removal_plan(spec, specs, root) {
                Ok(plan) => {
                    status.removable_bytes = plan.iter().map(|(_, bytes)| bytes).sum();
                    if plan.is_empty() && !files.is_empty() {
                        status.removal_error =
                            Some("These files are shared with another model package.".into());
                    }
                }
                Err(error) => status.removal_error = Some(error.to_string()),
            }
        }
        Err(error) => status.removal_error = Some(error.to_string()),
    }
}

/// Caller must hold the existing runtime operation boundary and unload inference,
/// exactly as for installation. No frontend-provided paths are used for deletion.
pub fn remove_model(
    config: &AppConfig,
    root: &Path,
    expected_root: &Path,
    model_id: &str,
    role: ModelRole,
    quantization: Option<RewriteQuantization>,
) -> Result<ModelRemoval> {
    if root != expected_root {
        return Err(ModelError::InvalidRequest(
            "The model folder changed. Refresh the list before removing files.".into(),
        ));
    }
    let specs = model_install_specs(config);
    let matches: Vec<_> = specs
        .iter()
        .filter(|spec| {
            spec.model_id == model_id && spec.role == role && spec.quantization == quantization
        })
        .collect();
    if matches.len() != 1 {
        return Err(ModelError::InvalidRequest(
            "This model package is no longer in the catalog. Refresh the list.".into(),
        ));
    }
    let spec = matches[0];
    let plan = removal_plan(spec, &specs, root)?;
    if plan.is_empty() && !local_weights(spec, root)?.is_empty() {
        return Err(ModelError::InvalidRequest(
            "These files are shared with another model package.".into(),
        ));
    }
    let mut result = ModelRemoval {
        removed_bytes: 0,
        removed_files: 0,
    };
    for (path, bytes) in plan {
        // Revalidate immediately before unlinking, including every parent component.
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ModelError::UnsafePath(path.clone()))?;
        let checked = artifact_path(root, &relative.to_string_lossy())?;
        std::fs::remove_file(checked)?;
        result.removed_bytes += bytes;
        result.removed_files += 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_weight(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn removal_is_precision_specific_and_preserves_settings_and_other_files() {
        let root = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let before = config.clone();
        let model = RewriteModel::Qwen35_08b;
        let q4 = built_in_rewriter_spec(&config, model, RewriteQuantization::Q4_0);
        let q8 = built_in_rewriter_spec(&config, model, RewriteQuantization::Q8_0);
        let small = write_weight(root.path(), &q4.artifacts[0].relative_path, b"q4");
        let large = write_weight(root.path(), &q8.artifacts[0].relative_path, b"q8 other");
        let unrelated = write_weight(root.path(), "notes.txt", b"keep me");
        let inventory = list_model_installations(&config, root.path());
        let status = inventory
            .iter()
            .find(|m| m.model_id == model.id() && m.quantization == Some(RewriteQuantization::Q4_0))
            .unwrap();
        assert!(status.selected);
        assert!(!status.artifacts_installed); // Corrupt/incomplete weights still take space.
        assert_eq!(status.downloaded_bytes, 2);
        assert_eq!(status.removable_bytes, 2);
        assert_eq!(status.local_file_count, 1);
        let removed = remove_model(
            &config,
            root.path(),
            root.path(),
            model.id(),
            ModelRole::TextReconstruction,
            Some(RewriteQuantization::Q4_0),
        )
        .unwrap();
        assert_eq!((removed.removed_files, removed.removed_bytes), (1, 2));
        assert!(!small.exists());
        assert_eq!(std::fs::read(large).unwrap(), b"q8 other");
        assert_eq!(std::fs::read(unrelated).unwrap(), b"keep me");
        assert_eq!(config, before);
        assert!(!inspect(&config, root.path()).ready);
        assert_eq!(
            list_model_installations(&config, root.path())
                .iter()
                .find(|m| m.model_id == model.id()
                    && m.quantization == Some(RewriteQuantization::Q4_0))
                .unwrap()
                .local_file_count,
            0
        );
    }

    #[test]
    fn rejects_unknown_role_precision_and_stale_folder_before_deleting() {
        let root = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let spec =
            built_in_rewriter_spec(&config, RewriteModel::Qwen35_08b, RewriteQuantization::Q4_0);
        let path = write_weight(root.path(), &spec.artifacts[0].relative_path, b"weights");
        for (id, role, quantization, expected) in [
            (
                "../../notes",
                spec.role,
                spec.quantization,
                root.path().to_path_buf(),
            ),
            (
                spec.model_id.as_str(),
                ModelRole::SpeechRecognition,
                spec.quantization,
                root.path().to_path_buf(),
            ),
            (
                spec.model_id.as_str(),
                spec.role,
                None,
                root.path().to_path_buf(),
            ),
            (
                spec.model_id.as_str(),
                spec.role,
                spec.quantization,
                root.path().join("old"),
            ),
        ] {
            assert!(remove_model(&config, root.path(), &expected, id, role, quantization).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"weights");
        }
    }

    #[test]
    fn shipped_presets_remain_manageable_after_switching_away() {
        let root = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let presets: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../models/presets.json")).unwrap();
        for preset in presets {
            let profile: HuggingFaceProfile =
                serde_json::from_value(preset["profile"].clone()).unwrap();
            let file = &profile.files[0];
            let path = write_weight(root.path(), &profile.relative_path(file), b"download");
            let inventory = list_model_installations(&config, root.path());
            let status = inventory.iter().find(|m| m.model_id == profile.id).unwrap();
            assert!(!status.selected);
            assert_eq!(status.local_file_count, 1);
            assert_eq!(status.downloaded_bytes, 8);
            remove_model(
                &config,
                root.path(),
                root.path(),
                &status.model_id,
                status.role,
                status.quantization,
            )
            .unwrap();
            assert!(!path.exists());
        }
    }

    #[test]
    fn shared_weights_and_runtime_files_are_kept() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        let presets: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../models/presets.json")).unwrap();
        let mut profile: HuggingFaceProfile =
            serde_json::from_value(presets[0]["profile"].clone()).unwrap();
        profile.id = "shared-alias".into();
        let path = write_weight(
            root.path(),
            &profile.relative_path(&profile.files[0]),
            b"shared weights",
        );
        match profile.runtime {
            transcribe_core::ModelRuntime::LlamaCpp => {
                config.models.custom_rewriter = Some(profile)
            }
            transcribe_core::ModelRuntime::SherpaOnnxTransducer => {
                config.models.custom_asr = Some(profile)
            }
        }
        let inventory = list_model_installations(&config, root.path());
        let alias = inventory
            .iter()
            .find(|m| m.model_id == "shared-alias")
            .unwrap();
        assert_eq!(alias.removable_bytes, 0);
        assert!(alias.removal_error.is_some());
        assert!(remove_model(
            &config,
            root.path(),
            root.path(),
            &alias.model_id,
            alias.role,
            alias.quantization
        )
        .is_err());
        assert!(path.exists());

        let spec = built_in_asr_spec(&config, AsrModel::Nemotron35AsrStreaming06b);
        let mut paths = Vec::new();
        for artifact in &spec.artifacts {
            paths.push((
                write_weight(root.path(), &artifact.relative_path, b"file"),
                artifact.role == "asr_runtime",
            ));
        }
        remove_model(
            &config,
            root.path(),
            root.path(),
            &spec.model_id,
            spec.role,
            None,
        )
        .unwrap();
        for (path, runtime) in paths {
            assert_eq!(path.exists(), runtime);
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_preflight_leaves_all_files_untouched() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let spec = built_in_asr_spec(&config, AsrModel::ParakeetTdt06bV3);
        let paths: Vec<_> = weight_paths(&spec).into_iter().collect();
        assert!(paths.len() >= 2);
        let first = write_weight(root.path(), paths[0], b"keep first");
        let outside_file = write_weight(outside.path(), "private.txt", b"keep outside");
        let last = root.path().join(paths[paths.len() - 1]);
        std::fs::create_dir_all(last.parent().unwrap()).unwrap();
        symlink(&outside_file, &last).unwrap();
        assert!(remove_model(
            &config,
            root.path(),
            root.path(),
            &spec.model_id,
            spec.role,
            None
        )
        .is_err());
        assert!(first.exists());
        assert_eq!(std::fs::read(&outside_file).unwrap(), b"keep outside");
        std::fs::remove_file(last).unwrap();
        let custom = root.path().join("huggingface");
        symlink(outside.path(), &custom).unwrap();
        assert!(matches!(
            artifact_path(root.path(), "huggingface/private.txt"),
            Err(ModelError::UnsafePath(_))
        ));
        assert!(matches!(
            artifact_path(root.path(), "../private.txt"),
            Err(ModelError::UnsafePath(_))
        ));
    }
}
