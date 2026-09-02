use crate::models::{SplitManifest, SPLIT_MANIFEST_SUFFIX, SPLIT_MANIFEST_VERSION};
use std::collections::HashSet;

pub(crate) const MAX_SPLIT_MANIFEST_BYTES: u64 = 1024 * 1024;

pub(crate) fn is_split_manifest_candidate(
    document_name: &str,
    mime_type: Option<&str>,
    document_size: u64,
    caption: &str,
) -> bool {
    if document_name
        .to_ascii_lowercase()
        .ends_with(SPLIT_MANIFEST_SUFFIX)
    {
        return true;
    }

    let caption = caption.trim().to_ascii_lowercase();
    let legacy_video_caption = caption.ends_with(".mkv") || caption.ends_with(".mp4");
    document_size <= MAX_SPLIT_MANIFEST_BYTES
        && mime_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
        && legacy_video_caption
}

pub(crate) fn validate_split_manifest(manifest: &SplitManifest) -> Result<(), String> {
    if manifest.telestash_split != SPLIT_MANIFEST_VERSION {
        return Err(format!("Unsupported split manifest version {}", manifest.telestash_split));
    }
    if manifest.filename.trim().is_empty() {
        return Err("Split manifest filename is empty".to_string());
    }
    if manifest.size == 0 {
        return Err("Split manifest size is zero".to_string());
    }
    if manifest.part_size == 0 {
        return Err("Split manifest part_size is zero".to_string());
    }
    if manifest.parts.is_empty() {
        return Err("Split manifest has no parts".to_string());
    }

    let expected_count = expected_part_count(manifest.size, manifest.part_size)?;
    if manifest.parts.len() != expected_count {
        return Err(format!(
            "Split manifest expected {} parts but contains {}",
            expected_count,
            manifest.parts.len()
        ));
    }

    let mut seen = HashSet::new();
    let mut total = 0u64;
    for (index, part) in manifest.parts.iter().enumerate() {
        let part_number = index + 1;
        if part.message_id <= 0 {
            return Err(format!("Split manifest part {} has invalid message_id {}", part_number, part.message_id));
        }
        if !seen.insert(part.message_id) {
            return Err(format!("Split manifest has duplicate message_id {}", part.message_id));
        }
        if part.size == 0 {
            return Err(format!("Split manifest part {} has zero size", part_number));
        }
        let is_last = index == manifest.parts.len() - 1;
        if !is_last && part.size != manifest.part_size {
            return Err(format!(
                "Split manifest part {} size mismatch: expected {}, got {}",
                part_number, manifest.part_size, part.size
            ));
        }
        if is_last && part.size > manifest.part_size {
            return Err(format!(
                "Split manifest last part size {} exceeds part_size {}",
                part.size, manifest.part_size
            ));
        }
        total = total
            .checked_add(part.size)
            .ok_or_else(|| "Split manifest total size overflow".to_string())?;
    }

    if total != manifest.size {
        return Err(format!(
            "Split manifest total size mismatch: expected {}, got {}",
            manifest.size, total
        ));
    }

    Ok(())
}

pub(crate) fn expected_part_count(size: u64, part_size: u64) -> Result<usize, String> {
    if size == 0 {
        return Err("Split manifest size is zero".to_string());
    }
    if part_size == 0 {
        return Err("Split manifest part_size is zero".to_string());
    }
    usize::try_from(((size - 1) / part_size) + 1)
        .map_err(|_| "Split manifest part count exceeds platform limit".to_string())
}

#[cfg(test)]
mod tests {
    use crate::models::{SplitManifest, SplitPart, SPLIT_MANIFEST_VERSION};

    #[test]
    fn recognizes_legacy_video_manifest_when_filename_suffix_was_lost() {
        assert!(super::is_split_manifest_candidate(
            "Obsession.2025.1080p.BluRay.x265.DD+5.1-Pahe.in.mkv.tdmanife",
            Some("application/json"),
            392,
            "Obsession.2025.1080p.BluRay.x265.DD+5.1-Pahe.in.mkv",
        ));
    }

    #[test]
    fn does_not_treat_regular_json_as_split_manifest() {
        assert!(!super::is_split_manifest_candidate(
            "notes.json",
            Some("application/json"),
            392,
            "notes.json",
        ));
    }

    #[test]
    fn recognizes_canonical_split_manifest_filename() {
        assert!(super::is_split_manifest_candidate(
            "telestash.tdmanifest.json",
            Some("application/octet-stream"),
            392,
            "movie.mkv",
        ));
    }

    #[test]
    fn accepts_valid_manifest() {
        let manifest = SplitManifest {
            telestash_split: SPLIT_MANIFEST_VERSION,
            filename: "movie.mkv".to_string(),
            size: 5,
            mime_type: "video/x-matroska".to_string(),
            file_ext: Some("mkv".to_string()),
            part_size: 2,
            parts: vec![
                SplitPart { message_id: 10, size: 2 },
                SplitPart { message_id: 11, size: 2 },
                SplitPart { message_id: 12, size: 1 },
            ],
        };

        assert!(super::validate_split_manifest(&manifest).is_ok());
    }

    #[test]
    fn rejects_wrong_part_count() {
        let manifest = SplitManifest {
            telestash_split: SPLIT_MANIFEST_VERSION,
            filename: "movie.mp4".to_string(),
            size: 5,
            mime_type: "video/mp4".to_string(),
            file_ext: Some("mp4".to_string()),
            part_size: 2,
            parts: vec![
                SplitPart { message_id: 10, size: 2 },
                SplitPart { message_id: 11, size: 3 },
            ],
        };

        assert!(super::validate_split_manifest(&manifest)
            .unwrap_err()
            .contains("expected 3 parts"));
    }

    #[test]
    fn rejects_duplicate_message_ids() {
        let manifest = SplitManifest {
            telestash_split: SPLIT_MANIFEST_VERSION,
            filename: "movie.mp4".to_string(),
            size: 4,
            mime_type: "video/mp4".to_string(),
            file_ext: Some("mp4".to_string()),
            part_size: 2,
            parts: vec![
                SplitPart { message_id: 10, size: 2 },
                SplitPart { message_id: 10, size: 2 },
            ],
        };

        assert!(super::validate_split_manifest(&manifest)
            .unwrap_err()
            .contains("duplicate"));
    }

    #[test]
    fn recognizes_legacy_split_part_captions() {
        assert!(crate::models::is_split_part_caption("[telestash-part] movie.mkv 1/5"));
        assert!(crate::models::is_split_part_caption("[teledrive-part] movie.mkv 1/5"));
        assert!(crate::models::is_split_part_caption("[telegram-drive-part] movie.mkv 1/5"));
        assert!(!crate::models::is_split_part_caption("Regular movie caption"));
    }

    #[test]
    fn recognizes_split_part_filenames() {
        assert!(crate::models::is_split_part_filename("movie.mkv.tdpart0001of0005"));
        assert!(crate::models::is_split_part_filename("a.tdpart0001of0001"));
        assert!(crate::models::is_split_part_filename("big file.mkv.tdpart0012of0012"));
        assert!(!crate::models::is_split_part_filename("movie.mkv"));
        assert!(!crate::models::is_split_part_filename("notes.tdpart12of34.txt"));
        assert!(!crate::models::is_split_part_filename("my.tdpart0001of0005.bak"));
    }

    #[test]
    fn deserializes_legacy_teledrive_manifest_json() {
        let json_data = r#"{
            "teledrive_split": 1,
            "filename": "legacy_movie.mkv",
            "size": 1048576,
            "mime_type": "video/x-matroska",
            "file_ext": "mkv",
            "part_size": 524288,
            "parts": [{"message_id": 10, "size": 524288}, {"message_id": 11, "size": 524288}]
        }"#;
        let manifest: Result<SplitManifest, _> = serde_json::from_str(json_data);
        assert!(manifest.is_ok());
        assert_eq!(manifest.unwrap().filename, "legacy_movie.mkv");
    }
}
