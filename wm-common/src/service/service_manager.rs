use windows::Win32::System::Services;
use windows::core::PCSTR;

use crate::error::WindowsError;
use crate::service::status::ServiceStatus;

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
        service_name: &str,
        desired_access: u32,
    ) -> Result<Services::SC_HANDLE, WindowsError> {
        Ok(unsafe {
            Services::OpenServiceA(
                self._scm,
                PCSTR::from_raw(service_name.as_ptr()),
                desired_access,
            )?
        })
    }

    pub fn create_service(&self, service_name: &str, exe_path: &str) -> Result<(), WindowsError> {
        let service_name = PCSTR::from_raw(service_name.as_ptr());
        unsafe {
            Services::CreateServiceA(
                self._scm,
                service_name,
                service_name,
                Services::SERVICE_ALL_ACCESS,
                Services::SERVICE_WIN32_OWN_PROCESS,
                Services::SERVICE_AUTO_START,
                Services::SERVICE_ERROR_NORMAL,
                PCSTR::from_raw(exe_path.as_ptr()),
                None,
                None,
                None,
                None,
                None,
            )?;
        }

        Ok(())
    }

    pub fn delete_service(&self, service_name: &str) -> Result<(), WindowsError> {
        let handle = self._open_service(service_name, 0x10000)?; // Missing constant in library?
        unsafe {
            Services::DeleteService(handle)?;
        }

        Ok(())
    }

    pub fn query_service_status(&self, service_name: &str) -> Result<ServiceStatus, WindowsError> {
        let handle = self._open_service(service_name, Services::SERVICE_QUERY_STATUS)?;
        let mut status = Services::SERVICE_STATUS::default();
        unsafe {
            Services::QueryServiceStatus(handle, &mut status)?;
        }

        Ok(ServiceStatus::new(status))
    }
}
