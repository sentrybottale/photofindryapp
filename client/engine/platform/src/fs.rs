//! Native filesystem mechanisms. Unix callers retain the standard-library types
//! and traits; Windows callers use explicit handle identities, never invented
//! Unix device/inode numbers or permission bits.

#[cfg(unix)]
pub use std::fs::*;
#[cfg(unix)]
pub use std::os::unix::fs::{
    DirBuilderExt, FileExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt,
};

#[cfg(windows)]
#[path = "fs_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::*;
