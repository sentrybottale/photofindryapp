//! Optional support invitation. Host-only, bounded operational state; never source evidence.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::{Read, Write}, os::unix::{fs::OpenOptionsExt, io::AsRawFd}, path::PathBuf, time::Instant};
const REQUIRED: f64 = 8.0 * 60.0 * 60.0;
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    #[serde(default)] active_seconds: f64,
    #[serde(default)] active_days: Vec<String>,
    #[serde(default)] automatic_prompt_handled: bool,
}
impl Saved {
    fn clean(mut self) -> Self {
        self.active_seconds = if self.active_seconds.is_finite() { self.active_seconds.clamp(0.0, REQUIRED) } else { 0.0 };
        let days=std::mem::take(&mut self.active_days);
        for day in days.into_iter().take(32) {
            let p:Vec<_>=day.split('-').collect();
            if p.len()==3 && p[0].len()==4 && p[1].len()==2 && p[2].len()==2
                && p[0].parse::<u32>().is_ok_and(|n|(1..=9999).contains(&n))
                && p[1].parse::<u32>().is_ok_and(|n|(1..=12).contains(&n))
                && p[2].parse::<u32>().is_ok_and(|n|(1..=31).contains(&n)) && !self.active_days.contains(&day) {
                self.active_days.push(day); if self.active_days.len()==3 {break;}
            }
        }
        self
    }
}
#[derive(Default)]
struct Clock { previous: Option<f64>, active_since: Option<f64>, last_input: Option<f64> }
impl Clock {
    fn suspend(&mut self) { *self=Self::default(); }
    fn input(&mut self, now:f64) {
        if self.last_input.is_some_and(|old|now<old) {self.suspend();}
        if self.last_input.is_none_or(|old|now-old>300.0) {self.active_since=Some(now);}
        self.last_input=Some(now); if self.previous.is_none(){self.previous=Some(now);}
    }
    fn sample(&mut self, now:f64) -> f64 {
        let Some(previous)=self.previous else {self.previous=Some(now);return 0.0;};
        if !(0.0..=30.0).contains(&(now-previous)) {self.suspend();return 0.0;}
        self.previous=Some(now);
        match (self.active_since,self.last_input) { (Some(active),Some(input))=>(now.min(input+300.0)-previous.max(active)).max(0.0), _=>0.0 }
    }
    fn quiet(&self,now:f64)->bool {self.last_input.is_some_and(|input|(30.0..=300.0).contains(&(now-input)))}
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Event {client_id:String,event:String,foreground:bool,quiet:bool}
pub struct Usage {path:PathBuf, saved:Saved, clock:Clock, started:Instant, owner:Option<String>}
// A concurrent process spawn can briefly inherit the open file description
// before exec closes its CLOEXEC descriptor. Closing only our descriptor would
// leave that transaction locked until the child execs. Unlock this transaction
// explicitly, including every early-return path, before closing its descriptor.
struct UsageLock(fs::File);
impl Drop for UsageLock {
    fn drop(&mut self) {unsafe {libc::flock(self.0.as_raw_fd(),libc::LOCK_UN);}}
}
impl Usage {
    pub fn open(path:PathBuf, legacy:Option<Value>)->Result<Self,String> {
        let saved=match read(&path) {Ok(Some(v))=>v,Ok(None)=>legacy.and_then(|v|serde_json::from_value::<Saved>(v).ok()).unwrap_or_default().clean(),Err(_)=>Saved{automatic_prompt_handled:true,..Saved::default()}};
        Ok(Self{path,saved,clock:Clock::default(),started:Instant::now(),owner:None})
    }
    pub fn handle(&mut self,value:Value)->Result<Value,String> {let now=self.started.elapsed().as_secs_f64();self.at(value,now,&local_day())}
    fn at(&mut self,value:Value,now:f64,day:&str)->Result<Value,String> {
        let event:Event=serde_json::from_value(value).map_err(|_|"Invalid support usage event.")?;
        if event.client_id.len()!=36 || !event.client_id.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_hexdigit()})
            || !["input","sample","suspend","manual"].contains(&event.event.as_str()){return Err("Invalid support usage event.".into());}
        let parent=self.path.parent().ok_or("Invalid support preferences path.")?;
        fs::create_dir_all(parent).map_err(|_|"Support preferences are unavailable.")?;
        let lock=fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(self.path.with_extension("lock")).map_err(|_|"Support preferences are unavailable.")?;
        if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Support preferences are busy.".into());}
        let _lock=UsageLock(lock);
        // Re-read under the file lock so another companion's manual suppression
        // or automatic claim cannot be overwritten by this process.
        match read(&self.path) { Ok(Some(saved))=>self.saved=saved, Ok(None)=>{}, Err(_)=>{self.saved.automatic_prompt_handled=true;self.clock.suspend();return Ok(json!({"showInvitation":false,"handled":true}));} }
        let previous=self.saved.clone(); let mut invitation=false;
        if event.event=="manual" {self.saved.automatic_prompt_handled=true;self.clock.suspend();}
        else if !self.saved.automatic_prompt_handled {
            if event.event=="input" && event.foreground {
                if self.owner.as_deref()!=Some(&event.client_id){self.clock.suspend();self.owner=Some(event.client_id.clone());}
                self.clock.input(now);
                if self.saved.active_days.len()<3 && !self.saved.active_days.iter().any(|v|v==day){self.saved.active_days.push(day.into());self.saved=self.saved.clone().clean();}
            } else if self.owner.as_deref()==Some(&event.client_id) {
                if event.event=="suspend" || !event.foreground {self.clock.suspend();}
                else if event.event=="sample" {
                    self.saved.active_seconds=(self.saved.active_seconds+self.clock.sample(now)).min(REQUIRED);
                    if event.quiet && self.clock.quiet(now) && self.saved.active_seconds>=REQUIRED && self.saved.active_days.len()>=3 {
                        self.saved.automatic_prompt_handled=true; self.clock.suspend(); invitation=true;
                    }
                }
            }
        }
        if self.saved!=previous || !self.path.exists() {save(&self.path,&self.saved)?;}
        Ok(json!({"showInvitation":invitation,"handled":self.saved.automatic_prompt_handled}))
    }
}
fn read(path:&PathBuf)->Result<Option<Saved>,String>{
    let mut file=match fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path){Ok(v)=>v,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(_)=>return Err("Support preferences are unavailable.".into())};
    if !file.metadata().map_err(|_|"Support preferences are unavailable.")?.is_file(){return Err("Invalid support preferences.".into());}
    let mut bytes=Vec::new();Read::by_ref(&mut file).take(2049).read_to_end(&mut bytes).map_err(|_|"Support preferences are unavailable.")?;
    if bytes.len()>2048{return Err("Invalid support preferences.".into());}
    Ok(Some(serde_json::from_slice::<Saved>(&bytes).map_err(|_|"Invalid support preferences.")?.clean()))
}
fn save(path:&PathBuf,state:&Saved)->Result<(),String>{
    let mut random=[0u8;8];getrandom::fill(&mut random).map_err(|_|"Cannot save support preferences.")?;
    let suffix:String=random.iter().map(|b|format!("{b:02x}")).collect();let temp=path.with_extension(format!("tmp-{suffix}"));
    let result=(||{let mut file=fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp).map_err(|_|"Cannot save support preferences.")?;
        file.write_all(&serde_json::to_vec(state).map_err(|_|"Cannot save support preferences.")?).map_err(|_|"Cannot save support preferences.")?;file.sync_all().map_err(|_|"Cannot save support preferences.")?;
        fs::rename(&temp,path).map_err(|_|"Cannot save support preferences.")?;fs::File::open(path.parent().ok_or("Invalid support preferences path.")?).and_then(|directory|directory.sync_all()).map_err(|_|"Cannot save support preferences.")?;Ok(())})();let _=fs::remove_file(temp);result
}
fn local_day()->String{unsafe{let now=libc::time(std::ptr::null_mut());let mut value:libc::tm=std::mem::zeroed();if libc::localtime_r(&now,&mut value).is_null(){return String::new();}format!("{:04}-{:02}-{:02}",value.tm_year+1900,value.tm_mon+1,value.tm_mday)}}

// Read the native app's existing bounded Data preference without writing it.
// Foundation stores this property as CFData containing the JSON above.
#[cfg(target_os="macos")]
pub fn legacy_state()->Option<Value>{
    use std::ffi::c_void;
    #[link(name="CoreFoundation",kind="framework")]
    unsafe extern "C" {fn CFStringCreateWithCString(a:*const c_void,s:*const i8,e:u32)->*const c_void;fn CFPreferencesCopyAppValue(k:*const c_void,a:*const c_void)->*const c_void;fn CFGetTypeID(v:*const c_void)->usize;fn CFDataGetTypeID()->usize;fn CFDataGetLength(v:*const c_void)->isize;fn CFDataGetBytePtr(v:*const c_void)->*const u8;fn CFRelease(v:*const c_void);}
    unsafe{let key=CFStringCreateWithCString(std::ptr::null(),c"PHOTORECALL.coffeeSupport.usage.v1".as_ptr(),0x08000100);let app=CFStringCreateWithCString(std::ptr::null(),c"com.sentrybottale.photorecall".as_ptr(),0x08000100);if key.is_null()||app.is_null(){return None;}
        let value=CFPreferencesCopyAppValue(key,app);CFRelease(key);CFRelease(app);if value.is_null(){return None;}
        let result=if CFGetTypeID(value)==CFDataGetTypeID(){let n=CFDataGetLength(value);if (1..=2048).contains(&n){serde_json::from_slice(std::slice::from_raw_parts(CFDataGetBytePtr(value),n as usize)).ok()}else{None}}else{None};CFRelease(value);result}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn releasing_transaction_unlocks_inherited_file_description(){
  let mut entropy=[0u8;8];getrandom::fill(&mut entropy).unwrap();let suffix:String=entropy.iter().map(|byte|format!("{byte:02x}")).collect();
  let path=std::env::temp_dir().join(format!("pf-support-lock-{}-{suffix}",std::process::id()));
  let file=fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&path).unwrap();
  assert_eq!(unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
  // dup and fork retain the same open file description. Keep this alias alive
  // while the parent transaction ends to reproduce the spawn window exactly.
  let inherited=file.try_clone().unwrap();let transaction=UsageLock(file);drop(transaction);
  let next=fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
  assert_eq!(unsafe{libc::flock(next.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
  drop(UsageLock(next));drop(inherited);fs::remove_file(path).unwrap();
 }
 fn event(name:&str,foreground:bool,quiet:bool)->Value{json!({"clientId":"00000000-0000-0000-0000-000000000001","event":name,"foreground":foreground,"quiet":quiet})}
 #[test]fn policy_does_not_credit_idle_sleep_background_or_restart(){let mut c=Clock::default();assert_eq!(c.sample(100.0),0.0);c.input(100.0);assert_eq!(c.sample(115.0),15.0);assert_eq!(c.sample(146.0),0.0);assert_eq!(c.sample(161.0),0.0);c.input(162.0);assert_eq!(c.sample(177.0),15.0);c.suspend();assert_eq!(c.sample(200.0),0.0);assert!(!c.quiet(250.0));}
 #[test]fn invitation_claim_is_durable_once_and_manual_suppresses(){let path=std::env::temp_dir().join(format!("pf-support-{}",std::process::id())).join("usage.json");let _=fs::remove_dir_all(path.parent().unwrap());let legacy=json!({"activeSeconds":REQUIRED,"activeDays":["2026-10-13","2026-10-14","2026-10-15"]});let mut u=Usage::open(path.clone(),Some(legacy)).unwrap();u.at(event("input",true,false),0.0,"2026-10-16").unwrap();assert_eq!(u.at(event("sample",true,true),15.0,"2026-10-16").unwrap()["showInvitation"],false);assert_eq!(u.at(event("sample",true,true),30.0,"2026-10-16").unwrap()["showInvitation"],true);let mut again=Usage::open(path.clone(),None).unwrap();assert_eq!(again.at(event("sample",true,true),45.0,"2026-10-16").unwrap()["showInvitation"],false);assert!(again.saved.automatic_prompt_handled);let _=fs::remove_dir_all(path.parent().unwrap());}
 #[test]fn legacy_handled_and_bounds_survive_handover(){let s:Saved=serde_json::from_value(json!({"activeSeconds":1e12,"activeDays":["bad","2026-10-13","2026-10-13","2026-10-14","2026-10-15","2026-10-16"],"automaticPromptHandled":true})).unwrap();let s=s.clean();assert_eq!(s.active_seconds,REQUIRED);assert_eq!(s.active_days.len(),3);assert!(s.automatic_prompt_handled);}
}
