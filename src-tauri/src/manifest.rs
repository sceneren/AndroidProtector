use crate::models::ArtifactKind;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

pub const PROTECTOR_APPLICATION: &str = "com.protector.runtime.ProtectorApplication";

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const NO_INDEX: u32 = 0xffff_ffff;
const UTF8_FLAG: u32 = 0x0000_0100;
const TYPE_STRING: u8 = 0x03;
const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";

#[derive(Debug, Clone, Default)]
pub struct ManifestInfo {
    pub package_name: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub application_class: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManifestPatch {
    pub entry_name: String,
    pub bytes: Vec<u8>,
    pub package_name: Option<String>,
    pub original_application: Option<String>,
    pub protector_application: String,
    pub status: String,
    pub issues: Vec<String>,
}

pub fn inspect_manifest_bytes(bytes: &[u8]) -> Result<ManifestInfo, String> {
    if is_binary_xml(bytes) {
        inspect_binary_manifest(bytes)
    } else {
        Ok(inspect_text_manifest(&String::from_utf8_lossy(bytes)))
    }
}

pub fn patch_manifest_in_artifact(
    input: &Path,
    kind: ArtifactKind,
) -> Result<ManifestPatch, String> {
    let entry_name =
        manifest_entry_name(kind).ok_or_else(|| "unsupported artifact type".to_string())?;
    let file = File::open(input).map_err(|err| format!("failed to open artifact: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("failed to read zip: {err}"))?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|err| format!("manifest not found at {entry_name}: {err}"))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read manifest: {err}"))?;

    let patched = patch_manifest_bytes(&bytes)?;
    Ok(ManifestPatch {
        entry_name: entry_name.to_string(),
        bytes: patched.bytes,
        package_name: patched.info.package_name,
        original_application: patched.original_application,
        protector_application: PROTECTOR_APPLICATION.to_string(),
        status: patched.status,
        issues: patched.issues,
    })
}

fn manifest_entry_name(kind: ArtifactKind) -> Option<&'static str> {
    match kind {
        ArtifactKind::Apk => Some("AndroidManifest.xml"),
        ArtifactKind::Aab => Some("base/manifest/AndroidManifest.xml"),
        ArtifactKind::Unknown => None,
    }
}

fn patch_manifest_bytes(bytes: &[u8]) -> Result<PatchedManifestBytes, String> {
    if is_binary_xml(bytes) {
        patch_binary_manifest(bytes)
    } else {
        patch_text_manifest(&String::from_utf8_lossy(bytes))
    }
}

fn is_binary_xml(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && read_u16(bytes, 0) == Some(RES_XML_TYPE)
}

#[derive(Debug, Clone)]
struct PatchedManifestBytes {
    bytes: Vec<u8>,
    info: ManifestInfo,
    original_application: Option<String>,
    status: String,
    issues: Vec<String>,
}

fn inspect_text_manifest(text: &str) -> ManifestInfo {
    ManifestInfo {
        package_name: capture_attr(text, "package"),
        version_name: capture_attr(text, "android:versionName")
            .or_else(|| capture_attr(text, "versionName")),
        version_code: capture_attr(text, "android:versionCode")
            .or_else(|| capture_attr(text, "versionCode")),
        application_class: capture_application_name(text),
        min_sdk: capture_attr(text, "android:minSdkVersion")
            .or_else(|| capture_attr(text, "minSdkVersion")),
        target_sdk: capture_attr(text, "android:targetSdkVersion")
            .or_else(|| capture_attr(text, "targetSdkVersion")),
    }
}

fn patch_text_manifest(text: &str) -> Result<PatchedManifestBytes, String> {
    let info = inspect_text_manifest(text);
    let original_application = info
        .application_class
        .as_deref()
        .and_then(|name| normalize_application_name(info.package_name.as_deref(), name));
    let app_tag_regex = Regex::new(r#"(?s)<application\b[^>]*>"#).map_err(|err| err.to_string())?;
    let Some(app_match) = app_tag_regex.find(text) else {
        return Err("manifest application tag not found".to_string());
    };

    let app_tag = app_match.as_str();
    let name_regex = Regex::new(r#"(?s)((?:android:)?name\s*=\s*)["'][^"']*["']"#)
        .map_err(|err| err.to_string())?;
    let patched_tag = if name_regex.is_match(app_tag) {
        name_regex
            .replace(app_tag, format!("$1\"{PROTECTOR_APPLICATION}\""))
            .to_string()
    } else {
        let insert_at = app_tag
            .rfind("/>")
            .or_else(|| app_tag.rfind('>'))
            .ok_or_else(|| "malformed application tag".to_string())?;
        let mut tag = app_tag.to_string();
        tag.insert_str(
            insert_at,
            &format!(" android:name=\"{PROTECTOR_APPLICATION}\""),
        );
        tag
    };

    let mut output = text.to_string();
    output.replace_range(app_match.range(), &patched_tag);

    Ok(PatchedManifestBytes {
        bytes: output.into_bytes(),
        info,
        original_application,
        status: "text manifest patched".to_string(),
        issues: Vec::new(),
    })
}

fn capture_attr(text: &str, attr: &str) -> Option<String> {
    let pattern = format!(r#"{}\s*=\s*["']([^"']+)["']"#, regex::escape(attr));
    Regex::new(&pattern)
        .ok()
        .and_then(|regex| regex.captures(text))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn capture_application_name(text: &str) -> Option<String> {
    Regex::new(r#"<application\b[^>]*(?:android:)?name\s*=\s*["']([^"']+)["']"#)
        .ok()
        .and_then(|regex| regex.captures(text))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn inspect_binary_manifest(bytes: &[u8]) -> Result<ManifestInfo, String> {
    let document = BinaryXmlDocument::parse(bytes)?;
    Ok(document.manifest_info())
}

fn patch_binary_manifest(bytes: &[u8]) -> Result<PatchedManifestBytes, String> {
    let document = BinaryXmlDocument::parse(bytes)?;
    let mut info = document.manifest_info();
    let original_application = info
        .application_class
        .as_deref()
        .and_then(|name| normalize_application_name(info.package_name.as_deref(), name));

    let application = document
        .application
        .as_ref()
        .ok_or_else(|| "manifest application tag not found".to_string())?;
    let existing_name_attr = application.name_attr_offset;
    let mut additions = vec![PROTECTOR_APPLICATION.to_string()];
    if existing_name_attr.is_none() {
        if document.string_index("name").is_none() {
            additions.push("name".to_string());
        }
        if document.string_index(ANDROID_NS).is_none() {
            additions.push(ANDROID_NS.to_string());
        }
    }

    let rebuilt_pool = document
        .string_pool
        .rebuild_with_additions(bytes, &additions)?;
    let protector_index = rebuilt_pool
        .index_of(PROTECTOR_APPLICATION)
        .ok_or_else(|| "failed to allocate protector application string".to_string())?;
    let name_index = rebuilt_pool
        .index_of("name")
        .ok_or_else(|| "failed to resolve android:name string".to_string())?;
    let android_ns_index = rebuilt_pool
        .index_of(ANDROID_NS)
        .ok_or_else(|| "failed to resolve android namespace string".to_string())?;

    let output_capacity = (bytes.len() as isize + rebuilt_pool.size_delta() + 32).max(0) as usize;
    let mut output = Vec::with_capacity(output_capacity);
    let mut cursor = 0usize;
    let mut xml_size_delta = rebuilt_pool.size_delta();
    output.extend_from_slice(&bytes[cursor..document.string_pool.chunk_start]);
    cursor = document.string_pool.chunk_start + document.string_pool.chunk_size;
    output.extend_from_slice(&rebuilt_pool.bytes);

    while cursor < bytes.len() {
        if cursor == application.chunk_offset {
            let (patched_chunk, delta) = patch_application_chunk(
                &bytes[cursor..cursor + application.chunk_size],
                existing_name_attr.map(|offset| offset - application.chunk_offset),
                protector_index,
                name_index,
                android_ns_index,
            )?;
            xml_size_delta += delta;
            output.extend_from_slice(&patched_chunk);
            cursor += application.chunk_size;
        } else {
            let chunk_size = read_u32(bytes, cursor + 4)
                .ok_or_else(|| format!("invalid XML chunk at offset {cursor}"))?
                as usize;
            output.extend_from_slice(&bytes[cursor..cursor + chunk_size]);
            cursor += chunk_size;
        }
    }

    let new_xml_size = (bytes.len() as isize + xml_size_delta) as u32;
    write_u32(&mut output, 4, new_xml_size);
    info.application_class = Some(PROTECTOR_APPLICATION.to_string());

    Ok(PatchedManifestBytes {
        bytes: output,
        info,
        original_application,
        status: "binary manifest patched".to_string(),
        issues: Vec::new(),
    })
}

#[derive(Debug, Clone)]
struct BinaryXmlDocument {
    string_pool: StringPool,
    manifest: Option<StartElementInfo>,
    application: Option<StartElementInfo>,
}

impl BinaryXmlDocument {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if !is_binary_xml(bytes) {
            return Err("not a binary AndroidManifest.xml".to_string());
        }
        let xml_size = read_u32(bytes, 4)
            .ok_or_else(|| "binary XML header is truncated".to_string())?
            as usize;
        if xml_size > bytes.len() {
            return Err("binary XML size exceeds input length".to_string());
        }

        let mut cursor = 8usize;
        let mut string_pool = None;
        while cursor + 8 <= xml_size {
            let chunk_type =
                read_u16(bytes, cursor).ok_or_else(|| "truncated chunk".to_string())?;
            let chunk_size = read_u32(bytes, cursor + 4)
                .ok_or_else(|| "truncated chunk size".to_string())?
                as usize;
            if chunk_size < 8 || cursor + chunk_size > xml_size {
                return Err(format!("invalid XML chunk at offset {cursor}"));
            }
            if chunk_type == RES_STRING_POOL_TYPE {
                string_pool = Some(StringPool::parse(bytes, cursor)?);
                break;
            }
            cursor += chunk_size;
        }
        let string_pool =
            string_pool.ok_or_else(|| "manifest string pool not found".to_string())?;

        let mut cursor = string_pool.chunk_start + string_pool.chunk_size;
        let mut manifest = None;
        let mut application = None;
        while cursor + 8 <= xml_size {
            let chunk_type =
                read_u16(bytes, cursor).ok_or_else(|| "truncated chunk".to_string())?;
            let chunk_size = read_u32(bytes, cursor + 4)
                .ok_or_else(|| "truncated chunk size".to_string())?
                as usize;
            if chunk_size < 8 || cursor + chunk_size > xml_size {
                return Err(format!("invalid XML chunk at offset {cursor}"));
            }
            if chunk_type == RES_XML_START_ELEMENT_TYPE {
                let element = StartElementInfo::parse(bytes, cursor, &string_pool)?;
                match element.element_name.as_deref() {
                    Some("manifest") => manifest = Some(element),
                    Some("application") => application = Some(element),
                    _ => {}
                }
            }
            cursor += chunk_size;
        }

        Ok(Self {
            string_pool,
            manifest,
            application,
        })
    }

    fn string_index(&self, value: &str) -> Option<u32> {
        self.string_pool.index_of(value)
    }

    fn manifest_info(&self) -> ManifestInfo {
        let package_name = self
            .manifest
            .as_ref()
            .and_then(|element| element.string_attr("", "package", &self.string_pool));
        let application_class = self
            .application
            .as_ref()
            .and_then(|element| element.string_attr(ANDROID_NS, "name", &self.string_pool));
        let version_name = self
            .manifest
            .as_ref()
            .and_then(|element| element.string_attr(ANDROID_NS, "versionName", &self.string_pool));
        let version_code = self
            .manifest
            .as_ref()
            .and_then(|element| element.value_attr(ANDROID_NS, "versionCode", &self.string_pool));

        ManifestInfo {
            package_name,
            version_name,
            version_code,
            application_class,
            min_sdk: None,
            target_sdk: None,
        }
    }
}

#[derive(Debug, Clone)]
struct StartElementInfo {
    chunk_offset: usize,
    chunk_size: usize,
    element_name: Option<String>,
    attributes: Vec<AttributeInfo>,
    name_attr_offset: Option<usize>,
}

impl StartElementInfo {
    fn parse(bytes: &[u8], chunk_offset: usize, strings: &StringPool) -> Result<Self, String> {
        let chunk_size = read_u32(bytes, chunk_offset + 4)
            .ok_or_else(|| "truncated start element chunk size".to_string())?
            as usize;
        let element_name_index = read_u32(bytes, chunk_offset + 20)
            .ok_or_else(|| "truncated start element name".to_string())?;
        let attr_start = read_u16(bytes, chunk_offset + 24)
            .ok_or_else(|| "truncated attribute start".to_string())?
            as usize;
        let attr_size = read_u16(bytes, chunk_offset + 26)
            .ok_or_else(|| "truncated attribute size".to_string())?
            as usize;
        let attr_count = read_u16(bytes, chunk_offset + 28)
            .ok_or_else(|| "truncated attribute count".to_string())?
            as usize;
        if attr_size < 20 {
            return Err("unsupported binary XML attribute size".to_string());
        }
        let attr_base = chunk_offset + 16 + attr_start;
        let mut attributes = Vec::new();
        let mut name_attr_offset = None;
        for index in 0..attr_count {
            let offset = attr_base + index * attr_size;
            if offset + 20 > chunk_offset + chunk_size {
                return Err("attribute extends beyond start element chunk".to_string());
            }
            let attribute = AttributeInfo::parse(bytes, offset, strings)?;
            if attribute.local_name.as_deref() == Some("name")
                && attribute.namespace.as_deref() == Some(ANDROID_NS)
            {
                name_attr_offset = Some(offset);
            }
            attributes.push(attribute);
        }

        Ok(Self {
            chunk_offset,
            chunk_size,
            element_name: strings.get(element_name_index).map(ToOwned::to_owned),
            attributes,
            name_attr_offset,
        })
    }

    fn string_attr(&self, namespace: &str, name: &str, strings: &StringPool) -> Option<String> {
        self.attributes
            .iter()
            .find(|attr| attr.matches(namespace, name))
            .and_then(|attr| attr.string_value(strings))
    }

    fn value_attr(&self, namespace: &str, name: &str, strings: &StringPool) -> Option<String> {
        self.attributes
            .iter()
            .find(|attr| attr.matches(namespace, name))
            .and_then(|attr| {
                attr.string_value(strings)
                    .or_else(|| attr.data.map(|value| value.to_string()))
            })
    }
}

#[derive(Debug, Clone)]
struct AttributeInfo {
    namespace: Option<String>,
    local_name: Option<String>,
    raw_value: Option<u32>,
    data_type: u8,
    data: Option<u32>,
}

impl AttributeInfo {
    fn parse(bytes: &[u8], offset: usize, strings: &StringPool) -> Result<Self, String> {
        let ns = read_u32(bytes, offset).ok_or_else(|| "truncated attribute ns".to_string())?;
        let name =
            read_u32(bytes, offset + 4).ok_or_else(|| "truncated attribute name".to_string())?;
        let raw =
            read_u32(bytes, offset + 8).ok_or_else(|| "truncated attribute raw".to_string())?;
        let data_type = *bytes
            .get(offset + 15)
            .ok_or_else(|| "truncated attribute value type".to_string())?;
        let data =
            read_u32(bytes, offset + 16).ok_or_else(|| "truncated attribute data".to_string())?;

        Ok(Self {
            namespace: (ns != NO_INDEX)
                .then(|| strings.get(ns).map(ToOwned::to_owned))
                .flatten(),
            local_name: strings.get(name).map(ToOwned::to_owned),
            raw_value: (raw != NO_INDEX).then_some(raw),
            data_type,
            data: (data != NO_INDEX).then_some(data),
        })
    }

    fn matches(&self, namespace: &str, name: &str) -> bool {
        self.local_name.as_deref() == Some(name)
            && match namespace {
                "" => self.namespace.is_none(),
                _ => self.namespace.as_deref() == Some(namespace),
            }
    }

    fn string_value(&self, strings: &StringPool) -> Option<String> {
        if let Some(raw) = self.raw_value {
            return strings.get(raw).map(ToOwned::to_owned);
        }
        if self.data_type == TYPE_STRING {
            return self
                .data
                .and_then(|index| strings.get(index).map(ToOwned::to_owned));
        }
        None
    }
}

#[derive(Debug, Clone)]
struct StringPool {
    chunk_start: usize,
    chunk_size: usize,
    header_size: usize,
    string_count: usize,
    style_count: usize,
    flags: u32,
    strings_start: usize,
    styles_start: usize,
    offsets: Vec<u32>,
    strings: Vec<String>,
}

impl StringPool {
    fn parse(bytes: &[u8], chunk_start: usize) -> Result<Self, String> {
        let header_size = read_u16(bytes, chunk_start + 2)
            .ok_or_else(|| "truncated string pool header size".to_string())?
            as usize;
        let chunk_size = read_u32(bytes, chunk_start + 4)
            .ok_or_else(|| "truncated string pool size".to_string())?
            as usize;
        let string_count = read_u32(bytes, chunk_start + 8)
            .ok_or_else(|| "truncated string count".to_string())?
            as usize;
        let style_count = read_u32(bytes, chunk_start + 12)
            .ok_or_else(|| "truncated style count".to_string())? as usize;
        let flags = read_u32(bytes, chunk_start + 16)
            .ok_or_else(|| "truncated string pool flags".to_string())?;
        let strings_start = read_u32(bytes, chunk_start + 20)
            .ok_or_else(|| "truncated strings start".to_string())?
            as usize;
        let styles_start = read_u32(bytes, chunk_start + 24)
            .ok_or_else(|| "truncated styles start".to_string())?
            as usize;
        if chunk_start + chunk_size > bytes.len() {
            return Err("string pool extends beyond input".to_string());
        }

        let mut offsets = Vec::with_capacity(string_count);
        for index in 0..string_count {
            let offset = read_u32(bytes, chunk_start + header_size + index * 4)
                .ok_or_else(|| "truncated string offset table".to_string())?;
            offsets.push(offset);
        }

        let utf8 = flags & UTF8_FLAG != 0;
        let mut strings = Vec::with_capacity(string_count);
        for offset in &offsets {
            let absolute = chunk_start + strings_start + *offset as usize;
            strings.push(decode_string(bytes, absolute, utf8)?);
        }

        Ok(Self {
            chunk_start,
            chunk_size,
            header_size,
            string_count,
            style_count,
            flags,
            strings_start,
            styles_start,
            offsets,
            strings,
        })
    }

    fn get(&self, index: u32) -> Option<&str> {
        self.strings.get(index as usize).map(String::as_str)
    }

    fn index_of(&self, value: &str) -> Option<u32> {
        self.strings
            .iter()
            .position(|item| item == value)
            .map(|index| index as u32)
    }

    fn rebuild_with_additions(
        &self,
        bytes: &[u8],
        additions: &[String],
    ) -> Result<RebuiltStringPool, String> {
        if self.style_count != 0 {
            return Err(
                "binary manifest string pools with styles are not supported yet".to_string(),
            );
        }

        let mut unique = Vec::new();
        for value in additions {
            if self.index_of(value).is_none() && !unique.iter().any(|item| item == value) {
                unique.push(value.clone());
            }
        }
        if unique.is_empty() {
            let pool = bytes[self.chunk_start..self.chunk_start + self.chunk_size].to_vec();
            let indices = self
                .strings
                .iter()
                .enumerate()
                .map(|(index, value)| (value.clone(), index as u32))
                .collect();
            return Ok(RebuiltStringPool {
                bytes: pool,
                old_size: self.chunk_size,
                indices,
            });
        }

        let old_data_start = self.chunk_start + self.strings_start;
        let old_data_end = self.chunk_start
            + if self.styles_start == 0 {
                self.chunk_size
            } else {
                self.styles_start
            };
        let old_data = &bytes[old_data_start..old_data_end];
        let gap_start = self.header_size + self.string_count * 4;
        let gap = &bytes[self.chunk_start + gap_start..self.chunk_start + self.strings_start];

        let mut new_offsets = self.offsets.clone();
        let mut new_data = old_data.to_vec();
        let utf8 = self.flags & UTF8_FLAG != 0;
        let mut indices = self
            .strings
            .iter()
            .enumerate()
            .map(|(index, value)| (value.clone(), index as u32))
            .collect::<HashMap<_, _>>();

        for value in unique {
            new_offsets.push(new_data.len() as u32);
            let index = new_offsets.len() as u32 - 1;
            indices.insert(value.clone(), index);
            new_data.extend_from_slice(&encode_string(&value, utf8));
        }
        while new_data.len() % 4 != 0 {
            new_data.push(0);
        }

        let new_string_count = new_offsets.len();
        let new_strings_start = self.header_size + new_string_count * 4 + gap.len();
        let new_chunk_size = new_strings_start + new_data.len();
        let mut output = Vec::with_capacity(new_chunk_size);
        output.extend_from_slice(&bytes[self.chunk_start..self.chunk_start + self.header_size]);
        write_u32(&mut output, 4, new_chunk_size as u32);
        write_u32(&mut output, 8, new_string_count as u32);
        write_u32(&mut output, 20, new_strings_start as u32);
        for offset in new_offsets {
            output.extend_from_slice(&offset.to_le_bytes());
        }
        output.extend_from_slice(gap);
        output.extend_from_slice(&new_data);

        Ok(RebuiltStringPool {
            bytes: output,
            old_size: self.chunk_size,
            indices,
        })
    }
}

#[derive(Debug, Clone)]
struct RebuiltStringPool {
    bytes: Vec<u8>,
    old_size: usize,
    indices: HashMap<String, u32>,
}

impl RebuiltStringPool {
    fn size_delta(&self) -> isize {
        self.bytes.len() as isize - self.old_size as isize
    }

    fn index_of(&self, value: &str) -> Option<u32> {
        self.indices.get(value).copied()
    }
}

fn patch_application_chunk(
    chunk: &[u8],
    name_attr_offset_in_chunk: Option<usize>,
    protector_index: u32,
    name_index: u32,
    android_ns_index: u32,
) -> Result<(Vec<u8>, isize), String> {
    let mut output = chunk.to_vec();
    if let Some(attr_offset) = name_attr_offset_in_chunk {
        write_u32(&mut output, attr_offset + 8, protector_index);
        output[attr_offset + 15] = TYPE_STRING;
        write_u32(&mut output, attr_offset + 16, protector_index);
        return Ok((output, 0));
    }

    let attr_count_offset = 28usize;
    let attr_count = read_u16(chunk, attr_count_offset)
        .ok_or_else(|| "truncated application attribute count".to_string())?;
    let mut attr = Vec::with_capacity(20);
    attr.extend_from_slice(&android_ns_index.to_le_bytes());
    attr.extend_from_slice(&name_index.to_le_bytes());
    attr.extend_from_slice(&protector_index.to_le_bytes());
    attr.extend_from_slice(&8u16.to_le_bytes());
    attr.push(0);
    attr.push(TYPE_STRING);
    attr.extend_from_slice(&protector_index.to_le_bytes());

    let new_size = output.len() + attr.len();
    write_u32(&mut output, 4, new_size as u32);
    write_u16(&mut output, attr_count_offset, attr_count.saturating_add(1));
    output.extend_from_slice(&attr);
    Ok((output, 20))
}

fn normalize_application_name(package_name: Option<&str>, class_name: &str) -> Option<String> {
    let trimmed = class_name.trim();
    if trimmed.is_empty() || trimmed == PROTECTOR_APPLICATION {
        return None;
    }
    if let Some(package_name) = package_name.filter(|value| !value.is_empty()) {
        if trimmed.starts_with('.') {
            return Some(format!("{package_name}{trimmed}"));
        }
    }
    Some(trimmed.to_string())
}

fn decode_string(bytes: &[u8], offset: usize, utf8: bool) -> Result<String, String> {
    if utf8 {
        let (_, next) = read_length8(bytes, offset)?;
        let (byte_len, data_start) = read_length8(bytes, next)?;
        let data_end = data_start + byte_len;
        if data_end > bytes.len() {
            return Err("UTF-8 string extends beyond input".to_string());
        }
        String::from_utf8(bytes[data_start..data_end].to_vec())
            .map_err(|err| format!("invalid UTF-8 manifest string: {err}"))
    } else {
        let (unit_len, data_start) = read_length16(bytes, offset)?;
        let byte_len = unit_len * 2;
        let data_end = data_start + byte_len;
        if data_end > bytes.len() {
            return Err("UTF-16 string extends beyond input".to_string());
        }
        let units = bytes[data_start..data_end]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|err| format!("invalid UTF-16 manifest string: {err}"))
    }
}

fn encode_string(value: &str, utf8: bool) -> Vec<u8> {
    if utf8 {
        let mut output = Vec::new();
        write_length8(&mut output, value.encode_utf16().count());
        write_length8(&mut output, value.len());
        output.extend_from_slice(value.as_bytes());
        output.push(0);
        output
    } else {
        let units = value.encode_utf16().collect::<Vec<_>>();
        let mut output = Vec::new();
        write_length16(&mut output, units.len());
        for unit in units {
            output.extend_from_slice(&unit.to_le_bytes());
        }
        output.extend_from_slice(&0u16.to_le_bytes());
        output
    }
}

fn read_length8(bytes: &[u8], offset: usize) -> Result<(usize, usize), String> {
    let first = *bytes
        .get(offset)
        .ok_or_else(|| "truncated UTF-8 length".to_string())?;
    if first & 0x80 == 0 {
        Ok((first as usize, offset + 1))
    } else {
        let second = *bytes
            .get(offset + 1)
            .ok_or_else(|| "truncated UTF-8 length".to_string())?;
        Ok((
            (((first & 0x7f) as usize) << 8) | second as usize,
            offset + 2,
        ))
    }
}

fn read_length16(bytes: &[u8], offset: usize) -> Result<(usize, usize), String> {
    let first = read_u16(bytes, offset).ok_or_else(|| "truncated UTF-16 length".to_string())?;
    if first & 0x8000 == 0 {
        Ok((first as usize, offset + 2))
    } else {
        let second =
            read_u16(bytes, offset + 2).ok_or_else(|| "truncated UTF-16 length".to_string())?;
        Ok((
            (((first & 0x7fff) as usize) << 16) | second as usize,
            offset + 4,
        ))
    }
}

fn write_length8(output: &mut Vec<u8>, value: usize) {
    if value > 0x7f {
        output.push(((value >> 8) as u8) | 0x80);
        output.push(value as u8);
    } else {
        output.push(value as u8);
    }
}

fn write_length16(output: &mut Vec<u8>, value: usize) {
    if value > 0x7fff {
        output.extend_from_slice(&(((value >> 16) as u16) | 0x8000).to_le_bytes());
        output.extend_from_slice(&(value as u16).to_le_bytes());
    } else {
        output.extend_from_slice(&(value as u16).to_le_bytes());
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn patches_plain_manifest_application_name() {
        let input = br#"<manifest package="com.example"><application android:name=".App"></application></manifest>"#;
        let patched = patch_manifest_bytes(input).unwrap();
        let text = String::from_utf8(patched.bytes).unwrap();

        assert!(text.contains(PROTECTOR_APPLICATION));
        assert_eq!(
            patched.original_application.as_deref(),
            Some("com.example.App")
        );
    }

    #[test]
    fn patches_binary_manifest_application_name() {
        let binary = binary_manifest_fixture(Some(".App"));
        let patched = patch_manifest_bytes(&binary).unwrap();
        let info = inspect_manifest_bytes(&patched.bytes).unwrap();

        assert_eq!(info.package_name.as_deref(), Some("com.example"));
        assert_eq!(
            info.application_class.as_deref(),
            Some(PROTECTOR_APPLICATION)
        );
        assert_eq!(
            patched.original_application.as_deref(),
            Some("com.example.App")
        );
    }

    #[test]
    fn adds_application_name_when_binary_manifest_has_none() {
        let binary = binary_manifest_fixture(None);
        let patched = patch_manifest_bytes(&binary).unwrap();
        let info = inspect_manifest_bytes(&patched.bytes).unwrap();

        assert_eq!(
            info.application_class.as_deref(),
            Some(PROTECTOR_APPLICATION)
        );
        assert_eq!(patched.original_application, None);
    }

    #[test]
    fn patches_manifest_inside_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let apk = temp.path().join("app.apk");
        {
            let file = File::create(&apk).unwrap();
            let mut writer = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            writer.start_file("AndroidManifest.xml", options).unwrap();
            writer
                .write_all(br#"<manifest package="com.example"><application android:name=".App"/></manifest>"#)
                .unwrap();
            writer.finish().unwrap();
        }

        let patch = patch_manifest_in_artifact(&apk, ArtifactKind::Apk).unwrap();

        assert_eq!(patch.entry_name, "AndroidManifest.xml");
        assert_eq!(
            patch.original_application.as_deref(),
            Some("com.example.App")
        );
        assert!(String::from_utf8(patch.bytes)
            .unwrap()
            .contains(PROTECTOR_APPLICATION));
    }

    fn binary_manifest_fixture(application_name: Option<&str>) -> Vec<u8> {
        let mut strings = vec![
            "manifest".to_string(),
            "application".to_string(),
            "package".to_string(),
            "name".to_string(),
            ANDROID_NS.to_string(),
            "com.example".to_string(),
        ];
        if let Some(name) = application_name {
            strings.push(name.to_string());
        }
        let manifest_idx = 0;
        let application_idx = 1;
        let package_idx = 2;
        let name_idx = 3;
        let android_ns_idx = 4;
        let package_value_idx = 5;
        let app_value_idx = application_name.map(|_| 6);

        let string_pool = build_test_string_pool(&strings);
        let start_manifest = build_start_element(
            manifest_idx,
            &[TestAttr {
                ns: NO_INDEX,
                name: package_idx,
                value: package_value_idx,
            }],
        );
        let app_attrs = app_value_idx
            .map(|value| {
                vec![TestAttr {
                    ns: android_ns_idx,
                    name: name_idx,
                    value,
                }]
            })
            .unwrap_or_default();
        let start_application = build_start_element(application_idx, &app_attrs);
        let end_application = build_end_element(application_idx);
        let end_manifest = build_end_element(manifest_idx);

        let size = 8
            + string_pool.len()
            + start_manifest.len()
            + start_application.len()
            + end_application.len()
            + end_manifest.len();
        let mut output = Vec::with_capacity(size);
        output.extend_from_slice(&RES_XML_TYPE.to_le_bytes());
        output.extend_from_slice(&8u16.to_le_bytes());
        output.extend_from_slice(&(size as u32).to_le_bytes());
        output.extend_from_slice(&string_pool);
        output.extend_from_slice(&start_manifest);
        output.extend_from_slice(&start_application);
        output.extend_from_slice(&end_application);
        output.extend_from_slice(&end_manifest);
        output
    }

    fn build_test_string_pool(strings: &[String]) -> Vec<u8> {
        let header_size = 28usize;
        let strings_start = header_size + strings.len() * 4;
        let mut data = Vec::new();
        let mut offsets = Vec::new();
        for string in strings {
            offsets.push(data.len() as u32);
            data.extend_from_slice(&encode_string(string, true));
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let size = strings_start + data.len();
        let mut output = Vec::new();
        output.extend_from_slice(&RES_STRING_POOL_TYPE.to_le_bytes());
        output.extend_from_slice(&(header_size as u16).to_le_bytes());
        output.extend_from_slice(&(size as u32).to_le_bytes());
        output.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend_from_slice(&UTF8_FLAG.to_le_bytes());
        output.extend_from_slice(&(strings_start as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        for offset in offsets {
            output.extend_from_slice(&offset.to_le_bytes());
        }
        output.extend_from_slice(&data);
        output
    }

    struct TestAttr {
        ns: u32,
        name: u32,
        value: u32,
    }

    fn build_start_element(name: u32, attrs: &[TestAttr]) -> Vec<u8> {
        let size = 36 + attrs.len() * 20;
        let mut output = Vec::new();
        output.extend_from_slice(&RES_XML_START_ELEMENT_TYPE.to_le_bytes());
        output.extend_from_slice(&36u16.to_le_bytes());
        output.extend_from_slice(&(size as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend_from_slice(&NO_INDEX.to_le_bytes());
        output.extend_from_slice(&NO_INDEX.to_le_bytes());
        output.extend_from_slice(&name.to_le_bytes());
        output.extend_from_slice(&20u16.to_le_bytes());
        output.extend_from_slice(&20u16.to_le_bytes());
        output.extend_from_slice(&(attrs.len() as u16).to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        for attr in attrs {
            output.extend_from_slice(&attr.ns.to_le_bytes());
            output.extend_from_slice(&attr.name.to_le_bytes());
            output.extend_from_slice(&attr.value.to_le_bytes());
            output.extend_from_slice(&8u16.to_le_bytes());
            output.push(0);
            output.push(TYPE_STRING);
            output.extend_from_slice(&attr.value.to_le_bytes());
        }
        output
    }

    fn build_end_element(name: u32) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(&0x0103u16.to_le_bytes());
        output.extend_from_slice(&24u16.to_le_bytes());
        output.extend_from_slice(&24u32.to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend_from_slice(&NO_INDEX.to_le_bytes());
        output.extend_from_slice(&NO_INDEX.to_le_bytes());
        output.extend_from_slice(&name.to_le_bytes());
        output
    }
}
