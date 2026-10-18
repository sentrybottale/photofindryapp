use serde_json::{Value,json};
use std::{path::{Path,PathBuf},process::Stdio,time::Duration};
use tokio::{io::{AsyncReadExt,AsyncWriteExt},process::Command,sync::Mutex};
pub struct Labels{executable:PathBuf,active:Mutex<()>}
impl Labels{
 pub fn new(runtime:&Path)->Self{let sibling=std::env::current_exe().ok().and_then(|p|p.parent().map(|v|v.join("CompanionLabels")));let executable=sibling.filter(|p|p.is_file()).unwrap_or_else(||runtime.join("CompanionLabels"));let bundle=executable.parent().unwrap_or(runtime).join("CompanionLabels.app/Contents/MacOS/CompanionLabels");Self{executable:if bundle.is_file(){bundle}else{executable},active:Mutex::new(())}}
 pub async fn label(&self,id:&str,label:&str,operation:&str)->Result<Value,String>{if !["show","print","savePDF"].contains(&operation)||id.len()>128||label.len()>1024{return Err("Invalid archive label request.".into());}self.run(json!({"operation":operation,"sourceId":id,"label":label})).await}
 pub async fn scan(&self,mode:&str)->Result<Value,String>{let operation=match mode{"image"=>"scanImage","camera"=>"scanCamera",_=>return Err("Choose QR image or camera scanning.".into())};self.run(json!({"operation":operation})).await}
 async fn run(&self,input:Value)->Result<Value,String>{
  let _active=self.active.try_lock().map_err(|_|"Finish the open archive-label dialog first.")?;
  let mut child=Command::new(&self.executable).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The native archive-label helper is unavailable.")?;
  let mut stdin=child.stdin.take().ok_or("The archive-label helper could not start.")?;let stdout=child.stdout.take().ok_or("The archive-label helper could not start.")?;
  let task=async{
   stdin.write_all(format!("{input}\n").as_bytes()).await.map_err(|_|"Archive-label helper disconnected.")?;stdin.shutdown().await.map_err(|_|"Archive-label helper disconnected.")?;
   let mut bytes=Vec::new();stdout.take(400001).read_to_end(&mut bytes).await.map_err(|_|"Archive-label helper disconnected.")?;
   if bytes.len()>400000{return Err("Archive-label response exceeded its bound.");}
   let status=child.wait().await.map_err(|_|"Archive-label helper stopped.")?;
   let value:Value=serde_json::from_slice(&bytes).map_err(|_|"The archive-label response was invalid.")?;
   if !status.success(){return Err("The label could not be read or created. Choose a valid PhotoFindry QR image, or enter the archive ID.");}
   Ok(value)
  };
  match tokio::time::timeout(Duration::from_secs(300),task).await{Ok(value)=>value.map_err(str::to_owned),Err(_)=>{let _=child.kill().await;Err("The archive-label dialog timed out.".into())}}
 }
}
