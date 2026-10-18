//! Content-free diagnostics: allowlisted state names and integer counters only.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::{Mutex,OnceLock}, time::Instant};
const STATES:&[&str]=&["idle","running","queued","paused","pausing","completed","failed","cancelled","starting","ready","stopped","error","processing","generating","other"];
const PHASES:&[&str]=&["idle","retrieving","checkingSearchModel","preparingCandidates","preview","refining","complete","stopped","error","paused","queued","tagging","indexing","screening","checking originals","checking model performance","processing","generating","other"];
const KINDS:&[&str]=&["aiSearch","tagging","improve","reviewScreen","semantic","faces","metadata","previews","discovery","sourceRecovery","libraryCleanup","other","queue","runtime","worker","model"];
const METRICS:&[&str]=&["running","pending","completed","total","failed","generatedTokens","requests","appRSSMiB","modelRSSMiB"];
#[derive(Clone,PartialEq,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Event { seconds_ago:u32, component:String,state:String,phase:String,metrics:BTreeMap<String,u64> }
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {schema:u32,consent:bool,events:Vec<Event>}
struct Entry{time:f64,event:Event}
struct Log{start:Instant,entries:Vec<Entry>}
static LOG:OnceLock<Mutex<Log>>=OnceLock::new();
fn allowed(value:&Value,choices:&[&str])->String{value.as_str().filter(|s|choices.contains(s)).unwrap_or("other").into()}
fn metric(value:&Value)->u64{value.as_u64().unwrap_or(0).min(1_000_000_000)}
impl Log{
 fn append(&mut self,now:f64,component:&str,state:&Value,phase:&Value,metrics:BTreeMap<String,u64>){
  self.entries.retain(|entry|now-entry.time<=1800.0);
  let event=Event{seconds_ago:0,component:if KINDS.contains(&component){component.into()}else{"other".into()},state:allowed(state,STATES),phase:allowed(phase,PHASES),metrics};
  if self.entries.iter().rev().find(|e|e.event.component==event.component).is_some_and(|last|last.event.state==event.state && last.event.phase==event.phase && (last.event==event || now-last.time<5.0)){return;}
  self.entries.push(Entry{time:now,event});if self.entries.len()>200{self.entries.remove(0);}
 }
 fn snapshot(&self,now:f64)->Value{json!({"schema":1,"consent":true,"events":self.entries.iter().filter(|e|now-e.time<=1800.0).map(|entry|{let mut event=entry.event.clone();event.seconds_ago=(now-entry.time).clamp(0.0,1800.0)as u32;event}).collect::<Vec<_>>()})}
}
pub fn observe(snapshot:&Value){
 let mut log=LOG.get_or_init(||Mutex::new(Log{start:Instant::now(),entries:vec![]})).lock().unwrap();let now=log.start.elapsed().as_secs_f64();
 if let Some(jobs)=snapshot["jobs"]["jobs"].as_array(){
  let running=jobs.iter().filter(|j|j["state"]=="running").count()as u64;
  let pending=jobs.iter().filter(|j|!["completed","failed","cancelled"].contains(&j["state"].as_str().unwrap_or(""))).count()as u64;
  log.append(now,"queue",&json!(if snapshot["jobs"]["paused"]==true{"paused"}else{"running"}),&json!("idle"),BTreeMap::from([("running".into(),running), ("pending".into(),pending)]));
  for job in jobs.iter().take(200){let metrics=["completed","total","failed"].iter().map(|key|((*key).into(),metric(&job[*key]))).collect();log.append(now,job["kind"].as_str().unwrap_or("other"),&job["state"],job.get("phase").unwrap_or(&Value::Null),metrics);}
 }
 if snapshot["runtime"].is_object(){log.append(now,"runtime",&snapshot["runtime"]["status"],&json!("idle"),BTreeMap::new());}
 let model=if snapshot["monitor"].is_object(){&snapshot["monitor"]}else if snapshot["metrics"].is_object(){&snapshot["metrics"]}else if snapshot["modelMetrics"].is_object(){&snapshot["modelMetrics"]}else{snapshot};
 if model["generatedTokens"].is_number()||model["requests"].is_number(){let mut values=BTreeMap::from([("generatedTokens".into(),metric(&model["generatedTokens"])),("requests".into(),metric(&model["requests"]))]);
  for (key,field) in [("appRSSMiB","appRSSBytes"),("modelRSSMiB","modelRSSBytes")]{if let Some(bytes)=model[field].as_f64().filter(|v|v.is_finite()&&*v>=0.0){values.insert(key.into(),(bytes/1048576.0).min(1_000_000_000.0)as u64);}}
  log.append(now,"model",&model["phase"],&model["phase"],values);
 }
}
pub fn snapshot()->Value{let log=LOG.get_or_init(||Mutex::new(Log{start:Instant::now(),entries:vec![]})).lock().unwrap();log.snapshot(log.start.elapsed().as_secs_f64())}
pub fn validate(value:&Value)->Result<Value,String>{let parsed:Snapshot=serde_json::from_value(value.clone()).map_err(|_|"Invalid diagnostic attachment.")?;
 if parsed.schema!=1||!parsed.consent||parsed.events.len()>200||parsed.events.iter().any(|e|e.seconds_ago>1800||!KINDS.contains(&e.component.as_str())||!STATES.contains(&e.state.as_str())||!PHASES.contains(&e.phase.as_str())||e.metrics.len()>METRICS.len()||e.metrics.iter().any(|(key,value)|!METRICS.contains(&key.as_str())||*value>1_000_000_000)){return Err("Invalid diagnostic attachment.".into());}
 serde_json::to_value(parsed).map_err(|_|"Invalid diagnostic attachment.".into())}
#[cfg(test)]mod tests{use super::*;
 #[test]fn snapshots_are_bounded_expire_and_exclude_strings(){let mut log=Log{start:Instant::now(),entries:vec![]};for n in 0..250{log.append(n as f64*6.0,"tagging",&json!("/private/user.jpg"),&json!("secret query"),BTreeMap::from([("completed".into(),n)]));}let value=log.snapshot(1500.0);assert_eq!(value["events"].as_array().unwrap().len(),200);assert!(!value.to_string().contains("private"));assert!(validate(&value).is_ok());assert_eq!(log.snapshot(4000.0)["events"],json!([]));}
 #[test]fn worker_monitor_shape_keeps_token_and_memory_counters_without_model_names(){
  observe(&json!({"monitor":{"phase":"generating","generatedTokens":123,"requests":4,"appRSSBytes":2097152,"modelRSSBytes":3145728,"modelName":"private-weight.gguf","rawText":"private-generated-text"}}));
  let data=snapshot();let rows=data["events"].as_array().unwrap();let model=rows.iter().find(|e|e["component"]=="model").unwrap();
  assert_eq!(model["metrics"]["generatedTokens"],123);assert_eq!(model["metrics"]["modelRSSMiB"],3);assert!(!data.to_string().contains("private-"));
 }
 #[test]fn malformed_or_content_bearing_diagnostics_are_rejected(){let good=json!({"schema":1,"consent":true,"events":[{"secondsAgo":1,"component":"model","state":"generating","phase":"generating","metrics":{"requests":2}}]});assert!(validate(&good).is_ok());for (field,value) in [("message",json!("secret")),("component",json!("private-model.gguf")),("metrics",json!({"query":10})),("secondsAgo",json!(1801))]{let mut bad=good.clone();bad["events"][0][field]=value;assert!(validate(&bad).is_err());}}
}
