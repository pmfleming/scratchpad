use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{
    AppCommand, FileCommand, SearchCommand, SettingsCommand, WorkspaceCommand,
};
use crate::app::domain::{SplitAxis, ViewId};
use eframe::egui;

const DEFAULT_SPLIT_RATIO: f32 = 0.5;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_global_shortcuts(app, ctx);
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    handle_tile_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

fn handle_global_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !crate::app::app_state::settings_state::showing_settings(app)
        && handle_region_traversal_shortcut(app, ctx)
    {
        return;
    }

    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenUserManual));
        return;
    }

    if !crate::app::app_state::settings_state::showing_settings(app)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::F))
    {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Open));
        ctx.request_repaint();
        return;
    }

    if !crate::app::app_state::settings_state::showing_settings(app)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::H))
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::OpenAndReplace),
        );
        ctx.request_repaint();
        return;
    }

    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Comma)) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::OpenSettings),
        );
        return;
    }

    if crate::app::app_state::settings_state::showing_settings(app)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::CloseSettings),
        );
        return;
    }

    if app.state.search_state.open()
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Close));
        ctx.request_repaint();
    }
}

fn handle_region_traversal_shortcut(app: &mut ScratchpadApp, ctx: &egui::Context) -> bool {
    let direction = ctx.input_mut(|input| {
        if input.consume_key(egui::Modifiers::NONE, egui::Key::F6) {
            Some(1)
        } else if input.consume_key(egui::Modifiers::SHIFT, egui::Key::F6) {
            Some(-1)
        } else {
            None
        }
    });
    let Some(direction) = direction else {
        return false;
    };
    let Some(next_view_id) = next_view_for_region_traversal(app, direction) else {
        return true;
    };

    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateView {
            view_id: next_view_id,
        }),
    );
    true
}

fn next_view_for_region_traversal(app: &ScratchpadApp, direction: i32) -> Option<ViewId> {
    let tab = app.tab_manager.active_tab()?;
    let ordered = tab.ordered_view_ids_in_layout_order();
    if ordered.len() <= 1 {
        return None;
    }

    let current = ordered
        .iter()
        .position(|view_id| *view_id == tab.layout.active_view_id)
        .unwrap_or(0);
    let next = next_region_index(current, ordered.len(), direction)?;
    ordered.get(next).copied()
}

fn next_region_index(current: usize, len: usize, direction: i32) -> Option<usize> {
    if len <= 1 {
        return None;
    }
    Some(if direction < 0 {
        current.checked_sub(1).unwrap_or(len - 1)
    } else {
        (current + 1) % len
    })
}

fn handle_file_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let tile_file_modifiers = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };

    if ctx.input_mut(|input| input.consume_key(tile_file_modifiers, egui::Key::O)) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFileHere));
        return;
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
        crate::app::commands::handle_command(app, AppCommand::Workspace(WorkspaceCommand::NewTab));
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFile));
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::SaveFile));
    }
}

fn handle_view_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| {
        input.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
            || input.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
    }) {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() + 1.0,
        );
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)) {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() - 1.0,
        );
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Num0))
        && let Some(tab) = app.tab_manager.active_tab_mut()
    {
        let next_visible = !tab.layout.line_numbers_visible();
        tab.layout.set_line_numbers_visible(next_visible);
        app.tab_manager.mark_session_dirty();
    }
}

fn handle_tab_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W)) {
        if crate::app::app_state::settings_state::showing_settings(app) {
            crate::app::commands::handle_command(
                app,
                AppCommand::Settings(SettingsCommand::CloseSettings),
            );
        } else if !app.tab_manager.tabs.as_slice().is_empty() {
            crate::app::commands::handle_command(
                app,
                AppCommand::Workspace(WorkspaceCommand::RequestCloseTab {
                    index: app.tab_manager.active_tab_index,
                }),
            );
        }
    }
}

fn handle_tile_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let modifiers = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };

    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::T))
        && let Some(tab) = app.tab_manager.active_tab()
        && crate::app::domain::tab::summary::can_promote_view(tab, tab.layout.active_view_id)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::PromoteViewToTab {
                view_id: tab.layout.active_view_id,
            }),
        );
        return;
    }

    if ctx.input_mut(|input| input.consume_key(modifiers, egui::Key::T))
        && let Some(tab) = app.tab_manager.active_tab()
        && crate::app::domain::tab::summary::can_promote_all_files(tab)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::PromoteTabFilesToTabs {
                index: app.tab_manager.active_tab_index,
            }),
        );
        return;
    }

    if ctx.input_mut(|input| input.consume_key(modifiers, egui::Key::W))
        && let Some(tab) = app.tab_manager.active_tab()
        && tab.layout.root_pane.leaf_count() > 1
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::CloseView {
                view_id: tab.layout.active_view_id,
            }),
        );
        return;
    }

    let split = ctx.input_mut(|input| {
        if input.consume_key(modifiers, egui::Key::ArrowUp) {
            Some((SplitAxis::Horizontal, true))
        } else if input.consume_key(modifiers, egui::Key::ArrowDown) {
            Some((SplitAxis::Horizontal, false))
        } else if input.consume_key(modifiers, egui::Key::ArrowLeft) {
            Some((SplitAxis::Vertical, true))
        } else if input.consume_key(modifiers, egui::Key::ArrowRight) {
            Some((SplitAxis::Vertical, false))
        } else {
            None
        }
    });

    if let Some((axis, new_view_first)) = split {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio: DEFAULT_SPLIT_RATIO,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::next_region_index;

    #[test]
    fn f6_traversal_wraps_forward() {
        assert_eq!(next_region_index(2, 3, 1), Some(0));
    }

    #[test]
    fn shift_f6_traversal_wraps_backward() {
        assert_eq!(next_region_index(0, 3, -1), Some(2));
    }
}
