use crate::models::{ArtifactInfo, ArtifactKind};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const LOADER_DEX_NAMES: &[&str] = &["classes.dex", "protector-loader.dex", "loader.dex"];
const NATIVE_LIBRARY_NAME: &str = "libprotector_vm.so";
const KNOWN_ABIS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86_64", "x86"];
const EMBEDDED_LOADER_DEX: &[u8] = include_bytes!("../../tools/loader/classes.dex");

#[derive(Debug, Clone, Default)]
pub struct LoaderInjectionPlan {
    pub files: Vec<LoaderInjectionFile>,
    pub dex_targets: Vec<String>,
    pub native_targets: Vec<String>,
    pub issues: Vec<String>,
}

impl LoaderInjectionPlan {
    pub fn status(&self) -> String {
        if self.files.is_empty() {
            "loader artifacts not found; injection skipped".to_string()
        } else if self.issues.is_empty() {
            "loader artifacts injected".to_string()
        } else {
            "loader artifacts injected with warnings".to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoaderInjectionFile {
    pub source: LoaderArtifactSource,
    pub target: String,
    pub kind: LoaderArtifactKind,
}

#[derive(Debug, Clone)]
pub enum LoaderArtifactSource {
    File(PathBuf),
    Embedded {
        name: &'static str,
        bytes: &'static [u8],
    },
}

impl LoaderInjectionFile {
    pub fn source_label(&self) -> String {
        match &self.source {
            LoaderArtifactSource::File(path) => path.display().to_string(),
            LoaderArtifactSource::Embedded { name, .. } => (*name).to_string(),
        }
    }

    pub fn read_bytes(&self) -> Result<Vec<u8>, String> {
        match &self.source {
            LoaderArtifactSource::File(path) => fs::read(path)
                .map_err(|err| format!("failed to read loader artifact {}: {err}", path.display())),
            LoaderArtifactSource::Embedded { bytes, .. } => Ok(bytes.to_vec()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderArtifactKind {
    Dex,
    NativeLibrary,
}

pub fn build_loader_injection_plan(
    kind: ArtifactKind,
    artifact: &ArtifactInfo,
) -> LoaderInjectionPlan {
    let mut plan = LoaderInjectionPlan::default();
    let roots = loader_artifact_roots();
    let mut seen_sources = HashSet::new();

    let dex_sources = discover_loader_dex_files(&roots);
    let mut next_dex_index = next_loader_dex_index(kind, artifact);
    for source in dex_sources {
        if !seen_sources.insert(source.clone()) {
            continue;
        }
        let target = loader_dex_target(kind, next_dex_index);
        next_dex_index += 1;
        plan.dex_targets.push(target.clone());
        plan.files.push(LoaderInjectionFile {
            source: LoaderArtifactSource::File(source),
            target,
            kind: LoaderArtifactKind::Dex,
        });
    }
    if plan.dex_targets.is_empty() && !EMBEDDED_LOADER_DEX.is_empty() {
        let target = loader_dex_target(kind, next_dex_index);
        plan.dex_targets.push(target.clone());
        plan.files.push(LoaderInjectionFile {
            source: LoaderArtifactSource::Embedded {
                name: "built-in tools/loader/classes.dex",
                bytes: EMBEDDED_LOADER_DEX,
            },
            target,
            kind: LoaderArtifactKind::Dex,
        });
    }

    for source in discover_native_libraries(&roots) {
        if !seen_sources.insert(source.clone()) {
            continue;
        }
        let Some(abi) = infer_abi(&source) else {
            plan.issues.push(format!(
                "skipped native loader without ABI parent: {}",
                source.display()
            ));
            continue;
        };
        let target = native_library_target(kind, abi);
        plan.native_targets.push(target.clone());
        plan.files.push(LoaderInjectionFile {
            source: LoaderArtifactSource::File(source),
            target,
            kind: LoaderArtifactKind::NativeLibrary,
        });
    }

    if plan.files.is_empty() {
        plan.issues.push(
            "place loader artifacts under tools/loader/classes.dex and tools/loader/lib/<abi>/libprotector_vm.so"
                .to_string(),
        );
    }

    plan
}

fn loader_artifact_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(path) = env::var("ANDROID_PROTECTOR_LOADER") {
        roots.push(PathBuf::from(path));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("tools").join("loader"));
            roots.push(dir.join("loader"));
            if let Some(parent) = dir.parent() {
                roots.push(parent.join("tools").join("loader"));
                roots.push(parent.join("loader"));
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd.join("tools").join("loader"));
        roots.push(cwd.join("loader-artifacts"));
        roots.push(
            cwd.join("loader-android")
                .join("protector-loader")
                .join("build")
                .join("protector-artifacts"),
        );
    }
    dedupe_existing_roots(roots)
}

fn dedupe_existing_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|path| path.exists())
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn discover_loader_dex_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).max_depth(5).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if LOADER_DEX_NAMES.iter().any(|candidate| *candidate == name) {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    files.sort();
    files
}

fn discover_native_libraries(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).max_depth(6).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.file_name().to_string_lossy() == NATIVE_LIBRARY_NAME {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    files.sort();
    files
}

fn next_loader_dex_index(kind: ArtifactKind, artifact: &ArtifactInfo) -> u32 {
    let max_existing = artifact
        .dex_files
        .iter()
        .filter_map(|dex| dex_index(kind, &dex.name))
        .max()
        .unwrap_or(0);
    max_existing + 1
}

fn dex_index(kind: ArtifactKind, name: &str) -> Option<u32> {
    let file_name = match kind {
        ArtifactKind::Apk => name,
        ArtifactKind::Aab => name.strip_prefix("base/dex/")?,
        ArtifactKind::Unknown => return None,
    };
    if file_name == "classes.dex" {
        return Some(1);
    }
    file_name
        .strip_prefix("classes")
        .and_then(|rest| rest.strip_suffix(".dex"))
        .and_then(|number| number.parse::<u32>().ok())
}

fn loader_dex_target(kind: ArtifactKind, index: u32) -> String {
    let file_name = if index <= 1 {
        "classes.dex".to_string()
    } else {
        format!("classes{index}.dex")
    };
    match kind {
        ArtifactKind::Apk | ArtifactKind::Unknown => file_name,
        ArtifactKind::Aab => format!("base/dex/{file_name}"),
    }
}

fn native_library_target(kind: ArtifactKind, abi: &str) -> String {
    match kind {
        ArtifactKind::Apk | ArtifactKind::Unknown => format!("lib/{abi}/{NATIVE_LIBRARY_NAME}"),
        ArtifactKind::Aab => format!("base/lib/{abi}/{NATIVE_LIBRARY_NAME}"),
    }
}

fn infer_abi(path: &Path) -> Option<&str> {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|abi| KNOWN_ABIS.contains(abi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DexFileInfo;
    use std::fs;

    fn artifact_with_dex(names: &[&str]) -> ArtifactInfo {
        ArtifactInfo {
            dex_files: names
                .iter()
                .map(|name| DexFileInfo {
                    name: (*name).to_string(),
                    ..DexFileInfo::default()
                })
                .collect(),
            ..ArtifactInfo::default()
        }
    }

    #[test]
    fn picks_next_apk_loader_dex_name_after_multidex() {
        let artifact = artifact_with_dex(&["classes.dex", "classes2.dex"]);
        assert_eq!(next_loader_dex_index(ArtifactKind::Apk, &artifact), 3);
        assert_eq!(loader_dex_target(ArtifactKind::Apk, 3), "classes3.dex");
    }

    #[test]
    fn picks_next_aab_loader_dex_name_after_base_dex() {
        let artifact = artifact_with_dex(&["base/dex/classes.dex", "base/dex/classes2.dex"]);
        assert_eq!(next_loader_dex_index(ArtifactKind::Aab, &artifact), 3);
        assert_eq!(
            loader_dex_target(ArtifactKind::Aab, 3),
            "base/dex/classes3.dex"
        );
    }

    #[test]
    fn maps_native_library_targets_by_artifact_kind() {
        assert_eq!(
            native_library_target(ArtifactKind::Apk, "arm64-v8a"),
            "lib/arm64-v8a/libprotector_vm.so"
        );
        assert_eq!(
            native_library_target(ArtifactKind::Aab, "arm64-v8a"),
            "base/lib/arm64-v8a/libprotector_vm.so"
        );
    }

    #[test]
    fn discovers_loader_artifacts_from_root() {
        let temp = tempfile::tempdir().unwrap();
        let dex = temp.path().join("classes.dex");
        let so_dir = temp.path().join("lib").join("arm64-v8a");
        fs::create_dir_all(&so_dir).unwrap();
        fs::write(&dex, b"dex").unwrap();
        fs::write(so_dir.join(NATIVE_LIBRARY_NAME), b"so").unwrap();

        let roots = vec![temp.path().to_path_buf()];

        assert_eq!(discover_loader_dex_files(&roots), vec![dex]);
        assert_eq!(discover_native_libraries(&roots).len(), 1);
    }

    #[test]
    fn embedded_loader_dex_contains_application_entrypoint() {
        let dex_strings = String::from_utf8_lossy(EMBEDDED_LOADER_DEX);
        assert!(dex_strings.contains("Lcom/protector/runtime/ProtectorApplication;"));
        assert!(dex_strings.contains("Lcom/protector/runtime/ProtectorRuntime;"));
        assert!(dex_strings.contains("Landroidx/core/app/CoreComponentFactory;"));
        assert!(dex_strings.contains("Ldalvik/system/DexClassLoader;"));
    }
}
