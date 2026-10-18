//! Fixed-layout validation for the signed consumer launcher enclosing a sealed
//! companion. Never select an ancestor from a browser-supplied path.
use super::launch;
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,fs::{self,OpenOptions},io::Read,
    os::unix::fs::{MetadataExt,OpenOptionsExt},path::{Path,PathBuf},process::{Command,Stdio}};
type Result<T>=std::result::Result<T,String>;
const BUNDLE:&str="com.sentrybottale.photorecall";
fn bytes(path:&Path,limit:u64)->Result<Vec<u8>> {
    let before=fs::symlink_metadata(path).map_err(|_|"The installed browser app is incomplete. Reinstall PhotoFindry before removing it.".to_owned())?;
    if !before.is_file()||before.file_type().is_symlink()||before.nlink()!=1||before.len()>limit||before.mode()&0o022!=0||![0,unsafe{libc::geteuid()}].contains(&before.uid()) {return Err("The installed browser app contains unsafe or oversized metadata. No files were removed.".into());}
    let file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK).open(path).map_err(|_|"Cannot inspect the installed browser app.".to_owned())?;
    let opened=file.metadata().map_err(|_|"Cannot inspect browser app metadata.".to_owned())?;
    if opened.dev()!=before.dev()||opened.ino()!=before.ino(){return Err("Browser app metadata changed while opening it.".into());}
    let mut value=Vec::new();file.take(limit+1).read_to_end(&mut value).map_err(|_|"Cannot read browser app metadata.".to_owned())?;
    if value.len() as u64>limit{return Err("Browser app metadata exceeds its bound.".into());}Ok(value)
}
fn plist(path:&Path)->Result<Value>{
    bytes(path,2*1024*1024)?;
    let output=Command::new("/usr/bin/plutil").args(["-convert","json","-o","-","--"]).arg(path).stderr(Stdio::null()).output().map_err(|_|"Cannot inspect browser app identity.".to_owned())?;
    if !output.status.success()||output.stdout.len()>4*1024*1024{return Err("Browser app identity is invalid or oversized.".into());}
    serde_json::from_slice(&output.stdout).map_err(|_|"Browser app identity is invalid.".into())
}
fn outer_path(executable:&Path)->Option<PathBuf>{
    let bin=executable.parent()?;let companion=bin.parent()?;let resources=companion.parent()?;let contents=resources.parent()?;let app=contents.parent()?;
    (executable.file_name()?=="photofindry"&&bin.file_name()?=="bin"&&companion.file_name()?=="companion"&&resources.file_name()?=="Resources"&&contents.file_name()?=="Contents"&&app.extension()?=="app").then(||app.to_path_buf())
}
pub fn enclosing(executable:&Path)->Result<Option<PathBuf>> {
    let Some(app)=outer_path(executable)else{return Ok(None)};
    if launch::canonical(&app,true)?!=app{return Err("The installed browser app moved or traverses a link. Reopen the installed app and retry.".into());}
    let marker:Value=serde_json::from_slice(&bytes(&app.join("Contents/Resources/browser-host-manifest.json"),16*1024)?).map_err(|_|"The browser app manifest is invalid.".to_owned())?;
    let info=plist(&app.join("Contents/Info.plist"))?;
    if marker["schema"]!=1||marker["kind"]!="browser-host"||marker["bundleIdentifier"]!=BUNDLE||marker["companion"]!="companion"
        ||info["CFBundleIdentifier"]!=BUNDLE||info["CFBundleExecutable"]!="PhotoFindry"
        ||marker["version"]!=info["CFBundleShortVersionString"]||marker["build"]!=info["CFBundleVersion"] {
        return Err("The enclosing app identity or companion layout is incompatible. Reinstall PhotoFindry before removing this package.".into());
    }
    let status=Command::new("/usr/bin/codesign").args(["--verify","--deep","--strict"]).arg(&app).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|_|"Cannot verify the installed browser app signature.".to_owned())?;
    if !status.success(){return Err("The installed browser app failed its signature check. No app or library files were removed.".into());}
    Ok(Some(app))
}
fn outer_files()->BTreeSet<String>{
    let mut names:[&str;9]=["Contents/Info.plist","Contents/MacOS/PhotoFindry","Contents/_CodeSignature/CodeResources","Contents/Resources/AppIcon.icns","Contents/Resources/BrandMark.png","Contents/Resources/PrivacyInfo.xcprivacy","Contents/Resources/licenses/Sparkle-LICENSE.txt","Contents/Resources/runtime-manifest.json","Contents/Resources/browser-host-manifest.json"];
    names.sort();names.into_iter().map(str::to_owned).collect()
}
fn framework_files()->BTreeSet<String>{
    let mut files=BTreeSet::new();
    for name in ["Sparkle","Autoupdate","_CodeSignature/CodeResources","Resources/Info.plist","Resources/ReleaseNotesColorStyle.css","Resources/SUUpdateAlert.nib","Resources/SUUpdatePermissionPrompt.nib","Updater.app/Contents/Info.plist","Updater.app/Contents/PkgInfo","Updater.app/Contents/MacOS/Updater","Updater.app/Contents/_CodeSignature/CodeResources"] {files.insert(format!("Contents/Frameworks/Sparkle.framework/Versions/B/{name}"));}
    for name in ["Downloader","Installer"]{for suffix in ["Info.plist".to_owned(),format!("MacOS/{name}"),"_CodeSignature/CodeResources".to_owned()]{files.insert(format!("Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices/{name}.xpc/Contents/{suffix}"));}}
    for language in ["Base","ar","ca","cs","da","de","el","es","fa","fi","fr","he","hr","hu","is","it","ja","ko","nb","nl","nn","pl","pt-BR","pt-PT","ro","ru","sk","sl","sv","tr","uk","vi","zh_CN","zh_HK","zh_TW","th"]{files.insert(format!("Contents/Frameworks/Sparkle.framework/Versions/B/Resources/{language}.lproj/Sparkle.strings"));}files
}
fn aliases()->std::collections::BTreeMap<String,String>{
    let prefix="Contents/Frameworks/Sparkle.framework/";
    let mut map=std::collections::BTreeMap::new();
    for name in ["Sparkle","Autoupdate","Resources","Updater.app","XPCServices"]{map.insert(format!("{prefix}{name}"),format!("Versions/Current/{name}"));}
    map.insert(format!("{prefix}Versions/Current"),"B".into());map
}
/// The nested inventory is checked independently by uninstall. Check every
/// outer entry against the pinned launcher/Sparkle runtime shape, then rely on
/// the verified code signature for the bytes. Extra user files block removal.
pub fn verify_layout(app:&Path)->Result<(u64,usize,String)>{
    let required=outer_files().union(&framework_files()).cloned().collect::<BTreeSet<_>>();
    let optional=BTreeSet::from(["Contents/Resources/release/component-map.json".to_owned(),"Contents/Resources/release/disclosure-review.json".to_owned()]);
    let aliases=aliases();let mut dirs=BTreeSet::new();
    for name in required.iter().chain(optional.iter()).chain(aliases.keys()){
        for ancestor in Path::new(name).ancestors().skip(1).filter(|path|!path.as_os_str().is_empty()){dirs.insert(ancestor.to_string_lossy().into_owned());}
    }
    dirs.insert("Contents/Resources/companion".into());
    let mut pending=vec![app.to_path_buf()];let mut found=BTreeSet::new();let mut total=0u64;let mut count=0usize;
    while let Some(folder)=pending.pop(){
        for entry in fs::read_dir(&folder).map_err(|_|"Cannot inspect the complete browser app before removal.".to_owned())?{
            let path=entry.map_err(|_|"Cannot inspect browser app entry.".to_owned())?.path();
            let relative=path.strip_prefix(app).map_err(|_|"A browser app entry escaped its root.".to_owned())?.to_string_lossy().into_owned();
            let metadata=fs::symlink_metadata(&path).map_err(|_|"The browser app changed during review.".to_owned())?;
            if metadata.mode()&0o022!=0 && !metadata.file_type().is_symlink() || ![0,unsafe{libc::geteuid()}].contains(&metadata.uid()){return Err("The browser app contains unsafe-owner files. No files were removed.".into());}
            if metadata.file_type().is_symlink(){
                let expected=aliases.get(&relative).ok_or("The browser app contains an unexpected link. Move extra files elsewhere before uninstalling.")?;
                if fs::read_link(&path).map_err(|_|"Cannot inspect browser app link.".to_owned())?!=PathBuf::from(expected)||!fs::canonicalize(&path).map_err(|_|"The Sparkle runtime link is unavailable.".to_owned())?.starts_with(app.join("Contents/Frameworks/Sparkle.framework")){return Err("The Sparkle runtime link changed. No app files were removed.".into());}
                found.insert(relative);continue;
            }
            if metadata.is_dir(){
                if !dirs.contains(&relative){return Err("The browser app contains an unknown folder. Move extra files elsewhere before uninstalling.".into());}
                if relative!="Contents/Resources/companion"{pending.push(path);}continue;
            }
            if !metadata.is_file()||metadata.nlink()!=1||(!required.contains(&relative)&&!optional.contains(&relative)){return Err("The browser app contains extra or linked files. Move extra files elsewhere before uninstalling.".into());}
            count+=1;total=total.checked_add(metadata.len()).ok_or("The browser app exceeds its size limit.")?;
            if count>1024||total>1024*1024*1024{return Err("The browser app exceeds its removal bound.".into());}found.insert(relative);
        }
    }
    if !required.iter().all(|name|found.contains(name))||!aliases.keys().all(|name|found.contains(name)){return Err("The browser app is incomplete. Reinstall it before removing it; saved data is kept.".into());}
    let mut digest=Sha256::new();
    for name in ["Contents/_CodeSignature/CodeResources","Contents/Info.plist","Contents/Resources/browser-host-manifest.json"]{digest.update(bytes(&app.join(name),2*1024*1024)?);}
    Ok((total,count,format!("{:x}",digest.finalize())))
}
#[cfg(test)]mod tests{use super::*;#[test]fn fixed_ancestor_selection_never_accepts_other_nesting(){assert_eq!(outer_path(Path::new("/Applications/PhotoFindry.app/Contents/Resources/companion/bin/photofindry")),Some(PathBuf::from("/Applications/PhotoFindry.app")));for value in ["/Applications/PhotoFindry.app/Contents/Resources/other/bin/photofindry","/Applications/PhotoFindry.app/Contents/Resources/companion/bin/other","/Applications/PhotoFindry/Contents/Resources/companion/bin/photofindry","/tmp/package/bin/photofindry"]{assert_eq!(outer_path(Path::new(value)),None);}}}
