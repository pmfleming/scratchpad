use std::fmt;

#[derive(Debug)]
pub(super) struct UnsupportedSchema {
    pub version: i64,
    pub application: i64,
}

impl fmt::Display for UnsupportedSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Session format version {} is incompatible with this build (expected version 12; application ID {:#x}).",
            self.version, self.application
        )
    }
}
impl std::error::Error for UnsupportedSchema {}
