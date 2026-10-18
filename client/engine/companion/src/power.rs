//! Only an owned idle-system-sleep assertion; never display/lid/manual sleep.
use serde_json::{Value,json};
use std::{fs::{self,OpenOptions},io::{Read,Write},os::unix::fs::OpenOptionsExt,path::{Path,PathBuf},process::{Child,Command,Stdio}};

pub struct Power { mode:String, child:Option<Child>, file:PathBuf, error:Option<String> }
fn needed(mode:&str,queue:&Value)->bool {
    let jobs=queue["jobs"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let running=jobs.iter().any(|j|matches!(j["state"].as_str(),Some("running"|"pausing")));
    let waiting=jobs.iter().any(|j|matches!(j["state"].as_str(),Some("queued"|"scheduled")));
    match mode {"processing"=>running,"queue"=>running||(waiting&&queue["paused"]!=true),_=>false}
}
impl Power {
    pub fn new(data:&Path)->Self {
        let file=data.join("companion-power.json");
        let mut bytes=Vec::new();
        if let Ok(open)=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(&file){
            if open.metadata().is_ok_and(|m|m.is_file()&&m.len()<=1024){let _=open.take(1025).read_to_end(&mut bytes);}
        }
        let saved=serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
        let mode=saved["mode"].as_str().filter(|s|["off","processing","queue"].contains(s)).unwrap_or("processing").to_owned();
        Self {mode,child:None,file,error:None}
    }
    pub fn configure(&mut self,mode:&str)->Result<Value,String>{
        if !["off","processing","queue"].contains(&mode){return Err("Choose a valid keep-awake policy.".into());}
        let mut random=[0u8;8];getrandom::fill(&mut random).map_err(|_|"The system random source is unavailable.")?;
        let suffix:String=random.iter().map(|b|format!("{b:02x}")).collect();
        let stage=self.file.with_file_name(format!(".companion-power-{suffix}.json"));
        let bytes=serde_json::to_vec(&json!({"version":1,"mode":mode})).unwrap();
        let result=(||->std::io::Result<()>{
            let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&stage)?;
            f.write_all(&bytes)?;f.sync_all()?;fs::rename(&stage,&self.file)?;Ok(())
        })();
        if result.is_err(){let _=fs::remove_file(&stage);return Err("Could not save the local keep-awake preference.".into());}
        self.mode=mode.into();self.error=None;if mode=="off"{self.stop();}Ok(self.state())
    }
    pub fn update(&mut self,queue:&Value){
        if self.child.as_mut().is_some_and(|child|!matches!(child.try_wait(),Ok(None))){self.child=None;}
        if !needed(&self.mode,queue){self.stop();return;}
        if self.child.is_none(){
            // -w also releases the assertion if the CLI is killed before Drop runs.
            match Command::new("/usr/bin/caffeinate").args(["-i","-w"]).arg(std::process::id().to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn(){
                Ok(child)=>{self.child=Some(child);self.error=None;},
                Err(_)=>self.error=Some("Keep-awake is unavailable; system sleep preferences remain in effect.".into()),
            }
        }
    }
    pub fn state(&self)->Value{json!({"mode":self.mode,"active":self.child.is_some(),"error":self.error})}
    pub fn stop(&mut self){if let Some(mut child)=self.child.take(){let _=child.kill();let _=child.wait();}}
}
impl Drop for Power {fn drop(&mut self){self.stop();}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn policy_preserves_pause_and_individual_holds(){
        assert!(!needed("processing",&json!({"paused":false,"jobs":[{"state":"scheduled"}]})));
        assert!(needed("queue",&json!({"paused":false,"jobs":[{"state":"scheduled"}]})));
        assert!(!needed("queue",&json!({"paused":true,"jobs":[{"state":"scheduled"}]})));
        assert!(needed("processing",&json!({"paused":true,"jobs":[{"state":"pausing"}]})));
        assert!(!needed("queue",&json!({"paused":false,"jobs":[{"state":"paused"}]})));
        assert!(!needed("off",&json!({"jobs":[{"state":"running"}]})));
    }
}
