use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::Path;

use tokio::fs::File;
use windows::Win32::Foundation::{GENERIC_ACCESS_RIGHTS, GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_CREATION_DISPOSITION, FILE_SHARE_NONE,
    OPEN_ALWAYS, OPEN_EXISTING,
};
use windows::core::PCWSTR;

use crate::error::WindowsError;

fn _osstr_to_vec16(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn _exclusive_createfile(
    path: &Path,
    desired_access: GENERIC_ACCESS_RIGHTS,
    creation_disposition: FILE_CREATION_DISPOSITION,
) -> Result<File, WindowsError> {
    let temp_buf = _osstr_to_vec16(path.as_os_str());
    unsafe {
        let handle = CreateFileW(
            PCWSTR::from_raw(temp_buf.as_ptr()),
            desired_access.0,
            FILE_SHARE_NONE,
            None,
            creation_disposition,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )?;
        Ok(File::from_raw_handle(handle.0))
    }
}

pub fn open_exclusively(path: impl AsRef<Path>) -> Result<File, WindowsError> {
    _exclusive_createfile(path.as_ref(), GENERIC_READ, OPEN_EXISTING)
}

pub fn create_exclusively(path: impl AsRef<Path>) -> Result<File, WindowsError> {
    _exclusive_createfile(path.as_ref(), GENERIC_WRITE, OPEN_ALWAYS)
}

pub fn create_new_exclusively(path: impl AsRef<Path>) -> Result<File, WindowsError> {
    _exclusive_createfile(path.as_ref(), GENERIC_WRITE, CREATE_NEW)
}
