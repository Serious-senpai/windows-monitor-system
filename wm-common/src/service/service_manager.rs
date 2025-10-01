use std::ffi::CStr;

use windows::Win32::System::Services;
use windows::core::PCSTR;

use crate::error::WindowsError;
use crate::service::status::ServiceStatus;
use crate::service::status_process::ServiceStatusProcess;

pub struct ServiceManager {
    _scm: Services::SC_HANDLE,
}

impl ServiceManager {
    pub fn new(desired_access: u32) -> Result<Self, WindowsError> {
        unsafe {
            Ok(Self {
                _scm: Services::OpenSCManagerA(None, None, desired_access)?,
            })
        }
    }

    fn _open_service(
        &self,
        service_name: &CStr,
        desired_access: u32,
    ) -> Result<Services::SC_HANDLE, WindowsError> {
        Ok(unsafe {
            Services::OpenServiceA(
                self._scm,
                PCSTR::from_raw(service_name.as_ptr() as *const u8),
                desired_access,
            )?
        })
    }

    pub fn create_service(&self, service_name: &CStr, exe_path: &CStr) -> Result<(), WindowsError> {
        let service_name = PCSTR::from_raw(service_name.as_ptr() as *const u8);
        unsafe {
            Services::CreateServiceA(
                self._scm,
                service_name,
                service_name,
                Services::SERVICE_ALL_ACCESS,
                Services::SERVICE_WIN32_OWN_PROCESS,
                Services::SERVICE_AUTO_START,
                Services::SERVICE_ERROR_NORMAL,
                PCSTR::from_raw(exe_path.as_ptr() as *const u8),
                None,
                None,
                None,
                None,
                None,
            )?;
        }

        Ok(())
    }

    pub fn delete_service(&self, service_name: &CStr) -> Result<(), WindowsError> {
        let handle = self._open_service(service_name, 0x10000)?; // Missing constant in library?
        unsafe {
            Services::DeleteService(handle)?;
        }

        Ok(())
    }

    pub fn stop_service(&self, service_name: &CStr) -> Result<ServiceStatus, WindowsError> {
        let handle = self._open_service(service_name, Services::SERVICE_STOP)?;
        let mut status = Services::SERVICE_STATUS::default();
        unsafe {
            Services::ControlService(handle, Services::SERVICE_CONTROL_STOP, &mut status)?;
        }

        Ok(ServiceStatus::new(status))
    }

    pub fn query_service_status(&self, service_name: &CStr) -> Result<ServiceStatus, WindowsError> {
        let handle = self._open_service(service_name, Services::SERVICE_QUERY_STATUS)?;
        let mut status = Services::SERVICE_STATUS::default();
        unsafe {
            Services::QueryServiceStatus(handle, &mut status)?;
        }

        Ok(ServiceStatus::new(status))
    }

    // TODO: This is not safe I guess, but we are not using it anyway.
    pub fn query_service_status_process(
        &self,
        service_name: &CStr,
    ) -> Result<ServiceStatusProcess, WindowsError> {
        let handle = self._open_service(service_name, Services::SERVICE_QUERY_STATUS)?;

        let mut size = 0;
        let status = unsafe {
            Services::QueryServiceStatusEx(
                handle,
                Services::SC_STATUS_PROCESS_INFO,
                None,
                &mut size,
            )?;

            let mut buffer = vec![0; size as usize];
            Services::QueryServiceStatusEx(
                handle,
                Services::SC_STATUS_PROCESS_INFO,
                Some(&mut buffer),
                &mut size,
            )?;

            let ptr = *buffer.as_ptr() as *const Services::SERVICE_STATUS_PROCESS;
            ptr.read_unaligned()
        };

        Ok(ServiceStatusProcess::new(status))
    }

    pub fn change_service_user(
        &self,
        service_name: &CStr,
        username: &CStr,
        password: &CStr,
    ) -> Result<(), WindowsError> {
        let handle = self._open_service(service_name, Services::SERVICE_CHANGE_CONFIG)?;
        unsafe {
            Services::ChangeServiceConfigA(
                handle,
                Services::ENUM_SERVICE_TYPE(Services::SERVICE_NO_CHANGE),
                Services::SERVICE_START_TYPE(Services::SERVICE_NO_CHANGE),
                Services::SERVICE_ERROR(Services::SERVICE_NO_CHANGE),
                None,
                None,
                None,
                None,
                PCSTR::from_raw(username.as_ptr() as *const u8),
                PCSTR::from_raw(password.as_ptr() as *const u8),
                None,
            )?;
        }

        Ok(())
    }
}
