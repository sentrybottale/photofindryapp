//! Feature availability shared by the private Linux and Windows companions.
//!
//! A capability is advertised only when its provider is actually packaged:
//! People needs the platform FaceHelper plus all three pinned face models and
//! the face-policy manifest beside them. Camera/location metadata and photo
//! improvement use the shared worker with the platform PhotoHelper and the
//! user's tagging model. Dedicated decision reviewers (CLEF Flash, Intern) use
//! Mac-only runtimes and stay unavailable; the default tagging-model reviewer
//! needs no extra files. macOS never includes this module.
use serde_json::{Value, json};
use std::{path::Path, sync::OnceLock};

pub const FACE_MODELS: [&str; 3] = [
    "face_detection_yunet_2023mar.onnx",
    "face_recognition_sface_2021dec.onnx",
    "auraface-v1.onnx",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Features {
    pub faces: bool,
}

static FEATURES: OnceLock<Features> = OnceLock::new();

fn executable(runtime: &Path, name: &str) -> std::path::PathBuf {
    runtime.join(if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() })
}

fn nonempty_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
}

pub fn detect(runtime: &Path) -> Features {
    let faces = runtime.join("faces");
    Features {
        faces: nonempty_file(&executable(runtime, "FaceHelper"))
            && nonempty_file(&faces.join("face-policy-manifest.json"))
            && FACE_MODELS.iter().all(|m| nonempty_file(&faces.join(m))),
    }
}

/// Record the packaged providers once, before the worker is admitted.
pub fn init(runtime: &Path) -> Features {
    *FEATURES.get_or_init(|| detect(runtime))
}

pub fn current() -> Features {
    FEATURES.get().copied().unwrap_or_default()
}

/// Capability fields owned by this module, merged into each platform's list.
pub fn capability_fields(features: Features) -> Value {
    json!({
        "faceDetection": features.faces,
        "metadata": true,
        "photoReview": true,
        "photoReviewProviders": ["tagging-model"],
        "heifImages": "platform-codec",
        // Whole-library .photofindrybackup export/inspect/restore. Source
        // archives (`backupTransfer`) still have no Linux/Windows provider.
        "libraryBackup": true,
        "rawImages": "embedded-jpeg-rendition",
    })
}

pub fn merge(mut base: Value, features: Features) -> Value {
    if let (Some(map), Value::Object(extra)) = (base.as_object_mut(), capability_fields(features)) {
        map.extend(extra);
    }
    base
}

/// Browser actions that still have no provider on this platform.
pub fn action_unavailable(action: &str, params: &Value, features: Features) -> bool {
    match action {
        "configureFaceScan" | "scanFaces" => !features.faces,
        "enqueueJob" => job_unavailable(params["kind"].as_str().unwrap_or(""), features),
        _ => false,
    }
}

pub fn job_unavailable(kind: &str, features: Features) -> bool {
    match kind {
        "faces" => !features.faces,
        // Automatic performance trials are a separate, unchanged preview limit.
        "performance" => true,
        _ => false,
    }
}

/// Only the tagging-model reviewer is available outside macOS.
pub fn review_settings_error(params: &Value) -> Option<&'static str> {
    match params.get("provider").and_then(Value::as_str) {
        None | Some("tagging-model") => None,
        Some(_) => Some(
            "CLEF Flash and Intern-Decision reviewers need the Mac review runtime. Use your current tagging model for photo detail review on this computer.",
        ),
    }
}

/// Read-only reviewer preflight with the same response shape as macOS.
pub fn review_check(settings: &Value) -> Value {
    match review_settings_error(settings) {
        None => json!({"canSave":true,"files":[],"summary":"Uses your current tagging model for photo detail review. No separate reviewer files are required and no model was started.","physicalMemoryBytes":null,"minimumMemoryBytes":0}),
        Some(message) => json!({"canSave":false,"files":[],"summary":message,"physicalMemoryBytes":null,"minimumMemoryBytes":0}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("photofindry-preview-features-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("faces")).unwrap();
        path
    }
    #[test]
    fn faces_need_helper_policy_and_every_model() {
        let root = runtime("faces");
        assert!(!detect(&root).faces);
        std::fs::write(executable(&root, "FaceHelper"), b"helper").unwrap();
        std::fs::write(root.join("faces/face-policy-manifest.json"), b"{}").unwrap();
        for model in &FACE_MODELS[..2] {
            std::fs::write(root.join("faces").join(model), b"onnx").unwrap();
        }
        assert!(!detect(&root).faces, "a missing recogniser keeps People unavailable");
        std::fs::write(root.join("faces").join(FACE_MODELS[2]), b"").unwrap();
        assert!(!detect(&root).faces, "an empty model is not a provider");
        std::fs::write(root.join("faces").join(FACE_MODELS[2]), b"onnx").unwrap();
        assert!(detect(&root).faces);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn actions_follow_detected_providers() {
        let none = Features { faces: false };
        let faces = Features { faces: true };
        assert!(action_unavailable("scanFaces", &json!({}), none));
        assert!(!action_unavailable("scanFaces", &json!({}), faces));
        for kind in ["metadata", "improve", "reviewScreen", "tagging"] {
            assert!(!action_unavailable("enqueueJob", &json!({"kind":kind}), none), "{kind}");
        }
        assert!(action_unavailable("enqueueJob", &json!({"kind":"faces"}), none));
        assert!(action_unavailable("enqueueJob", &json!({"kind":"performance"}), faces));
        assert!(!action_unavailable("refreshMetadata", &json!({}), none));
        assert!(!action_unavailable("configureImprovement", &json!({}), none));
    }
    #[test]
    fn only_the_tagging_model_reviewer_is_accepted() {
        assert!(review_settings_error(&json!({})).is_none());
        assert!(review_settings_error(&json!({"provider":"tagging-model"})).is_none());
        assert!(review_settings_error(&json!({"provider":"clef-flash"})).is_some());
        assert_eq!(review_check(&json!({"provider":"intern-decision-4b"}))["canSave"], false);
        assert_eq!(review_check(&json!({"provider":"tagging-model"}))["canSave"], true);
        let merged = merge(json!({"platform":"linux","metadata":false}), Features { faces: true });
        assert_eq!(merged["metadata"], true);
        assert_eq!(merged["faceDetection"], true);
        assert_eq!(merged["platform"], "linux");
    }
}
