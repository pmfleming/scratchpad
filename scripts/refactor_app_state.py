import sys

with open(r'C:\Code\scratchpad\src\app\app_state.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Remove from struct
content = content.replace('''    pub(crate) tab_manager: TabManager,
    pub(crate) font_size: f32,
    pub(crate) word_wrap: bool,
    pub(crate) logging_enabled: bool,
    pub(crate) editor_gutter: u8,
    pub(crate) editor_font: EditorFontPreset,
    pub(crate) app_settings: AppSettings,''', '''    pub(crate) tab_manager: TabManager,
    pub(crate) app_settings: AppSettings,''')

# 2. Remove from with_stores_and_startup
content = content.replace('''        let mut app = Self {
            tab_manager: TabManager::default(),
            font_size: app_settings.font_size,
            word_wrap: app_settings.word_wrap,
            logging_enabled: app_settings.logging_enabled,
            editor_gutter: app_settings.editor_gutter,
            editor_font: app_settings.editor_font,
            app_settings,''', '''        let mut app = Self {
            tab_manager: TabManager::default(),
            app_settings,''')

# 3. Update getters
content = content.replace('''    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn editor_font(&self) -> EditorFontPreset {
        self.editor_font
    }

    pub fn editor_gutter(&self) -> u8 {
        self.editor_gutter
    }''', '''    pub fn font_size(&self) -> f32 {
        self.app_settings.font_size
    }

    pub fn editor_font(&self) -> EditorFontPreset {
        self.app_settings.editor_font
    }

    pub fn editor_gutter(&self) -> u8 {
        self.app_settings.editor_gutter
    }''')

content = content.replace('''    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    pub fn logging_enabled(&self) -> bool {
        self.logging_enabled
    }''', '''    pub fn word_wrap(&self) -> bool {
        self.app_settings.word_wrap
    }

    pub fn logging_enabled(&self) -> bool {
        self.app_settings.logging_enabled
    }''')

# 4. apply_settings
content = content.replace('''    fn apply_settings(&mut self, settings: AppSettings) {
        self.font_size = settings.font_size;
        self.word_wrap = settings.word_wrap;
        self.logging_enabled = settings.logging_enabled;
        self.editor_gutter = settings.editor_gutter;
        self.editor_font = settings.editor_font;
        self.active_surface = if settings.settings_tab_open {''', '''    fn apply_settings(&mut self, settings: AppSettings) {
        self.active_surface = if settings.settings_tab_open {''')

# 5. refresh_settings_snapshot
content = content.replace('''    fn refresh_settings_snapshot(&mut self) {
        self.app_settings.font_size = self.font_size;
        self.app_settings.word_wrap = self.word_wrap;
        self.app_settings.logging_enabled = self.logging_enabled;
        self.app_settings.editor_gutter = self.editor_gutter;
        self.app_settings.editor_font = self.editor_font;
        self.app_settings.settings_tab_open = self.settings_tab_open();''', '''    fn refresh_settings_snapshot(&mut self) {
        self.app_settings.settings_tab_open = self.settings_tab_open();''')

# 6. Setters
content = content.replace('''    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.font_size = next;''', '''    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.app_settings.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.font_size = next;''')

content = content.replace('''    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        if self.editor_font == editor_font {
            return;
        }

        self.editor_font = editor_font;''', '''    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        if self.app_settings.editor_font == editor_font {
            return;
        }

        self.app_settings.editor_font = editor_font;''')

content = content.replace('''    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        if self.word_wrap == enabled {
            return;
        }

        self.word_wrap = enabled;''', '''    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        if self.app_settings.word_wrap == enabled {
            return;
        }

        self.app_settings.word_wrap = enabled;''')

content = content.replace('''    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        if self.editor_gutter == next {
            return;
        }

        self.editor_gutter = next;''', '''    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        if self.app_settings.editor_gutter == next {
            return;
        }

        self.app_settings.editor_gutter = next;''')

content = content.replace('''    pub(crate) fn set_logging_enabled(&mut self, enabled: bool) {
        if self.logging_enabled == enabled {
            return;
        }

        self.logging_enabled = enabled;''', '''    pub(crate) fn set_logging_enabled(&mut self, enabled: bool) {
        if self.app_settings.logging_enabled == enabled {
            return;
        }

        self.app_settings.logging_enabled = enabled;''')

# 7. Other uses
content = content.replace('''    pub(crate) fn log_event(&self, level: LogLevel, message: impl Into<String>) {
        if self.logging_enabled {''', '''    pub(crate) fn log_event(&self, level: LogLevel, message: impl Into<String>) {
        if self.app_settings.logging_enabled {''')

content = content.replace('''    fn set_status(&mut self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        self.status_message = Some(message.clone());
        if self.logging_enabled {''', '''    fn set_status(&mut self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        self.status_message = Some(message.clone());
        if self.app_settings.logging_enabled {''')

content = content.replace('''    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.applied_editor_font == Some(self.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.editor_font) {
            self.set_warning_status(format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                self.editor_font.label()
            ));
        }
        self.applied_editor_font = Some(self.editor_font);
    }''', '''    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.applied_editor_font == Some(self.app_settings.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.app_settings.editor_font) {
            self.set_warning_status(format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                self.app_settings.editor_font.label()
            ));
        }
        self.applied_editor_font = Some(self.app_settings.editor_font);
    }''')

with open(r'C:\Code\scratchpad\src\app\app_state.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("Replacement complete")