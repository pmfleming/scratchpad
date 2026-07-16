use super::super::stock_editor_palette_for_selection;
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, EditorAppearanceSource, FileOpenDisposition, IndentationStyle,
    NewTabPlacement, StartupSessionBehavior, TabKeyBehavior, color_to_hex,
};
use eframe::egui;

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

    pub(super) fn set_editor_font(&mut self, editor_font: EditorFontPreset) -> bool {
        replace_if_changed(&mut self.editor.editor_font, editor_font)
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

    pub(super) fn set_tab_key_behavior(&mut self, behavior: TabKeyBehavior) -> bool {
        replace_if_changed(&mut self.editor.tab_key_behavior, behavior)
    }

    pub(super) fn set_indentation_style(&mut self, style: IndentationStyle) -> bool {
        replace_if_changed(&mut self.editor.indentation_style, style)
    }

    pub(super) fn set_show_tab_characters(&mut self, visible: bool) -> bool {
        replace_if_changed(&mut self.editor.show_tab_characters, visible)
    }

    pub(super) fn apply_theme_mode_preset(
        &mut self,
        theme_mode: AppThemeMode,
        system_theme: Option<egui::Theme>,
    ) -> bool {
        let (text_color, background_color) =
            stock_editor_palette_for_selection(theme_mode, system_theme);
        if self.editor.theme_mode == theme_mode
            && self.editor.editor_text_color == text_color
            && self.editor.editor_background_color == background_color
        {
            return false;
        }

        self.editor.theme_mode = theme_mode;
        text_color.clone_into(&mut self.editor.editor_text_color);
        background_color.clone_into(&mut self.editor.editor_background_color);
        true
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
