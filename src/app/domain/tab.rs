use crate::app::domain::BufferState;

pub struct WorkspaceTab {
    pub buffer: BufferState,
}

impl WorkspaceTab {
    pub fn new(buffer: BufferState) -> Self {
        Self { buffer }
    }

    pub fn untitled() -> Self {
        Self::new(BufferState::new("Untitled".to_owned(), String::new(), None))
    }

    pub fn display_name(&self) -> String {
        self.buffer.display_name()
    }

    pub fn full_display_name(&self, has_duplicate: bool) -> String {
        let name = self.display_name();
        if has_duplicate && let Some(context) = self.overflow_context_label() {
            return format!("{} ({})", name, context);
        }
        name
    }

    pub fn overflow_context_label(&self) -> Option<String> {
        self.buffer.overflow_context_label()
    }
}
