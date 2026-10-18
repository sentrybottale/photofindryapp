//! Explicit candidate inspection produces an ephemeral metadata-free image.
use std::{fs::{self,File,OpenOptions},io::Read,os::unix::fs::{DirBuilderExt,MetadataExt,OpenOptionsExt},path::{Path,PathBuf},process::Stdio,time::Duration};
use tokio::process::Command;
static SLOT:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(1);
const MAX_IMAGE:u64=8*1024*1024;
type Result<T>=std::result::Result<T,String>;
struct Temporary(PathBuf);
impl Drop for Temporary { fn drop(&mut self) { let _=fs::remove_file(self.0.join("preview.jpg")); let _=fs::remove_dir(&self.0); } }
fn identity(path:&Path)->Result<(u64,u64,u64,i64,i64)> {
    let metadata=fs::metadata(path).map_err(|_|"The possible original is unavailable.")?;
    if !metadata.is_file() { return Err("The possible original is not a regular photograph.".into()); }
    Ok((metadata.dev(),metadata.ino(),metadata.len(),metadata.mtime(),metadata.mtime_nsec()))
}
pub async fn prepare(runtime:&Path,path:&Path)->Result<Vec<u8>> {
    let _slot=SLOT.try_acquire().map_err(|_|"Another original-location preview is being prepared. Try again shortly.")?;
    if !path.is_absolute() { return Err("The saved original-location candidate is invalid.".into()); }
    let before=identity(path)?;
    let mut random=[0u8;16];getrandom::fill(&mut random).map_err(|_|"Cannot prepare a private candidate preview.")?;
    let suffix:String=random.iter().map(|byte|format!("{byte:02x}")).collect();
    let temporary=Temporary(std::env::temp_dir().join(format!("photofindry-candidate-{suffix}")));
    fs::DirBuilder::new().mode(0o700).create(&temporary.0).map_err(|_|"Cannot create the private candidate preview.")?;
    let output=temporary.0.join("preview.jpg");
    let mut child=Command::new(runtime.join("PhotoHelper")).arg("preparePreview").arg(path).arg(&output).arg("1024")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true)
        .spawn().map_err(|_|"The local photograph decoder is unavailable.")?;
    match tokio::time::timeout(Duration::from_secs(30),child.wait()).await {
        Ok(Ok(status)) if status.success()=>{},
        Ok(_)=>return Err("The possible original could not be decoded as a photograph.".into()),
        Err(_)=>{let _=child.kill().await;return Err("The original-location preview took too long. Reconnect its source and try again.".into());},
    }
    if identity(path)?!=before { return Err("The possible original changed while its preview was prepared. Try again.".into()); }
    let file:File=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK).open(output)
        .map_err(|_|"The candidate preview was not produced.")?;
    let metadata=file.metadata().map_err(|_|"Cannot inspect the candidate preview.")?;
    if !metadata.is_file()||metadata.len()>MAX_IMAGE { return Err("The candidate preview exceeds its image bound.".into()); }
    let mut bytes=Vec::new();file.take(MAX_IMAGE+1).read_to_end(&mut bytes).map_err(|_|"Cannot read the candidate preview.")?;
    if bytes.len() as u64>MAX_IMAGE || !bytes.starts_with(&[0xff,0xd8,0xff]) {return Err("The candidate preview is invalid.".into());}
    Ok(bytes)
}

#[cfg(test)] mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires the prepared native PhotoHelper; run explicitly against the qualified runtime"]
    async fn native_candidate_preview_decodes_fresh_pixels_and_preserves_input() {
        let project=Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let runtime=std::env::var_os("PHOTOFINDRY_COMPANION_RUNTIME").map(PathBuf::from)
            .unwrap_or_else(||project.join("dist/PhotoFindry.app/Contents/Resources/runtime"));
        assert!(runtime.join("PhotoHelper").is_file(),"Prepare the native runtime first");
        let source=project.join("Resources/BrandMark.png");let before=fs::read(&source).unwrap();
        let bytes=prepare(&runtime,&source).await.unwrap();
        assert!(bytes.starts_with(&[0xff,0xd8,0xff]));assert!(bytes.len() as u64<=MAX_IMAGE);
        assert_eq!(fs::read(source).unwrap(),before);
    }
    #[tokio::test]
    async fn invalid_candidate_never_starts_a_decoder() {
        assert!(prepare(Path::new("/missing-runtime"),Path::new("relative.jpg")).await.is_err());
        assert!(prepare(Path::new("/missing-runtime"),Path::new("/" )).await.is_err());
    }
}
