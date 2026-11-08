use std::{
    ffi::OsStr,
    io::{self, Read},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Start command-line helpers without a visible console in Windows GUI builds.
pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Wait for the main process without waiting for EOF on inherited pipes.
/// The caller remains responsible for terminating its process tree on error.
pub fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> io::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "The process timed out.",
                ))
            }
        }
    }
}

/// Drain both pipes concurrently and bound process execution. On timeout, stop
/// the process tree on Windows so an installer cannot continue after cleanup.
pub fn output_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let read = |mut pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                #[cfg(windows)]
                {
                    let _ = hidden_command("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "The process timed out; retry or check your network connection.",
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    };
    let stdout = out
        .join()
        .map_err(|_| io::Error::other("Could not read process output"))??;
    let stderr = err
        .join()
        .map_err(|_| io::Error::other("Could not read process errors"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
/// Own a helper's process tree so a crash or normal application exit cannot
/// leave downloads running against the same directory after Pully restarts.
#[cfg(windows)]
pub struct ProcessTreeGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProcessTreeGuard {
    pub fn attach(child: &std::process::Child) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: The job is owned by this guard, the structure is initialized
        // before passing its correct size, and the child handle remains valid.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0
            {
                let error = io::Error::last_os_error();
                CloseHandle(job);
                return Err(error);
            }
            Ok(Self(job))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTreeGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
pub struct ProcessTreeGuard;
#[cfg(not(windows))]
impl ProcessTreeGuard {
    pub fn attach(_child: &std::process::Child) -> io::Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn timeout_stops_a_stalled_command() {
        let start = Instant::now();
        let error = output_with_timeout(
            hidden_command("cmd").args(["/C", "ping -n 8 127.0.0.1 > NUL"]),
            Duration::from_millis(250),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(6));
    }

    #[cfg(windows)]
    #[test]
    fn timed_wait_does_not_wait_for_a_stalled_child() {
        let mut child = hidden_command("cmd")
            .args(["/C", "ping -n 20 127.0.0.1 > NUL"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let guard = ProcessTreeGuard::attach(&child).unwrap();
        let error = wait_with_timeout(&mut child, Duration::from_millis(250)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        drop(guard);
        child.wait().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn closing_the_job_stops_its_child() {
        let mut child = hidden_command("cmd")
            .args(["/C", "ping -n 20 127.0.0.1 > NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let guard = ProcessTreeGuard::attach(&child).expect("child can be assigned to a job");
        assert!(
            child.try_wait().unwrap().is_none(),
            "child must be running before the job closes"
        );
        drop(guard);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "child survived job closure");
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(windows)]
    #[test]
    fn captures_successful_process_output() {
        let output = output_with_timeout(
            hidden_command("cmd").args(["/C", "echo ready"]),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("ready"));
    }
}
