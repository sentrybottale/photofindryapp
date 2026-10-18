//! Bounded recipe-only native font rendering; never scheduled on animation ticks.
use serde_json::{Value,json};
use std::{collections::VecDeque,path::{Path,PathBuf},process::Stdio,time::Duration};
use tokio::{io::{AsyncReadExt,AsyncWriteExt},process::Command,sync::Mutex};

pub struct Glyphs { executable:PathBuf,catalog:PathBuf,cache:Mutex<VecDeque<(String,Value)>> }
impl Glyphs {
    pub fn new(runtime:&Path)->Self {
        let sibling=std::env::current_exe().ok().and_then(|p|p.parent().map(|v|v.join("CompanionGlyphs")));
        Self {executable:sibling.filter(|p|p.is_file()).unwrap_or_else(||runtime.join("CompanionGlyphs")),catalog:runtime.join("emoji/emoji-catalog.json"),cache:Mutex::new(VecDeque::new())}
    }
    pub async fn render(&self,art:&Value)->Result<Value,String>{
        let neutral=||json!({"side":16,"beats":[[],[],[]]});
        if art["version"]!=4{return Ok(neutral());}
        let subject=art["subject"]["id"].as_str().unwrap_or("");
        let effect=art["effect"]["id"].as_str().filter(|v|!v.is_empty());
        let setting=art["setting"]["id"].as_str().filter(|v|!v.is_empty());
        if subject.is_empty(){return Ok(neutral());}
        let ids=[subject,effect.or(setting).unwrap_or(subject),setting.unwrap_or(subject)];
        if ids.iter().any(|id|id.len()>96||!id.bytes().all(|c|c.is_ascii_hexdigit()||c==b'-')){return Ok(neutral());}
        let input=json!({"ids":ids});let key=input.to_string();
        let mut cache=self.cache.lock().await;
        if let Some(at)=cache.iter().position(|(old,_)|old==&key){let item=cache.remove(at).unwrap();let result=item.1.clone();cache.push_back(item);return Ok(result);}
        let mut child=Command::new(&self.executable).arg(&self.catalog).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The native story renderer is unavailable.")?;
        let mut stdin=child.stdin.take().ok_or("Story renderer input is unavailable.")?;
        let stdout=child.stdout.take().ok_or("Story renderer output is unavailable.")?;
        let work=async {
            stdin.write_all(format!("{key}\n").as_bytes()).await.map_err(|_|"Story renderer disconnected.")?;
            stdin.shutdown().await.map_err(|_|"Story renderer disconnected.")?;
            let mut bytes=Vec::new();stdout.take(100001).read_to_end(&mut bytes).await.map_err(|_|"Story renderer disconnected.")?;
            if bytes.len()>100000{return Err("Story renderer response exceeded its bound.");}
            if !child.wait().await.map_err(|_|"Story renderer stopped.")?.success(){return Err("Story renderer failed.");}
            let mut result:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid story cells.")?;
            let beats=result["beats"].as_array().filter(|beats|beats.len()==3).ok_or("Invalid story beats.")?;
            if result["side"]!=16||beats.iter().any(|beat|!beat.as_array().is_some_and(|cells|cells.len()<=256&&cells.iter().all(|cell|cell["column"].as_u64().is_some_and(|n|n<16)&&cell["row"].as_u64().is_some_and(|n|n<16)&&["red","green","blue"].iter().all(|key|cell[*key].as_u64().is_some_and(|n|n<=255))))){return Err("Invalid story cells.");}
            // Match native story behavior: unsupported effect/setting repeats the subject.
            if result["beats"][0].as_array().is_some_and(|a|a.is_empty()){return Ok(neutral());}
            if result["beats"][2].as_array().is_some_and(|a|a.is_empty()){result["beats"][2]=result["beats"][0].clone();}
            if result["beats"][1].as_array().is_some_and(|a|a.is_empty()){result["beats"][1]=result["beats"][2].clone();}
            Ok(result)
        };
        let result=match tokio::time::timeout(Duration::from_secs(10),work).await {Ok(value)=>value.map_err(str::to_owned)?,Err(_)=>{let _=child.kill().await;return Err("The native story renderer timed out.".into());}};
        if cache.len()>=32{cache.pop_front();}cache.push_back((key,result.clone()));Ok(result)
    }
}
