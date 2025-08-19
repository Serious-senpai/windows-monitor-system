use std::ffi::c_void;
use std::{ptr, slice};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALA, CredFree, CredReadA,
    CredWriteA,
};
use windows::core::{PCSTR, PSTR};

use crate::error::WindowsError;

pub struct CredentialManager;

impl CredentialManager {
    pub fn read(name: &str) -> Result<Vec<u8>, WindowsError> {
        let mut ptr = ptr::null_mut();
        unsafe {
            CredReadA(
                PCSTR::from_raw(name.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
                &mut ptr,
            )?;

            let credential = &*ptr;
            let result = slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec();
            CredFree(ptr as *const c_void);

            Ok(result)
        }
    }

    pub fn write(name: &mut str, data: &[u8]) -> Result<(), WindowsError> {
        let credential = CREDENTIALA {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PSTR::from_raw(name.as_mut_ptr()),
            Comment: PSTR::null(),
            LastWritten: FILETIME::default(),
            CredentialBlobSize: data.len() as u32,
            CredentialBlob: data.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: PSTR::null(),
            UserName: PSTR::null(),
        };

        unsafe {
            CredWriteA(&credential, 0)?;
        }

        Ok(())
    }
}
