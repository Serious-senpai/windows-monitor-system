use std::ffi::{CStr, c_void};
use std::ptr;

use windows::Win32::Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_A, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SetEntriesInAclA, TRUSTEE_A,
    TRUSTEE_IS_SID, TRUSTEE_IS_USER,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, InitializeSecurityDescriptor, PSECURITY_DESCRIPTOR,
    SECURITY_DESCRIPTOR, SUB_CONTAINERS_AND_OBJECTS_INHERIT, SetSecurityDescriptorDacl,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, REG_BINARY, REG_OPTION_NON_VOLATILE, RegCreateKeyExA,
    RegQueryValueExA, RegSetKeySecurity, RegSetValueExA,
};
use windows::Win32::System::SystemServices::SECURITY_DESCRIPTOR_REVISION;
use windows::core::{PCSTR, PSTR};

use crate::error::RuntimeError;
use crate::ptr_guard::PtrGuard;
use crate::utils::convert_sid;

pub struct RegistryKey {
    _hkey: HKEY,
}

impl RegistryKey {
    pub fn new(subkey: &CStr) -> Result<Self, RuntimeError> {
        let mut hkey = HKEY::default();

        let error = unsafe {
            RegCreateKeyExA(
                HKEY_LOCAL_MACHINE,
                PCSTR::from_raw(subkey.as_ptr() as *const u8),
                Some(0),
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_ALL_ACCESS,
                None,
                &mut hkey,
                None,
            )
        };

        if error == ERROR_SUCCESS {
            Ok(Self { _hkey: hkey })
        } else {
            Err(RuntimeError::new(format!(
                "RegCreateKeyExA error: {error:?}"
            )))
        }
    }

    pub fn allow_only(&self, stringsids: &[&CStr]) -> Result<(), RuntimeError> {
        let mut sids = Vec::with_capacity(stringsids.len());
        for stringsid in stringsids {
            sids.push(convert_sid(stringsid)?);
        }

        let mut ea = Vec::with_capacity(stringsids.len());
        for sid in &mut sids {
            ea.push(EXPLICIT_ACCESS_A {
                grfAccessPermissions: KEY_ALL_ACCESS.0,
                grfAccessMode: SET_ACCESS,
                grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
                Trustee: TRUSTEE_A {
                    pMultipleTrustee: ptr::null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_USER,
                    ptstrName: PSTR::from_raw(sid.as_ptr() as *mut u8),
                },
            });
        }

        let mut pacl = PtrGuard::new(|p| unsafe {
            let _ = LocalFree(Some(HLOCAL(p as *mut c_void)));
        });
        let error = unsafe { SetEntriesInAclA(Some(&ea), None, pacl.as_mut_ptr()) };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!(
                "SetEntriesInAclA error {error:?}"
            )));
        }

        let mut descriptor = SECURITY_DESCRIPTOR::default();
        let error = unsafe {
            let pdescriptor =
                PSECURITY_DESCRIPTOR(&mut descriptor as *mut SECURITY_DESCRIPTOR as *mut c_void);

            InitializeSecurityDescriptor(pdescriptor, SECURITY_DESCRIPTOR_REVISION)?;
            SetSecurityDescriptorDacl(pdescriptor, true, Some(pacl.as_ptr()), false)?;

            RegSetKeySecurity(self._hkey, DACL_SECURITY_INFORMATION, pdescriptor)
        };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!(
                "RegSetKeySecurity error {error:?}"
            )));
        }

        Ok(())
    }

    pub fn store(&self, data: &[u8]) -> Result<(), RuntimeError> {
        let error = unsafe { RegSetValueExA(self._hkey, None, Some(0), REG_BINARY, Some(data)) };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!("RegSetValueExA error {error:?}")));
        }

        Ok(())
    }

    pub fn read(&self) -> Result<Vec<u8>, RuntimeError> {
        let mut size = 0;
        let error =
            unsafe { RegQueryValueExA(self._hkey, None, None, None, None, Some(&mut size)) };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!(
                "RegQueryValueExA error {error:?}"
            )));
        }

        let mut data = vec![0; size as usize];
        let error = unsafe {
            RegQueryValueExA(
                self._hkey,
                None,
                None,
                None,
                Some(data.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!(
                "RegQueryValueExA error {error:?}"
            )));
        }

        data.truncate(size as usize);
        Ok(data)
    }
}
