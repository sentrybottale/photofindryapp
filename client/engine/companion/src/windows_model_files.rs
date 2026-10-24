//! Bounded, read-only file guidance. Never starts inference or renders template text.
//! GGUF layout: https://github.com/ggml-org/ggml/blob/master/docs/gguf.md
use photofindry_platform::fs::{self as native_fs, File};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufReader, Read},
    time::{Duration, Instant},
};
const MAX_METADATA: u64 = 32 * 1024 * 1024;
const MAX_TEMPLATE: u64 = 256 * 1024;
#[derive(Debug)]
enum Problem {
    Invalid,
    Limit,
}
type Result<T> = std::result::Result<T, Problem>;
struct Reader {
    file: BufReader<File>,
    position: u64,
    length: u64,
    steps: u64,
    started: Instant,
}
impl Reader {
    fn advance(&mut self, count: u64) -> Result<()> {
        self.steps += 1;
        if self.steps > 1_000_000 || self.started.elapsed() > Duration::from_secs(1) {
            return Err(Problem::Limit);
        }
        let next = self.position.checked_add(count).ok_or(Problem::Invalid)?;
        if next > self.length {
            return Err(Problem::Invalid);
        }
        if next > MAX_METADATA {
            return Err(Problem::Limit);
        }
        self.position = next;
        Ok(())
    }
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.advance(N as u64)?;
        let mut bytes = [0; N];
        self.file
            .read_exact(&mut bytes)
            .map_err(|_| Problem::Invalid)?;
        Ok(bytes)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn skip(&mut self, bytes: u64) -> Result<()> {
        self.advance(bytes)?;
        self.file
            .seek_relative(bytes as i64)
            .map_err(|_| Problem::Invalid)
    }
    fn string(&mut self, capture: bool) -> Result<String> {
        let length = self.u64()?;
        if !capture {
            self.skip(length)?;
            return Ok(String::new());
        }
        if length > 65535 {
            return Err(Problem::Limit);
        }
        self.advance(length)?;
        let mut bytes = vec![0; length as usize];
        self.file
            .read_exact(&mut bytes)
            .map_err(|_| Problem::Invalid)?;
        String::from_utf8(bytes).map_err(|_| Problem::Invalid)
    }
    fn value(&mut self, kind: u32, capture: bool, depth: u8) -> Result<Value> {
        let size = match kind {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4 | 5 | 6 => 4,
            10 | 11 | 12 => 8,
            _ => 0,
        };
        if kind == 8 {
            return Ok(json!(self.string(capture)?));
        }
        if kind == 4 && capture {
            return Ok(json!(self.u32()?));
        }
        if kind == 7 && capture {
            return match self.bytes::<1>()?[0] {
                0 => Ok(json!(false)),
                1 => Ok(json!(true)),
                _ => Err(Problem::Invalid),
            };
        }
        if size > 0 {
            self.skip(size)?;
            return Ok(Value::Null);
        }
        if kind != 9 {
            return Err(Problem::Invalid);
        }
        if depth >= 4 {
            return Err(Problem::Limit);
        }
        let child = self.u32()?;
        let count = self.u64()?;
        if count > 1_000_000 {
            return Err(Problem::Limit);
        }
        if capture && child == 8 && count <= 8 {
            let mut values = Vec::new();
            for _ in 0..count {
                values.push(self.string(true)?);
            }
            return Ok(json!(values));
        }
        let width = match child {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4 | 5 | 6 => 4,
            10 | 11 | 12 => 8,
            8 | 9 => 0,
            _ => return Err(Problem::Invalid),
        };
        if width > 0 {
            self.skip(count.checked_mul(width).ok_or(Problem::Invalid)?)?;
        } else {
            for _ in 0..count {
                self.value(child, false, depth + 1)?;
            }
        }
        Ok(Value::Null)
    }
}
struct Header {
    values: HashMap<String, Value>,
    complete: bool,
}
fn header(file: File, length: u64) -> Result<Header> {
    let mut reader = Reader {
        file: BufReader::with_capacity(65536, file),
        position: 0,
        length,
        steps: 0,
        started: Instant::now(),
    };
    if reader.bytes::<4>()? != *b"GGUF" {
        return Err(Problem::Invalid);
    }
    if !matches!(reader.u32()?, 2 | 3) {
        return Err(Problem::Limit);
    }
    if reader.u64()? == 0 {
        return Err(Problem::Invalid);
    }
    let count = reader.u64()?;
    if count > 4096 {
        return Err(Problem::Limit);
    }
    let mut header = Header {
        values: HashMap::new(),
        complete: true,
    };
    for _ in 0..count {
        let entry = (|| {
            let key = reader.string(true)?;
            let kind = reader.u32()?;
            let wanted = matches!(
                key.as_str(),
                "general.architecture"
                    | "clip.has_vision_encoder"
                    | "tokenizer.ggml.model"
                    | "qwen3.pooling_type"
                    | "qwen3.classifier.output_labels"
            );
            if matches!(
                key.as_str(),
                "tokenizer.chat_template" | "tokenizer.chat_template.rerank"
            ) {
                // Record only presence/non-emptiness; do not disclose/execute model prompt text.
                if kind == 8 {
                    let size = reader.u64()?;
                    reader.skip(size)?;
                    return Ok((key, json!(size > 0)));
                }
                reader.value(kind, false, 0)?;
                return Ok((key, Value::Null));
            }
            let value = reader.value(kind, wanted, 0)?;
            Ok((if wanted { key } else { String::new() }, value))
        })();
        match entry {
            Ok((key, value)) if !key.is_empty() => {
                if header.values.insert(key, value).is_some() {
                    return Err(Problem::Invalid);
                }
            }
            Ok(_) => {}
            Err(Problem::Limit) => {
                header.complete = false;
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(header)
}
fn result(role: &str, status: &str, message: &str) -> Value {
    json!({"role":role,"status":status,"message":message})
}
fn inspect(role: &str, path: &str) -> (Value, Option<(u64, [u8; 16])>) {
    if path.is_empty() {
        return (
            result(
                role,
                "empty",
                if role == "template" {
                    "No override selected. With model chat formatting enabled, the model's built-in format is used."
                } else if role == "reranker" {
                    "Optional for tagging; needed for Local AI Search."
                } else {
                    "Choose this file before tagging."
                },
            ),
            None,
        );
    }
    let inspected = (|| -> std::result::Result<_, String> {
        let file = match native_fs::open_read_no_reparse(std::path::Path::new(path)) {
            Ok(file) => file,
            Err(_) => {
                return Ok((
                    result(
                        role,
                        "warning",
                        "Not checked: this saved file is unavailable. Its path is kept; reconnect its drive or choose it again before using the model.",
                    ),
                    None,
                ));
            }
        };
        let metadata = file
            .metadata()
            .map_err(|_| "This file cannot be checked.")?;
        if !metadata.is_file() {
            return Err("Choose a regular local file.".into());
        }
        let identity = {
            let id =
                native_fs::identity(&file).map_err(|_| "The file identity cannot be checked.")?;
            Some((id.volume, id.file_id))
        };
        if role == "template" {
            if metadata.len() > MAX_TEMPLATE {
                return Err("A chat-template override is a small text file (.jinja or .txt), not model weights. Leave it empty to use the built-in format.".into());
            }
            let mut bytes = Vec::new();
            file.take(MAX_TEMPLATE + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "The template could not be read.")?;
            if bytes.starts_with(b"GGUF") || bytes.len() as u64 > MAX_TEMPLATE {
                return Err("This is a model-weight file. A chat-template override must be text; normally leave it empty.".into());
            }
            let text = std::str::from_utf8(&bytes).map_err(
                |_| "Choose a UTF-8 text template (.jinja or .txt), not a binary model file.",
            )?;
            if text.contains('\0') || text.trim().is_empty() {
                return Err("The template is empty or contains binary data. Choose the model author's text template or clear the override.".into());
            }
            if !text.contains("{{") && !text.contains("{%") {
                return Ok((
                    result(
                        role,
                        "warning",
                        "Text is readable but has no recognizable Jinja markers. Check the model author's instructions, or clear this optional override.",
                    ),
                    identity,
                ));
            }
            return Ok((
                result(
                    role,
                    "warning",
                    "Text template found. Syntax and model compatibility are checked by the local runtime when it loads; this quick check does not execute templates.",
                ),
                identity,
            ));
        }
        let parsed = match header(file,metadata.len()) {
            Ok(value)=>value,
            Err(Problem::Invalid)=>return Err("This is not a readable GGUF model header. Choose the actual .gguf weights, not a webpage, text template or incomplete download.".into()),
            Err(Problem::Limit)=>return Ok((result(role,"warning","The quick check cannot inspect this GGUF version or metadata within its limits. Compatibility is not confirmed; check the model author's instructions."),identity)),
        };
        let architecture = parsed
            .values
            .get("general.architecture")
            .and_then(Value::as_str)
            .unwrap_or("");
        let projector = architecture == "clip";
        if projector && role != "projector" {
            return Err("This file identifies as a vision adapter (mmproj). Choose it under Vision adapter, and choose the main model weights separately.".into());
        }
        if role == "projector" && !projector && parsed.values.contains_key("tokenizer.ggml.model") {
            return Err("This file identifies as language-model weights. Select the matching mmproj .gguf here instead.".into());
        }
        if projector && parsed.values.get("clip.has_vision_encoder") == Some(&json!(false)) {
            return Err("This adapter says it has no vision encoder. Choose the model's image/vision mmproj file.".into());
        }
        // The pinned Qwen converter emits these together for its ranking head.
        // Architecture/name alone also match ordinary language models; label order
        // matters because the runtime returns the first classification output.
        let rerank_format =
            parsed.values.get("tokenizer.chat_template.rerank") == Some(&json!(true));
        let qwen_reranker = parsed.complete
            && architecture == "qwen3"
            && rerank_format
            && parsed.values.get("qwen3.pooling_type") == Some(&json!(4))
            && parsed.values.get("qwen3.classifier.output_labels") == Some(&json!(["yes", "no"]));
        let message = if role == "projector" && projector {
            "Vision-adapter GGUF header identified. It still needs to match the selected main model."
        } else if role == "projector" {
            "GGUF header readable, but its vision-adapter role is unconfirmed. Check that this is the matching mmproj file."
        } else if role == "reranker" && qwen_reranker {
            "Qwen3 search reranker identified. It loads automatically for Local AI Search."
        } else if role == "reranker" && !parsed.complete {
            "GGUF header readable, but the quick check did not reach all reranking metadata. Use the recommended search reranker below, or check the model author's instructions."
        } else if role == "reranker" && architecture == "qwen3" {
            "Qwen3 GGUF found, but its expected reranking metadata is missing or different. Use the recommended search reranker below, or check the model author's instructions."
        } else if role == "reranker" && rerank_format {
            "Embedded reranking format found. This model's compatibility is checked when Local AI Search loads it."
        } else if role == "reranker" {
            "GGUF file readable, but a reranking model was not identified. Use the recommended search reranker below, or check the model author's instructions."
        } else if parsed.values.get("tokenizer.chat_template") == Some(&json!(true)) {
            "Model GGUF header readable; a built-in chat format is present. A separate template is normally unnecessary."
        } else if parsed.complete {
            "Model GGUF header readable. No single built-in chat format was identified; consult the model author's instructions."
        } else {
            "GGUF header readable. The bounded check did not reach all metadata; built-in chat format and compatibility remain unconfirmed."
        };
        Ok((
            result(
                role,
                if role == "projector" && projector
                    || role == "model"
                        && parsed.values.get("tokenizer.chat_template") == Some(&json!(true))
                    || role == "reranker" && qwen_reranker
                {
                    "ok"
                } else {
                    "warning"
                },
                message,
            ),
            identity,
        ))
    })();
    inspected.unwrap_or_else(|message| (result(role, "error", &message), None))
}
pub fn check(settings: &Value) -> Value {
    let mut files = Vec::new();
    let mut identities = Vec::new();
    for (role, key) in [
        ("model", "modelPath"),
        ("projector", "projectorPath"),
        ("reranker", "rerankerModelPath"),
        ("template", "templatePath"),
    ] {
        let (report, identity) = inspect(role, settings[key].as_str().unwrap_or(""));
        files.push(report);
        identities.push(identity);
    }
    if identities[0].is_some() && identities[0] == identities[1] {
        files[1] = result(
            "projector",
            "error",
            "The main model and vision adapter are the same file. Choose the separate matching mmproj .gguf.",
        );
    }
    if settings["jinja"] == false && !settings["templatePath"].as_str().unwrap_or("").is_empty() {
        files[3] = result(
            "template",
            "error",
            "Enable model chat formatting to use a custom template, or clear the optional override.",
        );
    }
    if settings["jinja"] == false && settings["templatePath"].as_str().unwrap_or("").is_empty() {
        files[3] = result(
            "template",
            "warning",
            "Model chat formatting is disabled. Leave it enabled unless the model author's instructions require otherwise.",
        );
    }
    json!({"canSave":!files.iter().any(|v|v["status"]=="error"),"files":files,"summary":"Quick header checks only. No model was started. These checks do not verify all weights, a complete download or successful model loading. The vision adapter must still match the main model."})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "pf-model-check-{}-{:x}",
                std::process::id(),
                rand_id()
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn write(&self, name: &str, bytes: &[u8]) -> String {
            let p = self.0.join(name);
            fs::write(&p, bytes).unwrap();
            p.to_string_lossy().into()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn rand_id() -> u64 {
        let mut b = [0; 8];
        getrandom::fill(&mut b).unwrap();
        u64::from_le_bytes(b)
    }
    fn string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    fn gguf(architecture: &str, template: bool) -> Vec<u8> {
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        b.extend((if template { 3u64 } else { 1u64 }).to_le_bytes());
        string(&mut b, "general.architecture");
        b.extend(8u32.to_le_bytes());
        string(&mut b, architecture);
        if template {
            string(&mut b, "tokenizer.ggml.model");
            b.extend(8u32.to_le_bytes());
            string(&mut b, "gpt2");
            string(&mut b, "tokenizer.chat_template");
            b.extend(8u32.to_le_bytes());
            string(&mut b, "{{ messages }}");
        }
        b
    }
    fn reranker(architecture: &str, pooling: u32, labels: &[&str], template: &str) -> Vec<u8> {
        let mut b = gguf(architecture, false);
        b[16..24].copy_from_slice(&5u64.to_le_bytes());
        // Names are deliberately unhelpful: validation must use the ranking head.
        string(&mut b, "general.name");
        b.extend(8u32.to_le_bytes());
        string(&mut b, "ordinary language model");
        string(&mut b, "qwen3.pooling_type");
        b.extend(4u32.to_le_bytes());
        b.extend(pooling.to_le_bytes());
        string(&mut b, "qwen3.classifier.output_labels");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend((labels.len() as u64).to_le_bytes());
        for label in labels {
            string(&mut b, label);
        }
        string(&mut b, "tokenizer.chat_template.rerank");
        b.extend(8u32.to_le_bytes());
        string(&mut b, template);
        b
    }
    #[test]
    fn identifies_ranking_head_without_trusting_filenames_or_disclosing_prompts() {
        let f = Fixture::new();
        let path = f.write(
            "renamed-model.bin",
            &reranker("qwen3", 4, &["yes", "no"], "{{ secret_ranking_prompt }}"),
        );
        let report = inspect("reranker", &path).0;
        assert_eq!(report["status"], "ok");
        assert!(
            report["message"]
                .as_str()
                .unwrap()
                .contains("Qwen3 search reranker identified")
        );
        assert!(!report.to_string().contains("secret_ranking_prompt"));
        let ordinary = f.write("Qwen3-Reranker-0.6B.gguf", &gguf("qwen3", true));
        assert_eq!(inspect("reranker", &ordinary).0["status"], "warning");
        for (pooling, labels, template) in [
            (4, vec!["no", "yes"], "{{ ranking }}"),
            (4, vec!["yes"], "{{ ranking }}"),
            (1, vec!["yes", "no"], "{{ ranking }}"),
            (4, vec!["yes", "no"], ""),
        ] {
            let path = f.write(
                "incomplete-head.gguf",
                &reranker("qwen3", pooling, &labels, template),
            );
            assert_eq!(inspect("reranker", &path).0["status"], "warning");
        }
        let other = f.write(
            "external.gguf",
            &reranker("other", 4, &["yes", "no"], "{{ external_ranking_prompt }}"),
        );
        let report = inspect("reranker", &other).0;
        assert_eq!(report["status"], "warning");
        assert!(
            report["message"]
                .as_str()
                .unwrap()
                .contains("Embedded reranking format found")
        );
        assert!(!report.to_string().contains("external_ranking_prompt"));
    }
    #[test]
    fn bounded_or_ambiguous_ranking_metadata_never_gets_a_positive_identification() {
        let f = Fixture::new();
        let mut b = reranker("qwen3", 4, &["yes", "no"], "{{ ranking }}");
        b[16..24].copy_from_slice(&6u64.to_le_bytes());
        string(&mut b, "oversized-array");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(u64::MAX.to_le_bytes());
        let path = f.write("partial.gguf", &b);
        assert_eq!(inspect("reranker", &path).0["status"], "warning");
        let mut b = reranker("qwen3", 4, &["yes", "no"], "{{ ranking }}");
        b[16..24].copy_from_slice(&6u64.to_le_bytes());
        string(&mut b, "qwen3.pooling_type");
        b.extend(4u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        let path = f.write("duplicate.gguf", &b);
        assert_eq!(inspect("reranker", &path).0["status"], "error");
        let path = f.write(
            "many-labels.gguf",
            &reranker("qwen3", 4, &["yes"; 9], "{{ ranking }}"),
        );
        assert_eq!(inspect("reranker", &path).0["status"], "warning");
    }
    #[test]
    fn detects_swapped_model_projector_and_binary_template_without_names() {
        let f = Fixture::new();
        let model = f.write("renamed.bin", &gguf("qwen35", true));
        let adapter = f.write("other.bin", &gguf("clip", false));
        assert_eq!(inspect("model", &model).0["status"], "ok");
        assert_eq!(inspect("projector", &adapter).0["status"], "ok");
        assert_eq!(inspect("projector", &model).0["status"], "error");
        assert_eq!(inspect("model", &adapter).0["status"], "error");
        assert_eq!(inspect("reranker", &adapter).0["status"], "error");
        assert_eq!(inspect("template", &model).0["status"], "error");
    }
    #[test]
    fn limits_do_not_claim_compatibility_or_expose_template_contents() {
        let f = Fixture::new();
        let text = f.write("template.jinja", b"{{ secret_private_prompt }}");
        let value = inspect("template", &text).0;
        assert_eq!(value["status"], "warning");
        assert!(!value.to_string().contains("secret_private_prompt"));
        let huge = f.write("huge.jinja", &vec![b'x'; MAX_TEMPLATE as usize + 1]);
        assert_eq!(inspect("template", &huge).0["status"], "error");
        let bad = f.write("wrong.gguf", b"<html>download page</html>");
        assert_eq!(inspect("model", &bad).0["status"], "error");
        let mut unsupported = gguf("future", false);
        unsupported[4..8].copy_from_slice(&100u32.to_le_bytes());
        let future = f.write("future.gguf", &unsupported);
        assert_eq!(inspect("model", &future).0["status"], "warning");
        let mut truncated = gguf("clip", false);
        truncated.pop();
        let broken = f.write("broken.gguf", &truncated);
        assert_eq!(inspect("projector", &broken).0["status"], "error");
        assert_eq!(
            inspect("model", f.0.join("offline.gguf").to_str().unwrap()).0["status"],
            "warning"
        );
    }
    #[test]
    fn same_file_aliases_and_disabled_custom_format_are_errors() {
        let f = Fixture::new();
        let path = f.write("model.gguf", &gguf("unknown", false));
        let alias = f.0.join("alias.gguf");
        fs::hard_link(&path, &alias).unwrap();
        let checked = check(&json!({"modelPath":path,"projectorPath":alias,"jinja":true}));
        assert_eq!(checked["canSave"], false);
        assert!(
            checked["files"][1]["message"]
                .as_str()
                .unwrap()
                .contains("same file")
        );
        let template = f.write("custom.jinja", b"{{ messages }}");
        assert_eq!(
            check(&json!({"templatePath":template,"jinja":false}))["canSave"],
            false
        );
        assert_eq!(check(&json!({"jinja":true}))["canSave"], true);
    }
    #[test]
    fn nested_arrays_and_hostile_lengths_are_bounded() {
        let f = Fixture::new();
        let mut b = gguf("qwen35", false);
        b[16..24].copy_from_slice(&2u64.to_le_bytes());
        string(&mut b, "tokenizer.tokens");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(u64::MAX.to_le_bytes());
        let path = f.write("bounded.gguf", &b);
        assert_eq!(inspect("model", &path).0["status"], "warning");
        let mut b = gguf("qwen35", false);
        b[16..24].copy_from_slice(&2u64.to_le_bytes());
        string(&mut b, "array");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(2u64.to_le_bytes());
        string(&mut b, "a");
        string(&mut b, "b");
        let path = f.write("array.gguf", &b);
        assert_ne!(inspect("model", &path).0["status"], "error");
    }
}
