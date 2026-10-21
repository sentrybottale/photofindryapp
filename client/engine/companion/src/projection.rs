//! Bounded public presentation and opaque, process-local folder scope identities.
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Mutex};

#[derive(Default)]
pub struct Scopes {
    paths: Mutex<HashMap<String, String>>,
}
impl Scopes {
    pub fn reset(&self) { self.paths.lock().unwrap().clear(); }
    pub fn register(&self, path: &str) -> Option<String> {
        if !Path::new(path).is_absolute() || path.len() > 16384 {
            return None;
        }
        let mut paths = self.paths.lock().unwrap();
        if let Some((id, _)) = paths.iter().find(|(_, saved)| saved.as_str() == path) {
            return Some(id.clone());
        }
        if paths.len() >= 4096 {
            return None;
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).ok()?;
        let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
        paths.insert(id.clone(), path.into());
        Some(id)
    }
    pub fn resolve(&self, id: &str) -> Result<String, String> {
        self.paths
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("This folder scope is no longer available. Refresh Sources.".into())
    }
    pub fn project(&self, value: &Value) -> Value {
        self.project_depth(value, 0)
    }
    /// Authenticated Files presentation may show a source's complete location so
    /// an imported folder with the same name on another Mac is distinguishable.
    /// These labels grant no authority: actions still use source/root IDs and
    /// native picker grants. Keep generic photo/model projection unchanged.
    pub fn project_source_locations(&self, value: &Value) -> Value {
        let mut projected = self.project(value);
        if let (Some(sources), Some(presentations)) =
            (value["sources"].as_array(), projected["sources"].as_array_mut())
        {
            for (source, presentation) in sources.iter().zip(presentations.iter_mut()) {
                source_root_labels(source, presentation);
            }
        }
        if value["source"].is_object() {
            source_root_labels(&value["source"], &mut projected["source"]);
        }
        // Only the prepareSourceConnection response has this outer identity.
        // Callers use this projection solely for state and the two Files reads.
        if value["sourceId"].as_str().is_some_and(|id| !id.is_empty())
            && value["planId"].as_str().is_some_and(|id| !id.is_empty())
        {
            for (key, label) in [
                ("rootPath", "rootLocationLabel"),
                ("previousRootPath", "previousRootLocationLabel"),
            ] {
                if let Some(path) = source_location_label(&value[key]) {
                    projected[label] = json!(path);
                }
            }
        }
        projected
    }
    fn project_depth(&self, value: &Value, depth: usize) -> Value {
        if depth > 24 {
            return Value::Null;
        }
        match value {
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|v| self.project_depth(v, depth + 1))
                    .collect(),
            ),
            Value::Object(values) => {
                let mut result = serde_json::Map::new();
                let candidate = values.contains_key("reason")
                    && values.contains_key("id")
                    && values.contains_key("path");
                let photo = candidate
                    || values.contains_key("caption")
                    || values.contains_key("thumbnailPath");
                for (key, value) in values {
                    if [
                        "token",
                        "secret",
                        "password",
                        "authorization",
                        "embedding",
                        "vector",
                        "rawOutput",
                        "rawText",
                        "prompt",
                        "runPath",
                        "checkpoint",
                        "settingsJSON",
                        "input",
                        "nativeHost",
                        "sourceAccess",
                        "readerBookmark",
                        "ownerBookmark",
                        "ownerGrantId",
                    ]
                    .contains(&key.as_str())
                    {
                        continue;
                    }
                    if [
                        "modelPath",
                        "projectorPath",
                        "rerankerModelPath",
                        "templatePath",
                    ]
                    .contains(&key.as_str())
                    {
                        let base = key.trim_end_matches("Path");
                        let path = value.as_str().unwrap_or("");
                        result.insert(format!("{base}Configured"), json!(!path.is_empty()));
                        result.insert(
                            format!("{base}Label"),
                            json!(
                                Path::new(path)
                                    .file_name()
                                    .and_then(|v| v.to_str())
                                    .unwrap_or("")
                            ),
                        );
                        continue;
                    }
                    if key == "folders"
                        && value
                            .as_array()
                            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                    {
                        result.insert(
                            "scopeIds".into(),
                            json!(
                                value
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .filter_map(|v| self.register(v.as_str().unwrap()))
                                    .collect::<Vec<_>>()
                            ),
                        );
                        continue;
                    }
                    if value.as_str().is_some_and(|s| s.starts_with('/'))
                        && (key == "path"
                            || key == "folder"
                            || key == "aiFolder"
                            || key == "root"
                            || key == "source"
                            || key == "destination"
                            || key.ends_with("Path"))
                    {
                        let path = value.as_str().unwrap();
                        if candidate && key == "path" {
                            result.insert(
                                "locationLabel".into(),
                                json!(path.chars().take(4096).collect::<String>()),
                            );
                        }
                        if ["thumbnailPath", "cropPath", "coverPath", "previewPath"]
                            .contains(&key.as_str())
                        {
                            result.insert("hasPreview".into(), json!(true));
                            continue;
                        }
                        let label = Path::new(path)
                            .file_name()
                            .and_then(|v| v.to_str())
                            .unwrap_or("Folder");
                        result.insert(
                            if key == "path" {
                                "pathLabel".into()
                            } else {
                                format!("{}Label", key.trim_end_matches("Path"))
                            },
                            json!(label),
                        );
                        if !photo || key == "folder" {
                            if let Some(id) = self.register(path) {
                                let scope_key = match key.as_str() {
                                    "rootPath" => "rootScopeId",
                                    "parentPath" => "parentScopeId",
                                    "path" | "folder" => "scopeId",
                                    "aiFolder" => "aiScopeId",
                                    _ => "locationScopeId",
                                };
                                result.insert(scope_key.into(), json!(id));
                            }
                        }
                        continue;
                    }
                    // Serialized native result context may contain folder paths. Expose the
                    // explicit projected fields instead of an opaque JSON string bypass.
                    if ["aiMetadataFiltersJSON", "aiLocationOptionsJSON"].contains(&key.as_str()) {
                        let parsed = value
                            .as_str()
                            .filter(|s| s.len() <= 65536)
                            .and_then(|s| serde_json::from_str::<Value>(s).ok());
                        let valid = parsed.as_ref().is_some_and(|p| {
                            if key == "aiMetadataFiltersJSON" {
                                super::operations::metadata(p).is_ok()
                            } else {
                                super::operations::locations(p).is_ok()
                            }
                        });
                        if valid {
                            result.insert(key.trim_end_matches("JSON").into(), parsed.unwrap());
                        } else {
                            result.insert("aiContextUnavailable".into(), json!(true));
                        }
                        continue;
                    }
                    if key.ends_with("JSON") {
                        continue;
                    }
                    result.insert(key.clone(), self.project_depth(value, depth + 1));
                }
                Value::Object(result)
            }
            _ => value.clone(),
        }
    }
}

fn source_location_label(value: &Value) -> Option<String> {
    let path = value.as_str().filter(|path| Path::new(path).is_absolute() && !path.contains('\0'))?;
    Some(path.chars().take(4096).collect())
}

fn source_root_labels(source: &Value, projected: &mut Value) {
    if !source["id"].as_str().is_some_and(|id| !id.is_empty()) {
        return;
    }
    if let (Some(roots), Some(presentations)) =
        (source["roots"].as_array(), projected["roots"].as_array_mut())
    {
        for (root, presentation) in roots.iter().zip(presentations.iter_mut()) {
            if root["id"].as_str().is_some_and(|id| !id.is_empty()) {
                if let Some(path) = source_location_label(&root["path"]) {
                    presentation["locationLabel"] = json!(path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_automatic_lifecycle_and_selection_proofs_survive_authenticated_presentation() {
        let scopes=Scopes::default();
        let source=json!({"id":"source","automaticLifecycleVersion":1,"originalsFolderSelected":true,
            "mode":"connected","status":"needsReview","needsReview":2,"offline":3,
            "folderSelectionPending":true,"observationChecked":32,"observationTotal":90000,
            "roots":[{"id":"selected","path":"/Volumes/Photos/Selected","selected":true,"selectionPending":true,
                "status":"checking","available":30,"needsReview":2,"observationChecked":32,"observationTotal":90000},
                {"id":"imported","path":"/Users/other/Photos","selected":false}]});
        let snapshot=scopes.project_source_locations(&json!({"sources":[source.clone()]}));
        let details=scopes.project_source_locations(&json!({"source":source}));
        for item in [&snapshot["sources"][0],&details["source"]] {
            assert_eq!(item["automaticLifecycleVersion"],1);assert_eq!(item["originalsFolderSelected"],true);
            assert_eq!(item["needsReview"],2);assert_eq!(item["offline"],3);assert_eq!(item["status"],"needsReview");
            assert_eq!(item["folderSelectionPending"],true);assert_eq!(item["observationChecked"],32);assert_eq!(item["observationTotal"],90000);
            assert_eq!(item["roots"][0]["selected"],true);assert_eq!(item["roots"][1]["selected"],false);
            assert_eq!(item["roots"][0]["selectionPending"],true);assert_eq!(item["roots"][0]["status"],"checking");
            assert_eq!(item["roots"][0]["available"],30);assert_eq!(item["roots"][0]["needsReview"],2);
            assert_eq!(item["roots"][0]["observationChecked"],32);assert_eq!(item["roots"][0]["observationTotal"],90000);
            assert_eq!(item["roots"][0]["locationLabel"],"/Volumes/Photos/Selected");
            assert!(item["roots"][0].get("path").is_none(),"display labels never become filesystem authority");
        }
    }
    #[test]
    fn paths_are_capabilities_not_browser_parameters() {
        let scopes = Scopes::default();
        let value=scopes.project(&json!({"settings":{"modelPath":"/private/models/a.gguf"},"folders":[{"path":"/private/photos","count":2}],"faces":[{"id":"face","cropPath":"/private/crop.jpg","box":[0,0,1,1]}]}));
        assert!(!value.to_string().contains("/private"));
        assert_eq!(value["settings"]["modelLabel"], "a.gguf");
        let id = value["folders"][0]["scopeId"].as_str().unwrap();
        assert_eq!(scopes.resolve(id).unwrap(), "/private/photos");
        assert!(scopes.resolve("/etc").is_err());
        let folder = scopes.project(
            &json!({"path":"/photos/subtree","rootPath":"/photos","parentPath":"/photos/parent"}),
        );
        assert_eq!(
            scopes.resolve(folder["scopeId"].as_str().unwrap()).unwrap(),
            "/photos/subtree"
        );
        assert_eq!(
            scopes
                .resolve(folder["rootScopeId"].as_str().unwrap())
                .unwrap(),
            "/photos"
        );
        let ai=scopes.project(&json!({"aiFolder":"/photos/ai","aiMetadataFiltersJSON":"{\"camera\":\"Fuji\"}","aiLocationOptionsJSON":"{\"enabled\":true,\"radiusKm\":30}"}));
        assert_eq!(
            scopes.resolve(ai["aiScopeId"].as_str().unwrap()).unwrap(),
            "/photos/ai"
        );
        assert_eq!(ai["aiMetadataFilters"]["camera"], "Fuji");
        assert_eq!(ai["aiLocationOptions"]["radiusKm"], 30);
        assert!(ai.get("aiFolder").is_none());
    }
    #[test]
    fn files_locations_distinguish_imported_mac_paths_without_unredacting_photos_or_models() {
        let scopes = Scopes::default();
        let old = "/Users/other-mac/pCloud Drive/Automatic Upload";
        let current = "/Users/this-mac/pCloud Drive/Automatic Upload";
        let source = json!({"id":"source","roots":[{"id":"old","path":old},{"id":"current","path":current}]});
        let raw = json!({"sources":[source],"settings":{"modelPath":"/private/models/model.gguf","projectorPath":"/private/models/mmproj.gguf"},
            "photos":[{"id":"photo","caption":"Saved details","path":"/private/photos/photo.jpg","thumbnailPath":"/private/cache/photo.jpg"}],
            "folders":[{"path":"/private/other-folder"}]});
        let generic = scopes.project(&raw);
        assert!(generic["sources"][0]["roots"][0].get("locationLabel").is_none());
        assert!(!generic.to_string().contains("/Users/"));
        let value = scopes.project_source_locations(&raw);
        assert_eq!(value["sources"][0]["roots"][0]["locationLabel"], old);
        assert_eq!(value["sources"][0]["roots"][1]["locationLabel"], current);
        assert_eq!(value["sources"][0]["roots"][0]["pathLabel"], "Automatic Upload");
        assert_eq!(value["sources"][0]["roots"][1]["pathLabel"], "Automatic Upload");
        assert!(!value.to_string().contains("/private"));
        assert_eq!(value["settings"]["modelLabel"], "model.gguf");
        assert!(value["photos"][0].get("locationLabel").is_none());
        assert!(value["folders"][0].get("locationLabel").is_none());
        let root = &value["sources"][0]["roots"][1];
        assert!(root.get("path").is_none());
        assert_eq!(scopes.resolve(root["scopeId"].as_str().unwrap()).unwrap(), current);
        assert!(scopes.resolve(current).is_err(), "A display label is never an opaque scope ID");
        let details = scopes.project_source_locations(&json!({"source":source,"candidates":[]}));
        assert_eq!(details["source"]["roots"][0]["locationLabel"], old);
    }
    #[test]
    fn reviewed_folder_labels_are_bounded_and_require_the_source_plan_shape() {
        let scopes = Scopes::default();
        let raw = json!({"sourceId":"source","planId":"plan","rootPath":"/Users/current/Photos","previousRootPath":"/Users/previous/Photos",
            "examples":[{"photoId":"photo","path":"/private/example.jpg","relativePath":"example.jpg"}]});
        let value = scopes.project_source_locations(&raw);
        assert_eq!(value["rootLocationLabel"], "/Users/current/Photos");
        assert_eq!(value["previousRootLocationLabel"], "/Users/previous/Photos");
        assert!(value.get("rootPath").is_none());
        assert!(value.get("previousRootPath").is_none());
        assert!(!value.to_string().contains("/private"));
        assert_eq!(scopes.resolve(value["rootScopeId"].as_str().unwrap()).unwrap(), "/Users/current/Photos");
        for incomplete in [json!({"sourceId":"source","rootPath":"/private/photos"}), json!({"planId":"plan","rootPath":"/private/photos"})] {
            assert!(!scopes.project_source_locations(&incomplete).to_string().contains("/private"));
        }
        let long = format!("/{}", "界".repeat(5000));
        let bounded = scopes.project_source_locations(&json!({"sources":[{"id":"source","roots":[{"id":"root","path":long}]}],
            "sourceId":"source","planId":"plan","rootPath":long,"previousRootPath":"relative/path"}));
        assert_eq!(bounded["sources"][0]["roots"][0]["locationLabel"].as_str().unwrap().chars().count(), 4096);
        assert_eq!(bounded["rootLocationLabel"].as_str().unwrap().chars().count(), 4096);
        assert!(bounded.get("previousRootLocationLabel").is_none());
    }
}
    #[test]
    fn an_empty_library_keeps_its_folder_list_for_first_run_setup() {
        let scopes = Scopes::default();
        let value = scopes.project(&json!({"folders":[],"facePreferences":{"folders":[]}}));
        assert_eq!(value["folders"], json!([]));
        let paths = scopes.project(&json!({"folders":["/private/photos"]}));
        assert!(paths.get("folders").is_none());
        assert_eq!(paths["scopeIds"].as_array().unwrap().len(), 1);
    }

#[cfg(test)]
#[test]
fn native_access_carriers_and_owner_references_never_reach_presentation() {
    let projected=Scopes::default().project(&json!({"sourceAccess":[{"readerBookmark":"reader-secret"}],"readerBookmark":"reader-secret","ownerBookmark":"owner-secret","ownerGrantId":"owner-reference","source":{"id":"source","roots":[{"id":"root","ownerGrantId":"owner-reference","selected":true}]}}));
    let text=projected.to_string();for private in ["reader-secret","owner-secret","owner-reference","readerBookmark","ownerBookmark","sourceAccess"]{assert!(!text.contains(private));}
    assert_eq!(projected["source"]["roots"][0]["id"],"root");assert_eq!(projected["source"]["roots"][0]["selected"],true);
}
