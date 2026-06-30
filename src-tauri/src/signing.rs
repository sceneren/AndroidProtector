use crate::models::{SigningAliasInfo, SigningAliasInspection, SigningConfig, SigningValidation};
use crate::toolchain;
use regex::Regex;
use std::path::Path;

pub fn validate_signing_config(config: &SigningConfig) -> SigningValidation {
    let issues = validate_basic_fields(config, true);
    if !issues.is_empty() {
        return SigningValidation {
            valid: false,
            alias_found: false,
            certificate_summary: None,
            issues,
        };
    }

    let Some(keytool) = find_keytool() else {
        return SigningValidation {
            valid: false,
            alias_found: false,
            certificate_summary: None,
            issues: vec!["keytool not found in JAVA_HOME or PATH".to_string()],
        };
    };

    let mut command = toolchain::command_for_tool(&keytool);
    command
        .arg("-J-Duser.language=en")
        .arg("-J-Duser.country=US")
        .arg("-list")
        .arg("-v")
        .arg("-keystore")
        .arg(&config.keystore_path)
        .arg("-storepass")
        .arg(&config.store_password)
        .arg("-alias")
        .arg(&config.alias);
    if let Some(store_type) = config
        .store_type
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        command.arg("-storetype").arg(store_type);
    }

    match command.output() {
        Ok(output) if output.status.success() => {
            let text = command_output_text(&output);
            SigningValidation {
                valid: true,
                alias_found: true,
                certificate_summary: certificate_summary(&text),
                issues: Vec::new(),
            }
        }
        Ok(output) => SigningValidation {
            valid: false,
            alias_found: false,
            certificate_summary: None,
            issues: vec![summarize_output(&output)],
        },
        Err(err) => SigningValidation {
            valid: false,
            alias_found: false,
            certificate_summary: None,
            issues: vec![format!("failed to execute keytool: {err}")],
        },
    }
}

pub fn inspect_aliases(config: &SigningConfig) -> SigningAliasInspection {
    let issues = validate_basic_fields(config, false);
    if !issues.is_empty() {
        return SigningAliasInspection {
            valid: false,
            store_type: None,
            aliases: Vec::new(),
            issues,
        };
    }

    let Some(keytool) = find_keytool() else {
        return SigningAliasInspection {
            valid: false,
            store_type: None,
            aliases: Vec::new(),
            issues: vec!["keytool not found in JAVA_HOME or PATH".to_string()],
        };
    };

    let mut command = toolchain::command_for_tool(&keytool);
    command
        .arg("-J-Duser.language=en")
        .arg("-J-Duser.country=US")
        .arg("-list")
        .arg("-v")
        .arg("-keystore")
        .arg(&config.keystore_path)
        .arg("-storepass")
        .arg(&config.store_password);
    if let Some(store_type) = config
        .store_type
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        command.arg("-storetype").arg(store_type);
    }

    match command.output() {
        Ok(output) if output.status.success() => {
            let text = command_output_text(&output);
            let aliases = parse_aliases(&text);
            SigningAliasInspection {
                valid: !aliases.is_empty(),
                store_type: parse_store_type(&text),
                aliases,
                issues: Vec::new(),
            }
        }
        Ok(output) => SigningAliasInspection {
            valid: false,
            store_type: None,
            aliases: Vec::new(),
            issues: vec![summarize_output(&output)],
        },
        Err(err) => SigningAliasInspection {
            valid: false,
            store_type: None,
            aliases: Vec::new(),
            issues: vec![format!("failed to execute keytool: {err}")],
        },
    }
}

fn validate_basic_fields(config: &SigningConfig, require_alias: bool) -> Vec<String> {
    let mut issues = Vec::new();
    if config.keystore_path.trim().is_empty() {
        issues.push("keystore path is required".to_string());
    } else if !Path::new(&config.keystore_path).exists() {
        issues.push("keystore file does not exist".to_string());
    }
    if config.store_password.is_empty() {
        issues.push("store password is required".to_string());
    }
    if require_alias && config.alias.trim().is_empty() {
        issues.push("alias is required".to_string());
    }
    issues
}

fn parse_aliases(text: &str) -> Vec<SigningAliasInfo> {
    let alias_regex = Regex::new(r"(?i)^\s*Alias name:\s*(.+?)\s*$").expect("valid alias regex");
    let entry_regex = Regex::new(r"(?i)^\s*Entry type:\s*(.+?)\s*$").expect("valid entry regex");
    let sha_regex = Regex::new(r"(?i)^\s*SHA256:\s*(.+?)\s*$").expect("valid sha regex");
    let mut aliases = Vec::new();
    let mut current: Option<SigningAliasInfo> = None;

    for line in text.lines() {
        if let Some(captures) = alias_regex.captures(line) {
            if let Some(alias) = current.take() {
                aliases.push(alias);
            }
            current = Some(SigningAliasInfo {
                alias: captures
                    .get(1)
                    .map(|value| value.as_str().trim().to_string())
                    .unwrap_or_default(),
                entry_type: None,
                certificate_summary: None,
            });
        } else if let Some(captures) = entry_regex.captures(line) {
            if let Some(alias) = current.as_mut() {
                alias.entry_type = captures
                    .get(1)
                    .map(|value| value.as_str().trim().to_string());
            }
        } else if let Some(captures) = sha_regex.captures(line) {
            if let Some(alias) = current.as_mut() {
                alias.certificate_summary = captures
                    .get(1)
                    .map(|value| format!("SHA256 {}", value.as_str().trim()));
            }
        }
    }

    if let Some(alias) = current.take() {
        aliases.push(alias);
    }

    if aliases.is_empty() {
        parse_compact_aliases(text)
    } else {
        aliases
    }
}

fn parse_store_type(text: &str) -> Option<String> {
    let regex = Regex::new(r"(?i)^\s*Keystore type:\s*(.+?)\s*$").expect("valid store type regex");
    text.lines()
        .find_map(|line| regex.captures(line))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_string())
}

fn parse_compact_aliases(text: &str) -> Vec<SigningAliasInfo> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.contains(',') || !trimmed.to_ascii_lowercase().contains("entry") {
                return None;
            }
            trimmed.split(',').next().map(|alias| SigningAliasInfo {
                alias: alias.trim().to_string(),
                entry_type: None,
                certificate_summary: None,
            })
        })
        .collect()
}

fn certificate_summary(text: &str) -> Option<String> {
    text.lines()
        .find(|line| {
            line.contains("SHA256") || line.contains("Owner:") || line.contains("Alias name:")
        })
        .map(|line| line.trim().to_string())
}

fn summarize_output(output: &std::process::Output) -> String {
    let summary = command_output_text(output)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("\n");
    if summary.is_empty() {
        "keytool failed to validate keystore".to_string()
    } else {
        summary
    }
}

fn command_output_text(output: &std::process::Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn find_keytool() -> Option<String> {
    let exe = if cfg!(target_os = "windows") {
        "keytool.exe"
    } else {
        "keytool"
    };
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let candidate = Path::new(&java_home).join("bin").join(exe);
        if candidate.exists() {
            return Some(candidate.display().to_string());
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.exists())
        .map(|path| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verbose_keytool_aliases() {
        let text = "Alias name: release\nEntry type: PrivateKeyEntry\nSHA256: AA:BB\nAlias name: upload\nEntry type: PrivateKeyEntry";
        let aliases = parse_aliases(text);
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].alias, "release");
        assert_eq!(aliases[0].entry_type.as_deref(), Some("PrivateKeyEntry"));
    }

    #[test]
    fn parses_keystore_type() {
        assert_eq!(
            parse_store_type("Keystore type: PKCS12").as_deref(),
            Some("PKCS12")
        );
    }
}
