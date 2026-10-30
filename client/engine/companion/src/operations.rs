//! Reviewed action schemas. Browser fields cannot select native methods or paths.
use super::{native_picker::Picker, projection::Scopes};
use serde_json::{Value, json};
type Result<T> = std::result::Result<T, String>;

fn object(value: &Value, fields: &[&str]) -> Result<()> {
    let map = value.as_object().ok_or("Parameters must be an object.")?;
    if map.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err("Unknown action parameter.".into());
    }
    for (key, value) in map {
        if value.is_null() {
            if [
                "sourceId", "scopeId", "personId", "photoId", "rootId", "ageBand",
            ]
            .contains(&key.as_str())
            {
                continue;
            }
            return Err(format!("Invalid null {key} field."));
        }
        let valid = match key.as_str() {
            "enabled" | "retry" | "jinja" | "smartCrops" | "force" | "recursive" | "automatic"
            | "verifyContents" | "confirmed" | "confirm" | "confirmReplace"
            | "confirmClearImages" | "disconnect" | "includePreviews" | "includeImageCaches"
            | "broad" | "reject" | "undo" | "all" | "stop" | "keepResults"
            | "progressArtEnabled" | "interleave" | "location" | "camera" | "lens" | "focal"
            | "date" | "exposure" | "keepOnBrowse" | "clearTemplate" | "clearReranker"
            | "includeRelated" | "broaden" | "resume" | "applied" | "lowMemoryTrial" | "background" => {
                value.is_boolean()
            }
            "offset" | "limit" | "expectedCount" | "dimension" | "budgetBytes" => {
                value.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
            }
            "photoIds" | "faceIds" => value.as_array().is_some_and(|a| {
                !a.is_empty() && a.len() <= 100000 && a.iter().all(|v| valid_text(v, 128))
            }),
            "folderGrants" | "scopeIds" => value.as_array().is_some_and(|a| {
                (key == "scopeIds" || !a.is_empty()) && a.len() <= 256 && a.iter().all(|v| valid_text(v, 128))
            }),
            "aliases" => value
                .as_array()
                .is_some_and(|a| a.len() <= 64 && a.iter().all(|v| valid_text(v, 256))),
            "metadataFilters" => metadata(value).is_ok(),
            "locationOptions" => locations(value).is_ok(),
            "schedule" => schedule(value).is_ok(),
            "params" => value.is_object(),
            _ => valid_text(
                value,
                if ["query", "name", "label"].contains(&key.as_str()) {
                    4096
                } else {
                    256
                },
            ),
        };
        if !valid {
            return Err(format!("Invalid {key} field."));
        }
    }
    Ok(())
}
fn valid_text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|v| v.len() <= max && !v.contains('\0'))
}
fn required(value: &Value, keys: &[&str]) -> Result<()> {
    if keys
        .iter()
        .any(|key| value[*key].is_null() || value[*key] == "")
    {
        return Err("A required action field is missing.".into());
    }
    Ok(())
}
fn choice(value: &Value, key: &str, choices: &[&str]) -> Result<()> {
    if value
        .get(key)
        .is_some_and(|v| !v.is_null() && !choices.contains(&v.as_str().unwrap_or("")))
    {
        return Err(format!("Invalid {key} choice."));
    }
    Ok(())
}
pub fn metadata(value: &Value) -> Result<()> {
    let map = value.as_object().ok_or("Invalid metadata filters.")?;
    for (k, v) in map {
        if ![
            "camera",
            "lens",
            "place",
            "dateFrom",
            "dateTo",
            "focalMin",
            "focalMax",
            "latitude",
            "longitude",
            "radiusKm",
        ]
        .contains(&k.as_str())
            || !(valid_text(v, 1024) || v.as_f64().is_some_and(f64::is_finite))
        {
            return Err("Invalid metadata filter.".into());
        }
    }
    Ok(())
}
pub fn locations(value: &Value) -> Result<()> {
    let map = value.as_object().ok_or("Invalid location options.")?;
    for (k, v) in map {
        let valid = match k.as_str() {
            "enabled" | "applied" => v.is_boolean(),
            "placeId" => v.is_null() || valid_text(v, 128),
            "radiusKm" => v.as_f64().is_some_and(|n| (0.1..=1000.).contains(&n)),
            _ => false,
        };
        if !valid {
            return Err("Invalid location option.".into());
        }
    }
    Ok(())
}
fn schedule(value: &Value) -> Result<()> {
    object(value, &["mode", "start", "end"])?;
    choice(value, "mode", &["now", "overnight"])?;
    for key in ["start", "end"] {
        if let Some(s) = value[key].as_str() {
            let p: Vec<_> = s.split(':').collect();
            if p.len() != 2
                || p[0].len() != 2
                || p[1].len() != 2
                || p[0].parse::<u8>().map_or(true, |n| n > 23)
                || p[1].parse::<u8>().map_or(true, |n| n > 59)
            {
                return Err("Use HH:MM schedule times.".into());
            }
        }
    }
    Ok(())
}
fn scope(params: &mut Value, scopes: &Scopes) -> Result<()> {
    if let Some(id) = params.as_object_mut().unwrap().remove("scopeId") {
        if !id.is_null() {
            params["folder"] = json!(scopes.resolve(id.as_str().ok_or("Invalid folder scope.")?)?);
        }
    }
    Ok(())
}
fn grant(params: &mut Value, key: &str, dest: &str, kind: &str, picker: &Picker) -> Result<()> {
    if let Some(id) = params.as_object_mut().unwrap().remove(key) {
        let id=id.as_str().ok_or("Choose a file using the native picker.")?;
        params[dest] = json!(
            picker.resolve(id,kind)?
        );
        if kind=="folder" {if let Some(access)=picker.resolve_source_access(id)?{params["sourceAccess"]=json!([access]);}}
    }
    Ok(())
}
fn transfer(params: &mut Value, key: &str, dest: &str, kind: &str, picker: &Picker) -> Result<()> {
    required(params, &[key])?;
    grant(params, key, dest, kind, picker)
}

pub fn prepare(
    action: &str,
    mut params: Value,
    scopes: &Scopes,
    picker: &Picker,
) -> Result<(&'static str, Value)> {
    let method = match action {
        "configure" | "switchModel" => {
            object(
                &params,
                &[
                    "modelGrant",
                    "projectorGrant",
                    "rerankerModelGrant",
                    "templateGrant",
                    "jinja",
                    "smartCrops",
                    "clearTemplate",
                    "clearReranker",
                ],
            )?;
            for (k, d, t) in [
                ("modelGrant", "modelPath", "model"),
                ("projectorGrant", "projectorPath", "projector"),
                ("rerankerModelGrant", "rerankerModelPath", "reranker"),
                ("templateGrant", "templatePath", "template"),
            ] {
                grant(&mut params, k, d, t, picker)?;
            }
            if params["clearTemplate"] == true {
                params["templatePath"] = json!("");
            }
            if params["clearReranker"] == true {
                params["rerankerModelPath"] = json!("");
            }
            params.as_object_mut().unwrap().remove("clearTemplate");
            params.as_object_mut().unwrap().remove("clearReranker");
            if action == "configure" {
                "configure"
            } else {
                "switchModel"
            }
        }
        "configurePhotoReview" => {
            object(
                &params,
                &["provider", "modelGrant", "projectorGrant", "lowMemoryTrial"],
            )?;
            choice(
                &params,
                "provider",
                &["tagging-model", "clef-flash", "intern-decision-4b"],
            )?;
            grant(&mut params, "modelGrant", "modelPath", "model", picker)?;
            grant(
                &mut params,
                "projectorGrant",
                "projectorPath",
                "projector",
                picker,
            )?;
            "configurePhotoReview"
        }
        "startModel"
        | "stopModel"
        | "optimizeModel"
        | "cancelModelOptimization"
        | "cancel"
        | "cancelSearch"
        | "cancelPreviews"
        | "cancelFaces"
        | "cancelDiscovery"
        | "cancelTransfer"
        | "cancelSourceTransfer"
        | "retrySourceTransfer"
        | "beginCleanupSession"
        | "cleanupSession"
        | "endCleanupSession"
        | "peopleGroups"
        | "discardAISearch" => {
            object(&params, &[])?;
            match action {
                "startModel" => "startModel",
                "stopModel" => "stopModel",
                "optimizeModel" => "optimizeModel",
                "cancelModelOptimization" => "cancelModelOptimization",
                "cancel" => "cancel",
                "cancelSearch" => "cancelSearch",
                "cancelPreviews" => "cancelPreviews",
                "cancelFaces" => "cancelFaces",
                "cancelDiscovery" => "cancelDiscovery",
                "cancelTransfer" => "cancelTransfer",
                "cancelSourceTransfer" => "cancelSourceTransfer",
                "retrySourceTransfer" => "retrySourceTransfer",
                "beginCleanupSession" => "beginCleanupSession",
                "cleanupSession" => "cleanupSession",
                "endCleanupSession" => "endCleanupSession",
                "peopleGroups" => "peopleGroups",
                _ => "discardAISearch",
            }
        }
        "configureMetadata" => {
            object(
                &params,
                &["location", "camera", "lens", "focal", "date", "exposure"],
            )?;
            "configureMetadata"
        }
        "configureSearchIndex" => {
            object(&params, &["enabled", "retry"])?;
            required(&params, &["enabled"])?;
            "configureSearchIndex"
        }
        "configureSearchAppearance" => {
            object(&params, &["progressArtEnabled"])?;
            required(&params, &["progressArtEnabled"])?;
            "configureSearchAppearance"
        }
        "configureStorage" => {
            object(&params, &["keepOnBrowse", "dimension", "budgetBytes"])?;
            "configureStorage"
        }
        "configureImprovement" => {
            object(&params, &["enabled", "mode", "start", "end", "interleave"])?;
            let mut timing = json!({});
            for key in ["mode", "start", "end"] {
                if let Some(v) = params.get(key) {
                    timing[key] = v.clone();
                }
            }
            schedule(&timing)?;
            "configureImprovement"
        }
        "configureFaceScan" => {
            object(&params, &["enabled", "scope", "scopeIds"])?;
            choice(&params, "scope", &["new", "library", "folders"])?;
            if params["scope"] == "folders" && params["scopeIds"].as_array().is_none_or(Vec::is_empty) {
                return Err("Choose at least one folder for face discovery.".into());
            }
            if params["scopeIds"].as_array().is_some_and(Vec::is_empty)
                && !matches!(params["scope"].as_str(),Some("new"|"library")) {
                return Err("Choose new photos or the library before clearing folder scopes.".into());
            }
            if let Some(ids) = params.as_object_mut().unwrap().remove("scopeIds") {
                params["folders"] = json!(
                    ids.as_array()
                        .unwrap()
                        .iter()
                        .map(|id| scopes.resolve(id.as_str().unwrap()))
                        .collect::<Result<Vec<_>>>()?
                );
            }
            "configureFaceScan"
        }
        "enqueueJob" => {
            object(&params, &["kind", "params", "schedule"])?;
            required(&params, &["kind"])?;
            choice(
                &params,
                "kind",
                &[
                    "tagging",
                    "improve",
                    "reviewScreen",
                    "metadata",
                    "previews",
                    "faces",
                    "aiSearch",
                    "semantic",
                ],
            )?;
            let mut inner = params.get("params").cloned().unwrap_or(json!({}));
            let kind = params["kind"].as_str().unwrap();
            let fields: &[&str] = if kind == "aiSearch" {
                &[
                    "query",
                    "sourceId",
                    "scopeId",
                    "metadataFilters",
                    "locationOptions",
                    "requestId",
                    "broaden",
                    "sort",
                    "candidateView",
                    "includeRelated",
                ]
            } else {
                &["photoIds", "sourceId", "scopeId", "force", "automatic"]
            };
            object(&inner, fields)?;
            if kind == "aiSearch" {
                required(&inner, &["query"])?;
            }
            scope(&mut inner, scopes)?;
            params["params"] = inner;
            "enqueueJob"
        }
        "jobAction" => {
            if params["action"]=="finishDeletion" {
                object(&params,&["action","deletionId"])?;
                required(&params,&["deletionId"])?;
                if !params["deletionId"].as_str().is_some_and(|id|id.len()<=128&&id.bytes().all(|byte|byte.is_ascii_alphanumeric()||byte==b'-')) {
                    return Err("Choose the current interrupted deletion in Jobs.".into());
                }
                return Ok(("jobAction",params));
            }
            object(&params, &["id", "action", "schedule"])?;
            required(&params, &["action"])?;
            choice(
                &params,
                "action",
                &[
                    "pause",
                    "resume",
                    "cancel",
                    "retry",
                    "retryMissing",
                    "continueAvailable",
                    "runNext",
                    "runNow",
                    "setSchedule",
                    "schedule",
                    "cancelAll",
                    "dismissAll",
                    "dismiss",
                    "dismissJob",
                    "markRead",
                    "markAllRead",
                ],
            )?;
            if params["action"] == "setSchedule" {
                params["action"] = json!("schedule");
            }
            "jobAction"
        }
        "jobResults" | "aiSearchPage" => {
            object(
                &params,
                if action == "jobResults" {
                    &["id", "searchId", "offset", "limit", "sort", "candidateView"]
                } else {
                    &["searchId", "offset", "limit", "sort", "candidateView"]
                },
            )?;
            required(
                &params,
                if action == "jobResults" {
                    &["id"]
                } else {
                    &["searchId"]
                },
            )?;
            if params["limit"].as_u64().is_some_and(|n| n == 0 || n > 240) {
                return Err("Result page limit must be 1–240.".into());
            }
            if action == "jobResults" {
                "jobResults"
            } else {
                "aiSearchPage"
            }
        }
        "cancelAISearch" => {
            object(&params, &["stop", "keepResults", "sort", "candidateView"])?;
            "cancelAISearch"
        }
        "addFolders" => {
            object(&params, &["folderGrants"])?;
            required(&params, &["folderGrants"])?;
            let paths = params["folderGrants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| picker.resolve(v.as_str().unwrap(), "folder"))
                .collect::<Result<Vec<_>>>()?;
            let access=params["folderGrants"].as_array().unwrap().iter().map(|id|picker.resolve_source_access(id.as_str().unwrap())).collect::<Result<Vec<_>>>()?.into_iter().flatten().collect::<Vec<_>>();
            params = json!({"paths":paths});
            if !access.is_empty(){params["sourceAccess"]=json!(access);}
            "addFolders"
        }
        "discoverFolders" => {
            object(&params, &["scopeId", "recursive"])?;
            scope(&mut params, scopes)?;
            "discoverFolders"
        }
        "folderChildren" => {
            object(&params, &["scopeId"])?;
            required(&params, &["scopeId"])?;
            scope(&mut params, scopes)?;
            "folderChildren"
        }
        "sourceDetails" => {
            object(&params, &["sourceId", "offset", "limit"])?;
            required(&params, &["sourceId"])?;
            "sourceDetails"
        }
        "checkSources" => {
            object(&params, &["sourceId", "scopeIds", "automatic"])?;
            if params["scopeIds"].as_array().is_some_and(Vec::is_empty) {
                return Err("Choose at least one folder to check.".into());
            }
            if let Some(ids) = params.as_object_mut().unwrap().remove("scopeIds") {
                params["paths"] = json!(
                    ids.as_array()
                        .unwrap()
                        .iter()
                        .map(|id| scopes.resolve(id.as_str().unwrap()))
                        .collect::<Result<Vec<_>>>()?
                );
            }
            "checkSources"
        }
        "locateSource" => {
            object(
                &params,
                &[
                    "sourceId",
                    "rootId",
                    "folderGrant",
                    "scopeId",
                    "verifyContents",
                    "resumeJobId",
                ],
            )?;
            required(&params, &["sourceId"])?;
            if params.get("folderGrant").is_some() == params.get("scopeId").is_some() {
                return Err("Choose exactly one folder location.".into());
            }
            if let Some(id) = params.as_object_mut().unwrap().remove("scopeId") {
                params["path"] = json!(scopes.resolve(id.as_str().ok_or("Invalid scope.")?)?);
            }
            grant(&mut params, "folderGrant", "path", "folder", picker)?;
            "locateSource"
        }
        "renameSource" => {
            object(&params, &["sourceId", "label"])?;
            required(&params, &["sourceId", "label"])?;
            "renameSource"
        }
        "prepareSourceConnection" => {
            object(&params, &["sourceId", "rootId", "background"])?;
            required(&params, &["sourceId"])?;
            "prepareSourceConnection"
        }
        "cancelSourceConnectionReview" => {
            object(&params, &["planId"])?;
            "cancelSourceConnectionReview"
        }
        "sourceConnectionReview" => {
            object(&params, &["sourceId", "planId"])?;
            required(&params, &["sourceId", "planId"])?;
            "sourceConnectionReview"
        }
        "retrySourceObservation" => {
            object(&params, &["sourceId", "rootId"])?;
            required(&params, &["sourceId"])?;
            "retrySourceObservation"
        }
        "confirmSourceConnection" => {
            object(&params, &["sourceId", "planId", "confirmed", "resumeJobId"])?;
            required(&params, &["sourceId", "planId"])?;
            if params["confirmed"] != true {
                return Err("Confirm the reviewed source connection.".into());
            }
            "confirmSourceConnection"
        }
        "confirmSourceMatch" => {
            object(&params, &["sourceId", "photoId", "candidateId"])?;
            required(&params, &["sourceId", "photoId", "candidateId"])?;
            "confirmSourceMatch"
        }
        "undoSourceRelink" => {
            object(&params, &["sourceId"])?;
            required(&params, &["sourceId"])?;
            "undoSourceRelink"
        }
        "recoverJobOriginals" => {
            object(&params, &["jobId"])?;
            required(&params, &["jobId"])?;
            "recoverJobOriginals"
        }
        "prepareSourceArchive" => {
            object(&params, &["sourceId", "mode", "includeImageCaches"])?;
            required(&params, &["sourceId"])?;
            choice(&params, "mode", &["metadata", "detach"])?;
            "prepareSourceArchive"
        }
        "inspectSourceArchive" | "attachSourceArchive" => {
            object(&params, &["sourceGrant", "confirm"])?;
            transfer(&mut params, "sourceGrant", "source", "archiveOpen", picker)?;
            if action == "attachSourceArchive" && params["confirm"] != true {
                return Err("Confirm attaching this source archive.".into());
            }
            if action == "inspectSourceArchive" {
                "inspectSourceArchive"
            } else {
                "attachSourceArchive"
            }
        }
        "exportSourceArchive" => {
            object(
                &params,
                &[
                    "planId",
                    "destinationGrant",
                    "disconnect",
                    "confirmClearImages",
                ],
            )?;
            required(&params, &["planId"])?;
            transfer(
                &mut params,
                "destinationGrant",
                "destination",
                "archiveSave",
                picker,
            )?;
            "exportSourceArchive"
        }
        "inspectBackup" | "restoreLibrary" => {
            object(&params, &["sourceGrant", "confirmReplace"])?;
            transfer(&mut params, "sourceGrant", "source", "backupOpen", picker)?;
            if action == "restoreLibrary" && params["confirmReplace"] != true {
                return Err("Confirm replacing this library with the reviewed backup.".into());
            }
            if action == "inspectBackup" {
                "inspectBackup"
            } else {
                "restoreLibrary"
            }
        }
        "exportLibrary" => {
            object(&params, &["destinationGrant", "includePreviews"])?;
            transfer(
                &mut params,
                "destinationGrant",
                "destination",
                "backupSave",
                picker,
            )?;
            "exportLibrary"
        }
        "browsePeople" => {
            object(&params, &["personId", "offset"])?;
            "browsePeople"
        }
        "peopleSuggestions" => {
            object(&params, &["personId", "broad"])?;
            required(&params, &["personId"])?;
            "peopleSuggestions"
        }
        "assignPerson" => {
            object(
                &params,
                &["faceIds", "photoId", "name", "personId", "ageBand"],
            )?;
            if params["photoId"].is_null() && params["faceIds"].is_null() {
                return Err("Choose a photo or faces to name.".into());
            }
            "assignPerson"
        }
        "reviewPeople" => {
            object(&params, &["faceIds", "personId", "decision"])?;
            required(&params, &["faceIds", "personId", "decision"])?;
            choice(&params, "decision", &["confirm", "exclude"])?;
            "reviewPeople"
        }
        "editPerson" => {
            object(
                &params,
                &[
                    "undo",
                    "personId",
                    "name",
                    "aliases",
                    "mergeInto",
                    "faceId",
                    "reject",
                ],
            )?;
            if params["undo"] != true {
                required(&params, &["personId"])?;
            }
            "editPerson"
        }
        "ensurePreview" | "taggingDiagnostic" | "undoImprovement" => {
            object(&params, &["photoId"])?;
            required(&params, &["photoId"])?;
            match action {
                "ensurePreview" => "ensurePreview",
                "taggingDiagnostic" => "taggingDiagnostic",
                _ => "undoImprovement",
            }
        }
        "refreshMetadata" | "prepareOfflinePreviews" | "scanFaces" => {
            object(&params, &["sourceId", "scopeId", "photoIds", "force"])?;
            scope(&mut params, scopes)?;
            match action {
                "refreshMetadata" => "refreshMetadata",
                "prepareOfflinePreviews" => "prepareOfflinePreviews",
                _ => "scanFaces",
            }
        }
        "metadataSuggestions" => {
            object(
                &params,
                &[
                    "field",
                    "query",
                    "sourceId",
                    "scopeId",
                    "statusFilter",
                    "metadataFilters",
                    "limit",
                ],
            )?;
            choice(&params, "field", &["camera", "lens", "place"])?;
            scope(&mut params, scopes)?;
            "metadataSuggestions"
        }
        "prepareCleanup" => {
            object(&params, &["sourceId"])?;
            "prepareCleanup"
        }
        "cleanupReview" | "cleanupRemoved" => {
            object(&params, &["sourceId", "planId", "offset", "limit"])?;
            if action == "cleanupReview" {
                "cleanupReview"
            } else {
                "cleanupRemoved"
            }
        }
        "cleanupRemovalResult" => {
            object(&params, &["jobId"])?;
            required(&params, &["jobId"])?;
            "cleanupRemovalResult"
        }
        "removeLibraryEntries" => {
            object(
                &params,
                &["planId", "photoIds", "all", "expectedCount", "confirmed"],
            )?;
            required(&params, &["planId"])?;
            if params["confirmed"] != true {
                return Err("Confirm moving reviewed entries to Removed.".into());
            }
            if params["all"] != true {
                required(&params, &["photoIds"])?;
            } else {
                required(&params, &["expectedCount"])?;
            }
            "removeLibraryEntries"
        }
        "restoreLibraryEntries" => {
            object(
                &params,
                &["photoIds", "removalJobId", "expectedCount", "confirmed"],
            )?;
            if params["confirmed"] != true {
                return Err("Confirm restoring removed entries.".into());
            }
            if params["photoIds"].is_null() {
                required(&params, &["removalJobId", "expectedCount"])?;
            }
            "restoreLibraryEntries"
        }
        "prepareRemovedDeletion" => {
            object(&params, &["sourceId", "photoIds"])?;
            "prepareRemovedDeletion"
        }
        "deleteRemovedEntries" => {
            object(&params, &["deletionId", "confirmed"])?;
            required(&params, &["deletionId"])?;
            if params["confirmed"] != true {
                return Err("Confirm the prepared permanent deletion.".into());
            }
            "deleteRemovedEntries"
        }
        _ => return Err("Unknown or retired application action.".into()),
    };
    Ok((method, params))
}

pub fn library(mut value: Value, scopes: &Scopes) -> Result<Value> {
    object(
        &value,
        &[
            "offset",
            "query",
            "sourceId",
            "scopeId",
            "status",
            "sort",
            "metadataFilters",
            "locationOptions",
            "includeRelated",
        ],
    )?;
    choice(
        &value,
        "status",
        &["all", "tagged", "pending", "needsAttention"],
    )?;
    choice(
        &value,
        "sort",
        &[
            "library",
            "filename",
            "recentlyTagged",
            "recentlyImproved",
            "dateTaken",
            "relevance",
        ],
    )?;
    scope(&mut value, scopes)?;
    Ok(value)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn reviewed_folder_confirmation_preserves_the_waiting_job_identity() {
        let scopes=Scopes::default();let picker=Picker::new(std::path::Path::new("/unavailable"));
        let request=json!({"sourceId":"source","planId":"reviewed-plan","confirmed":true,"resumeJobId":"waiting-job"});
        assert_eq!(prepare("confirmSourceConnection",request.clone(),&scopes,&picker).unwrap().1,request);
        assert!(prepare("confirmSourceConnection",json!({"sourceId":"source","planId":"plan","confirmed":true,"resumeJobId":{"path":"/private"}}),&scopes,&picker).is_err());
    }
    #[test]
    fn face_discovery_can_clear_old_folders_only_for_explicit_nonfolder_scopes() {
        let scopes=Scopes::default(); let picker=Picker::new(std::path::Path::new("/unavailable"));
        for scope in ["new","library"] {
            let (_,result)=prepare("configureFaceScan",json!({"enabled":true,"scope":scope,"scopeIds":[]}),&scopes,&picker).unwrap();
            assert_eq!(result["folders"],json!([]));assert_eq!(result["scope"],scope);
        }
        for scope in ["folders",""] { assert!(prepare("configureFaceScan",json!({"scope":scope,"scopeIds":[]}),&scopes,&picker).is_err()); }
        assert!(prepare("checkSources",json!({"scopeIds":[]}),&scopes,&picker).is_err());
    }
    #[test]
    fn interrupted_deletion_accepts_only_its_frozen_plan_identity() {
        let scopes=Scopes::default();let picker=Picker::new(std::path::Path::new("/unavailable"));
        let request=json!({"action":"finishDeletion","deletionId":"5b382288-51f3-44b6-9389-8c8c5bbecb1d"});
        assert_eq!(prepare("jobAction",request.clone(),&scopes,&picker).unwrap().1,request);
        for input in [json!({"action":"finishDeletion"}),json!({"action":"finishDeletion","deletionId":"../other"}),
            json!({"action":"finishDeletion","deletionId":"plan","id":"job"}),json!({"action":"finishDeletion","deletionId":"plan","schedule":{"mode":"now"}}),
            json!({"action":"resume","deletionId":"plan"})] {
            assert!(prepare("jobAction",input,&scopes,&picker).is_err());
        }
    }
    #[test]
    fn rejects_unknown_retired_and_path_injection() {
        let scopes = Scopes::default();
        let picker = Picker::new(std::path::Path::new("/unavailable"));
        for (a, p) in [
            ("shutdown", json!({})),
            ("inspect", json!({})),
            ("configure", json!({"modelPath":"/etc/passwd"})),
            (
                "enqueueJob",
                json!({"kind":"tagging","params":{"folder":"/etc"}}),
            ),
            ("addFolders", json!({"paths":["/etc"]})),
        ] {
            assert!(prepare(a, p, &scopes, &picker).is_err());
        }
        assert!(
            prepare(
                "enqueueJob",
                json!({"kind":"tagging","params":{"photoIds":[]}}),
                &scopes,
                &picker
            )
            .is_err()
        );
        assert!(
            prepare(
                "deleteRemovedEntries",
                json!({"deletionId":"x","confirmed":false}),
                &scopes,
                &picker
            )
            .is_err()
        );
        assert!(prepare("aiSearchPage", json!({"id":"unbound"}), &scopes, &picker).is_err());
        assert!(
            prepare(
                "aiSearchPage",
                json!({"searchId":"bound","offset":0}),
                &scopes,
                &picker
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
#[test]
fn native_access_rpc_and_carriers_are_never_browser_authority() {
    let scopes=Scopes::default();let folder=std::env::temp_dir();let picker=Picker::new(&folder);
    for action in ["sourceAccessRefs","installSourceAccess"] {assert!(prepare(action,json!({}),&scopes,&picker).is_err());}
    let grant=picker.grant_path("folder",folder.to_str().unwrap()).unwrap();
    let id=grant["grantId"].as_str().unwrap();
    assert!(prepare("addFolders",json!({"folderGrants":[id],"sourceAccess":[{"readerBookmark":"untrusted-browser-token"}]}),&scopes,&picker).is_err());
    assert!(prepare("locateSource",json!({"sourceId":"source","folderGrant":id,"sourceAccess":[]}),&scopes,&picker).is_err());
    let(_,params)=prepare("addFolders",json!({"folderGrants":[id]}),&scopes,&picker).unwrap();
    assert!(params.get("sourceAccess").is_none(),"explicit stdin/local path grants do not invent OS access");
}

#[cfg(all(test,windows))]
mod windows_tests{
 use super::*;
 #[test]fn http_actions_accept_only_typed_current_local_grants(){
  let mut nonce=[0;16];getrandom::fill(&mut nonce).unwrap();let id:String=nonce.iter().map(|b|format!("{b:02x}")).collect();let root=std::env::temp_dir().join(format!("pf-win-actions-{id}"));photofindry_platform::fs::private_dir(&root,true).unwrap();let picker=Picker::new(&root);let scopes=Scopes::default();
  let grant=picker.grant_path("folder",root.to_str().unwrap()).unwrap();let id=grant["grantId"].as_str().unwrap();let(method,params)=prepare("addFolders",json!({"folderGrants":[id]}),&scopes,&picker).unwrap();assert_eq!(method,"addFolders");assert_eq!(params["paths"][0],std::fs::canonicalize(&root).unwrap().to_str().unwrap());
  for input in [json!({"folders":[root]}),json!({"folderGrants":[root]}),json!({"folderGrants":[id],"sourceAccess":[{"readerBookmark":"browser"}]})]{assert!(prepare("addFolders",input,&scopes,&picker).is_err());}
  picker.revoke();assert!(prepare("addFolders",json!({"folderGrants":[id]}),&scopes,&picker).is_err());std::fs::remove_dir(&root).unwrap();
 }
}
