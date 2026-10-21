//! Native user selection produces short-lived path capabilities. Neither paths
//! nor executable-picker arguments are accepted from browser input.
use serde_json::{Value, json};
use std::{collections::HashMap, path::{Path,PathBuf}, process::Stdio, sync::{Mutex,atomic::{AtomicU64,Ordering}}, time::{Duration,Instant}};
use std::os::unix::fs::MetadataExt;
use tokio::io::{AsyncRead,AsyncReadExt};
use std::os::unix::process::ExitStatusExt;

const KINDS: &[&str] = &["folder","sourceFolder","model","projector","reviewModel","reviewProjector","template","reranker","backupOpen","backupSave","archiveOpen","archiveSave","photoExportSave","screenshot"];
const LIFE: Duration = Duration::from_secs(1800);
struct Grant { path: PathBuf, kind: String, expires: Instant, identity: Option<(u64,u64,u64,i64,i64)>, access:Option<super::source_access::AccessGrant>, _reader_lease:Option<std::sync::Arc<super::native_access::ReaderLease>> }
pub struct Picker { executable: PathBuf, grants: Mutex<HashMap<String,Grant>>, active: tokio::sync::Mutex<()>, reader_open:std::sync::Arc<tokio::sync::Semaphore>, generation: AtomicU64 }

fn identity(path: &Path) -> Result<(u64,u64,u64,i64,i64),String> {
    let metadata = std::fs::metadata(path).map_err(|_| "The selected location is no longer available.".to_owned())?;
    Ok((metadata.dev(),metadata.ino(),metadata.len(),metadata.mtime(),metadata.mtime_nsec()))
}

// Drain both child pipes concurrently. Diagnostics are bounded and used only
// for fixed local failure categories; raw AppKit text can contain private paths.
async fn read_picker_pipe<R:AsyncRead+Unpin>(mut input:R,limit:usize)->Result<(Vec<u8>,bool),String>{
    let mut retained=Vec::new();let mut exceeded=false;let mut buffer=[0u8;4096];
    loop {
        let count=input.read(&mut buffer).await.map_err(|_|"The file chooser disconnected.".to_owned())?;
        if count==0 {break;}
        let keep=count.min(limit.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
        exceeded|=keep<count;
    }
    Ok((retained,exceeded))
}
fn picker_failure(status:std::process::ExitStatus,diagnostics:&[u8])->String{
    let diagnostics=String::from_utf8_lossy(diagnostics);
    if diagnostics.contains("_RegisterApplication")||diagnostics.contains("unable to get application ASN") {
        return "macOS could not connect the file chooser to your desktop session. Quit the companion, then open PhotoFindry again from the installed launcher.".into();
    }
    if status.signal().is_some() {
        return "macOS stopped the file chooser before a selection was returned. Open PhotoFindry again from the installed launcher and retry.".into();
    }
    "The installed file chooser could not complete this action. Reopen PhotoFindry from the installed launcher and retry; if it continues, reinstall the companion package.".into()
}
async fn bounded_reader<T:Send+'static>(permit:tokio::sync::OwnedSemaphorePermit,open:impl FnOnce()->Result<T,String>+Send+'static,wait:Duration)->Result<T,String>{
    let(sender,opening)=tokio::sync::oneshot::channel();
    // A stalled provider must not hold an async executor or Tokio shutdown.
    // The single permit stays with this read-only thread; late output drops.
    std::thread::Builder::new().name("photofindry-folder-access".into()).spawn(move||{let _permit=permit;let _=sender.send(open());})
        .map_err(|_|"The native folder access check could not start.")?;
    tokio::time::timeout(wait,opening).await
        .map_err(|_|"Opening native folder access took too long. Saved work is kept; its current check is still finishing.")?
        .map_err(|_|"The native folder access check stopped unexpectedly.")?
}

impl Picker {
    pub fn new(runtime: &Path) -> Self {
        let sibling = std::env::current_exe().ok().and_then(|p|p.parent().map(|v|v.join("CompanionPicker")));
        let executable = sibling.filter(|p|p.is_file()).unwrap_or_else(||runtime.join("CompanionPicker"));
        let bundle=executable.parent().unwrap_or(runtime).join("CompanionPicker.app/Contents/MacOS/CompanionPicker");
        let executable=if bundle.is_file(){bundle}else{executable};
        Self { executable,grants:Mutex::new(HashMap::new()),active:tokio::sync::Mutex::new(()),reader_open:std::sync::Arc::new(tokio::sync::Semaphore::new(1)),generation:AtomicU64::new(0) }
    }
    pub fn revoke(&self) {
        self.generation.fetch_add(1,Ordering::AcqRel);
        self.grants.lock().unwrap().clear();
    }
    /// Called only with a saved, availability-checked photo location or the owned
    /// data directory. There is deliberately no browser path argument.
    pub async fn open_photo(&self,path:&str,reveal:bool)->Result<Value,String>{
        if !Path::new(path).is_absolute()||path.len()>16384||path.contains('\0'){return Err("Invalid saved photo location.".into());}
        let mut child=super::launch::desktop_command(&self.executable).arg(if reveal {"reveal"} else {"openPhoto"}).arg(path)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The native photo opener is unavailable.")?;
        let stdout=child.stdout.take().ok_or("The native photo opener is unavailable.")?;
        let task=async {
            let mut bytes=Vec::new();stdout.take(4097).read_to_end(&mut bytes).await.map_err(|_|"The native photo opener disconnected.")?;
            if bytes.len()>4096||!child.wait().await.map_err(|_|"The native photo opener stopped.")?.success(){return Err("The original could not be opened. Reconnect its source and try again.");}
            let value:Value=serde_json::from_slice(&bytes).map_err(|_|"The native photo opener returned an invalid response.")?;
            if value["opened"]!=true{return Err("The original could not be opened.");}Ok(json!({"opened":true}))
        };
        match tokio::time::timeout(Duration::from_secs(15),task).await{Ok(value)=>value.map_err(str::to_owned),Err(_)=>{let _=child.kill().await;Err("The native photo opener timed out.".into())}}
    }
    pub async fn pick(&self,kind:&str)->Result<Value,String> {
        if !KINDS.contains(&kind) { return Err("Unknown file selection.".into()); }
        let generation=self.generation.load(Ordering::Acquire);
        let _active=self.active.try_lock().map_err(|_|"Finish the open file chooser first.")?;
        if ["folder","sourceFolder"].contains(&kind) {
            let executable=tokio::task::spawn_blocking(super::lifecycle::source_access_helper_for_current_executable).await
                .map_err(|_|"The installed folder access owner could not be verified.")??;
            if let Some(executable)=executable {
                let permit=self.reader_open.clone().try_acquire_owned().map_err(|_|"The previous folder access check is still finishing. Saved work is kept; try again shortly.")?;
                let selection=super::source_access::Owner::verified(executable).select(kind,generation).await?;
                if generation!=self.generation.load(Ordering::Acquire){return Err("The local session changed; choose the folder again.".into());}
                return match selection {
                    super::source_access::Selection::Cancelled=>Ok(json!({"cancelled":true})),
                    super::source_access::Selection::Granted(mut access)=>{
                        let bookmark=access.reader_bookmark().to_owned();let expected=PathBuf::from(&access.path);
                        let lease=bounded_reader(permit,move||super::native_access::ReaderLease::open(&bookmark,&expected).map(std::sync::Arc::new),Duration::from_secs(30)).await?;
                        if generation!=self.generation.load(Ordering::Acquire){return Err("The local session changed; choose the folder again.".into());}
                        let path=lease.path().to_string_lossy().into_owned();access.path=path.clone();self.grant_path_with_access(kind,&path,generation,Some(access),Some(lease))
                    }
                };
            }
        }
        let mut child=super::launch::desktop_command(&self.executable).arg(kind).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().map_err(|_|"The native file chooser is unavailable. Rebuild the companion tools.")?;
        let stdout=child.stdout.take().ok_or("The file chooser could not start.")?;
        let stderr=child.stderr.take().ok_or("The file chooser could not start.")?;
        let task=async {
            let (output,diagnostics)=tokio::join!(read_picker_pipe(stdout,32768),read_picker_pipe(stderr,4096));
            let (bytes,exceeded)=output?;let (diagnostics,_)=diagnostics?;
            let status=child.wait().await.map_err(|_|"The file chooser stopped unexpectedly.".to_owned())?;
            if !status.success(){return Err(picker_failure(status,&diagnostics));}
            if exceeded{return Err("The file chooser response is too large.".to_owned());}
            serde_json::from_slice::<Value>(&bytes).map_err(|_|"The file chooser returned an invalid selection.".to_owned())
        };
        let value=match tokio::time::timeout(Duration::from_secs(300),task).await {
            Ok(result)=>result?,
            Err(_)=>{let _=child.kill().await;return Err("The file chooser timed out. Choose the file again.".into());}
        };
        if generation!=self.generation.load(Ordering::Acquire){return Err("The local session changed; choose the file again.".into());}
        if value["cancelled"]==true{return Ok(json!({"cancelled":true}));}
        let raw=value["path"].as_str().filter(|p|!p.contains('\0')&&p.len()<=16384).ok_or("Invalid selected location.")?;
        self.grant_path_at(kind,raw,generation)
    }
    /// Explicit CLI-only equivalent of a native selection; never an HTTP route.
    pub fn grant_path(&self,kind:&str,raw:&str)->Result<Value,String>{
        self.grant_path_at(kind,raw,self.generation.load(Ordering::Acquire))
    }
    fn grant_path_at(&self,kind:&str,raw:&str,generation:u64)->Result<Value,String>{
        self.grant_path_with_access(kind,raw,generation,None,None)
    }
    fn grant_path_with_access(&self,kind:&str,raw:&str,generation:u64,access:Option<super::source_access::AccessGrant>,lease:Option<std::sync::Arc<super::native_access::ReaderLease>>)->Result<Value,String>{
        if !KINDS.contains(&kind)||raw.contains('\0')||raw.len()>16384{return Err("Invalid local selection.".into());}
        // Context changes only the native chooser copy. These aliases retain
        // the established typed capabilities and selection validation.
        let kind=match kind {"sourceFolder"=>"folder","reviewModel"=>"model","reviewProjector"=>"projector",other=>other};
        let path=PathBuf::from(raw);
        if !path.is_absolute(){return Err("The selected location must be absolute.".into());}
        let save=kind.ends_with("Save");
        let recorded=if save {
            if !path.parent().is_some_and(|p|p.is_dir()){return Err("The destination folder is unavailable.".into());}
            None
        } else {
            if (kind=="folder"&&!path.is_dir())||(kind=="backupOpen"&&!path.is_dir()&&!path.is_file())||(!["folder","backupOpen"].contains(&kind)&&!path.is_file()){return Err("The selected location has the wrong type.".into());}
            // Folder timestamps change during discovery, so bind only directory identity.
            let mut value=identity(&path)?;if kind=="folder" {value.2=0;value.3=0;value.4=0;}Some(value)
        };
        let mut random=[0u8;32];getrandom::fill(&mut random).map_err(|_|"The system random source is unavailable.")?;
        let grant_id:String=random.iter().map(|b|format!("{b:02x}")).collect();
        let label=path.file_name().map(|s|s.to_string_lossy().into_owned()).unwrap_or_else(||"Selected folder".into());
        let mut grants=self.grants.lock().unwrap();
        grants.retain(|_,g|g.expires>Instant::now());
        if grants.len()>=256{return Err("Too many pending selections. Reconnect to clear them.".into());}
        if generation!=self.generation.load(Ordering::Acquire){return Err("The local session changed; choose the file again.".into());}
        let mut presentation=json!({"grantId":grant_id,"label":label,"kind":kind,"cancelled":false});
        // Folder context is display-only. The selected path remains exclusively
        // inside the typed grant, and no path is accepted back from the browser.
        if kind=="folder" {presentation["locationLabel"]=json!(raw.chars().take(4096).collect::<String>());}
        grants.insert(grant_id.clone(),Grant{path,kind:kind.into(),expires:Instant::now()+LIFE,identity:recorded,access,_reader_lease:lease});
        Ok(presentation)
    }
    pub fn resolve(&self,grant:&str,kind:&str)->Result<String,String> {
        let grants=self.grants.lock().unwrap();
        let selected=grants.get(grant).filter(|g|g.kind==kind&&g.expires>Instant::now()).ok_or("Choose this file or folder again; its permission expired.")?;
        if let Some(expected)=selected.identity {
            let mut current=identity(&selected.path)?;
            if selected.kind=="folder" {current.2=0;current.3=0;current.4=0;}
            if current!=expected {return Err("The selected file changed. Choose it again before continuing.".into());}
        }
        Ok(selected.path.to_string_lossy().into_owned())
    }
    /// Private action carrier. Browser callers cannot select this field; it is
    /// attached only after the existing opaque, typed picker ID is validated.
    pub fn resolve_source_access(&self,grant:&str)->Result<Option<Value>,String>{
        self.resolve(grant,"folder")?;
        let grants=self.grants.lock().unwrap();
        let selected=grants.get(grant).filter(|g|g.kind=="folder"&&g.expires>Instant::now()).ok_or("Choose the originals folder again; its permission expired.")?;
        Ok(selected.access.as_ref().map(|access|access.internal_value()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn appkit_registration_failure_is_actionable_without_private_diagnostics(){
        let text=picker_failure(std::process::ExitStatus::from_raw(6),b"_RegisterApplication unable to get application ASN /Users/private-secret/photo.jpg");
        assert!(text.contains("desktop session"));assert!(text.contains("installed launcher"));
        assert!(!text.contains("private-secret"));assert!(!text.contains("photo.jpg"));
        assert!(picker_failure(std::process::ExitStatus::from_raw(9),b"private diagnostic").contains("macOS stopped"));
        assert!(!picker_failure(std::process::ExitStatus::from_raw(2<<8),b"private diagnostic").contains("private diagnostic"));
    }
    #[tokio::test]async fn chooser_drains_large_diagnostics_and_preserves_cancel_and_archive_grants(){
        use std::os::unix::fs::PermissionsExt;
        let mut nonce=[0u8;12];getrandom::fill(&mut nonce).unwrap();
        let suffix:String=nonce.iter().map(|v|format!("{v:02x}")).collect();
        let root=std::env::temp_dir().join(format!("photofindry-picker-process-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
        let _cleanup=Cleanup(root.clone());
        let executable=root.join("chooser");
        let picker=Picker{executable:executable.clone(),grants:Mutex::new(HashMap::new()),active:tokio::sync::Mutex::new(()),reader_open:std::sync::Arc::new(tokio::sync::Semaphore::new(1)),generation:AtomicU64::new(0)};
        let script=|body:&str|{
            std::fs::write(&executable,format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&executable,std::fs::Permissions::from_mode(0o700)).unwrap();
        };
        script("/usr/bin/head -c 65536 /dev/zero >&2\nprintf '%s\\n' '{\"cancelled\":true}'");
        assert_eq!(tokio::time::timeout(Duration::from_secs(5),picker.pick("archiveOpen")).await.unwrap().unwrap(),json!({"cancelled":true}));
        script("printf '%s' '_RegisterApplication unable to get application ASN /Users/private-secret' >&2\nexit 6");
        let failure=picker.pick("backupOpen").await.unwrap_err();
        assert!(failure.contains("desktop session"));assert!(!failure.contains("private-secret"));
        script("/usr/bin/head -c 65536 /dev/zero");
        assert_eq!(tokio::time::timeout(Duration::from_secs(5),picker.pick("archiveOpen")).await.unwrap().unwrap_err(),"The file chooser response is too large.");
        let archive=root.join("fixture.photofindrysource.zip");std::fs::write(&archive,b"test archive bytes").unwrap();
        let response=json!({"cancelled":false,"path":archive}).to_string().replace('\'',"'\\''");
        script(&format!("printf '%s\\n' '{response}'"));
        let grant=picker.pick("archiveOpen").await.unwrap();
        assert_eq!(grant["label"],"fixture.photofindrysource.zip");
        assert_eq!(picker.resolve(grant["grantId"].as_str().unwrap(),"archiveOpen").unwrap(),archive.to_string_lossy());
        assert!(!grant.to_string().contains(root.to_string_lossy().as_ref()));
        for (requested,capability,other) in [("reviewModel","model","projector"),("reviewProjector","projector","model")] {
            let file=root.join(format!("{requested}.gguf"));std::fs::write(&file,b"review fixture").unwrap();
            let response=json!({"cancelled":false,"path":file}).to_string().replace('\'',"'\\''");
            script(&format!("test \"$#\" -eq 1 && test \"$1\" = '{requested}' || exit 2\nprintf '%s\\n' '{response}'"));
            let grant=picker.pick(requested).await.unwrap();
            let id=grant["grantId"].as_str().unwrap();
            assert_eq!(grant["kind"],capability);
            assert_eq!(picker.resolve(id,capability).unwrap(),file.to_string_lossy());
            assert!(picker.resolve(id,other).is_err());
            assert!(picker.resolve(id,requested).is_err());
            assert!(!grant.to_string().contains(root.to_string_lossy().as_ref()));
            std::fs::write(&file,b"changed review fixture").unwrap();
            assert!(picker.resolve(id,capability).is_err());
            let renewed=picker.grant_path(requested,file.to_str().unwrap()).unwrap();
            assert_eq!(renewed["kind"],capability);
            assert!(picker.resolve(renewed["grantId"].as_str().unwrap(),capability).is_ok());
            picker.revoke();assert!(picker.resolve(renewed["grantId"].as_str().unwrap(),capability).is_err());
        }
        let folder=root.join("Original photos");std::fs::create_dir(&folder).unwrap();
        let response=json!({"cancelled":false,"path":folder}).to_string().replace('\'',"'\\''");
        script(&format!("test \"$#\" -eq 1 && test \"$1\" = 'sourceFolder' || exit 2\nprintf '%s\\n' '{response}'"));
        let selected=picker.pick("sourceFolder").await.unwrap();let id=selected["grantId"].as_str().unwrap();
        assert_eq!(selected["kind"],"folder");assert_eq!(selected["locationLabel"],folder.to_str().unwrap());
        assert_eq!(picker.resolve(id,"folder").unwrap(),folder.to_str().unwrap());
        assert!(picker.resolve(id,"sourceFolder").is_err());assert!(picker.resolve(id,"archiveOpen").is_err());
        assert!(picker.pick("reviewModel:clef-flash").await.is_err());
    }
    #[test]fn grants_are_typed_revocable_and_bound_to_selected_file(){
        let root=std::env::temp_dir().join(format!("photofindry-picker-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file=root.join("model.gguf");std::fs::write(&file,"test").unwrap();
        let picker=Picker::new(&root);
        let grant=picker.grant_path("model",file.to_str().unwrap()).unwrap();
        let id=grant["grantId"].as_str().unwrap();
        assert!(grant.get("locationLabel").is_none());
        assert_eq!(picker.resolve(id,"model").unwrap(),file.to_str().unwrap());
        assert!(picker.resolve(id,"backupOpen").is_err());
        std::fs::write(&file,"changed").unwrap();assert!(picker.resolve(id,"model").is_err());
        let folder=picker.grant_path("folder",root.to_str().unwrap()).unwrap();
        assert_eq!(folder["locationLabel"],root.to_str().unwrap());
        assert!(folder.get("path").is_none());
        assert!(picker.resolve(folder["locationLabel"].as_str().unwrap(),"folder").is_err());
        assert!(picker.resolve(folder["grantId"].as_str().unwrap(),"model").is_err());
        std::fs::write(root.join("child"),"test").unwrap();
        assert!(picker.resolve(folder["grantId"].as_str().unwrap(),"folder").is_ok());
        picker.revoke();assert!(picker.resolve(folder["grantId"].as_str().unwrap(),"folder").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]fn folder_display_labels_distinguish_same_named_folders_without_broadening_grants(){
        let mut nonce=[0u8;12];getrandom::fill(&mut nonce).unwrap();
        let suffix:String=nonce.iter().map(|v|format!("{v:02x}")).collect();
        let root=std::env::temp_dir().join(format!("photofindry-picker-locations-{suffix}"));
        struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
        let _cleanup=Cleanup(root.clone());let picker=Picker::new(&root);
        let old=root.join("other-user/pCloud Drive/Automatic Upload");let current=root.join("current-user/pCloud Drive/Automatic Upload");
        std::fs::create_dir_all(&old).unwrap();std::fs::create_dir_all(&current).unwrap();
        let a=picker.grant_path("folder",old.to_str().unwrap()).unwrap();let b=picker.grant_path("folder",current.to_str().unwrap()).unwrap();
        assert_eq!(a["label"],b["label"]);assert_ne!(a["locationLabel"],b["locationLabel"]);
        assert_eq!(b["locationLabel"],current.to_str().unwrap());
        assert_eq!(picker.resolve(b["grantId"].as_str().unwrap(),"folder").unwrap(),current.to_str().unwrap());
        for kind in ["model","projector","template","reranker","archiveOpen","backupOpen"]{assert!(picker.resolve(b["grantId"].as_str().unwrap(),kind).is_err());}
        let contextual=picker.grant_path("sourceFolder",current.to_str().unwrap()).unwrap();
        assert_eq!(contextual["kind"],"folder");assert_eq!(contextual["locationLabel"],current.to_str().unwrap());
        assert_eq!(picker.resolve(contextual["grantId"].as_str().unwrap(),"folder").unwrap(),current.to_str().unwrap());
        assert!(picker.resolve(contextual["grantId"].as_str().unwrap(),"sourceFolder").is_err());
        let file=root.join("wrong-type.jpg");std::fs::write(&file,b"test").unwrap();
        assert!(picker.grant_path("sourceFolder",file.to_str().unwrap()).is_err());
        picker.revoke();assert!(picker.resolve(b["grantId"].as_str().unwrap(),"folder").is_err());
    }
    #[test]fn whole_current_and_legacy_backup_directories_keep_separate_archive_authority(){
        let mut nonce=[0u8;12];getrandom::fill(&mut nonce).unwrap();
        let suffix:String=nonce.iter().map(|v|format!("{v:02x}")).collect();
        let root=std::env::temp_dir().join(format!("photofindry-picker-backups-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
        let _cleanup=Cleanup(root.clone());
        let picker=Picker::new(&root);
        for extension in ["photofindrybackup","photorecallbackup"] {
            let package=root.join(format!("Library.{extension}"));
            std::fs::create_dir(&package).unwrap();
            std::fs::write(package.join("manifest.json"),b"inspection verifies the manifest separately").unwrap();
            let selected=picker.grant_path("backupOpen",package.to_str().unwrap()).unwrap();
            let id=selected["grantId"].as_str().unwrap();
            assert_eq!(selected["kind"],"backupOpen");
            assert_eq!(picker.resolve(id,"backupOpen").unwrap(),package.to_string_lossy());
            // Choosing a whole backup never grants permission to attach it as
            // a source ZIP, export over it, or use it as a photo folder.
            for other in ["archiveOpen","backupSave","folder"] {assert!(picker.resolve(id,other).is_err());}
            assert!(picker.grant_path("archiveOpen",package.to_str().unwrap()).is_err());
            assert!(!selected.to_string().contains(root.to_string_lossy().as_ref()));
            picker.revoke();assert!(picker.resolve(id,"backupOpen").is_err());
        }
        let archive=root.join("Library.photofindrysource.zip");std::fs::write(&archive,b"source archive fixture").unwrap();
        let selected=picker.grant_path("archiveOpen",archive.to_str().unwrap()).unwrap();
        let id=selected["grantId"].as_str().unwrap();
        assert_eq!(picker.resolve(id,"archiveOpen").unwrap(),archive.to_string_lossy());
        assert!(picker.resolve(id,"backupOpen").is_err());
    }
}

#[cfg(test)]
#[tokio::test]
async fn bounded_native_reader_timeout_keeps_single_flight_and_drops_late_scope() {
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    struct Scope(Arc<AtomicBool>);impl Drop for Scope{fn drop(&mut self){self.0.store(true,Ordering::Release);}}
    let semaphore=Arc::new(tokio::sync::Semaphore::new(1));let permit=semaphore.clone().try_acquire_owned().unwrap();
    let(release,blocked)=std::sync::mpsc::channel();let dropped=Arc::new(AtomicBool::new(false));let observed=dropped.clone();
    let result=bounded_reader(permit,move||{blocked.recv().unwrap();Ok(Scope(observed))},Duration::from_millis(20)).await;
    assert!(result.is_err());assert_eq!(semaphore.available_permits(),0,"a timed-out provider cannot spawn unbounded reader threads");
    release.send(()).unwrap();let end=tokio::time::Instant::now()+Duration::from_secs(2);
    while !dropped.load(Ordering::Acquire)||semaphore.available_permits()!=1{assert!(tokio::time::Instant::now()<end);tokio::time::sleep(Duration::from_millis(5)).await;}
}

#[cfg(test)]
#[tokio::test]
async fn cancelled_native_reader_drops_its_late_scope_without_holding_async_shutdown() {
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    struct Scope(Arc<AtomicBool>);impl Drop for Scope{fn drop(&mut self){self.0.store(true,Ordering::Release);}}
    let semaphore=Arc::new(tokio::sync::Semaphore::new(1));let permit=semaphore.clone().try_acquire_owned().unwrap();
    let(release,blocked)=std::sync::mpsc::channel();let(started,ready)=tokio::sync::oneshot::channel();let dropped=Arc::new(AtomicBool::new(false));let observed=dropped.clone();
    let task=tokio::spawn(bounded_reader(permit,move||{let _=started.send(());blocked.recv().unwrap();Ok(Scope(observed))},Duration::from_secs(30)));
    ready.await.unwrap();task.abort();assert!(task.await.is_err());assert_eq!(semaphore.available_permits(),0);
    release.send(()).unwrap();let end=tokio::time::Instant::now()+Duration::from_secs(2);
    while !dropped.load(Ordering::Acquire)||semaphore.available_permits()!=1{assert!(tokio::time::Instant::now()<end);tokio::time::sleep(Duration::from_millis(5)).await;}
}

#[cfg(test)]
#[tokio::test]
async fn native_folder_access_stays_behind_typed_picker_ids_until_private_action_preparation() {
    use std::os::unix::fs::PermissionsExt;
    let mut nonce=[0u8;12];getrandom::fill(&mut nonce).unwrap();let suffix=nonce.iter().map(|v|format!("{v:02x}")).collect::<String>();
    let root=std::env::temp_dir().join(format!("pf-picker-access-carrier-{suffix}"));std::fs::create_dir(&root).unwrap();
    struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    let _cleanup=Cleanup(root.clone());let folder=root.join("Originals");std::fs::create_dir(&folder).unwrap();
    let helper=root.join("owner-fixture");let path=serde_json::to_string(folder.to_str().unwrap()).unwrap();
    std::fs::write(&helper,format!("#!/usr/bin/python3\nimport sys,json,base64\nr=json.load(sys.stdin)\nv={{k:r[k] for k in ['schema','operation','requestNonce','generation']}}\nv['cancelled']=False\nv['selection']={{'ownerGrantId':'a'*64,'path':{path},'selectedPath':{path},'readerBookmark':base64.b64encode(b'fixture-only-implicit-reader').decode()}}\nprint(json.dumps(v))\n")).unwrap();
    std::fs::set_permissions(&helper,std::fs::Permissions::from_mode(0o700)).unwrap();
    let super::source_access::Selection::Granted(access)=super::source_access::Owner::verified(helper).select("sourceFolder",0).await.unwrap()else{panic!("missing selection")};
    let carrier=access.internal_value();let picker=Picker::new(&root);
    // The fake carrier deliberately has no real ReaderLease. This unit checks
    // private transport only; it never invokes an AppKit selection/TCC grant.
    let visible=picker.grant_path_with_access("sourceFolder",folder.to_str().unwrap(),0,Some(access),None).unwrap();
    let public=visible.to_string();assert!(!public.contains("readerBookmark"));assert!(!public.contains(&"a".repeat(64)));assert!(!public.contains("implicit-reader"));
    let id=visible["grantId"].as_str().unwrap();let scopes=super::projection::Scopes::default();
    let(_,add)=super::operations::prepare("addFolders",json!({"folderGrants":[id]}),&scopes,&picker).unwrap();assert_eq!(add["sourceAccess"],json!([carrier.clone()]));
    let(_,locate)=super::operations::prepare("locateSource",json!({"sourceId":"source","rootId":"root","folderGrant":id}),&scopes,&picker).unwrap();assert_eq!(locate["sourceAccess"],json!([carrier]));
    picker.revoke();assert!(picker.resolve_source_access(id).is_err());
}
