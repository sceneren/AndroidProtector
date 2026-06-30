use crate::models::{BuildToolInfo, ToolStatus, ToolchainPaths, ToolchainStatus};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn detect_toolchain(paths: Option<&ToolchainPaths>) -> ToolchainStatus {
    let java_home = paths
        .and_then(|p| clean_path_opt(p.java_home.as_deref()))
        .or_else(bundled_java_home)
        .or_else(|| env::var("JAVA_HOME").ok());
    let android_sdk = paths
        .and_then(|p| clean_path_opt(p.android_sdk.as_deref()))
        .or_else(bundled_android_sdk)
        .or_else(|| env::var("ANDROID_HOME").ok())
        .or_else(|| env::var("ANDROID_SDK_ROOT").ok())
        .or_else(common_android_sdk_path);

    let java = resolve_tool("java", java_home.as_deref(), None);
    let javac = resolve_tool("javac", java_home.as_deref(), None);
    let jarsigner = paths
        .and_then(|p| clean_path_opt(p.jarsigner.as_deref()))
        .map(|path| tool_status_from_path(&path))
        .unwrap_or_else(|| resolve_tool("jarsigner", java_home.as_deref(), None));

    let build_tools = collect_build_tools(android_sdk.as_deref());
    let selected_build_tools = select_build_tools(paths, &build_tools);
    let zipalign = paths
        .and_then(|p| clean_path_opt(p.zipalign.as_deref()))
        .map(|path| tool_status_from_path(&path))
        .unwrap_or_else(|| {
            selected_build_tools
                .as_ref()
                .and_then(|bt| bt.zipalign.as_ref())
                .map(|path| tool_status_from_path(path))
                .unwrap_or_default()
        });
    let apksigner = paths
        .and_then(|p| clean_path_opt(p.apksigner.as_deref()))
        .map(|path| tool_status_from_path(&path))
        .unwrap_or_else(|| {
            selected_build_tools
                .as_ref()
                .and_then(|bt| bt.apksigner.as_ref())
                .map(|path| tool_status_from_path(path))
                .unwrap_or_default()
        });
    let bundletool = resolve_bundletool(paths);

    let mut issues = Vec::new();
    if !java.available {
        issues.push("java not found".to_string());
    }
    if !jarsigner.available {
        issues.push("jarsigner not found; AAB signing will be unavailable".to_string());
    }
    if android_sdk.is_none() {
        issues.push(
            "Android SDK not found; place it under tools/android-sdk or set ANDROID_HOME"
                .to_string(),
        );
    }
    if !zipalign.available {
        issues.push("zipalign not found in selected Android build-tools".to_string());
    }
    if !apksigner.available {
        issues.push("apksigner not found in selected Android build-tools".to_string());
    }

    let ok = java.available && zipalign.available && apksigner.available;
    ToolchainStatus {
        ok,
        java_home,
        android_sdk,
        java,
        javac,
        jarsigner,
        bundletool,
        build_tools,
        selected_build_tools,
        zipalign,
        apksigner,
        issues,
    }
}

pub fn command_path(status: &ToolStatus) -> Option<String> {
    status.path.clone().filter(|_| status.available)
}

fn select_build_tools(
    paths: Option<&ToolchainPaths>,
    build_tools: &[BuildToolInfo],
) -> Option<BuildToolInfo> {
    if let Some(dir) = paths.and_then(|p| clean_path_opt(p.build_tools_dir.as_deref())) {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Some(build_tool_info(&path));
        }
    }
    build_tools
        .iter()
        .find(|tool| tool.zipalign.is_some() && tool.apksigner.is_some())
        .cloned()
        .or_else(|| build_tools.first().cloned())
}

fn collect_build_tools(android_sdk: Option<&str>) -> Vec<BuildToolInfo> {
    let Some(sdk) = android_sdk else {
        return Vec::new();
    };
    let root = Path::new(sdk).join("build-tools");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut tools = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| build_tool_info(&entry.path()))
        .collect::<Vec<_>>();
    tools.sort_by(|a, b| version_sort_key(&b.version).cmp(&version_sort_key(&a.version)));
    tools
}

fn build_tool_info(path: &Path) -> BuildToolInfo {
    let version = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let zipalign = existing_tool_in_dir(path, "zipalign");
    let apksigner = existing_tool_in_dir(path, "apksigner");
    let aapt2 = existing_tool_in_dir(path, "aapt2");
    BuildToolInfo {
        version,
        path: path.display().to_string(),
        zipalign,
        apksigner,
        aapt2,
    }
}

fn resolve_tool(name: &str, java_home: Option<&str>, version_arg: Option<&str>) -> ToolStatus {
    if let Some(home) = java_home {
        if let Some(path) = existing_tool_in_dir(&Path::new(home).join("bin"), name) {
            return tool_status_with_version(&path, version_arg.unwrap_or("-version"));
        }
    }
    find_on_path(name)
        .map(|path| tool_status_with_version(&path, version_arg.unwrap_or("-version")))
        .unwrap_or_default()
}

fn resolve_bundletool(paths: Option<&ToolchainPaths>) -> ToolStatus {
    if let Some(path) = paths.and_then(|p| clean_path_opt(p.bundletool.as_deref())) {
        return tool_status_from_path(&path);
    }
    if let Ok(path) = env::var("BUNDLETOOL") {
        return tool_status_from_path(&path);
    }
    if let Some(path) = find_on_path("bundletool") {
        return tool_status_with_version(&path, "version");
    }
    for candidate in common_bundletool_candidates() {
        if candidate.exists() {
            let path = candidate.display().to_string();
            return ToolStatus {
                available: true,
                version: run_java_jar_version(&path),
                path: Some(path),
            };
        }
    }
    ToolStatus::default()
}

fn common_bundletool_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("tools").join("bundletool.jar"));
        candidates.push(cwd.join("tools").join("bundletool").join("bundletool.jar"));
        candidates.push(cwd.join("tools").join("android-sdk").join("bundletool.jar"));
        candidates.push(cwd.join("bundletool.jar"));
    }
    for root in bundled_toolchain_roots() {
        candidates.push(root.join("bundletool.jar"));
        candidates.push(root.join("bundletool").join("bundletool.jar"));
        candidates.push(root.join("android-sdk").join("bundletool.jar"));
        candidates.push(
            root.join("android-sdk")
                .join("cmdline-tools")
                .join("latest")
                .join("bin")
                .join(first_tool_name("bundletool")),
        );
    }
    if let Ok(sdk) = env::var("ANDROID_HOME").or_else(|_| env::var("ANDROID_SDK_ROOT")) {
        candidates.push(Path::new(&sdk).join("bundletool.jar"));
        candidates.push(
            Path::new(&sdk)
                .join("cmdline-tools")
                .join("latest")
                .join("bin")
                .join(first_tool_name("bundletool")),
        );
    }
    candidates
}

fn common_android_sdk_path() -> Option<String> {
    if cfg!(target_os = "windows") {
        env::var("LOCALAPPDATA")
            .ok()
            .map(|dir| Path::new(&dir).join("Android").join("Sdk"))
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
    } else if cfg!(target_os = "macos") {
        env::var("HOME")
            .ok()
            .map(|dir| Path::new(&dir).join("Library").join("Android").join("sdk"))
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
    } else {
        env::var("HOME")
            .ok()
            .map(|dir| Path::new(&dir).join("Android").join("Sdk"))
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
    }
}

fn existing_tool_in_dir(dir: &Path, name: &str) -> Option<String> {
    tool_names(name)
        .into_iter()
        .map(|tool_name| dir.join(tool_name))
        .find(|candidate| candidate.exists())
        .map(|candidate| candidate.display().to_string())
}

fn find_on_path(name: &str) -> Option<String> {
    let path_var = env::var_os("PATH")?;
    let names = tool_names(name);
    for dir in env::split_paths(&path_var) {
        for name in &names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

fn first_tool_name(name: &str) -> String {
    tool_names(name).remove(0)
}

fn tool_names(name: &str) -> Vec<String> {
    if Path::new(name).extension().is_some() || !cfg!(target_os = "windows") {
        vec![name.to_string()]
    } else {
        vec![
            format!("{name}.exe"),
            format!("{name}.bat"),
            format!("{name}.cmd"),
            name.to_string(),
        ]
    }
}

fn tool_status_from_path(path: &str) -> ToolStatus {
    let exists = Path::new(path).exists();
    ToolStatus {
        available: exists,
        path: exists.then(|| path.to_string()),
        version: exists.then(|| "configured".to_string()),
    }
}

fn tool_status_with_version(path: &str, arg: &str) -> ToolStatus {
    let version = run_version(path, arg);
    ToolStatus {
        available: Path::new(path).exists(),
        path: Some(path.to_string()),
        version,
    }
}

fn run_version(path: &str, arg: &str) -> Option<String> {
    command_for_tool(path)
        .arg(arg)
        .output()
        .ok()
        .and_then(|output| {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            text.lines().next().map(|line| line.trim().to_string())
        })
}

pub fn command_for_tool(path: &str) -> Command {
    let mut command = if cfg!(target_os = "windows")
        && (path.to_ascii_lowercase().ends_with(".bat")
            || path.to_ascii_lowercase().ends_with(".cmd"))
    {
        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(path);
        command
    } else {
        Command::new(path)
    };
    hide_command_window(&mut command);
    command
}

pub fn hide_command_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn clean_path_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bundled_java_home() -> Option<String> {
    for root in bundled_toolchain_roots() {
        for candidate in [
            root.join("jdk"),
            root.join("java"),
            root.join("jre"),
            root.join("toolchain").join("jdk"),
        ] {
            if existing_tool_in_dir(&candidate.join("bin"), "java").is_some() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

fn bundled_android_sdk() -> Option<String> {
    for root in bundled_toolchain_roots() {
        for candidate in [
            root.join("android-sdk"),
            root.join("sdk"),
            root.join("Android").join("Sdk"),
            root.join("toolchain").join("android-sdk"),
        ] {
            if candidate.join("build-tools").exists() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

fn bundled_toolchain_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(path) = env::var("ANDROID_PROTECTOR_TOOLCHAIN") {
        roots.push(PathBuf::from(path));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("tools"));
            roots.push(dir.join("toolchain"));
            if let Some(parent) = dir.parent() {
                roots.push(parent.join("tools"));
                roots.push(parent.join("toolchain"));
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd.join("tools"));
        roots.push(cwd.join("src-tauri").join("resources").join("toolchain"));
    }
    roots
}

fn run_java_jar_version(path: &str) -> Option<String> {
    command_for_tool("java")
        .args(["-jar", path, "version"])
        .output()
        .ok()
        .and_then(|output| {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            text.lines().next().map(|line| line.trim().to_string())
        })
}

fn version_sort_key(version: &str) -> Vec<u32> {
    version
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_key_orders_numeric_segments() {
        assert!(version_sort_key("37.0.0") > version_sort_key("9.0.0"));
        assert_eq!(version_sort_key("36.1.0-rc1"), vec![36, 1, 0, 1]);
    }

    #[test]
    fn windows_tool_names_include_batch_wrappers() {
        let names = tool_names("apksigner");
        if cfg!(target_os = "windows") {
            assert!(names.contains(&"apksigner.bat".to_string()));
            assert!(names.contains(&"apksigner.cmd".to_string()));
        } else {
            assert_eq!(names, vec!["apksigner".to_string()]);
        }
    }

    #[test]
    fn clean_path_ignores_blank_values() {
        assert_eq!(clean_path_opt(Some("  ")), None);
        assert_eq!(clean_path_opt(Some(" /sdk ")).as_deref(), Some("/sdk"));
    }
}
