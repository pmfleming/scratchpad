use super::super::stock_editor_palette;
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::{EditorFontPreset, EditorFontSource};
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, EditorAppearanceSource, FileOpenDisposition,
    IndentationStyle, NewTabPlacement, StartupSessionBehavior, TabDisplayMode, color_to_hex,
};
use eframe::egui;

fn system_font_family() -> String {
    crate::app::system_appearance::editor_font_family().unwrap_or_default()
}

#[derive(Clone, Copy, Default)]
pub(super) struct SettingEffects {
    pub invalidate_font: bool,
    pub invalidate_theme: bool,
    pub relayout: bool,
    pub apply_history_budget: bool,
}

impl SettingEffects {
    pub const PERSIST_ONLY: Self = Self {
        invalidate_font: false,
        invalidate_theme: false,
        relayout: false,
        apply_history_budget: false,
    };
    pub const FONT: Self = Self {
        invalidate_font: true,
        ..Self::PERSIST_ONLY
    };
    pub const THEME: Self = Self {
        invalidate_theme: true,
        ..Self::PERSIST_ONLY
    };
    pub const FONT_AND_THEME: Self = Self {
        invalidate_font: true,
        invalidate_theme: true,
        ..Self::PERSIST_ONLY
    };
    pub const RELAYOUT: Self = Self {
        relayout: true,
        ..Self::PERSIST_ONLY
    };
    pub const HISTORY_BUDGET: Self = Self {
        apply_history_budget: true,
        ..Self::PERSIST_ONLY
    };
}

impl AppSettings {
    pub(super) fn set_editor_appearance_source(&mut self, source: EditorAppearanceSource) -> bool {
        replace_if_changed(&mut self.editor.appearance_source, source)
    }

    pub(super) fn set_font_size(&mut self, font_size: f32) -> bool {
        let next = font_size.clamp(8.0, 72.0);
        if (self.editor.font_size - next).abs() < f32::EPSILON {
            return false;
        }
        self.editor.font_size = next;
        true
    }

    /// `None` selects the desktop's font.
    pub(super) fn set_editor_font(&mut self, choice: Option<EditorFontPreset>) -> bool {
        match choice {
            Some(preset) => {
                replace_if_changed(&mut self.editor.font_source, EditorFontSource::Scratchpad)
                    | replace_if_changed(&mut self.editor.editor_font, preset)
            }
            None => {
                replace_if_changed(&mut self.editor.font_source, EditorFontSource::Os)
                    | replace_if_changed(&mut self.editor.os_font_family, system_font_family())
            }
        }
    }

    /// Editing any appearance control while following the system starts a
    /// custom set from what is on screen, so only the edited value changes.
    /// The font selection it stores resolves to the same font, so nothing
    /// reloads. Returns whether it forked.
    pub(super) fn adopt_resolved_appearance(&mut self) -> bool {
        if !self.uses_system_editor_appearance() {
            return false;
        }

        let font_size = self.font_size();
        self.store_resolved_colors();
        self.editor.font_size = font_size.clamp(8.0, 72.0);
        self.editor.font_source = EditorFontSource::Os;
        self.editor.os_font_family = system_font_family();
        self.editor.appearance_source = EditorAppearanceSource::App;
        true
    }

    /// Leaving a preset keeps the colors on screen as the starting custom set.
    pub(super) fn freeze_palette(&mut self) -> bool {
        if self.editor.theme_mode == AppThemeMode::Custom {
            return false;
        }

        self.store_resolved_colors();
        true
    }

    fn store_resolved_colors(&mut self) {
        let text = color_to_hex(self.editor_text_color());
        let background = color_to_hex(self.editor_background_color());
        let highlight = color_to_hex(self.editor_text_highlight_color());
        let highlight_text = color_to_hex(self.editor_text_highlight_text_color());
        self.editor.editor_text_color = text;
        self.editor.editor_background_color = background;
        self.editor.editor_text_highlight_color = highlight;
        self.editor.editor_text_highlight_text_color = highlight_text;
        self.editor.theme_mode = AppThemeMode::Custom;
    }

    pub(super) fn set_word_wrap(&mut self, enabled: bool) -> bool {
        replace_if_changed(&mut self.editor.word_wrap, enabled)
    }

    pub(super) fn set_editor_gutter(&mut self, gutter: u8) -> bool {
        replace_if_changed(&mut self.editor.editor_gutter, gutter.min(32))
    }

    pub(super) fn set_editor_tab_width(&mut self, tab_width: u8) -> bool {
        replace_if_changed(&mut self.editor.editor_tab_width, tab_width.clamp(1, 16))
    }

    pub(super) fn set_indentation_style(&mut self, style: IndentationStyle) -> bool {
        replace_if_changed(&mut self.editor.indentation_style, style)
    }

    pub(super) fn set_tab_display(&mut self, mode: TabDisplayMode) -> bool {
        replace_if_changed(&mut self.editor.tab_display, mode)
    }

    /// A preset resets all three swatches; `Custom` keeps the colors on screen.
    pub(super) fn set_theme_mode(&mut self, theme_mode: AppThemeMode) -> bool {
        let Some((text, background)) = stock_editor_palette(
            theme_mode,
            crate::app::system_appearance::system_color_scheme(),
        ) else {
            return self.freeze_palette();
        };

        // Stored stock values keep the file self-consistent; presets resolve
        // their colors without reading them.
        replace_if_changed(&mut self.editor.theme_mode, theme_mode)
            | replace_if_changed(&mut self.editor.editor_text_color, text.to_owned())
            | replace_if_changed(
                &mut self.editor.editor_background_color,
                background.to_owned(),
            )
            | replace_if_changed(
                &mut self.editor.editor_text_highlight_color,
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR.to_owned(),
            )
            | replace_if_changed(
                &mut self.editor.editor_text_highlight_text_color,
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR.to_owned(),
            )
    }

    pub(super) fn set_editor_text_color(&mut self, color: egui::Color32) -> bool {
        self.set_editor_palette_color(color_to_hex(color), true)
    }

    pub(super) fn set_editor_background_color(&mut self, color: egui::Color32) -> bool {
        self.set_editor_palette_color(color_to_hex(color), false)
    }

    pub(super) fn set_editor_text_highlight_color(&mut self, color: egui::Color32) -> bool {
        let next = color_to_hex(color);
        let next_text = color_to_hex(crate::app::color_contrast::optimal_text_color(color));
        if self.editor.editor_text_highlight_color == next
            && self.editor.editor_text_highlight_text_color == next_text
        {
            return false;
        }

        self.editor.editor_text_highlight_color = next;
        self.editor.editor_text_highlight_text_color = next_text;
        true
    }

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) -> bool {
        let current = if is_text_color {
            &mut self.editor.editor_text_color
        } else {
            &mut self.editor.editor_background_color
        };
        replace_if_changed(current, next)
    }

    pub(super) fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) -> bool {
        replace_if_changed(&mut self.workspace.file_open_disposition, disposition)
    }

    pub(super) fn set_new_tab_placement(&mut self, placement: NewTabPlacement) -> bool {
        replace_if_changed(&mut self.workspace.new_tab_placement, placement)
    }

    pub(super) fn set_startup_session_behavior(
        &mut self,
        behavior: StartupSessionBehavior,
    ) -> bool {
        replace_if_changed(&mut self.workspace.startup_session_behavior, behavior)
    }

    pub(super) fn set_history_budget(&mut self, mut budget: TextHistoryBudget) -> bool {
        budget = budget.sanitized();
        if self.history.budget == budget {
            return false;
        }
        budget.derived_from_memory = false;
        self.history.budget = budget;
        true
    }

    pub(super) fn reset_history_budget_to_auto(&mut self) {
        self.history.budget = TextHistoryBudget::derive_from_available_memory();
    }
}

pub(super) fn replace_if_changed<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}
