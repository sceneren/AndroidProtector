use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Apk,
    Aab,
    Unknown,
}

impl Default for ArtifactKind {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildToolInfo {
    pub version: String,
    pub path: String,
    pub zipalign: Option<String>,
    pub apksigner: Option<String>,
    pub aapt2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub ok: bool,
    pub java_home: Option<String>,
    pub android_sdk: Option<String>,
    pub java: ToolStatus,
    pub javac: ToolStatus,
    pub jarsigner: ToolStatus,
    pub bundletool: ToolStatus,
    pub build_tools: Vec<BuildToolInfo>,
    pub selected_build_tools: Option<BuildToolInfo>,
    pub zipalign: ToolStatus,
    pub apksigner: ToolStatus,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainPaths {
    pub java_home: Option<String>,
    pub android_sdk: Option<String>,
    pub build_tools_dir: Option<String>,
    pub zipalign: Option<String>,
    pub apksigner: Option<String>,
    pub jarsigner: Option<String>,
    pub bundletool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DexFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub method_count: u32,
    pub class_count: u32,
    pub virtualizable_methods: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactInfo {
    pub path: String,
    pub file_name: String,
    pub kind: ArtifactKind,
    pub size_bytes: u64,
    pub package_name: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub application_class: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub dex_files: Vec<DexFileInfo>,
    pub native_abis: Vec<String>,
    pub signed: bool,
    pub signature_schemes: Vec<String>,
    pub entry_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmpOptions {
    pub enabled: bool,
    pub include_rules: Vec<String>,
    pub exclude_rules: Vec<String>,
    pub max_method_size: u32,
    pub abi_selection: Vec<String>,
    pub unsupported_method_policy: String,
}

impl Default for VmpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            include_rules: Vec::new(),
            exclude_rules: Vec::new(),
            max_method_size: 800,
            abi_selection: vec![
                "arm64-v8a".to_string(),
                "armeabi-v7a".to_string(),
                "x86_64".to_string(),
            ],
            unsupported_method_policy: "report".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionOptions {
    pub dex_encryption: bool,
    pub anti_debug: bool,
    pub signature_tamper_check: bool,
    pub legacy_api_fallback: bool,
}

impl Default for ProtectionOptions {
    fn default() -> Self {
        Self {
            dex_encryption: true,
            anti_debug: true,
            signature_tamper_check: true,
            legacy_api_fallback: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningConfig {
    pub keystore_path: String,
    pub store_password: String,
    pub key_password: Option<String>,
    pub alias: String,
    pub store_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningProfile {
    pub id: String,
    pub name: String,
    pub keystore_path: String,
    pub alias: String,
    pub store_type: Option<String>,
    pub certificate_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub keystore_path: String,
    pub store_password: String,
    pub key_password: Option<String>,
    pub alias: String,
    pub store_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningAliasInfo {
    pub alias: String,
    pub entry_type: Option<String>,
    pub certificate_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningAliasInspection {
    pub valid: bool,
    pub store_type: Option<String>,
    pub aliases: Vec<SigningAliasInfo>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    pub signing_profiles: Vec<SigningProfile>,
    pub last_output_dir: Option<String>,
    pub selected_signing_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectRequest {
    pub input_path: String,
    pub output_dir: String,
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default)]
    pub vmp_options: VmpOptions,
    #[serde(default)]
    pub protection_options: ProtectionOptions,
    pub signing_config: Option<SigningConfig>,
    pub signing_profile_id: Option<String>,
    pub toolchain_paths: Option<ToolchainPaths>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkipReason {
    pub reason: String,
    pub count: u32,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VmpPlan {
    pub enabled: bool,
    pub candidate_methods: u32,
    pub virtualized_methods: u32,
    pub skipped_methods: u32,
    pub skipped_reasons: Vec<SkipReason>,
    pub risk_level: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningValidation {
    pub valid: bool,
    pub alias_found: bool,
    pub certificate_summary: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobLifecycle {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl Default for JobLifecycle {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobLogEntry {
    pub timestamp: String,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub id: String,
    pub lifecycle: JobLifecycle,
    pub stage: String,
    pub progress: f32,
    pub logs: Vec<JobLogEntry>,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
