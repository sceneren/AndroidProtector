use crate::channel::{validate_channels, ChannelPackage, ChannelPackageResult};
use crate::models::ToolchainPaths;
use crate::toolchain;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn write_channel_packages(
    input: &Path,
    output_dir: &Path,
    channels: &[String],
    paths: Option<&ToolchainPaths>,
) -> Result<ChannelPackageResult, String> {
    let channels = validate_channels(channels)?;
    let input = input
        .canonicalize()
        .map(|path| java_compatible_path(&path))
        .map_err(|err| format!("[vasdolly] source APK: {err}"))?;
    let stem = input
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "[vasdolly] invalid APK filename".to_string())?;
    let detected = toolchain::detect_toolchain(paths);
    let java = toolchain::command_path(&detected.java)
        .ok_or_else(|| "[vasdolly] 未找到 Java，请安装 JDK 或设置 JAVA_HOME。".to_string())?;
    fs::create_dir_all(output_dir).map_err(|err| format!("[vasdolly] output directory: {err}"))?;
    let output_dir = output_dir
        .canonicalize()
        .map(|path| java_compatible_path(&path))
        .map_err(|err| format!("[vasdolly] {err}"))?;
    for channel in &channels {
        let target = output_dir.join(format!("{stem}_{channel}.apk"));
        if target.exists() {
            return Err(format!(
                "[vasdolly] 输出文件已存在，请选择其他目录：{}",
                target.display()
            ));
        }
    }
    // Stage all outputs before publishing. An old APK can never count as CLI success.
    let scratch = tempfile::tempdir_in(&output_dir).map_err(|err| format!("[vasdolly] {err}"))?;
    let jar = toolchain::resolve_vasdolly_jar(scratch.path())?;
    let staged_jar = scratch.path().join("VasDolly.jar");
    if jar != staged_jar {
        fs::copy(&jar, &staged_jar).map_err(|err| format!("[vasdolly] prepare library: {err}"))?;
    }
    let bridge = scratch.path().join("VasDollyBridge.jar");
    fs::write(
        &bridge,
        include_bytes!("../../tools/vasdolly/VasDollyBridge.jar"),
    )
    .map_err(|err| format!("[vasdolly] prepare adapter: {err}"))?;
    let mut staged = Vec::new();
    for channel in channels {
        let filename = format!("{stem}_{channel}.apk");
        let candidate = scratch.path().join(&filename);
        let output = run_cli(
            &java,
            &bridge,
            scratch.path(),
            &[&input, &candidate],
            &channel,
        )?;
        ensure_verified_output(&output, &candidate, &channel)?;
        staged.push((candidate, output_dir.join(filename), channel));
    }
    let mut packages = Vec::new();
    for (candidate, target, channel) in staged {
        // No-clobber publication also protects files created during the CLI run.
        let temp_path = tempfile::TempPath::try_from_path(candidate)
            .map_err(|err| format!("[vasdolly] stage APK: {err}"))?;
        temp_path
            .persist_noclobber(&target)
            .map_err(|err| format!("[vasdolly] publish APK: {err}"))?;
        packages.push(ChannelPackage {
            value: channel.clone(),
            channel,
            path: target.display().to_string(),
        });
    }
    Ok(ChannelPackageResult {
        output_dir: output_dir.display().to_string(),
        packages,
    })
}

fn run_cli(
    java: &str,
    jar: &Path,
    cwd: &Path,
    files: &[&Path],
    channel: &str,
) -> Result<String, String> {
    let mut command = Command::new(java);
    toolchain::hide_command_window(&mut command);
    let output = command
        .current_dir(cwd)
        .arg("-Dfile.encoding=UTF-8")
        .arg("-jar")
        .arg(java_compatible_path(jar))
        .args(files.iter().map(|path| java_compatible_path(path)))
        .arg(channel)
        .output()
        .map_err(|err| format!("[vasdolly] failed to start Java: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || stderr.contains("Exception") {
        return Err(format!("[vasdolly] {}\n{}", stdout.trim(), stderr.trim()));
    }
    Ok(stdout)
}

fn java_compatible_path(path: &Path) -> std::path::PathBuf {
    // Rust canonicalize returns extended-length Windows paths, which Java's
    // JAR launcher cannot open. Preserve UNC paths when removing that prefix.
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return std::path::PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(disk) = value.strip_prefix(r"\\?\") {
            return std::path::PathBuf::from(disk);
        }
    }
    path.to_path_buf()
}

fn ensure_verified_output(stdout: &str, apk: &Path, channel: &str) -> Result<(), String> {
    // The adapter emits this marker only after official API read-back and
    // signature verification. The exit code alone never proves success.
    let expected = format!("VASDOLLY_VERIFIED_CHANNEL:{channel}");
    if !apk.is_file()
        || fs::metadata(apk).map(|m| m.len() == 0).unwrap_or(true)
        || !stdout.lines().any(|line| line.trim_end() == expected)
    {
        return Err(format!(
            "[vasdolly] 未生成通过签名校验的渠道包：{}",
            stdout.trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn java_paths_preserve_drive_and_unc_paths() {
        assert_eq!(
            java_compatible_path(Path::new(r"\\?\C:\渠道 & files\app.apk")),
            Path::new(r"C:\渠道 & files\app.apk")
        );
        assert_eq!(
            java_compatible_path(Path::new(r"\\?\UNC\server\share\app.apk")),
            Path::new(r"\\server\share\app.apk")
        );
    }

    #[test]
    fn zero_exit_or_existing_file_does_not_prove_success() {
        let temp = tempfile::tempdir().unwrap();
        let apk = temp.path().join("out.apk");
        assert!(
            ensure_verified_output("VASDOLLY_VERIFIED_CHANNEL:huawei", &apk, "huawei").is_err()
        );
        fs::write(&apk, b"apk").unwrap();
        assert!(ensure_verified_output("generatedChannelApk error", &apk, "huawei").is_err());
        assert!(ensure_verified_output("VASDOLLY_VERIFIED_CHANNEL:yyb", &apk, "huawei").is_err());
        assert!(
            ensure_verified_output("VASDOLLY_VERIFIED_CHANNEL:huawei\n", &apk, "huawei").is_ok()
        );
    }

    #[test]
    fn packaging_method_ipc_values_are_stable() {
        use crate::models::ChannelPackagingMethod;
        assert_eq!(
            serde_json::to_string(&ChannelPackagingMethod::VasDolly).unwrap(),
            "\"vasDolly\""
        );
        assert_eq!(
            serde_json::from_str::<ChannelPackagingMethod>("\"existing\"").unwrap(),
            ChannelPackagingMethod::default()
        );
        assert!(serde_json::from_str::<ChannelPackagingMethod>("\"unknown\"").is_err());
    }
}
