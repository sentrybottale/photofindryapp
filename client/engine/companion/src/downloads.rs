//! Explicit native pinned-model downloads, with no website-selected URL or code.
use super::native_picker::Picker;
use serde_json::{Value,json};
use std::{path::{Path,PathBuf},process::Stdio,sync::{Arc,Mutex},time::Duration};
use tokio::{io::{AsyncBufReadExt,AsyncWriteExt,BufReader},process::Command,sync::mpsc};
pub struct Downloads { executable:PathBuf,runtime:PathBuf,state:Arc<Mutex<Value>>,control:Arc<Mutex<Option<mpsc::Sender<()>>>> }
impl Downloads {
    pub fn new(runtime:&Path)->Self{
        let sibling=std::env::current_exe().ok().and_then(|p|p.parent().map(|v|v.join("CompanionDownloads")));
        let executable=sibling.filter(|p|p.is_file()).unwrap_or_else(||runtime.join("CompanionDownloads"));
        let bundle=executable.parent().unwrap_or(runtime).join("CompanionDownloads.app/Contents/MacOS/CompanionDownloads");
        Self{executable:if bundle.is_file(){bundle}else{executable},runtime:runtime.into(),state:Arc::new(Mutex::new(json!({"running":false}))),control:Arc::new(Mutex::new(None))}
    }
    pub fn snapshot(&self,picker:&Picker)->Value{
        let mut saved=self.state.lock().unwrap();
        for(field,kind,out)in [("rerankerPath","reranker","rerankerGrant"),("modelPath","model","modelGrant"),("projectorPath","projector","projectorGrant")]{
            if let Some(path)=saved.as_object_mut().and_then(|m|m.remove(field)).and_then(|p|p.as_str().map(str::to_owned)){
                if let Ok(grant)=picker.grant_path(kind,&path){saved[out]=grant["grantId"].clone();saved[format!("{out}Label")]=grant["label"].clone();}
                else{saved["message"]=json!("Download completed. Choose the downloaded files to use them.");}
            }
        }
        saved.clone()
    }
    pub async fn start(&self,kind:&str)->Result<Value,String>{
        if !["reranker","clef"].contains(&kind){return Err("Choose a supported pinned model download.".into());}
        let mut control=self.control.lock().unwrap();
        if control.is_some(){return Err("Finish or cancel the current download first.".into());}
        let mut child=Command::new(&self.executable).arg(kind).arg(&self.runtime).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The native download helper is unavailable. Rebuild companion tools.")?;
        let mut stdin=child.stdin.take().ok_or("The native downloader could not start.")?;
        let stdout=child.stdout.take().ok_or("The native downloader could not start.")?;
        let(sender,mut receiver)=mpsc::channel(1);*control=Some(sender);
        let state=self.state.clone();let controls=self.control.clone();
        *state.lock().unwrap()=json!({"kind":kind,"running":true,"progress":0,"message":"Choose a destination in the native file chooser."});
        tokio::spawn(async move{
            let mut reader=BufReader::new(stdout);let mut bytes=Vec::new();
            loop{
                bytes.clear();
                let read=tokio::select!{
                    _=receiver.recv()=>{let _=stdin.write_all(b"cancel\n").await;let _=stdin.shutdown().await;if tokio::time::timeout(Duration::from_secs(5),child.wait()).await.is_err(){let _=child.kill().await;}state.lock().unwrap()["running"]=json!(false);state.lock().unwrap()["message"]=json!("Download cancelled. Selected models are unchanged.");break;},
                    value=async{loop{let buf=reader.fill_buf().await?;if buf.is_empty(){return Ok::<usize,std::io::Error>(0);}let n=buf.iter().position(|b|*b==b'\n').map(|i|i+1).unwrap_or(buf.len());if bytes.len()+n>32768{return Err(std::io::Error::other("oversized"));}let done=buf[n-1]==b'\n';bytes.extend_from_slice(&buf[..n]);reader.consume(n);if done{return Ok(bytes.len());}}}=>value,
                };
                match read{Ok(0)=>break,Ok(_)=>{
                    let Ok(value)=serde_json::from_slice::<Value>(&bytes)else{break;};
                    if !value["running"].is_boolean()||!value["progress"].as_f64().is_some_and(|n|n.is_finite()&&(0.0..=1.0).contains(&n)){break;}
                    *state.lock().unwrap()=value;
                },Err(_)=>break}
            }
            if tokio::time::timeout(Duration::from_secs(2),child.wait()).await.is_err(){let _=child.kill().await;}
            let mut saved=state.lock().unwrap();if saved["running"]==true{saved["running"]=json!(false);saved["message"]=json!("The download helper stopped. Retry or choose existing local files.");}drop(saved);
            *controls.lock().unwrap()=None;
        });
        Ok(self.state.lock().unwrap().clone())
    }
    pub fn cancel(&self){if let Some(control)=self.control.lock().unwrap().as_ref(){let _=control.try_send(());}}
}
impl Drop for Downloads{fn drop(&mut self){self.cancel();}}
