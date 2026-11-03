use crate::app::domain::BufferState;

pub(crate) fn buffer_label(buffer: &BufferState) -> String {
    buffer
        .path
        .as_ref()
        .map_or_else(|| buffer.name.clone(), |path| path.display().to_string())
}
