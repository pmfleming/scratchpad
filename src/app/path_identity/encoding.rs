//! Lossless native paths in existing TEXT columns and exported session JSON.
//! UTF-8 paths retain their old representation. NUL cannot occur in a valid OS
//! pathname, so the tagged representation cannot collide with a real filename.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    io,
    path::{Path, PathBuf},
};

const PREFIX: &str = "\0scratchpad-native-path-v1:";
const HEX: &[u8; 16] = b"0123456789abcdef";

pub(crate) fn encode(path: &Path) -> String {
    if let Some(text) = path.to_str() {
        return text.to_owned();
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut text = format!("{PREFIX}unix:");
        for byte in path.as_os_str().as_bytes() {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 15)]));
        }
        text
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut text = format!("{PREFIX}windows:");
        for unit in path.as_os_str().encode_wide() {
            for shift in [12, 8, 4, 0] {
                text.push(char::from(HEX[usize::from((unit >> shift) & 15)]));
            }
        }
        text
    }
}

pub(crate) fn decode(text: &str) -> io::Result<PathBuf> {
    if !text.contains('\0') {
        return Ok(text.into());
    }
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid or foreign native path encoding",
        )
    };
    let tagged = text.strip_prefix(PREFIX).ok_or_else(bad)?;
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let hex = tagged.strip_prefix("unix:").ok_or_else(bad)?;
        if !hex.is_ascii() || hex.len() % 2 != 0 {
            return Err(bad());
        }
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad()))
            .collect::<io::Result<Vec<_>>>()?;
        if bytes.contains(&0) {
            return Err(bad());
        }
        Ok(OsString::from_vec(bytes).into())
    }
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        let hex = tagged.strip_prefix("windows:").ok_or_else(bad)?;
        if !hex.is_ascii() || hex.len() % 4 != 0 {
            return Err(bad());
        }
        let units = (0..hex.len())
            .step_by(4)
            .map(|i| u16::from_str_radix(&hex[i..i + 4], 16).map_err(|_| bad()))
            .collect::<io::Result<Vec<_>>>()?;
        if units.contains(&0) {
            return Err(bad());
        }
        Ok(OsString::from_wide(&units).into())
    }
}

#[allow(clippy::ref_option)] // Signature required by serde's `with` adapter.
pub(crate) fn serialize<S: Serializer>(
    path: &Option<PathBuf>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    path.as_deref().map(encode).serialize(serializer)
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PathBuf>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|s| decode(&s).map_err(serde::de::Error::custom))
        .transpose()
}

/// Serde adapter for a required pathname (recent-file settings).
pub(crate) mod required {
    use super::{Deserialize, Deserializer, Path, PathBuf, Serializer, decode, encode};
    pub(crate) fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(path))
    }
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<PathBuf, D::Error> {
        decode(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_text_and_invalid_encodings() {
        assert_eq!(decode("/tmp/hello").unwrap(), Path::new("/tmp/hello"));
        for invalid in [
            "\0oops",
            "\0scratchpad-native-path-v1:unix:zz",
            "\0scratchpad-native-path-v1:unix:00",
        ] {
            assert!(decode(invalid).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_paths_round_trip_without_display_collisions() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let a = PathBuf::from(OsString::from_vec(b"/tmp/file-\xff".to_vec()));
        let b = PathBuf::from(OsString::from_vec(b"/tmp/file-\xfe".to_vec()));
        assert_eq!(a.to_string_lossy(), b.to_string_lossy());
        assert_ne!(encode(&a), encode(&b));
        assert_eq!(decode(&encode(&a)).unwrap(), a);
        assert_eq!(decode(&encode(&b)).unwrap(), b);
    }
    #[cfg(windows)]
    #[test]
    fn unpaired_surrogates_round_trip() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_wide(&[67, 58, 92, 0xd800]));
        assert_eq!(decode(&encode(&path)).unwrap(), path);
    }
}
