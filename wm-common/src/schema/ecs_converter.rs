use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_ENCRYPTED, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
    FILE_ATTRIBUTE_TEMPORARY,
};

pub fn file_attributes(attributes: u32) -> Vec<String> {
    let mut results = vec![];
    if attributes & FILE_ATTRIBUTE_ARCHIVE.0 != 0 {
        results.push("archive".to_string());
    }

    if attributes & FILE_ATTRIBUTE_ENCRYPTED.0 != 0 {
        results.push("encrypted".to_string());
    }

    if attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0 {
        results.push("hidden".to_string());
    }

    if attributes & FILE_ATTRIBUTE_NORMAL.0 != 0 {
        results.push("normal".to_string());
    }

    if attributes & FILE_ATTRIBUTE_OFFLINE.0 != 0 {
        results.push("offline".to_string());
    }

    if attributes & FILE_ATTRIBUTE_READONLY.0 != 0 {
        results.push("readonly".to_string());
    }

    if attributes & FILE_ATTRIBUTE_SYSTEM.0 != 0 {
        results.push("system".to_string());
    }

    if attributes & FILE_ATTRIBUTE_TEMPORARY.0 != 0 {
        results.push("temporary".to_string());
    }

    results
}
