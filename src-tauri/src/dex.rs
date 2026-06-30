#[derive(Debug, Clone)]
pub struct DexMethodCandidate {
    pub dex_name: String,
    pub class_descriptor: String,
    pub method_name: String,
    pub access_flags: u32,
    pub code_units: u32,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DexParseResult {
    pub method_count: u32,
    pub class_count: u32,
    pub methods: Vec<DexMethodCandidate>,
}

const ACC_NATIVE: u32 = 0x0100;
const ACC_ABSTRACT: u32 = 0x0400;

pub fn parse_dex(dex_name: &str, bytes: &[u8]) -> Result<DexParseResult, String> {
    if bytes.len() < 112 || &bytes[0..3] != b"dex" {
        return Err("not a dex file".to_string());
    }

    let string_ids_size = read_u32(bytes, 56)? as usize;
    let string_ids_off = read_u32(bytes, 60)? as usize;
    let type_ids_size = read_u32(bytes, 64)? as usize;
    let type_ids_off = read_u32(bytes, 68)? as usize;
    let method_ids_size = read_u32(bytes, 88)? as usize;
    let method_ids_off = read_u32(bytes, 92)? as usize;
    let class_defs_size = read_u32(bytes, 96)? as usize;
    let class_defs_off = read_u32(bytes, 100)? as usize;

    let strings = read_strings(bytes, string_ids_size, string_ids_off)?;
    let types = read_types(bytes, type_ids_size, type_ids_off, &strings)?;
    let method_ids = read_method_ids(bytes, method_ids_size, method_ids_off, &strings, &types)?;

    let mut result = DexParseResult {
        method_count: method_ids_size as u32,
        class_count: class_defs_size as u32,
        methods: Vec::new(),
    };

    for class_index in 0..class_defs_size {
        let base = class_defs_off + class_index * 32;
        if base + 32 > bytes.len() {
            break;
        }
        let class_idx = read_u32(bytes, base)? as usize;
        let class_descriptor = types
            .get(class_idx)
            .cloned()
            .unwrap_or_else(|| format!("type#{}", class_idx));
        let class_data_off = read_u32(bytes, base + 24)? as usize;
        if class_data_off == 0 || class_data_off >= bytes.len() {
            continue;
        }

        parse_class_data(
            dex_name,
            bytes,
            class_data_off,
            &class_descriptor,
            &method_ids,
            &mut result.methods,
        )?;
    }

    Ok(result)
}

pub fn method_matches_rules(
    class_descriptor: &str,
    method_name: &str,
    include_rules: &[String],
    exclude_rules: &[String],
) -> bool {
    let haystack = format!("{}->{}", class_descriptor, method_name);
    let normalized_haystack = normalize_rule(&haystack);
    let excluded = exclude_rules
        .iter()
        .map(|rule| normalize_rule(rule))
        .any(|rule| !rule.is_empty() && normalized_haystack.contains(&rule));
    if excluded {
        return false;
    }

    if include_rules.is_empty() {
        return true;
    }

    include_rules
        .iter()
        .map(|rule| normalize_rule(rule))
        .any(|rule| !rule.is_empty() && normalized_haystack.contains(&rule))
}

fn normalize_rule(rule: &str) -> String {
    rule.trim()
        .trim_start_matches('L')
        .trim_end_matches(';')
        .replace(';', "")
        .replace('.', "/")
        .replace("::", "->")
        .to_ascii_lowercase()
}

fn parse_class_data(
    dex_name: &str,
    bytes: &[u8],
    mut cursor: usize,
    class_descriptor: &str,
    method_ids: &[MethodId],
    out: &mut Vec<DexMethodCandidate>,
) -> Result<(), String> {
    let static_fields_size = read_uleb128(bytes, &mut cursor)?;
    let instance_fields_size = read_uleb128(bytes, &mut cursor)?;
    let direct_methods_size = read_uleb128(bytes, &mut cursor)?;
    let virtual_methods_size = read_uleb128(bytes, &mut cursor)?;

    skip_encoded_fields(bytes, &mut cursor, static_fields_size)?;
    skip_encoded_fields(bytes, &mut cursor, instance_fields_size)?;
    parse_encoded_methods(
        dex_name,
        bytes,
        &mut cursor,
        direct_methods_size,
        class_descriptor,
        method_ids,
        out,
    )?;
    parse_encoded_methods(
        dex_name,
        bytes,
        &mut cursor,
        virtual_methods_size,
        class_descriptor,
        method_ids,
        out,
    )?;

    Ok(())
}

fn skip_encoded_fields(bytes: &[u8], cursor: &mut usize, count: u32) -> Result<(), String> {
    for _ in 0..count {
        let _field_idx_diff = read_uleb128(bytes, cursor)?;
        let _access_flags = read_uleb128(bytes, cursor)?;
    }
    Ok(())
}

fn parse_encoded_methods(
    dex_name: &str,
    bytes: &[u8],
    cursor: &mut usize,
    count: u32,
    class_descriptor: &str,
    method_ids: &[MethodId],
    out: &mut Vec<DexMethodCandidate>,
) -> Result<(), String> {
    let mut method_idx = 0u32;
    for _ in 0..count {
        method_idx = method_idx.saturating_add(read_uleb128(bytes, cursor)?);
        let access_flags = read_uleb128(bytes, cursor)?;
        let code_off = read_uleb128(bytes, cursor)? as usize;
        let method = method_ids.get(method_idx as usize);
        let method_name = method
            .map(|m| m.name.clone())
            .unwrap_or_else(|| format!("method#{}", method_idx));
        let owner = method
            .map(|m| m.class_descriptor.clone())
            .unwrap_or_else(|| class_descriptor.to_string());
        let code_units = if code_off > 0 && code_off + 16 <= bytes.len() {
            read_u32(bytes, code_off + 12).unwrap_or(0)
        } else {
            0
        };

        let skip_reason =
            classify_skip_reason(&owner, &method_name, access_flags, code_off, code_units);
        out.push(DexMethodCandidate {
            dex_name: dex_name.to_string(),
            class_descriptor: owner,
            method_name,
            access_flags,
            code_units,
            skip_reason,
        });
    }
    Ok(())
}

fn classify_skip_reason(
    class_descriptor: &str,
    method_name: &str,
    access_flags: u32,
    code_off: usize,
    code_units: u32,
) -> Option<String> {
    if method_name == "<init>" || method_name == "<clinit>" {
        return Some("constructor-or-class-initializer".to_string());
    }
    if access_flags & ACC_NATIVE != 0 {
        return Some("native-method".to_string());
    }
    if access_flags & ACC_ABSTRACT != 0 {
        return Some("abstract-method".to_string());
    }
    if code_off == 0 || code_units == 0 {
        return Some("no-code-item".to_string());
    }
    if is_component_lifecycle_entry(class_descriptor, method_name) {
        return Some("android-component-lifecycle".to_string());
    }
    None
}

fn is_component_lifecycle_entry(class_descriptor: &str, method_name: &str) -> bool {
    let lifecycle_name = matches!(
        method_name,
        "onCreate"
            | "attachBaseContext"
            | "onStart"
            | "onResume"
            | "onPause"
            | "onStop"
            | "onDestroy"
            | "onReceive"
            | "onBind"
            | "onStartCommand"
            | "onHandleIntent"
    );
    lifecycle_name
        && (class_descriptor.contains("/Application")
            || class_descriptor.contains("/Activity")
            || class_descriptor.contains("/Service")
            || class_descriptor.contains("/Receiver")
            || class_descriptor.contains("/Provider")
            || class_descriptor.ends_with("Application;")
            || class_descriptor.ends_with("Activity;")
            || class_descriptor.ends_with("Service;")
            || class_descriptor.ends_with("Receiver;")
            || class_descriptor.ends_with("Provider;"))
}

#[derive(Debug, Clone)]
struct MethodId {
    class_descriptor: String,
    name: String,
}

fn read_strings(bytes: &[u8], count: usize, offset: usize) -> Result<Vec<String>, String> {
    let mut strings = Vec::with_capacity(count);
    for index in 0..count {
        let item_off = offset + index * 4;
        let string_data_off = read_u32(bytes, item_off)? as usize;
        strings.push(read_string_data(bytes, string_data_off)?);
    }
    Ok(strings)
}

fn read_string_data(bytes: &[u8], offset: usize) -> Result<String, String> {
    if offset >= bytes.len() {
        return Err("string data offset out of bounds".to_string());
    }
    let mut cursor = offset;
    let _utf16_len = read_uleb128(bytes, &mut cursor)?;
    let end = bytes[cursor..]
        .iter()
        .position(|b| *b == 0)
        .map(|pos| cursor + pos)
        .ok_or_else(|| "unterminated dex string".to_string())?;
    Ok(String::from_utf8_lossy(&bytes[cursor..end]).to_string())
}

fn read_types(
    bytes: &[u8],
    count: usize,
    offset: usize,
    strings: &[String],
) -> Result<Vec<String>, String> {
    let mut types = Vec::with_capacity(count);
    for index in 0..count {
        let item_off = offset + index * 4;
        let descriptor_idx = read_u32(bytes, item_off)? as usize;
        types.push(
            strings
                .get(descriptor_idx)
                .cloned()
                .unwrap_or_else(|| format!("string#{}", descriptor_idx)),
        );
    }
    Ok(types)
}

fn read_method_ids(
    bytes: &[u8],
    count: usize,
    offset: usize,
    strings: &[String],
    types: &[String],
) -> Result<Vec<MethodId>, String> {
    let mut methods = Vec::with_capacity(count);
    for index in 0..count {
        let item_off = offset + index * 8;
        let class_idx = read_u16(bytes, item_off)? as usize;
        let name_idx = read_u32(bytes, item_off + 4)? as usize;
        methods.push(MethodId {
            class_descriptor: types
                .get(class_idx)
                .cloned()
                .unwrap_or_else(|| format!("type#{}", class_idx)),
            name: strings
                .get(name_idx)
                .cloned()
                .unwrap_or_else(|| format!("string#{}", name_idx)),
        });
    }
    Ok(methods)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "read_u16 out of bounds".to_string())?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "read_u32 out of bounds".to_string())?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_uleb128(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let mut result = 0u32;
    let mut shift = 0;
    for _ in 0..5 {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| "uleb128 out of bounds".to_string())?;
        *cursor += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
    Err("uleb128 too large".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_support_package_class_and_method_forms() {
        assert!(method_matches_rules(
            "Lcom/example/pay/Security;",
            "checkToken",
            &[String::from("com.example.pay")],
            &[]
        ));
        assert!(method_matches_rules(
            "Lcom/example/pay/Security;",
            "checkToken",
            &[String::from("Security::checkToken")],
            &[]
        ));
        assert!(!method_matches_rules(
            "Lcom/example/pay/Security;",
            "checkToken",
            &[String::from("com.example")],
            &[String::from("pay.Security")]
        ));
    }

    #[test]
    fn skip_lifecycle_entries_heuristically() {
        let reason = classify_skip_reason("Lcom/example/MainActivity;", "onCreate", 0, 120, 40);
        assert_eq!(reason.as_deref(), Some("android-component-lifecycle"));
    }
}
