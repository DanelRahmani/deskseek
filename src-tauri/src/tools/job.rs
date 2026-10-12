//! A Windows job object that every child process of the app joins.
//!
//! `Stop` kills the tree and `RunEvent::Exit` stops the MCP servers and the
//! automation browser, so the ordinary paths are covered. Neither covers
//! DeskSeek *itself* being killed — Task Manager, a crash, a power cut — and a
//! build the agent started then carries on with nobody left to stop it.
//!
//! A job created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` closes that hole at
//! the kernel: when this process ends, by any means, its handles close, and
//! Windows kills whatever is still inside the job. The handle is deliberately
//! never closed by us — it has to stay open for exactly as long as a child might
//! still be running, which is as long as this process lives.
//!
//! Best-effort by design. A machine that refuses the job (an old Windows, a
//! policy) falls back to the behaviour it already had, and `kill_tree` still
//! stops a child on a normal Stop. Nothing here may make a command fail to run.

#[cfg(windows)]
pub use imp::{adopt, adopt_pid};

/// Puts a process we are about to run under the app's job.
#[cfg(not(windows))]
pub fn adopt(_child: &tokio::process::Child) {}

/// The same, for a child whose handle a library owns and only its pid is known.
#[cfg(not(windows))]
pub fn adopt_pid(_pid: Option<u32>) {}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use tokio::process::Child;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    /// A pointer is not `Sync`, so the handle is kept as a `usize`. Zero means
    /// "no job", which is also what a failed creation stores.
    static JOB: OnceLock<usize> = OnceLock::new();

    /// The one job, created on first use and never closed.
    fn job() -> Option<HANDLE> {
        let stored = *JOB.get_or_init(|| {
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: a null attributes pointer and an unnamed job are both
            // valid, and the struct below is the one its class asks for.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return 0;
                }
                let ok = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    return 0;
                }
                job as usize
            }
        });
        if stored == 0 {
            None
        } else {
            Some(stored as HANDLE)
        }
    }

    pub fn adopt(child: &Child) {
        let Some(job) = job() else { return };
        // `None` once the child has been reaped, which is also when there is
        // nothing left to adopt.
        let Some(process) = child.raw_handle() else {
            return;
        };
        // SAFETY: both handles are live — the job for the life of the process,
        // the child's handle just confirmed by `raw_handle`.
        unsafe {
            AssignProcessToJobObject(job, process);
        }
    }

    pub fn adopt_pid(pid: Option<u32>) {
        let (Some(job), Some(pid)) = (job(), pid) else {
            return;
        };
        // SAFETY: a pid of a child we just spawned; the handle is closed here,
        // and the process is not started or touched through it.
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return;
            }
            AssignProcessToJobObject(job, process);
            CloseHandle(process);
        }
    }

    #[cfg(test)]
    fn in_job(child: &Child) -> bool {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        let (Some(job), Some(process)) = (job(), child.raw_handle()) else {
            return false;
        };
        let mut inside = 0;
        // SAFETY: both handles are live and `inside` is a valid out-pointer.
        let ok = unsafe { IsProcessInJob(process, job, &mut inside) };
        ok != 0 && inside != 0
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::Stdio;

        /// A child that is still running when we ask, rather than one that could
        /// have exited first. `ping` with a count, output thrown away.
        #[tokio::test]
        async fn an_adopted_child_is_in_the_job() {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "ping -n 6 127.0.0.1 >NUL"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = cmd.spawn().expect("could not start the test child");
            adopt(&child);
            let inside = in_job(&child);
            let _ = child.kill().await;
            assert!(
                inside,
                "the child was not assigned to the app's job, so a hard kill would orphan it"
            );
        }
    }
}
