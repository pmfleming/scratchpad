//! Safe Windows file identity comparison using full 128-bit file IDs.
#![cfg(windows)]

use std::{fs::File, io, path::Path};

/// Compares physical identity while both file handles remain open.
///
/// Returns an error if either path cannot be opened or its filesystem does not
/// provide a supported file identifier.
pub fn same_file(left: &Path, right: &Path) -> io::Result<bool> {
    let left = open(left)?;
    let right = open(right)?;
    compare(&left, &right)
}

fn open(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

fn compare(left: &File, right: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    fn identity(file: &File) -> io::Result<FILE_ID_INFO> {
        let mut info = FILE_ID_INFO::default();
        // SAFETY: the borrowed File owns a live handle; info is writable,
        // correctly aligned, and exactly the size required by FileIdInfo.
        let success = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                (&raw mut info).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.FileId.Identifier == [0; 16] || info.FileId.Identifier == [255; 16] {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "filesystem did not provide a file identifier",
            ));
        }
        Ok(info)
    }
    // Full 128-bit IDs support ReFS; unsupported filesystems yield uncertainty.
    // https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_id_info
    let left_id = identity(left)?;
    let right_id = identity(right)?;
    Ok(left_id.VolumeSerialNumber == right_id.VolumeSerialNumber
        && left_id.FileId.Identifier == right_id.FileId.Identifier)
}
