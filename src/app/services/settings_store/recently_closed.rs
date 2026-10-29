use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

/// File-only fallback for a closed tab. Older settings stored just the path.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct RecentlyClosedFile {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at_unix_seconds: Option<i64>,
}

impl<'de> Deserialize<'de> for RecentlyClosedFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Record {
                path: PathBuf,
                closed_at_unix_seconds: Option<i64>,
            },
            Legacy(PathBuf),
        }
        Ok(match Stored::deserialize(deserializer)? {
            Stored::Record {
                path,
                closed_at_unix_seconds,
            } => Self {
                path,
                closed_at_unix_seconds,
            },
            Stored::Legacy(path) => Self {
                path,
                closed_at_unix_seconds: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::AppSettings;

    #[test]
    fn legacy_paths_and_timestamped_records_round_trip() {
        let settings: AppSettings = toml::from_str(
            "[workspace]\nrecently_closed_files = ['old.txt', { path = 'new.txt', closed_at_unix_seconds = 123 }]",
        ).unwrap();
        let entries = &settings.workspace.recently_closed_files;
        assert_eq!(entries[0].closed_at_unix_seconds, None);
        assert_eq!(entries[1].closed_at_unix_seconds, Some(123));
        let restored: AppSettings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert_eq!(settings, restored);
    }
}
