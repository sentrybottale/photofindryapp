//! Windows children enter a private kill-on-close Job before any user code runs.
//! The caller retains its Child handle and must reap that exact child on error.
use std::{io, mem::{size_of, zeroed}, os::windows::{io::RawHandle, process::CommandExt}, process::Command, thread, time::{Duration, Instant}};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT, WAIT_OBJECT_0, ERROR_BROKEN_PIPE, ERROR_INVALID_PARAMETER},
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32, TH32CS_SNAPTHREAD, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS},
        JobObjects::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, QueryInformationJobObject, TerminateJobObject, JobObjectExtendedLimitInformation, JobObjectBasicAccountingInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE},
        Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED, GetProcessId, GetProcessIdOfThread, OpenThread, ResumeThread, WaitForSingleObject, THREAD_SUSPEND_RESUME, THREAD_QUERY_LIMITED_INFORMATION, GetCurrentProcessId, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, GetProcessTimes},
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Pipes::PeekNamedPipe,
        IO::CancelSynchronousIo,
    },
};

struct Handle(HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
fn handle(value: HANDLE) -> io::Result<Handle> {
    if value.is_null() || value == INVALID_HANDLE_VALUE { Err(io::Error::last_os_error()) } else { Ok(Handle(value)) }
}

/// Only headless bundled worker/helper/model commands use this configuration.
pub fn configure_suspended(command: &mut Command) {
    command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
}

pub struct OwnedJob { job: Handle }
impl OwnedJob {
    /// `process` must be the retained handle of the caller's newly spawned,
    /// CREATE_SUSPENDED child. Never pass a PID looked up from a file or name.
    pub fn attach_and_resume(process: RawHandle, pid: u32) -> io::Result<Self> {
        let process = process as HANDLE;
        if process.is_null() || unsafe { GetProcessId(process) } != pid || unsafe { WaitForSingleObject(process,0) } != WAIT_TIMEOUT {
            return Err(io::Error::other("The newly spawned child identity is unavailable."));
        }
        let job = handle(unsafe { CreateJobObjectW(std::ptr::null(),std::ptr::null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe { SetInformationJobObject(job.0,JobObjectExtendedLimitInformation,(&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let owned = Self { job };
        if unsafe { AssignProcessToJobObject(owned.job.0,process) } == 0 { return Err(io::Error::last_os_error()); }
        owned.resume_initial_thread(process,pid)?;
        Ok(owned)
    }

    fn resume_initial_thread(&self, process: HANDLE, pid: u32) -> io::Result<()> {
        let snapshot = handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD,0) })?;
        let mut entry: THREADENTRY32 = unsafe { zeroed() }; entry.dwSize=size_of::<THREADENTRY32>() as u32;
        let mut found=Vec::new();
        let mut present=unsafe { Thread32First(snapshot.0,&mut entry) };
        while present != 0 {
            if entry.th32OwnerProcessID == pid { found.push(entry.th32ThreadID); }
            present=unsafe { Thread32Next(snapshot.0,&mut entry) };
        }
        // A CREATE_SUSPENDED child has not executed its entry point and has one
        // primary thread. Fail closed rather than resuming an unexpected tree.
        if found.len()!=1 { return Err(io::Error::other("Cannot prove the suspended child's primary thread.")); }
        let thread = handle(unsafe { OpenThread(THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,0,found[0]) })?;
        if unsafe { GetProcessIdOfThread(thread.0) } != pid || unsafe { GetProcessId(process) } != pid || unsafe { WaitForSingleObject(process,0) } != WAIT_TIMEOUT {
            return Err(io::Error::other("The suspended child changed before Job assignment completed."));
        }
        let previous=unsafe { ResumeThread(thread.0) };
        if previous!=1 { return Err(io::Error::other("The child was not in the expected initial suspended state.")); }
        Ok(())
    }

    pub fn terminate_and_wait(&self, timeout: Duration) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.job.0,1) } == 0 { return Err(io::Error::last_os_error()); }
        let deadline=Instant::now()+timeout;
        loop {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION=unsafe { zeroed() };
            if unsafe { QueryInformationJobObject(self.job.0,JobObjectBasicAccountingInformation,(&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>()as u32,std::ptr::null_mut()) } == 0 { return Err(io::Error::last_os_error()); }
            if info.ActiveProcesses==0 { return Ok(()); }
            if Instant::now()>=deadline { return Err(io::Error::new(io::ErrorKind::TimedOut,"The owned process Job did not drain.")); }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedJob { fn drop(&mut self) { unsafe { TerminateJobObject(self.job.0,1); } } }

pub fn physical_memory() -> io::Result<u64> {
    let mut status: MEMORYSTATUSEX=unsafe { zeroed() }; status.dwLength=size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 { return Err(io::Error::last_os_error()); }
    if status.ullTotalPhys==0 { return Err(io::Error::other("The operating system returned no physical memory capacity.")); }
    Ok(status.ullTotalPhys)
}

pub fn parent_pid() -> io::Result<u32> {
    let snapshot=handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0) })?;
    let own=unsafe { GetCurrentProcessId() };let mut entry:PROCESSENTRY32W=unsafe {zeroed()};entry.dwSize=size_of::<PROCESSENTRY32W>()as u32;
    let mut present=unsafe { Process32FirstW(snapshot.0,&mut entry) };
    while present!=0 {if entry.th32ProcessID==own {if entry.th32ParentProcessID>0{return Ok(entry.th32ParentProcessID);}break;}present=unsafe{Process32NextW(snapshot.0,&mut entry)};}
    Err(io::Error::other("The parent process identity is unavailable."))
}

pub struct ProcessMemory { pub working_set_bytes:u64,pub peak_working_set_bytes:u64,pub private_bytes:u64,pub creation_time_100ns:u64 }
pub fn process_memory(pid:u32)->io::Result<ProcessMemory>{
    let process=handle(unsafe{OpenProcess(PROCESS_QUERY_INFORMATION|PROCESS_VM_READ,0,pid)})?;
    let mut counters:PROCESS_MEMORY_COUNTERS_EX=unsafe{zeroed()};counters.cb=size_of::<PROCESS_MEMORY_COUNTERS_EX>()as u32;
    if unsafe{GetProcessMemoryInfo(process.0,(&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),size_of::<PROCESS_MEMORY_COUNTERS_EX>()as u32)}==0{return Err(io::Error::last_os_error());}
    let mut created=unsafe{zeroed()};let mut exited=unsafe{zeroed()};let mut kernel=unsafe{zeroed()};let mut user=unsafe{zeroed()};
    if unsafe{GetProcessTimes(process.0,&mut created,&mut exited,&mut kernel,&mut user)}==0{return Err(io::Error::last_os_error());}
    Ok(ProcessMemory{working_set_bytes:counters.WorkingSetSize as u64,peak_working_set_bytes:counters.PeakWorkingSetSize as u64,private_bytes:counters.PrivateUsage as u64,creation_time_100ns:((created.dwHighDateTime as u64)<<32)|created.dwLowDateTime as u64})
}

/// Permission errors/unknown failures never prove that another library owner
/// exited. Only a native nonexistent-PID result or a signaled held handle does.
pub fn process_alive(pid:u32)->io::Result<bool>{
    if pid==0{return Err(io::Error::new(io::ErrorKind::InvalidInput,"Invalid process identity."));}
    let raw=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|0x0010_0000,0,pid)};
    if raw.is_null(){let error=io::Error::last_os_error();return if error.raw_os_error()==Some(ERROR_INVALID_PARAMETER as i32){Ok(false)}else{Err(error)};}
    let process=Handle(raw);match unsafe{WaitForSingleObject(process.0,0)} {WAIT_TIMEOUT=>Ok(true),WAIT_OBJECT_0=>Ok(false),_=>Err(io::Error::last_os_error())}
}

/// Anonymous pipes support PeekNamedPipe. None is EOF; Some(0) is a live pipe
/// without bytes. Callers read at most the returned count and remain cancellable.
pub fn pipe_available(pipe: RawHandle) -> io::Result<Option<usize>> {
    let mut count=0u32;
    if unsafe { PeekNamedPipe(pipe as HANDLE,std::ptr::null_mut(),0,std::ptr::null_mut(),&mut count,std::ptr::null_mut()) } != 0 { return Ok(Some(count as usize)); }
    let error=io::Error::last_os_error();
    if error.raw_os_error()==Some(ERROR_BROKEN_PIPE as i32) { Ok(None) } else { Err(error) }
}

/// This is only for a retained owned thread blocked writing to an owned pipe.
pub fn cancel_thread_io(thread: RawHandle) { unsafe { CancelSynchronousIo(thread as HANDLE); } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::{Path,PathBuf}, process::{Child,Stdio}, os::windows::io::AsRawHandle};
    struct Fixture(PathBuf);
    impl Fixture { fn new()->Self { let mut bytes=[0u8;16];crate::secure_random(&mut bytes).unwrap();let name=bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();let path=std::env::temp_dir().join(format!("photofindry-job-test-{name}"));fs::create_dir(&path).unwrap();Self(path) } }
    impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    struct ChildGuard(Child);
    impl Drop for ChildGuard {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    fn command(stage:&str,marker:&Path)->Command {
        let mut cmd=Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact","process::tests::native_child_fixture","--nocapture"]).env("PHOTOFINDRY_JOB_TEST_STAGE",stage).env("PHOTOFINDRY_JOB_TEST_MARKER",marker).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());cmd
    }
    fn wait_file(path:&Path){let until=Instant::now()+Duration::from_secs(10);while !path.exists(){assert!(Instant::now()<until,"Native fixture did not start.");thread::sleep(Duration::from_millis(10));}}
    /// Invoked in a separate copy of this test executable by the tests below.
    #[test]
    fn native_child_fixture(){
        let Ok(stage)=std::env::var("PHOTOFINDRY_JOB_TEST_STAGE")else{return;};
        let marker=PathBuf::from(std::env::var_os("PHOTOFINDRY_JOB_TEST_MARKER").unwrap());
        if stage=="parent" {let child_marker=marker.with_extension("descendant");let _child=command("leaf",&child_marker).spawn().unwrap();wait_file(&child_marker);}
        fs::write(&marker,std::process::id().to_string()).unwrap();
        loop {thread::sleep(Duration::from_secs(1));}
    }
    #[test]
    fn suspended_child_cannot_execute_before_job_assignment(){
        let fixture=Fixture::new();let marker=fixture.0.join("must-not-exist");let mut cmd=command("leaf",&marker);configure_suspended(&mut cmd);let child=ChildGuard(cmd.spawn().unwrap());
        thread::sleep(Duration::from_millis(150));assert!(!marker.exists());
        // Incorrect identity cannot authorize Resume; caller retains the exact
        // still-suspended Child and drains it via ChildGuard.
        assert!(OwnedJob::attach_and_resume(child.0.as_raw_handle(),child.0.id().wrapping_add(1)).is_err());assert!(!marker.exists());
    }
    #[test]
    fn owned_job_drains_descendants_and_preserves_unrelated_child(){
        let fixture=Fixture::new();let unrelated_marker=fixture.0.join("unrelated");let mut unrelated=ChildGuard(command("leaf",&unrelated_marker).spawn().unwrap());wait_file(&unrelated_marker);
        let marker=fixture.0.join("owned");let mut cmd=command("parent",&marker);configure_suspended(&mut cmd);let mut child=ChildGuard(cmd.spawn().unwrap());let job=OwnedJob::attach_and_resume(child.0.as_raw_handle(),child.0.id()).unwrap();wait_file(&marker);wait_file(&marker.with_extension("descendant"));
        job.terminate_and_wait(Duration::from_secs(5)).unwrap();assert!(child.0.wait().is_ok());assert!(unrelated.0.try_wait().unwrap().is_none(),"A different owned fixture outside the Job was terminated.");
    }
}
