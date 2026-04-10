use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{RenderedLayout, ViewId, WorkspaceTab};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::*;
use crate::app::ui::editor_content::{self, EditorContentOutcome};
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use eframe::egui;

struct TileBodyOutcome {
    changed: bool,
    focused: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn render_tile(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    rect: egui::Rect,
    is_active: bool,
    can_close: bool,
    actions: &mut Vec<TileAction>,
    any_editor_changed: &mut bool,
    preview_overlay: &mut Option<SplitPreviewOverlay>,
) {
    ui.scope_builder(tile_ui_builder(rect), |ui| {
        handle_tile_click(ui, rect, tab_index, view_id, actions);
        paint_tile_frame(ui, rect, is_active);

        let body_outcome = render_tile_body(ui, app, tab_index, view_id, rect);
        *any_editor_changed |= body_outcome.changed;
        apply_tile_body_focus(body_outcome.focused, is_active, view_id, actions);
        tile_header::render_tile_header(
            ui,
            app,
            tab_index,
            view_id,
            rect,
            can_close,
            actions,
            preview_overlay,
        );
    });
}

fn render_tile_body(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    rect: egui::Rect,
) -> TileBodyOutcome {
    ui.scope_builder(tile_ui_builder(rect), |ui| {
        let editor_font_id = editor_font_id(app.font_size());
        let scroll_bar_visibility = editor_scroll_bar_visibility(ui.ctx());
        let request_focus = app.should_focus_view(view_id);
        let word_wrap = app.word_wrap();
        let editor_gutter = app.editor_gutter();
        let tab = &mut app.tabs_mut()[tab_index];
        let previous_layout = take_previous_layout(tab, view_id);
        let outcome = show_editor_scroll_area(
            ui,
            editor_gutter,
            tab,
            tab_index,
            view_id,
            word_wrap,
            &editor_font_id,
            previous_layout.as_ref(),
            request_focus,
            scroll_bar_visibility,
        );
        restore_previous_layout_if_needed(tab, view_id, previous_layout);
        if request_focus {
            app.consume_focus_request(view_id);
        }

        TileBodyOutcome {
            changed: outcome.changed,
            focused: outcome.focused,
        }
    })
    .inner
}

fn tile_ui_builder(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn handle_tile_click(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    tab_index: usize,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    let tile_response = ui.interact(
        rect,
        ui.make_persistent_id(("tile", tab_index, view_id)),
        egui::Sense::click(),
    );
    if tile_response.clicked() {
        actions.push(TileAction::Activate(view_id));
    }
}

fn paint_tile_frame(ui: &egui::Ui, rect: egui::Rect, is_active: bool) {
    let bg = if is_active { HEADER_BG } else { EDITOR_BG };
    let border_color = if is_active {
        egui::Color32::LIGHT_BLUE
    } else {
        BORDER
    };

    ui.painter().rect_filled(rect, 4.0, bg);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
}

fn apply_tile_body_focus(
    body_focused: bool,
    is_active: bool,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if body_focused && !is_active {
        actions.push(TileAction::Activate(view_id));
    }
}

fn editor_font_id(font_size: f32) -> egui::FontId {
    egui::FontId::new(font_size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

fn editor_scroll_bar_visibility(ctx: &egui::Context) -> egui::scroll_area::ScrollBarVisibility {
    if tab_drag::has_tab_drag_for_context(ctx) {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    }
}

fn take_previous_layout(tab: &mut WorkspaceTab, view_id: ViewId) -> Option<RenderedLayout> {
    tab.view_mut(view_id)
        .and_then(|view| view.latest_layout.take())
}

#[allow(clippy::too_many_arguments)]
fn show_editor_scroll_area(
    ui: &mut egui::Ui,
    editor_gutter: u8,
    tab: &mut WorkspaceTab,
    tab_index: usize,
    view_id: ViewId,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
    previous_layout: Option<&RenderedLayout>,
    request_focus: bool,
    scroll_bar_visibility: egui::scroll_area::ScrollBarVisibility,
) -> EditorContentOutcome {
    egui::ScrollArea::both()
        .id_salt(("editor_scroll", tab_index, view_id))
        .auto_shrink([false, false])
        .scroll_bar_visibility(scroll_bar_visibility)
        .show(ui, |ui| {
            render_editor_body_content(
                ui,
                editor_gutter,
                tab,
                view_id,
                previous_layout,
                request_focus,
                word_wrap,
                editor_font_id,
            )
        })
        .inner
}

#[allow(clippy::too_many_arguments)]
fn render_editor_body_content(
    ui: &mut egui::Ui,
    editor_gutter: u8,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    previous_layout: Option<&RenderedLayout>,
    request_focus: bool,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> EditorContentOutcome {
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        editor_content::render_editor_content(
            ui,
            editor_gutter,
            buffer,
            view,
            previous_layout,
            request_focus,
            word_wrap,
            editor_font_id,
        )
    } else {
        missing_editor_content_outcome()
    }
}

fn restore_previous_layout_if_needed(
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    previous_layout: Option<RenderedLayout>,
) {
    if tab
        .view(view_id)
        .is_some_and(|view| view.latest_layout.is_none())
        && let Some(view) = tab.view_mut(view_id)
    {
        view.latest_layout = previous_layout;
    }
}

fn missing_editor_content_outcome() -> EditorContentOutcome {
    EditorContentOutcome {
        changed: false,
        focused: false,
    }
}
