use std::{ffi::OsStr, process::Command};

/// Start command-line helpers without a visible console in Windows GUI builds.
/// Redirected stdin/stdout/stderr still work for downloads and the browser host.
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
