use std::ffi::c_void;
use std::ptr;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_A, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SetEntriesInAclA, TRUSTEE_A,
    TRUSTEE_IS_SID, TRUSTEE_IS_USER,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, InitializeSecurityDescriptor, PSECURITY_DESCRIPTOR,
    SECURITY_DESCRIPTOR, SUB_CONTAINERS_AND_OBJECTS_INHERIT, SetSecurityDescriptorDacl,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, REG_OPTION_NON_VOLATILE, RegCloseKey,
    RegCreateKeyExA, RegSetKeySecurity,
};
use windows::Win32::System::SystemServices::SECURITY_DESCRIPTOR_REVISION;
use windows::core::{PCSTR, PSTR};

use crate::error::RuntimeError;
use crate::utils::convert_sid;

pub struct RegistryKey {
    _hkey: HKEY,
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self._hkey);
        }
    }
}
impl RegistryKey {
    pub fn new(subkey: &str) -> Result<Self, RuntimeError> {
        let mut hkey = HKEY::default();

        let error = unsafe {
            RegCreateKeyExA(
                HKEY_LOCAL_MACHINE,
                PCSTR::from_raw(subkey.as_ptr()),
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

    pub fn allow_only(&self, stringsids: &[&str]) -> Result<(), RuntimeError> {
        let mut ea = Vec::with_capacity(stringsids.len());
        for stringsid in stringsids {
            let sid = convert_sid(stringsid)?;
            ea.push(EXPLICIT_ACCESS_A {
                grfAccessPermissions: KEY_ALL_ACCESS.0,
                grfAccessMode: SET_ACCESS,
                grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
                Trustee: TRUSTEE_A {
                    pMultipleTrustee: ptr::null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_USER,
                    ptstrName: PSTR::from_raw(sid.0 as *mut u8),
                },
            });
        }

        // TODO: free this memory later via `LocalFree`, see https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setentriesinacla
        // In fact, most null pointer declared as `ptr::null_mut()` in the entire project must be manually freed somehow.
        let mut pacl = ptr::null_mut();
        let error = unsafe { SetEntriesInAclA(Some(&ea), None, &mut pacl) };
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
            SetSecurityDescriptorDacl(pdescriptor, true, Some(pacl), false)?;

            RegSetKeySecurity(self._hkey, DACL_SECURITY_INFORMATION, pdescriptor)
        };
        if error != ERROR_SUCCESS {
            return Err(RuntimeError::new(format!(
                "RegSetKeySecurity error {error:?}"
            )));
        }

        Ok(())
    }
}
