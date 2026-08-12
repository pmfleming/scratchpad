use crate::app::startup::{StartupOpenTarget, StartupOptions};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const PROTOCOL_VERSION: u16 = 1;
pub(crate) const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const FLAG_RESTORE_SESSION: u8 = 1;
const FLAG_RESTORE_EXPLICIT: u8 = 1 << 1;
const FLAG_TARGET_EXPLICIT: u8 = 1 << 2;
const FLAG_ACTIVATE: u8 = 1 << 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchRequest {
    pub invocation_id: u128,
    pub sender_pid: u32,
    pub options: StartupOptions,
    pub activate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerResponse {
    Accepted,
    Busy,
    Rejected(String),
    UnsupportedProtocol,
}

impl LaunchRequest {
    pub fn from_startup_options(invocation_id: u128, options: StartupOptions) -> io::Result<Self> {
        Ok(Self {
            invocation_id,
            sender_pid: std::process::id(),
            options: normalize_startup_paths(options)?,
            activate: true,
        })
    }

    pub fn validate_for_existing_primary(&self) -> Result<(), String> {
        if !self.options.restore_session && self.options.restore_session_explicit {
            return Err(
                "/clean cannot be applied while Scratchpad is already running. Close the existing window and try again."
                    .to_owned(),
            );
        }
        Ok(())
    }

    pub(super) fn encode(&self) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        body.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        body.extend_from_slice(&self.invocation_id.to_le_bytes());
        body.extend_from_slice(&self.sender_pid.to_le_bytes());
        let mut flags = 0;
        flags |= u8::from(self.options.restore_session) * FLAG_RESTORE_SESSION;
        flags |= u8::from(self.options.restore_session_explicit) * FLAG_RESTORE_EXPLICIT;
        flags |= u8::from(self.options.open_target_explicit) * FLAG_TARGET_EXPLICIT;
        flags |= u8::from(self.activate) * FLAG_ACTIVATE;
        body.push(flags);
        encode_target(self.options.open_target, &mut body)?;
        encode_optional_string(self.options.startup_notice.as_deref(), &mut body)?;
        write_u32(self.options.files.len(), &mut body)?;
        for path in &self.options.files {
            encode_path(path, &mut body)?;
        }
        bounded_frame(body)
    }

    pub(super) fn decode(frame: &[u8]) -> io::Result<Self> {
        let body = decode_frame_body(frame)?;
        let mut reader = Reader::new(body);
        let version = reader.read_u16()?;
        if version != PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported broker protocol version {version}"),
            ));
        }
        let invocation_id = reader.read_u128()?;
        let sender_pid = reader.read_u32()?;
        let flags = reader.read_u8()?;
        let open_target = decode_target(&mut reader)?;
        let startup_notice = decode_optional_string(&mut reader)?;
        let file_count = reader.read_u32()? as usize;
        let mut files = Vec::with_capacity(file_count.min(1024));
        for _ in 0..file_count {
            files.push(decode_path(&mut reader)?);
        }
        reader.finish()?;
        Ok(Self {
            invocation_id,
            sender_pid,
            options: StartupOptions {
                restore_session: flags & FLAG_RESTORE_SESSION != 0,
                restore_session_explicit: flags & FLAG_RESTORE_EXPLICIT != 0,
                open_target,
                open_target_explicit: flags & FLAG_TARGET_EXPLICIT != 0,
                files,
                startup_notice,
            },
            activate: flags & FLAG_ACTIVATE != 0,
        })
    }
}

impl BrokerResponse {
    pub(super) fn encode(&self) -> io::Result<Vec<u8>> {
        let mut body = vec![match self {
            Self::Accepted => 0,
            Self::Busy => 1,
            Self::Rejected(_) => 2,
            Self::UnsupportedProtocol => 3,
        }];
        if let Self::Rejected(reason) = self {
            encode_string(reason, &mut body)?;
        }
        bounded_frame(body)
    }

    pub(super) fn decode(frame: &[u8]) -> io::Result<Self> {
        let body = decode_frame_body(frame)?;
        let mut reader = Reader::new(body);
        let response = match reader.read_u8()? {
            0 => Self::Accepted,
            1 => Self::Busy,
            2 => Self::Rejected(reader.read_string()?),
            3 => Self::UnsupportedProtocol,
            value => return Err(invalid_data(format!("unknown broker response {value}"))),
        };
        reader.finish()?;
        Ok(response)
    }
}

fn bounded_frame(body: Vec<u8>) -> io::Result<Vec<u8>> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(invalid_data("broker frame exceeds the size limit"));
    }
    let mut frame = Vec::with_capacity(body.len() + 4);
    write_u32(body.len(), &mut frame)?;
    frame.extend_from_slice(&body);
    Ok(frame)
}

fn decode_frame_body(frame: &[u8]) -> io::Result<&[u8]> {
    if frame.len() < 4 {
        return Err(invalid_data("broker frame is truncated"));
    }
    let declared = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if declared > MAX_FRAME_BYTES || declared != frame.len() - 4 {
        return Err(invalid_data("broker frame length is invalid"));
    }
    Ok(&frame[4..])
}

fn encode_target(target: StartupOpenTarget, output: &mut Vec<u8>) -> io::Result<()> {
    match target {
        StartupOpenTarget::SeparateTabs => output.push(0),
        StartupOpenTarget::ActiveTab => output.push(1),
        StartupOpenTarget::TabIndex(index) => {
            output.push(2);
            write_u64(index, output)?;
        }
    }
    Ok(())
}

fn decode_target(reader: &mut Reader<'_>) -> io::Result<StartupOpenTarget> {
    match reader.read_u8()? {
        0 => Ok(StartupOpenTarget::SeparateTabs),
        1 => Ok(StartupOpenTarget::ActiveTab),
        2 => Ok(StartupOpenTarget::TabIndex(reader.read_u64()? as usize)),
        value => Err(invalid_data(format!("unknown launch target {value}"))),
    }
}

fn normalize_startup_paths(mut options: StartupOptions) -> io::Result<StartupOptions> {
    let current_dir = std::env::current_dir()?;
    for path in &mut options.files {
        if path.is_relative() {
            *path = current_dir.join(&*path);
        }
    }
    Ok(options)
}

fn encode_optional_string(value: Option<&str>, output: &mut Vec<u8>) -> io::Result<()> {
    if let Some(value) = value {
        output.push(1);
        encode_string(value, output)
    } else {
        output.push(0);
        Ok(())
    }
}

fn decode_optional_string(reader: &mut Reader<'_>) -> io::Result<Option<String>> {
    match reader.read_u8()? {
        0 => Ok(None),
        1 => reader.read_string().map(Some),
        value => Err(invalid_data(format!("invalid optional string tag {value}"))),
    }
}

fn encode_string(value: &str, output: &mut Vec<u8>) -> io::Result<()> {
    write_bytes(value.as_bytes(), output)
}

fn write_bytes(value: &[u8], output: &mut Vec<u8>) -> io::Result<()> {
    write_u32(value.len(), output)?;
    output.extend_from_slice(value);
    Ok(())
}

#[cfg(unix)]
fn encode_path(path: &Path, output: &mut Vec<u8>) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    write_bytes(path.as_os_str().as_bytes(), output)
}

#[cfg(unix)]
fn decode_path(reader: &mut Reader<'_>) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(OsString::from_vec(
        reader.read_bytes()?.to_vec(),
    )))
}

#[cfg(windows)]
fn encode_path(path: &Path, output: &mut Vec<u8>) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    write_u32(units.len(), output)?;
    for unit in units {
        output.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
}

#[cfg(windows)]
fn decode_path(reader: &mut Reader<'_>) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let count = reader.read_u32()? as usize;
    let byte_count = count
        .checked_mul(2)
        .ok_or_else(|| invalid_data("path length overflow"))?;
    let raw = reader.read_exact(byte_count)?;
    let units = raw
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

#[cfg(not(any(unix, windows)))]
fn encode_path(path: &Path, output: &mut Vec<u8>) -> io::Result<()> {
    encode_string(&path.to_string_lossy(), output)
}

#[cfg(not(any(unix, windows)))]
fn decode_path(reader: &mut Reader<'_>) -> io::Result<PathBuf> {
    reader.read_string().map(PathBuf::from)
}

fn write_u32(value: usize, output: &mut Vec<u8>) -> io::Result<()> {
    output.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| invalid_data("broker value exceeds u32"))?
            .to_le_bytes(),
    );
    Ok(())
}

fn write_u64(value: usize, output: &mut Vec<u8>) -> io::Result<()> {
    output.extend_from_slice(
        &u64::try_from(value)
            .map_err(|_| invalid_data("broker value exceeds u64"))?
            .to_le_bytes(),
    );
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn read_exact(&mut self, len: usize) -> io::Result<&'a [u8]> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or_else(|| invalid_data("broker frame cursor overflow"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| invalid_data("broker frame is truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        let bytes = self.read_exact(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_u128(&mut self) -> io::Result<u128> {
        let bytes = self.read_exact(16)?;
        let mut array = [0_u8; 16];
        array.copy_from_slice(bytes);
        Ok(u128::from_le_bytes(array))
    }

    fn read_bytes(&mut self) -> io::Result<&'a [u8]> {
        let len = self.read_u32()? as usize;
        self.read_exact(len)
    }

    fn read_string(&mut self) -> io::Result<String> {
        String::from_utf8(self.read_bytes()?.to_vec())
            .map_err(|error| invalid_data(error.to_string()))
    }

    fn finish(self) -> io::Result<()> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid_data("broker frame has trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BrokerResponse, LaunchRequest};
    use crate::app::startup::{StartupOpenTarget, StartupOptions};
    use std::path::PathBuf;

    #[test]
    fn request_round_trip_preserves_launch_fields() {
        let request = LaunchRequest {
            invocation_id: 42,
            sender_pid: 7,
            options: StartupOptions {
                restore_session: true,
                restore_session_explicit: false,
                open_target: StartupOpenTarget::TabIndex(3),
                open_target_explicit: true,
                files: vec![PathBuf::from("/tmp/one.txt")],
                startup_notice: Some("notice".to_owned()),
            },
            activate: true,
        };
        let encoded = request.encode().unwrap();
        assert_eq!(LaunchRequest::decode(&encoded).unwrap(), request);
    }

    #[test]
    fn response_round_trip_preserves_rejection_reason() {
        let response = BrokerResponse::Rejected("close first".to_owned());
        assert_eq!(
            BrokerResponse::decode(&response.encode().unwrap()).unwrap(),
            response
        );
    }

    #[test]
    fn clean_request_is_rejected_for_existing_primary() {
        let request = LaunchRequest {
            invocation_id: 1,
            sender_pid: 2,
            options: StartupOptions::clean(),
            activate: true,
        };
        assert!(request.validate_for_existing_primary().is_err());
    }

    #[test]
    fn malformed_frame_is_rejected() {
        assert!(LaunchRequest::decode(&[1, 0, 0, 0, 0]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_unix_path_round_trips() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', 0xff,
        ]));
        let request = LaunchRequest {
            invocation_id: 8,
            sender_pid: 9,
            options: StartupOptions {
                files: vec![path],
                ..StartupOptions::default()
            },
            activate: true,
        };
        assert_eq!(
            LaunchRequest::decode(&request.encode().unwrap()).unwrap(),
            request
        );
    }
}
