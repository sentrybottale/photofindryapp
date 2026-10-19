//! Trusted, finite migration commands for the installed macOS launcher. There is
//! no HTTP route, public-URL argument, pairing ticket, or raw worker proxy here.
use super::{handover, launch, uninstall, worker};
use serde_json::{Value, json};
use std::{ffi::CString, fs::OpenOptions, io::Write,
    os::{fd::AsRawFd, unix::{ffi::OsStrExt, fs::OpenOptionsExt}}, path::{Path,PathBuf},
    sync::{Arc, atomic::{AtomicBool,Ordering}}, time::Duration};
type Result<T> = std::result::Result<T,String>;
const RESERVE: u64=256*1024*1024;
#[derive(Default)]
struct Options { data:PathBuf, runtime:Option<PathBuf>, rollback:Option<PathBuf>, state:Option<PathBuf> }
fn parse(args:&[String])->Result<Options> {
    let migrate=args.first().is_some_and(|value|value=="migrate-library");
    let mut data=None;let mut runtime=None;let mut rollback=None;let mut state=None;let mut use_current=false;
    let mut items=args.iter().skip(1);
    while let Some(option)=items.next() {
        if option=="--current-library"&&!migrate&&!use_current {use_current=true;continue;}
        let slot=match option.as_str() {
            "--data-dir"=>&mut data,"--launch-state"=>&mut state,
            "--runtime" if migrate=>&mut runtime,"--rollback-copy" if migrate=>&mut rollback,
            _=>return Err("Unknown migration option. Reopen the installed PhotoFindry app and retry.".into()),
        };
        let value=items.next().ok_or("A migration option is missing its path. Reopen the installed app and retry.")?;
        let path=PathBuf::from(value);
        if !path.is_absolute() || value.chars().any(char::is_control) || value.len()>4096 || value.contains("//") || value.split('/').any(|part|matches!(part,"."|".."))
            || path.components().any(|part|matches!(part,std::path::Component::CurDir|std::path::Component::ParentDir)) {
            return Err("Migration requires normalized absolute local paths. Reopen the installed PhotoFindry app.".into());
        }
        if slot.replace(path).is_some(){return Err("Migration options cannot repeat.".into());}
    }
    if migrate && (runtime.is_none()||rollback.is_none()) {return Err("Choose the installed runtime and a new recovery-copy folder before switching.".into());}
    let data=data.ok_or("Choose a local library directory before checking migration.")?;
    let data=if use_current {super::lifecycle::current_library(&data,state.as_deref())?}else{data};
    Ok(Options{data,runtime,rollback,state})
}
fn check_cancel(cancelled:&AtomicBool)->Result<()> {if cancelled.load(Ordering::Acquire){Err("The switch was cancelled. Your library and recovery copy are kept. Choose Retry when ready.".into())}else{Ok(())}}
async fn wait_cancel(cancelled:&AtomicBool){while !cancelled.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(50)).await;}}
fn emit(value:Value)->Result<()> {
    let mut output=std::io::stdout().lock();
    serde_json::to_writer(&mut output,&value).map_err(|_|"Cannot report migration progress.")?;
    output.write_all(b"\n").and_then(|_|output.flush()).map_err(|_|"Cannot report migration progress.".into())
}
fn progress(stage:&str,message:&str)->Result<()> {emit(json!({"event":"progress","stage":stage,"message":message}))}
fn available(path:&Path)->Result<u64> {
    let path=CString::new(path.as_os_str().as_bytes()).map_err(|_|"The recovery disk path is invalid.")?;
    let mut space=std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe{libc::statvfs(path.as_ptr(),space.as_mut_ptr())}!=0{return Err("Cannot check free disk space. Reconnect the recovery disk, then retry.".into());}
    let space=unsafe{space.assume_init()};
    (space.f_bavail as u128).checked_mul(space.f_frsize as u128).filter(|bytes|*bytes<=u64::MAX as u128)
        .map(|bytes|bytes as u64).ok_or("The disk-space measurement is unavailable.".into())
}
fn required(bytes:u64)->Result<u64> {bytes.checked_add(RESERVE.max(bytes/20)).ok_or("The recovery copy exceeds the supported size.".into())}
fn state_directory(options:&Options)->Result<PathBuf> {options.state.clone().map(Ok).unwrap_or_else(launch::default_directory)}
fn executable()->Result<PathBuf> {std::env::current_exe().map_err(|_|"Cannot locate the installed PhotoFindry companion.".into())}
fn matches_registration(state:&Path,data:&Path,executable:&Path)->bool {
    launch::read_registration(state).is_ok_and(|record|{
        let packaged=executable.parent().is_some_and(|path|path.file_name().is_some_and(|name|name=="bin"));
        let runtime=executable.parent().and_then(Path::parent).map(|root|root.join("runtime"));
        record.data_dir==data&&record.executable==executable&&(!packaged||runtime.as_ref()==Some(&record.runtime))
    })
}
fn error_next(message:&str)->&'static str {
    if message.to_ascii_lowercase().contains("space")||message.contains("disk") {"Free disk space or reconnect the recovery disk, then choose Retry. Your original library is kept."}
    else if message.contains("owns")||message.contains("marker")||message.contains("busy")||message.contains("active")||message.contains("running")||message.contains("finishing") {"Finish or pause current work, then quit the other PhotoFindry app and choose Retry. Do not delete library lock files."}
    else {"Keep the previous app and recovery copy. Close PhotoFindry, then choose Retry; if it repeats, open the previous app to review library recovery."}
}
fn inspect(options:&Options)->Result<Value> {
    let inspection=handover::inspect_migration(&options.data)?;
    let state=state_directory(options)?;let executable=executable()?;
    let matched=matches_registration(&state,&options.data,&executable);
    let parent=options.data.parent().ok_or("The library has no parent directory.")?;
    let disk=parent.ancestors().find(|path|path.exists()).ok_or("The library disk is unavailable.")?;
    let free=available(disk)?;
    let needed=if inspection.state=="needs-handover" {None} else {Some(RESERVE)};
    let message=match inspection.state {
        "new"=>"A new local library is ready to be created.",
        "needs-handover"=>"Your existing library can be checked and copied before switching to the browser workspace.",
        "ready"=>"Your local library is ready for the browser workspace.",
        "busy"=>"Another PhotoFindry app still owns this library. Quit it, then choose Retry.",
        _=>"Keep the previous app and review this library before switching.",
    };
    Ok(json!({"ok":true,"state":inspection.state,"message":message,"nextAction":if inspection.state=="busy" {"Quit the other PhotoFindry app, then choose Retry. Never delete library lock files."} else {"Continue in the installed PhotoFindry app."},
        "libraryId":inspection.library_id,"estimatedBytes":if inspection.state=="needs-handover" {None} else {Some(0u64)},"estimatePending":inspection.state=="needs-handover","requiredFreeBytes":needed,"availableBytes":free,"registrationMatches":matched}))
}
fn ready_state(initial:&Value)->Result<&str> {
    if initial["loading"]==true||!initial["libraryError"].is_null(){return Err("The saved library did not open. Keep the recovery copy and open the previous app to review recovery.".into());}
    initial["libraryId"].as_str().filter(|id|!id.is_empty()).ok_or("The library did not report a usable identity. Keep the recovery copy and retry.".into())
}
async fn initialize(data:&Path,runtime:&Path,kind:&str,receipt:Option<&handover::Receipt>,admitted:Option<handover::Admission>,cancelled:&AtomicBool)->Result<String> {
    let (worker,task)=worker::Worker::spawn(&runtime.join("PhotoWorker"),data,runtime)?;
    let result=async {
        let initial=tokio::select!{
            value=tokio::time::timeout(Duration::from_secs(60),worker.call("state",json!({})))=>value,
            _=wait_cancel(cancelled)=>return check_cancel(cancelled).map(|_|String::new()),
        }.map_err(|_|"Opening the local library took too long. Keep the recovery copy, close the app, and choose Retry.")??;
        check_cancel(cancelled)?;
        let id=ready_state(&initial)?.to_owned();
        if kind=="new" && initial["jobs"]["paused"]!=true {
            worker.call("jobAction",json!({"action":"pause"})).await?;
            check_cancel(cancelled)?;
        }
        if kind!="new" && initial["jobs"]["paused"]!=true{return Err("The library did not reopen with processing paused. Close PhotoFindry and retry; no browser session was granted.".into());}
        match kind {
            "new"=>handover::publish_new_admission(data,&id)?,
            "needs-handover"=>{
                let receipt=receipt.ok_or("The verified recovery receipt is missing.")?;
                if id!=receipt.library_id{return Err("The library identity changed. Keep the recovery copy and review the previous app before retrying.".into());}
                handover::publish_handover_admission(data,receipt)?;
            },
            "ready"=>{let admission=admitted.as_ref().ok_or("The existing library admission was not checked.")?;handover::complete_admitted_start(data,admission,&id)?;},
            _=>return Err("The migration state changed. Close PhotoFindry and retry.".into()),
        }
        Ok(id)
    }.await;
    worker.stop();let _=task.await;
    result
}
async fn migrate(options:&Options,cancelled:Arc<AtomicBool>)->Result<Value> {
    check_cancel(&cancelled)?;
    progress("checking","Checking local ownership, compatibility and recovery space. No processing job will start.")?;
    let runtime=launch::canonical(options.runtime.as_ref().ok_or("Choose the installed runtime.")?,true)?;
    launch::canonical(&runtime.join("PhotoWorker"),false)?;launch::canonical(&runtime.join("PhotoEngine"),false)?;
    let executable=executable()?;let state=state_directory(options)?;
    let inspection=handover::inspect_migration(&options.data)?;
    if inspection.state=="ready"&&matches_registration(&state,&options.data,&executable)
        && launch::read_registration(&state)?.runtime==runtime {
        return Ok(json!({"ok":true,"state":"ready","libraryId":inspection.library_id,"registrationMatches":true,"message":"Your local library is ready. Opening PhotoFindry does not require pairing again."}));
    }
    if inspection.state=="busy" {return Err("Another PhotoFindry app owns this library. Quit the other app, then choose Retry. Do not remove its lock files.".into());}
    // Validate the old registration and broker before opening/changing stores.
    if state.exists() {
        let _owner=launch::private_directory(&state,false)?;
        launch::ensure_available(&state)?;
        let _broker=launch::try_lock(&state,"broker.lock")?.ok_or("The previous browser companion is still running. Quit it before switching this installation, then Retry.")?;
    }
    let _package=uninstall::runtime_lease(&executable,&runtime)?;
    let rollback=options.rollback.as_ref().ok_or("Choose a recovery-copy folder.")?;
    let receipt=if inspection.state=="needs-handover" {
        let backup_parent=rollback.parent().ok_or("Choose a recovery-copy folder with an existing parent.")?;
        if backup_parent.starts_with(&options.data)||options.data.starts_with(rollback){return Err("Choose a recovery-copy folder outside the current library.".into());}
        handover::create_migration_parent(backup_parent)?;
        let (source,cancel)=(options.data.clone(),cancelled.clone());
        let bytes=tokio::task::spawn_blocking(move||handover::migration_bytes(&source,&cancel)).await.map_err(|_|"The library size check stopped. Choose Retry; saved work is kept.")??;
        check_cancel(&cancelled)?;
        let needed=required(bytes)?;
        if !rollback.exists()&&available(backup_parent)?<needed {return Err(format!("There is not enough free disk space for the recovery copy ({} bytes required). Free space or choose another recovery disk, then Retry.",needed));}
        progress("backup","Saving and verifying a complete private recovery copy of the library folder. External photo and model folders stay in place.")?;
        let (data,rollback,cancel)=(options.data.clone(),rollback.clone(),cancelled.clone());
        let mut task=tokio::task::spawn_blocking(move||handover::prepare_or_resume(&data,&rollback,&cancel));
        let result=tokio::select!{result=&mut task=>Some(result),_=wait_cancel(&cancelled)=>None};
        match result {Some(result)=>Some(result.map_err(|_|"The recovery copy stopped unexpectedly. Choose Retry; the original library is kept.")??),None=>{cancelled.store(true,Ordering::Release);let _=task.await;return Err("The switch was cancelled. Your original library is kept; any incomplete recovery copy remains private. Choose Retry to continue.".into());}}
    }else{None};
    check_cancel(&cancelled)?;
    let data=if inspection.state=="new" {handover::create_migration_parent(options.data.parent().ok_or("The library parent is unavailable.")?)?;handover::use_empty_library(&options.data)?}else{launch::canonical(&options.data,true)?};
    let ownership=super::library_lock(&data)?;
    let admitted=if inspection.state=="ready" {Some(handover::validate_admission(&data)?)}else{None};
    progress("opening","Opening the saved library locally and checking its identity. Your processing queue remains paused.")?;
    let id=initialize(&data,&runtime,inspection.state,receipt.as_ref(),admitted,&cancelled).await?;
    check_cancel(&cancelled)?;
    drop(ownership);
    progress("registering","Connecting the installed launcher to this library. The website receives no library access.")?;
    if options.state.is_none(){register_links(&executable,&cancelled).await?;}
    check_cancel(&cancelled)?;
    launch::register(&state,&data,&runtime,&executable)?;
    Ok(json!({"ok":true,"state":"ready","libraryId":id,"rollbackPath":receipt.as_ref().map(|value|&value.rollback),"registrationMatches":true,"message":"Your library and model paths are preserved. Open PhotoFindry to continue; queued work remains paused."}))
}
fn library_stopped(data:&Path)->Result<bool> {
    let file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_DIRECTORY).open(data)
        .map_err(|_|"Cannot verify that the saved library has stopped.")?;
    if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Ok(false);}
    if data.join("worker.lock").symlink_metadata().is_ok(){return Ok(false);}
    let marker=data.join(".companion.lock");
    if marker.exists(){let lock=launch::private_file(&marker,false)?;if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Ok(false);}}
    Ok(true)
}
async fn prepare_update(options:&Options)->Result<Value> {
    let state=state_directory(options)?;
    if !options.data.exists(){
        if uninstall::try_update_runtime(&executable()?)?.is_some(){return Ok(json!({"ok":true,"ready":true}));}
        return Err("This app package still has an active local companion. Finish or pause its work and quit it, then Retry the update.".into());
    }
    if !state.exists(){if library_stopped(&options.data)? && uninstall::try_update_runtime(&executable()?)?.is_some() {return Ok(json!({"ok":true,"ready":true}));}return Err("The previous native app still owns the library. Quit it before updating, then Retry.".into());}
    let _guard=launch::LifecycleGuard::acquire(&state)?;
    if let Ok(record)=launch::read_registration(&state) {
        let executable=executable()?;
        let expected_runtime=executable.parent().and_then(Path::parent).map(|root|root.join("runtime"));
        if record.data_dir!=options.data||record.executable!=executable||expected_runtime.as_ref()!=Some(&record.runtime) {
            if library_stopped(&options.data)? && uninstall::try_update_runtime(&executable)?.is_some() {return Ok(json!({"ok":true,"ready":true}));}
            return Err("This library or app package is owned by another PhotoFindry installation. Quit that installation, then Retry; this app will not stop another package's processes.".into());
        }
    } else if library_stopped(&options.data)? && uninstall::try_update_runtime(&executable()?)?.is_some() {return Ok(json!({"ok":true,"ready":true}));}
    else {return Err("This library is still owned by an unregistered PhotoFindry app. Quit it, then Retry.".into());}
    launch::request_update_stop(&state).await?;
    let deadline=tokio::time::Instant::now()+Duration::from_secs(60);
    loop {
        if let Some(_broker)=launch::try_lock(&state,"broker.lock")? {
            if library_stopped(&options.data)? {
                if let Some(_package)=uninstall::try_update_runtime(&executable()?)? {return Ok(json!({"ok":true,"ready":true}));}
            }
        }
        if tokio::time::Instant::now()>=deadline{return Err("PhotoFindry is still closing its owned processes. Wait a moment, then Retry the update; saved work is kept.".into());}
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn register_links(executable:&Path,cancelled:&AtomicBool)->Result<()> {
    check_cancel(cancelled)?;
    let helper=launch::browser_opener(executable)?;
    let mut child=launch::desktop_command(&helper).arg("--register-links")
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .kill_on_drop(true).spawn().map_err(|_|"Cannot register the installed PhotoFindry Open link. Reopen this installed app and Retry.".to_owned())?;
    let result=tokio::select!{
        value=tokio::time::timeout(Duration::from_secs(15),child.wait())=>Some(value),
        _=wait_cancel(cancelled)=>None,
    };
    let status=match result{
        Some(Ok(Ok(status)))=>status,
        _=>{let _=child.kill().await;let _=child.wait().await;check_cancel(cancelled)?;return Err("macOS Open-link registration took too long. Keep this app installed and choose Retry.".into());}
    };
    check_cancel(cancelled)?;
    if !status.success(){return Err("macOS did not register this installed PhotoFindry app for Open links. Keep it in place and Retry.".into());}Ok(())
}
pub async fn command(args:&[String])->Result<()> {
    let cancelled=Arc::new(AtomicBool::new(false));
    let cancellation=cancelled.clone();
    let mut terminated=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|_|"Cannot protect migration cancellation.")?;
    let signal=tokio::spawn(async move{tokio::select!{_=tokio::signal::ctrl_c()=>{},_=terminated.recv()=>{}}cancellation.store(true,Ordering::Release);});
    let update=args.first().is_some_and(|value|value=="prepare-app-update");
    let result=match parse(args) {
        Ok(options)=>match args.first().map(String::as_str){Some("migration-status")=>inspect(&options),Some("migrate-library")=>migrate(&options,cancelled.clone()).await,Some("prepare-app-update")=>prepare_update(&options).await,_=>Err("Unknown migration command.".into())},
        Err(error)=>Err(error),
    };
    signal.abort();
    match result {
        Ok(value)=>emit(value),
        Err(message)=>{
            emit(if update {json!({"ok":false,"ready":false,"message":message,"nextAction":error_next(&message)})}
                else {json!({"ok":false,"state":"incompatible","message":message,"nextAction":error_next(&message)})})?;
            Err(message)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn strict_migration_options_never_accept_public_authority(){
        for args in [vec!["migration-status","--current-library","--data-dir","/tmp/not-account-default"],vec!["migration-status","--data-dir","relative"],vec!["migration-status","--data-dir","/tmp/a","--data-dir","/tmp/b"],vec!["migration-status","--data-dir","/tmp/a","--worker","/tmp/arbitrary"],vec!["prepare-app-update","--data-dir","/tmp/a","--force","true"],vec!["migrate-library","--data-dir","/tmp/a","--runtime","/tmp/runtime"]] {
            assert!(parse(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]fn recovery_space_reserves_bounded_extra_working_space(){assert_eq!(required(0).unwrap(),RESERVE);assert_eq!(required(1024).unwrap(),RESERVE+1024);assert!(required(u64::MAX).is_err());}
}
