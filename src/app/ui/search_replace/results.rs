//! A flat, virtualized list of matches on History's rail.
use super::result_model::{empty_message, match_preview};
use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{
    ScratchpadApp, SearchFocusTarget, SearchResultEntry, SearchResultGroup, SearchStatus,
};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme;
use crate::app::ui::{panel_style, widget_ids};
use eframe::egui;

pub(super) const ROW_HEIGHT: f32 = 44.0;
const RAIL_X: f32 = 11.0;
const DOT_Y: f32 = 12.5;
const ICON_X: f32 = 26.0;
const TITLE_X: f32 = 50.0;
const META_WIDTH: f32 = 72.0;
const TITLE_SIZE: f32 = 13.0;
const SUBTITLE_SIZE: f32 = 11.0;
/// Characters of context kept before a hit so it stays visible in narrow windows.
const LEADING_CONTEXT_CHARS: usize = 24;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default)]
struct Scroll {
    revealed: Option<usize>,
    offset: f32,
    viewport: f32,
}

fn scroll_key() -> egui::Id {
    egui::Id::unique("search.results.scroll")
}

pub(super) fn row_count(groups: &[SearchResultGroup]) -> usize {
    groups
        .last()
        .map_or(0, |group| group.first_match_index + group.total_match_count)
}

pub(super) fn group_for(
    groups: &[SearchResultGroup],
    match_index: usize,
) -> Option<&SearchResultGroup> {
    let index = groups
        .partition_point(|group| group.first_match_index <= match_index)
        .checked_sub(1)?;
    groups
        .get(index)
        .filter(|group| match_index < group.first_match_index + group.total_match_count)
}

pub(super) fn reviewing(app: &ScratchpadApp) -> bool {
    app.state.search_state.panel.replace_open
        && app.state.search_state.runtime.status == SearchStatus::Ready
}

pub(super) fn list_focus_id() -> egui::Id {
    egui::Id::unique("search.results.focus")
}

/// ↑ ↓ move, Home/End jump, Enter goes to the match and Space toggles inclusion.
/// ↑ on the first row returns to Find.
fn keyboard(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    total: usize,
    replacing: bool,
    actions: &mut SearchStripActions,
) {
    if total == 0 || !ui.memory(|m| m.has_focus(list_focus_id())) {
        return;
    }
    let consume = |modifiers, key| ui.input_mut(|i| i.consume_key(modifiers, key));
    let active = state.active_match_index.filter(|&i| i < total);
    let last = total - 1;
    if consume(egui::Modifiers::NONE, egui::Key::ArrowDown) {
        actions.preview_match_index = Some(active.map_or(0, |i| (i + 1).min(last)));
    }
    if consume(egui::Modifiers::NONE, egui::Key::ArrowUp) {
        match active {
            Some(i) if i > 0 => actions.preview_match_index = Some(i - 1),
            _ => actions.focus_find = true,
        }
    }
    if consume(egui::Modifiers::NONE, egui::Key::Home) {
        actions.preview_match_index = Some(0);
    }
    if consume(egui::Modifiers::NONE, egui::Key::End) {
        actions.preview_match_index = Some(last);
    }
    if consume(egui::Modifiers::NONE, egui::Key::Enter) {
        actions.selected_match_index = Some(active.unwrap_or(0));
    }
    if replacing && let Some(index) = active {
        // Shift first: an unmodified shortcut also matches Shift.
        let whole_file = consume(egui::Modifiers::SHIFT, egui::Key::Space);
        if whole_file || consume(egui::Modifiers::NONE, egui::Key::Space) {
            toggle_inclusion(app, &state.result_groups, index, whole_file);
        }
    }
}

fn toggle_inclusion(
    app: &mut ScratchpadApp,
    groups: &[SearchResultGroup],
    match_index: usize,
    whole_file: bool,
) {
    let included = !app
        .state
        .search_state
        .replace
        .excluded
        .contains(&match_index);
    let indices = match group_for(groups, match_index).filter(|_| whole_file) {
        Some(group) => group.first_match_index..group.first_match_index + group.total_match_count,
        None => match_index..match_index + 1,
    };
    set_included(app, indices, !included);
}

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
) {
    if let Some(message) = empty_message(state) {
        ui.add_space(ui.available_height() * 0.3);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(message)
                    .size(TITLE_SIZE)
                    .color(theme::text_muted(ui)),
            );
        });
        return;
    }
    let total = row_count(&state.result_groups);
    let mut scroll = ui
        .ctx()
        .data(|d| d.get_temp::<Scroll>(scroll_key()))
        .unwrap_or_default();
    let mut area = egui::ScrollArea::vertical()
        .id_salt(widget_ids::scroll_id(ui, "search_results_list"))
        .auto_shrink([false, false]);
    if let Some(active) = state.active_match_index
        && scroll.revealed != Some(active)
    {
        scroll.revealed = Some(active);
        let top = active as f32 * ROW_HEIGHT;
        if top < scroll.offset || top + ROW_HEIGHT > scroll.offset + scroll.viewport {
            area =
                area.vertical_scroll_offset((top - (scroll.viewport - ROW_HEIGHT) * 0.5).max(0.0));
        }
    }
    let replacing = reviewing(app);
    keyboard(ui, app, state, total, replacing, actions);
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            area.show_rows(ui, ROW_HEIGHT, total, |ui, range| {
                let rendered = range.len();
                for match_index in range {
                    show_row(ui, app, state, match_index, replacing, actions);
                }
                crate::app::capacity_metrics::record_search_result_groups(
                    rendered,
                    total - rendered,
                );
            })
        })
        .inner;
    scroll.offset = output.state.clamped_offset().y;
    scroll.viewport = output.inner_rect.height();
    ui.ctx().data_mut(|d| d.insert_temp(scroll_key(), scroll));
    // Focus-only, so rows keep their clicks; like History's list focus.
    let focus = widget_ids::interact(
        ui,
        output.inner_rect,
        list_focus_id(),
        egui::Sense::focusable_noninteractive(),
        "search-results-focus",
    );
    focus.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, "Search results"));
    state.sync_focus(&focus, SearchFocusTarget::Results);
    if focus.has_focus() {
        panel_style::lock_navigation_focus(ui.ctx(), list_focus_id());
    }
}

fn show_row(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    match_index: usize,
    replacing: bool,
    actions: &mut SearchStripActions,
) {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), ROW_HEIGHT));
    let Some(entry) = app.search_result_entry_at(match_index) else {
        return;
    };
    let group = group_for(&state.result_groups, match_index);
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("search.results.row", match_index)),
        egui::Sense::click(),
        "search-result-row",
    );
    let file = group.map_or(entry.buffer_label.as_str(), |g| g.buffer_label.as_str());
    let label = format!(
        "Line {} · {} · {}",
        entry.line_number,
        match_preview(&entry),
        file
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, entry.active, &label)
    });
    if entry.active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0,
            if entry.active {
                theme::tab_selected_bg(ui)
            } else {
                theme::action_bg(ui)
            },
        );
    }
    let included = !app
        .state
        .search_state
        .replace
        .excluded
        .contains(&match_index);
    let dot = if entry.active {
        panel_style::RailDot::Current
    } else if !replacing || included {
        panel_style::RailDot::Filled
    } else {
        panel_style::RailDot::Hollow
    };
    panel_style::rail(ui, rect, rect.left() + RAIL_X, rect.top() + DOT_Y, dot);

    let title_left = if replacing {
        panel_style::text(
            ui,
            egui::Rect::from_min_size(rect.min + egui::vec2(ICON_X, 0.0), egui::vec2(20.0, 25.0)),
            egui_phosphor::regular::SWAP,
            14.0,
            if included {
                theme::tab_selected_accent(ui)
            } else {
                theme::text_muted(ui)
            },
        );
        TITLE_X
    } else {
        ICON_X
    };
    let meta_left = rect.right() - META_WIDTH;
    panel_style::text_right(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(meta_left, rect.top()),
            egui::pos2(rect.right() - 8.0, rect.top() + 25.0),
        ),
        &format!("Ln {}", entry.line_number),
        SUBTITLE_SIZE,
        theme::text_muted(ui),
    );
    let title_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(title_left, 2.0),
        egui::pos2(meta_left - 6.0, rect.top() + 23.0),
    );
    let job = if replacing {
        replacement_title(ui, app, match_index, included)
    } else {
        hit_title(ui, app, &entry)
    };
    paint_job(ui, title_rect, job);
    let subtitle = if replacing && !included {
        format!("Excluded · {file}")
    } else {
        file.to_owned()
    };
    panel_style::text_end(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(title_left, 22.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 2.0),
        ),
        &subtitle,
        SUBTITLE_SIZE,
        theme::text_muted(ui),
    );

    if replacing {
        let dot_rect = egui::Rect::from_min_size(rect.min, egui::vec2(ICON_X, ROW_HEIGHT));
        let toggle = widget_ids::interact(
            ui,
            dot_rect,
            egui::Id::unique(("search.results.include", match_index)),
            egui::Sense::click(),
            "search-result-include",
        );
        toggle.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::CheckBox,
                true,
                included,
                format!("Include match {}", match_index + 1),
            )
        });
        let toggle = toggle.on_hover_text(
            "Include this match in Replace all. Shift+click toggles every match in this file.",
        );
        if toggle.clicked() {
            let whole_file = ui.input(|i| i.modifiers.shift);
            toggle_inclusion(app, &state.result_groups, match_index, whole_file);
        }
    }
    let response = response.on_hover_text(match group {
        Some(group) if group.tab_label != group.buffer_label => {
            format!("{}\nTab: {}", group.buffer_label, group.tab_label)
        }
        _ => file.to_owned(),
    });
    if response.clicked() {
        actions.selected_match_index = Some(entry.match_index);
    }
}

pub(super) fn set_included(
    app: &mut ScratchpadApp,
    indices: std::ops::Range<usize>,
    included: bool,
) {
    let excluded = &mut app.state.search_state.replace.excluded;
    for index in indices {
        if included {
            excluded.remove(&index);
        } else {
            excluded.insert(index);
        }
    }
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = None;
}

fn editor_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

fn append(job: &mut egui::text::LayoutJob, text: &str, format: egui::TextFormat) {
    if !text.is_empty() {
        job.append(text, 0.0, format);
    }
}

/// The line preview with the hit emphasized; leading context is shortened to keep it visible.
fn hit_title(
    ui: &egui::Ui,
    app: &ScratchpadApp,
    entry: &SearchResultEntry,
) -> egui::text::LayoutJob {
    let preview = match_preview(entry);
    let plain = egui::TextFormat::simple(editor_font(TITLE_SIZE), theme::text_primary(ui));
    let mut job = egui::text::LayoutJob::default();
    let matched = app
        .state
        .search_state
        .results
        .matches
        .get(entry.match_index)
        .and_then(|m| {
            crate::app::app_state::search_state::replace::matched_text_for_search_match(app, m)
        });
    let Some((start, end)) = matched.and_then(|text| hit_span(preview, &text, entry.column_number))
    else {
        append(&mut job, preview, plain);
        return job;
    };
    let before = &preview[..start];
    let skip = before.chars().count().saturating_sub(LEADING_CONTEXT_CHARS);
    if skip > 0 {
        let cut = before.char_indices().nth(skip).map_or(start, |(i, _)| i);
        append(&mut job, "…", plain.clone());
        append(&mut job, &before[cut..], plain.clone());
    } else {
        append(&mut job, before, plain.clone());
    }
    append(
        &mut job,
        &preview[start..end],
        egui::TextFormat {
            color: theme::tab_selected_accent(ui),
            background: theme::tab_selected_bg(ui),
            ..plain.clone()
        },
    );
    append(&mut job, &preview[end..], plain);
    job
}

/// Byte span of the hit in a trimmed preview. Trimming only removes leading text, so
/// the hit is the last occurrence that starts at or before its 1-based line column.
pub(super) fn hit_span(preview: &str, matched: &str, column: usize) -> Option<(usize, usize)> {
    if matched.is_empty() || matched.contains('\n') {
        return None;
    }
    let column_byte = preview
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(preview.len(), |(i, _)| i);
    let start = preview
        .match_indices(matched)
        .map(|(i, _)| i)
        .take_while(|&i| i <= column_byte)
        .last()?;
    Some((start, start + matched.len()))
}

fn replacement_title(
    ui: &egui::Ui,
    app: &ScratchpadApp,
    match_index: usize,
    included: bool,
) -> egui::text::LayoutJob {
    use crate::app::app_state::search_state::replace;
    let font = editor_font(TITLE_SIZE);
    let muted = egui::TextFormat::simple(font.clone(), theme::text_muted(ui));
    let mut job = egui::text::LayoutJob::default();
    let matched = app.state.search_state.results.matches.get(match_index);
    let old = matched.and_then(|m| replace::matched_text_for_search_match(app, m));
    let new = matched.and_then(|m| replace::replacement_for_match(app, m).ok());
    let (Some(old), Some(new)) = (old, new) else {
        append(&mut job, "Preview unavailable", muted);
        return job;
    };
    let (old_format, new_format) = if included {
        (
            egui::TextFormat {
                strikethrough: egui::Stroke::new(1.0, theme::diff_del(ui)),
                ..egui::TextFormat::simple(font.clone(), theme::diff_del(ui))
            },
            egui::TextFormat::simple(font, theme::diff_add(ui)),
        )
    } else {
        (muted.clone(), muted.clone())
    };
    append(&mut job, &old.replace('\n', "⏎"), old_format);
    append(&mut job, " → ", muted);
    append(&mut job, &new.replace('\n', "⏎"), new_format);
    job
}

fn paint_job(ui: &egui::Ui, rect: egui::Rect, mut job: egui::text::LayoutJob) {
    if rect.width() <= 0.0 {
        return;
    }
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(pos, galley, theme::text_primary(ui));
}
