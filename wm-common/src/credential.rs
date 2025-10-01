use std::ffi::{CStr, c_void};
use std::{ptr, slice};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALA, CredFree, CredReadA,
    CredWriteA,
};
use windows::core::{PCSTR, PSTR};

use crate::error::WindowsError;
use crate::ptr_guard::PtrGuard;

pub fn read(name: &CStr) -> Result<Vec<u8>, WindowsError> {
    unsafe {
        let mut cred = PtrGuard::new(|p: *mut CREDENTIALA| {
            if !p.is_null() {
                CredFree(p as *const c_void);
            }
        });
        CredReadA(
            PCSTR::from_raw(name.as_ptr() as *const u8),
            CRED_TYPE_GENERIC,
            None,
            cred.as_mut_ptr(),
        )?;

        Ok(match cred.as_ptr().as_ref() {
            Some(credential) => slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec(),
            None => vec![],
        })
    }
}

pub fn write(name: &mut CStr, data: &[u8]) -> Result<(), WindowsError> {
    let credential = CREDENTIALA {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PSTR::from_raw(name.as_ptr() as *mut u8),
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
