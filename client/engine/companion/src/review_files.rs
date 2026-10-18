//! Read-only reviewer preflight. Exact SHA-256/runtime verification remains in
//! the owned inference runner; this bounded check never loads a model.
use serde_json::{Value, json};

const GIB: u64 = 1024 * 1024 * 1024;
fn row(role: &str, status: &str, message: &str) -> Value { json!({"role":role,"status":status,"message":message}) }
pub fn physical_memory() -> Option<u64> {
    let mut bytes = 0u64;
    let mut length = std::mem::size_of::<u64>();
    let status = unsafe { libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut bytes as *mut u64).cast(), &mut length, std::ptr::null_mut(), 0) };
    (status == 0 && length == std::mem::size_of::<u64>() && bytes > 0).then_some(bytes)
}
pub fn check(settings: &Value, physical: Option<u64>) -> Value {
    let provider = settings["provider"].as_str().unwrap_or("tagging-model");
    if provider == "tagging-model" {
        return json!({"canSave":true,"files":[],"summary":"Uses your current tagging model for photo detail review. No separate reviewer files are required and no model was started.","physicalMemoryBytes":physical,"minimumMemoryBytes":0});
    }
    let intern = provider == "intern-decision-4b";
    if !intern && provider != "clef-flash" {
        return json!({"canSave":false,"files":[],"summary":"Choose a supported photo detail reviewer.","physicalMemoryBytes":physical,"minimumMemoryBytes":0});
    }
    let minimum = if intern { if settings["lowMemoryTrial"] == true {8*GIB} else {16*GIB} } else {24*GIB};
    // Matches worker::improvement_native::validate_review_files. These are exact
    // pinned package sizes, not filename guesses or a claim of complete integrity.
    let expected = if intern {[2_708_804_416u64,672_423_328]} else {[10_014_561_856u64,918_165_984]};
    let generic = super::model_files::check(settings);
    let mut files = Vec::new();
    for (index,(role,key)) in [("model","modelPath"),("projector","projectorPath")].into_iter().enumerate() {
        let path = settings[key].as_str().unwrap_or("");
        let value = if path.is_empty() { row(role,"error","Choose the reviewer file and its matching vision adapter.") }
        else { match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() && metadata.len() == expected[index] => {
                let inspected = &generic["files"][index];
                if inspected["status"] == "error" { inspected.clone() }
                else if inspected["status"] == "warning" { row(role,"warning",&format!("File size matches the selected reviewer package. {} The full pinned checksum is verified before inference.",inspected["message"].as_str().unwrap_or("Header compatibility is unconfirmed."))) }
                else { row(role,"ok","Header and file size match the selected reviewer package. The full pinned checksum is verified before inference.") }
            },
            Ok(_) => row(role,"error",if intern {"This is not the expected Intern-Decision 4B Q4 model / F16 vision-adapter pair. Choose its matching files."}else{"This is not the expected CLEF Flash 9B Q8 model / vision-adapter pair. For Intern files, select Intern-Decision 4B."}),
            Err(_) => row(role,"error","This selected reviewer file is unavailable. Reconnect its drive or choose its current location."),
        }};
        files.push(value);
    }
    let memory_ok = physical.is_some_and(|bytes| bytes >= minimum);
    let summary = if physical.is_none() { "Memory availability could not be measured. Retry the local check before saving this reviewer." }
        else if !memory_ok && intern && settings["lowMemoryTrial"] != true { "The normal Intern profile requires 16 GB. Reduced-memory mode permits 8 GB with smaller previews and question batches; actual 8 GB operation has not yet been qualified." }
        else if !memory_ok && intern { "Intern-Decision requires at least 8 GB of Mac memory." }
        else if !memory_ok { "CLEF Flash Q8 requires at least 24 GB of Mac memory." }
        else if intern && settings["lowMemoryTrial"] == true { "Quick local header, size and memory checks only. Reduced-memory mode uses smaller previews; actual 8 GB operation has not yet been qualified. Full checksums are verified before inference. No model was started." }
        else { "Quick local header, size and memory checks only. Full pinned checksums are verified before inference. No model was started." };
    json!({"canSave":memory_ok && !files.iter().any(|file|file["status"]=="error"),"files":files,"summary":summary,"physicalMemoryBytes":physical,"minimumMemoryBytes":minimum})
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn default_reviewer_requires_no_additional_files() {
        let result=check(&json!({"provider":"tagging-model"}),Some(8*GIB));
        assert_eq!(result["canSave"],true); assert_eq!(result["files"],json!([]));
    }
    #[test] fn reviewer_file_and_hardware_failures_are_explicit() {
        let result=check(&json!({"provider":"clef-flash","modelPath":"/missing/model.gguf"}),Some(16*GIB));
        assert_eq!(result["canSave"],false);assert_eq!(result["minimumMemoryBytes"],24*GIB);
        assert_eq!(result["files"][0]["status"],"error");assert!(result["summary"].as_str().unwrap().contains("24 GB"));
        let normal=check(&json!({"provider":"intern-decision-4b"}),Some(8*GIB));
        assert_eq!(normal["minimumMemoryBytes"],16*GIB);
        let reduced=check(&json!({"provider":"intern-decision-4b","lowMemoryTrial":true}),Some(8*GIB));
        assert_eq!(reduced["minimumMemoryBytes"],8*GIB);assert_eq!(reduced["canSave"],false,"Absent files still prevent saving");
        assert!(reduced["summary"].as_str().unwrap().contains("8 GB operation has not yet been qualified"));
    }
}
