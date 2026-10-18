//! Bounded references to completed native exports. Browser input never contains
//! a path; asynchronous acceptance is not proof of a completed backup.
use serde::Deserialize;
use serde_json::{Value,json};
use std::path::{Path,PathBuf};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {kind:String,id:String}
struct Receipt {kind:String,id:String,path:PathBuf,generation:u64,identity:String}
#[derive(Default)]
pub struct Results {receipts:Vec<Receipt>}
impl Results {
    fn retain(&mut self,generation:u64){self.receipts.retain(|r|r.generation==generation);}
    fn remember(&mut self,kind:&str,path:&str,generation:u64,identity:String){
        self.retain(generation);
        if !Path::new(path).is_absolute()||path.len()>16384||path.contains('\0')||Path::new(path).file_name().is_none(){return;}
        if self.receipts.iter().any(|r|r.kind==kind&&r.identity==identity){return;}
        self.receipts.retain(|r|r.kind!=kind);
        let mut bytes=[0u8;32];if getrandom::fill(&mut bytes).is_err(){return;}
        let id=bytes.iter().map(|b|format!("{b:02x}")).collect();
        self.receipts.push(Receipt{kind:kind.into(),id,path:path.into(),generation,identity});
    }
    pub fn observe(&mut self,snapshot:&Value,generation:u64){
        self.retain(generation);
        for(kind,key)in [("library","storageJob"),("source","sourceTransfer")]{
            let job=&snapshot[key];
            if job["running"]==false&&job["phase"]=="complete"&&job["kind"]=="export" {
                if let Some(path)=job["path"].as_str(){self.remember(kind,path,generation,json!([snapshot["workerGeneration"],job]).to_string());}
            }
        }
    }
    /// A source disconnect is completed by the companion's owned restart task;
    /// the replacement worker no longer has its former transient status record.
    pub fn completed_source(&mut self,result:&Value,generation:u64){
        if result["restarted"]!=true{return;}
        if let Some(path)=result["archive"].as_str(){self.remember("source",path,generation,json!(["completed-source-disconnect",result["sourceId"],path]).to_string());}
    }
    pub fn presentation(&mut self,generation:u64)->Value{
        self.retain(generation);
        json!(self.receipts.iter().map(|r|json!({"kind":r.kind,"id":r.id,"label":r.path.file_name().and_then(|n|n.to_str()).unwrap_or("Saved export")})).collect::<Vec<_>>())
    }
    pub fn resolve(&mut self,request:Value,generation:u64)->Result<PathBuf,String>{
        let request:Request=serde_json::from_value(request).map_err(|_|"Invalid completed export reference.")?;
        if !["library","source"].contains(&request.kind.as_str())||request.id.len()!=64||!request.id.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Invalid completed export reference.".into());}
        self.retain(generation);
        self.receipts.iter().find(|r|r.kind==request.kind&&r.id==request.id).map(|r|r.path.clone()).ok_or("That completed export is no longer current. Refresh Backups & archives.".into())
    }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn only_native_complete_exports_get_path_free_references(){let mut results=Results::default();let mut snapshot=json!({"workerGeneration":1,"storageJob":{"kind":"export","running":true,"phase":"preparing","path":"/private/My.photofindrybackup"}});
 results.observe(&snapshot,0);assert_eq!(results.presentation(0),json!([]));snapshot["storageJob"]["running"]=json!(false);snapshot["storageJob"]["phase"]=json!("error");results.observe(&snapshot,0);assert_eq!(results.presentation(0),json!([]));
 snapshot["storageJob"]["phase"]=json!("complete");results.observe(&snapshot,0);let reference=results.presentation(0)[0].clone();assert!(!reference.to_string().contains("/private"));let request=json!({"kind":reference["kind"],"id":reference["id"]});assert_eq!(results.resolve(request.clone(),0).unwrap(),PathBuf::from("/private/My.photofindrybackup"));
 results.observe(&snapshot,0);assert_eq!(results.presentation(0)[0]["id"],reference["id"]);assert!(results.resolve(json!({"kind":"library","id":reference["id"],"path":"/etc/passwd"}),0).is_err());assert!(results.resolve(request,1).is_err());}
 #[test]fn completed_disconnect_survives_worker_restart_and_new_exports_retire_old_references(){let mut results=Results::default();let final_result=json!({"sourceId":"source","archive":"/private/source.photofindrysource.zip","restartRequired":true});results.completed_source(&final_result,0);assert_eq!(results.presentation(0),json!([]));let mut final_result=final_result;final_result["restarted"]=json!(true);results.completed_source(&final_result,0);let first=results.presentation(0)[0].clone();results.observe(&json!({"workerGeneration":2,"sourceTransfer":{"kind":"idle","phase":"idle","running":false}}),0);assert_eq!(results.presentation(0)[0]["id"],first["id"]);
 results.observe(&json!({"workerGeneration":2,"sourceTransfer":{"kind":"export","phase":"complete","running":false,"path":"/private/new.zip"}}),0);assert!(results.resolve(json!({"kind":"source","id":first["id"]}),0).is_err());assert_eq!(results.presentation(0).as_array().unwrap().len(),1);}
}
