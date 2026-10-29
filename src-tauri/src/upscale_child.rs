//! Atomically create the runtime inside an OS-owned kill-on-close job.
//! Containment exists before any runtime code executes, including on app exit.
use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
            InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
            CREATE_NO_WINDOW, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
            PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTUPINFOEXW,
        },
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct Attributes {
    storage: Vec<u128>,
    initialized: bool,
}
impl Attributes {
    fn new() -> Result<Self, String> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size);
        }
        if size == 0 || size > 64 * 1024 {
            return Err("Cannot allocate AI process containment.".into());
        }
        let mut attributes = Self {
            storage: vec![0u128; size.div_ceil(16)],
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(attributes.pointer(), 1, 0, &mut size) } == 0
        {
            return Err(last_error("Cannot initialize AI process containment"));
        }
        attributes.initialized = true;
        Ok(attributes)
    }
    fn pointer(&mut self) -> *mut std::ffi::c_void {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

fn wide(value: &OsStr) -> Result<Vec<u16>, String> {
    let mut value = value.encode_wide().collect::<Vec<_>>();
    if value.contains(&0) {
        return Err("AI process argument contains a NUL character.".into());
    }
    value.push(0);
    Ok(value)
}

fn quote(value: &OsStr) -> Vec<u16> {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in value.encode_wide() {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    result
}

pub(super) struct JobChild {
    process: Handle,
    job: Option<Handle>,
}
impl JobChild {
    pub(super) fn spawn(exe: &Path, args: &[OsString], directory: &Path) -> Result<Self, String> {
        let application = wide(exe.as_os_str())?;
        let directory = wide(directory.as_os_str())?;
        let mut command = quote(exe.as_os_str());
        for arg in args {
            command.push(b' ' as u16);
            command.extend(quote(arg));
        }
        if command.contains(&0) || command.len() >= 32767 {
            return Err("AI process arguments exceed the Windows limit.".into());
        }
        command.push(0);
        let raw_job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw_job.is_null() {
            return Err(last_error("Cannot create AI process containment"));
        }
        let job = Handle(raw_job);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(last_error("Cannot configure AI process containment"));
        }
        let mut attributes = Attributes::new()?;
        let jobs = [job.0];
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                jobs.as_ptr().cast(),
                std::mem::size_of_val(&jobs),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(last_error(
                "This Windows version cannot contain the AI process",
            ));
        }
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes.pointer();
        let mut process = PROCESS_INFORMATION::default();
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                ptr::null(),
                directory.as_ptr(),
                &startup.StartupInfo,
                &mut process,
            )
        } == 0
        {
            return Err(last_error("Cannot start contained AI runtime"));
        }
        let _thread = Handle(process.hThread);
        Ok(Self {
            process: Handle(process.hProcess),
            job: Some(job),
        })
    }

    pub(super) fn try_wait(&self) -> Result<Option<bool>, String> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut exit = 0;
                if unsafe { GetExitCodeProcess(self.process.0, &mut exit) } == 0 {
                    return Err(last_error("Cannot inspect AI runtime exit"));
                }
                Ok(Some(exit == 0))
            }
            _ => Err(last_error("Cannot inspect AI runtime process")),
        }
    }
}

impl Drop for JobChild {
    fn drop(&mut self) {
        // Closing this non-inherited handle kills the entire child process tree.
        drop(self.job.take());
        unsafe {
            WaitForSingleObject(self.process.0, 5000);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_job_terminates_runtime_even_without_worker_destructors() {
        let system = std::env::var_os("SystemRoot").unwrap();
        let directory = Path::new(&system).join("System32");
        let child = JobChild::spawn(
            &directory.join("cmd.exe"),
            &[
                OsString::from("/D"),
                OsString::from("/C"),
                OsString::from("ping -n 30 127.0.0.1 >NUL"),
            ],
            &directory,
        );
        assert!(
            child.is_ok(),
            "a contained child must start: {}",
            child.err().unwrap_or_default()
        );
    }

    #[test]
    fn closing_job_kills_process_without_explicit_child_termination() {
        let system = std::env::var_os("SystemRoot").unwrap();
        let directory = Path::new(&system).join("System32");
        let mut child = JobChild::spawn(
            &directory.join("cmd.exe"),
            &[
                OsString::from("/D"),
                OsString::from("/C"),
                OsString::from("ping -n 30 127.0.0.1 >NUL"),
            ],
            &directory,
        )
        .unwrap();
        assert_eq!(child.try_wait().unwrap(), None);
        drop(child.job.take());
        assert_eq!(
            unsafe { WaitForSingleObject(child.process.0, 5000) },
            WAIT_OBJECT_0
        );
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn arguments_quote_spaces_embedded_quotes_and_trailing_slashes() {
        let quoted = quote(OsStr::new("C:\\photos with spaces\\"));
        assert_eq!(
            String::from_utf16(&quoted).unwrap(),
            "\"C:\\photos with spaces\\\\\""
        );
        assert_eq!(
            String::from_utf16(&quote(OsStr::new("a\"b"))).unwrap(),
            "\"a\\\"b\""
        );
        assert!(wide(OsStr::new("bad\0arg")).is_err());
    }
}
