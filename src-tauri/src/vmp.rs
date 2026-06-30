use crate::dex::{self, DexMethodCandidate};
use crate::models::{ArtifactKind, ProtectRequest, SkipReason, VmpOptions, VmpPlan};
use crate::scan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmpMethodEntry {
    pub dex_name: String,
    pub class_descriptor: String,
    pub method_name: String,
    pub access_flags: u32,
    pub code_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmpManifest {
    pub enabled: bool,
    pub vm_version: String,
    pub selected_methods: Vec<VmpMethodEntry>,
    pub skipped_reasons: Vec<SkipReason>,
}

pub fn estimate_vmp(request: &ProtectRequest) -> Result<VmpPlan, String> {
    if !request.vmp_options.enabled {
        return Ok(VmpPlan {
            enabled: false,
            risk_level: "off".to_string(),
            notes: vec!["VMP disabled; DEX encryption and loader stages can still run".to_string()],
            ..VmpPlan::default()
        });
    }
    let manifest = build_vmp_manifest(request)?;
    let virtualized_methods = manifest.selected_methods.len() as u32;
    let skipped_methods = manifest
        .skipped_reasons
        .iter()
        .map(|reason| reason.count)
        .sum::<u32>();
    let risk_level = risk_level(virtualized_methods, &request.vmp_options);
    let mut notes = vec![
        "VMP v1 uses selective DEX-bytecode virtualization boundaries".to_string(),
        "Unsupported or high-risk methods are left unchanged and reported".to_string(),
    ];
    if request.vmp_options.include_rules.is_empty() {
        notes.push(
            "No include rules set; all supported methods are candidates after exclusions"
                .to_string(),
        );
    }

    Ok(VmpPlan {
        enabled: true,
        candidate_methods: virtualized_methods + skipped_methods,
        virtualized_methods,
        skipped_methods,
        skipped_reasons: manifest.skipped_reasons,
        risk_level,
        notes,
    })
}

pub fn build_vmp_manifest(request: &ProtectRequest) -> Result<VmpManifest, String> {
    let input = Path::new(&request.input_path);
    let kind = request
        .artifact_kind
        .unwrap_or_else(|| scan::artifact_kind_from_path(input));
    if kind == ArtifactKind::Unknown {
        return Err("cannot estimate VMP for unknown artifact type".to_string());
    }

    let file = File::open(input).map_err(|err| format!("failed to open artifact: {err}"))?;
    let mut zip = ZipArchive::new(file).map_err(|err| format!("failed to read zip: {err}"))?;
    let mut selected_methods = Vec::new();
    let mut skipped = BTreeMap::<String, (u32, Vec<String>)>::new();

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("failed to read zip entry #{index}: {err}"))?;
        let name = entry.name().replace('\\', "/");
        if !scan::is_dex_entry(&name, kind) {
            continue;
        }

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("failed to read dex {name}: {err}"))?;
        match dex::parse_dex(&name, &bytes) {
            Ok(parsed) => {
                for method in parsed.methods {
                    evaluate_method(
                        &method,
                        &request.vmp_options,
                        &mut selected_methods,
                        &mut skipped,
                    );
                }
            }
            Err(err) => add_skip(&mut skipped, "dex-parse-failed", format!("{name}: {err}")),
        }
    }

    Ok(VmpManifest {
        enabled: request.vmp_options.enabled,
        vm_version: "dex-bytecode-vm-v1".to_string(),
        selected_methods,
        skipped_reasons: skipped
            .into_iter()
            .map(|(reason, (count, examples))| SkipReason {
                reason,
                count,
                examples,
            })
            .collect(),
    })
}

fn evaluate_method(
    method: &DexMethodCandidate,
    options: &VmpOptions,
    selected_methods: &mut Vec<VmpMethodEntry>,
    skipped: &mut BTreeMap<String, (u32, Vec<String>)>,
) {
    let display = format!("{}->{}", method.class_descriptor, method.method_name);
    if let Some(reason) = &method.skip_reason {
        add_skip(skipped, reason, display);
        return;
    }
    if method.code_units > options.max_method_size {
        add_skip(skipped, "method-too-large", display);
        return;
    }
    if !dex::method_matches_rules(
        &method.class_descriptor,
        &method.method_name,
        &options.include_rules,
        &options.exclude_rules,
    ) {
        add_skip(skipped, "not-selected-by-rules", display);
        return;
    }

    selected_methods.push(VmpMethodEntry {
        dex_name: method.dex_name.clone(),
        class_descriptor: method.class_descriptor.clone(),
        method_name: method.method_name.clone(),
        access_flags: method.access_flags,
        code_units: method.code_units,
    });
}

fn add_skip(skipped: &mut BTreeMap<String, (u32, Vec<String>)>, reason: &str, example: String) {
    let entry = skipped.entry(reason.to_string()).or_insert((0, Vec::new()));
    entry.0 += 1;
    if entry.1.len() < 3 {
        entry.1.push(example);
    }
}

fn risk_level(virtualized_methods: u32, options: &VmpOptions) -> String {
    if virtualized_methods == 0 {
        "none".to_string()
    } else if options.include_rules.is_empty() || virtualized_methods > 5_000 {
        "high".to_string()
    } else if virtualized_methods > 1_000 {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_increases_when_all_methods_are_selected() {
        let options = VmpOptions {
            enabled: true,
            include_rules: Vec::new(),
            ..VmpOptions::default()
        };
        assert_eq!(risk_level(10, &options), "high");
    }

    #[test]
    fn risk_is_low_for_targeted_small_selection() {
        let options = VmpOptions {
            enabled: true,
            include_rules: vec!["com.example.pay".to_string()],
            ..VmpOptions::default()
        };
        assert_eq!(risk_level(10, &options), "low");
    }
}
