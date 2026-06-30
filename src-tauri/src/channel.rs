use serde::Serialize;
use std::fs;
use std::path::Path;

const APK_SIGNATURE_SCHEME_V2_BLOCK_ID: u32 = 0x7109_871a;
#[cfg(test)]
const APK_SIGNATURE_SCHEME_V3_BLOCK_ID: u32 = 0xf053_68c0;
const VERITY_PADDING_BLOCK_ID: u32 = 0x4272_6577;
const APK_CHANNEL_BLOCK_ID: u32 = 0x7177_7777;
const ANDROID_COMMON_PAGE_ALIGNMENT_BYTES: usize = 4096;
const APK_SIGNING_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";
const ZIP_EOCD_REC_MIN_SIZE: usize = 22;
const ZIP_EOCD_REC_SIG: u32 = 0x0605_4b50;
const UINT16_MAX_VALUE: usize = 0xffff;

const ALLOWED_CHANNELS: &[&str] = &["huawei", "xiaomi", "oppo", "vivo", "honor", "yyb"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPackage {
    pub channel: String,
    pub path: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPackageResult {
    pub output_dir: String,
    pub packages: Vec<ChannelPackage>,
}

#[derive(Debug, Clone)]
struct SigningBlockRecord {
    center_start_offset: usize,
    eocd_offset: usize,
    sign_block_header_offset: usize,
    kv_pairs: Vec<(u32, Vec<u8>)>,
    magic: [u8; 16],
    comment_length: usize,
}

pub fn validate_channels(channels: &[String]) -> Result<Vec<String>, String> {
    let mut selected = Vec::new();
    for channel in channels {
        let normalized = channel.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if !ALLOWED_CHANNELS.contains(&normalized.as_str()) {
            return Err(format!("unsupported channel: {channel}"));
        }
        if !selected.contains(&normalized) {
            selected.push(normalized);
        }
    }
    if selected.is_empty() {
        return Err("select at least one channel".to_string());
    }
    Ok(selected)
}

pub fn write_channel_packages(
    signed_apk: &Path,
    channels: &[String],
) -> Result<ChannelPackageResult, String> {
    if !signed_apk.exists() {
        return Err(format!(
            "source APK does not exist: {}",
            signed_apk.display()
        ));
    }
    let channels = validate_channels(channels)?;
    let parent = signed_apk
        .parent()
        .ok_or_else(|| "source APK has no parent directory".to_string())?;
    let stem = signed_apk
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "protected".to_string());
    let output_dir = parent.join(format!("{stem}_channels"));
    write_channel_packages_to_dir(signed_apk, &output_dir, &channels)
}

pub fn write_channel_packages_to_dir(
    signed_apk: &Path,
    output_dir: &Path,
    channels: &[String],
) -> Result<ChannelPackageResult, String> {
    if !signed_apk.exists() {
        return Err(format!(
            "source APK does not exist: {}",
            signed_apk.display()
        ));
    }
    if signed_apk
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .as_deref()
        != Some("apk")
    {
        return Err("multi-channel packaging only supports APK files".to_string());
    }
    let channels = validate_channels(channels)?;
    let stem = signed_apk
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "protected".to_string());
    fs::create_dir_all(&output_dir)
        .map_err(|err| format!("failed to create channel output dir: {err}"))?;

    let mut packages = Vec::new();
    for channel in channels {
        let target = output_dir.join(format!("{stem}_{channel}.apk"));
        let value = channel_value(&channel);
        write_sign_key_value(signed_apk, &target, &value)?;
        let read_back = read_sign_key_value(&target)?;
        if read_back != value {
            return Err(format!(
                "channel verification failed for {channel}: expected {value}, got {read_back}"
            ));
        }
        packages.push(ChannelPackage {
            channel,
            path: target.display().to_string(),
            value,
        });
    }

    Ok(ChannelPackageResult {
        output_dir: output_dir.display().to_string(),
        packages,
    })
}

pub fn read_sign_key_value(apk: &Path) -> Result<String, String> {
    let bytes = fs::read(apk).map_err(|err| format!("failed to read APK: {err}"))?;
    let record = parse_signing_block(&bytes)?;
    record
        .kv_pairs
        .iter()
        .find(|(id, _)| *id == APK_CHANNEL_BLOCK_ID)
        .map(|(_, value)| String::from_utf8_lossy(value).to_string())
        .ok_or_else(|| "channel block not found".to_string())
}

fn write_sign_key_value(src: &Path, dest: &Path, extra: &str) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|err| format!("failed to read source APK: {err}"))?;
    let record = parse_signing_block(&bytes)?;
    let rebuilt = rebuild_with_channel(&bytes, &record, extra.as_bytes())?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create channel package dir: {err}"))?;
    }
    fs::write(dest, rebuilt).map_err(|err| format!("failed to write channel APK: {err}"))?;
    Ok(())
}

fn channel_value(channel: &str) -> String {
    format!("{{\"chn\":\"{channel}\"}}")
}

fn parse_signing_block(bytes: &[u8]) -> Result<SigningBlockRecord, String> {
    let eocd_offset = find_eocd(bytes)?;
    let comment_length = read_u16(bytes, eocd_offset + 20)? as usize;
    let center_start_offset = read_u32(bytes, eocd_offset + 16)? as usize;
    if center_start_offset < 32 || center_start_offset > bytes.len() {
        return Err("invalid central directory offset".to_string());
    }

    let footer_size_offset = center_start_offset
        .checked_sub(24)
        .ok_or_else(|| "APK Signing Block footer is missing".to_string())?;
    let sign_block_size_in_footer = read_u64(bytes, footer_size_offset)? as usize;
    let magic_start = center_start_offset - 16;
    let magic = bytes
        .get(magic_start..center_start_offset)
        .ok_or_else(|| "APK Signing Block magic is truncated".to_string())?;
    if magic != APK_SIGNING_BLOCK_MAGIC {
        return Err("APK Signing Block magic not found".to_string());
    }

    let sign_block_header_offset = center_start_offset
        .checked_sub(sign_block_size_in_footer)
        .and_then(|value| value.checked_sub(8))
        .ok_or_else(|| "invalid APK Signing Block size".to_string())?;
    let sign_block_size_in_header = read_u64(bytes, sign_block_header_offset)? as usize;
    if sign_block_size_in_header != sign_block_size_in_footer {
        return Err(format!(
            "APK Signing Block sizes in header and footer do not match: {sign_block_size_in_header} vs {sign_block_size_in_footer}"
        ));
    }
    if sign_block_size_in_footer < 24 {
        return Err("APK Signing Block is too small".to_string());
    }

    let kv_start = sign_block_header_offset + 8;
    let kv_end = center_start_offset - 24;
    let mut cursor = kv_start;
    let mut kv_pairs = Vec::new();
    while cursor < kv_end {
        let size = read_u64(bytes, cursor)? as usize;
        if size < 4 {
            return Err("invalid APK Signing Block pair size".to_string());
        }
        let pair_end = cursor
            .checked_add(8)
            .and_then(|value| value.checked_add(size))
            .ok_or_else(|| "APK Signing Block pair size overflow".to_string())?;
        if pair_end > kv_end {
            return Err("APK Signing Block pair extends beyond block".to_string());
        }
        let id = read_u32(bytes, cursor + 8)?;
        let value = bytes[cursor + 12..pair_end].to_vec();
        kv_pairs.push((id, value));
        cursor = pair_end;
    }

    if !kv_pairs
        .iter()
        .any(|(id, _)| *id == APK_SIGNATURE_SCHEME_V2_BLOCK_ID)
    {
        return Err("No APK Signature Scheme v2 block in APK Signing Block".to_string());
    }

    let mut magic = [0u8; 16];
    magic.copy_from_slice(APK_SIGNING_BLOCK_MAGIC);

    Ok(SigningBlockRecord {
        center_start_offset,
        eocd_offset,
        sign_block_header_offset,
        kv_pairs,
        magic,
        comment_length,
    })
}

fn rebuild_with_channel(
    bytes: &[u8],
    record: &SigningBlockRecord,
    extra: &[u8],
) -> Result<Vec<u8>, String> {
    let mut kv_pairs = record
        .kv_pairs
        .iter()
        .filter(|(id, _)| *id != APK_CHANNEL_BLOCK_ID)
        .map(|(id, value)| (*id, value.clone()))
        .collect::<Vec<_>>();
    let need_padding = kv_pairs
        .iter()
        .position(|(id, _)| *id == VERITY_PADDING_BLOCK_ID)
        .map(|index| {
            kv_pairs.remove(index);
            true
        })
        .unwrap_or(false);
    kv_pairs.push((APK_CHANNEL_BLOCK_ID, extra.to_vec()));
    if need_padding {
        let blocks_size = total_blocks_size(kv_pairs.iter().map(|(id, value)| (*id, value)));
        let result_size = 8 + blocks_size + 8 + 16;
        if result_size % ANDROID_COMMON_PAGE_ALIGNMENT_BYTES != 0 {
            let mut padding = ANDROID_COMMON_PAGE_ALIGNMENT_BYTES as isize
                - 12
                - (result_size % ANDROID_COMMON_PAGE_ALIGNMENT_BYTES) as isize;
            if padding < 0 {
                padding += ANDROID_COMMON_PAGE_ALIGNMENT_BYTES as isize;
            }
            kv_pairs.push((VERITY_PADDING_BLOCK_ID, vec![0; padding as usize]));
        }
    }

    let new_total_kv_block_size =
        total_blocks_size(kv_pairs.iter().map(|(id, value)| (*id, value)));
    let new_sign_block_size = new_total_kv_block_size + 8 + 16;
    let mut output = Vec::with_capacity(bytes.len() + new_sign_block_size);
    output.extend_from_slice(&bytes[..record.sign_block_header_offset]);
    output.extend_from_slice(&(new_sign_block_size as u64).to_le_bytes());
    for (id, value) in &kv_pairs {
        output.extend_from_slice(&((4 + value.len()) as u64).to_le_bytes());
        output.extend_from_slice(&id.to_le_bytes());
        output.extend_from_slice(value);
    }
    output.extend_from_slice(&(new_sign_block_size as u64).to_le_bytes());
    output.extend_from_slice(&record.magic);
    let new_center_start_offset = output.len();

    output.extend_from_slice(&bytes[record.center_start_offset..record.eocd_offset + 16]);
    output.extend_from_slice(&(new_center_start_offset as u32).to_le_bytes());
    output.extend_from_slice(&(record.comment_length as u16).to_le_bytes());
    let comment_start = record.eocd_offset + ZIP_EOCD_REC_MIN_SIZE;
    output.extend_from_slice(&bytes[comment_start..comment_start + record.comment_length]);

    Ok(output)
}

fn total_blocks_size<'a>(pairs: impl Iterator<Item = (u32, &'a Vec<u8>)>) -> usize {
    pairs.map(|(_, value)| 8 + 4 + value.len()).sum()
}

fn find_eocd(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < ZIP_EOCD_REC_MIN_SIZE {
        return Err("APK too small for ZIP End of Central Directory record".to_string());
    }
    let max_comment = (bytes.len() - ZIP_EOCD_REC_MIN_SIZE).min(UINT16_MAX_VALUE);
    let empty_comment_start = bytes.len() - ZIP_EOCD_REC_MIN_SIZE;
    for expected_comment_length in 0..=max_comment {
        let eocd_start = empty_comment_start - expected_comment_length;
        if read_u32(bytes, eocd_start)? == ZIP_EOCD_REC_SIG {
            let actual_comment_length = read_u16(bytes, eocd_start + 20)? as usize;
            if actual_comment_length == expected_comment_length {
                return Ok(eocd_start);
            }
        }
    }
    Err("ZIP End of Central Directory record not found".to_string())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("truncated u16 at offset {offset}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("truncated u32 at offset {offset}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("truncated u64 at offset {offset}"))?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validates_known_channels() {
        assert_eq!(
            validate_channels(&[
                "huawei".to_string(),
                "xiaomi".to_string(),
                "huawei".to_string()
            ])
            .unwrap(),
            vec!["huawei".to_string(), "xiaomi".to_string()]
        );
        assert!(validate_channels(&["unknown".to_string()]).is_err());
    }

    #[test]
    fn writes_and_reads_channel_value() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("signed.apk");
        let target = temp.path().join("channels").join("signed_huawei.apk");
        fs::write(&src, signed_apk_fixture(false)).unwrap();

        write_sign_key_value(&src, &target, "{\"chn\":\"huawei\"}").unwrap();

        assert_eq!(
            read_sign_key_value(&target).unwrap(),
            "{\"chn\":\"huawei\"}"
        );
        let bytes = fs::read(&target).unwrap();
        let record = parse_signing_block(&bytes).unwrap();
        assert!(record
            .kv_pairs
            .iter()
            .any(|(id, _)| *id == APK_SIGNATURE_SCHEME_V2_BLOCK_ID));
        assert_eq!(
            read_u32(&bytes, record.eocd_offset + 16).unwrap() as usize,
            record.center_start_offset
        );
    }

    #[test]
    fn creates_channel_package_folder() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("app.protected.apk");
        fs::write(&src, signed_apk_fixture(true)).unwrap();

        let result =
            write_channel_packages(&src, &["oppo".to_string(), "vivo".to_string()]).unwrap();

        assert_eq!(result.packages.len(), 2);
        assert!(PathBuf::from(&result.output_dir).ends_with("app.protected_channels"));
        for package in result.packages {
            assert!(Path::new(&package.path).exists());
            assert_eq!(
                read_sign_key_value(Path::new(&package.path)).unwrap(),
                package.value
            );
        }
    }

    #[test]
    fn writes_channel_packages_to_selected_output_dir() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("app.protected.apk");
        let selected_output = temp.path().join("selected-output");
        fs::write(&src, signed_apk_fixture(true)).unwrap();

        let result =
            write_channel_packages_to_dir(&src, &selected_output, &["yyb".to_string()]).unwrap();

        assert_eq!(result.packages.len(), 1);
        assert_eq!(PathBuf::from(&result.output_dir), selected_output);
        let package = &result.packages[0];
        assert_eq!(package.channel, "yyb");
        assert!(selected_output.join("app.protected_yyb.apk").exists());
        assert_eq!(
            read_sign_key_value(Path::new(&package.path)).unwrap(),
            "{\"chn\":\"yyb\"}"
        );
    }

    fn signed_apk_fixture(with_padding: bool) -> Vec<u8> {
        let prefix = b"local-file-data";
        let central_dir = b"central-dir";
        let mut pairs = vec![(APK_SIGNATURE_SCHEME_V2_BLOCK_ID, b"v2-signature".to_vec())];
        pairs.push((APK_SIGNATURE_SCHEME_V3_BLOCK_ID, b"v3-signature".to_vec()));
        if with_padding {
            pairs.push((VERITY_PADDING_BLOCK_ID, vec![0; 8]));
        }
        let kv_size = total_blocks_size(pairs.iter().map(|(id, value)| (*id, value)));
        let sign_block_size = kv_size + 8 + 16;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(prefix);
        bytes.extend_from_slice(&(sign_block_size as u64).to_le_bytes());
        for (id, value) in pairs {
            bytes.extend_from_slice(&((4 + value.len()) as u64).to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
            bytes.extend_from_slice(&value);
        }
        bytes.extend_from_slice(&(sign_block_size as u64).to_le_bytes());
        bytes.extend_from_slice(APK_SIGNING_BLOCK_MAGIC);
        let central_dir_offset = bytes.len();
        bytes.extend_from_slice(central_dir);

        bytes.extend_from_slice(&ZIP_EOCD_REC_SIG.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(central_dir.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(central_dir_offset as u32).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }
}
