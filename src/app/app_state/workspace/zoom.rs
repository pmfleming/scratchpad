use crate::app::{app_state::ScratchpadApp, domain::ViewId};

#[derive(Clone, Copy)]
pub(crate) enum ZoomChange {
    In,
    Out,
    Reset,
    Scale(f32),
    FontPoints(f32),
}

pub(crate) fn active_view(app: &mut ScratchpadApp, change: ZoomChange) -> bool {
    if crate::app::app_state::settings_state::showing_settings(app) {
        return false;
    }
    let Some(id) = app
        .tab_manager
        .active_tab()
        .map(|tab| tab.layout.active_view_id())
    else {
        return false;
    };
    apply(app, id, change)
}

pub(crate) fn apply(app: &mut ScratchpadApp, view_id: ViewId, change: ZoomChange) -> bool {
    let base_size = app.state.app_settings.font_size();
    let Some((tab_index, view)) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .enumerate()
        .find_map(|(index, tab)| tab.layout.view_mut(view_id).map(|view| (index, view)))
    else {
        return false;
    };
    let factor = view.zoom_factor();
    let next = match change {
        ZoomChange::In => factor + 0.1,
        ZoomChange::Out => factor - 0.1,
        ZoomChange::Reset => 1.0,
        ZoomChange::Scale(scale) => factor * scale,
        ZoomChange::FontPoints(points) => factor + points / base_size,
    };
    if !next.is_finite() || !view.set_zoom_factor(next) {
        return false;
    }
    let factor = view.zoom_factor();
    // Cold tabs persist their saved shell until hydration finishes. Keep its
    // presentation metadata in sync without loading file content on the UI thread.
    if let Some(cold) = app.tab_manager.cold_session_tabs.get_mut(&tab_index)
        && let Some(view) = cold.shell.views.iter_mut().find(|view| view.id == view_id)
    {
        view.zoom_factor = factor;
    }
    app.tab_manager.mark_session_dirty();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        app_state::test_support::test_app,
        domain::{BufferState, SplitAxis, WorkspaceTab},
    };

    #[test]
    fn zoom_while_a_cold_tab_loads_survives_persistence_and_stale_hydration() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        let tab = WorkspaceTab::new(BufferState::new("note".into(), "text".into(), None));
        let view_id = tab.layout.active_view_id();
        let cold = crate::app::services::session_store::cold_tab_from_workspace_tab(&tab);
        let hydration = tab.clone();
        app.tab_manager.set_tabs(vec![tab], 0);
        app.tab_manager.set_cold_session_tab(0, cold);
        apply(&mut app, view_id, ZoomChange::Scale(1.5));
        assert_eq!(
            app.tab_manager.cold_session_tabs()[&0].shell.views[0].zoom_factor,
            1.5
        );
        app.tab_manager.replace_restored_tab(0, hydration);
        assert_eq!(
            app.tab_manager
                .active_tab()
                .unwrap()
                .layout
                .view(view_id)
                .unwrap()
                .zoom_factor(),
            1.5
        );
    }

    #[test]
    fn split_inherits_zoom_then_changes_and_resets_independently() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        let mut tab = WorkspaceTab::new(BufferState::new("note".into(), "text".into(), None));
        let first = tab.layout.active_view_id();
        tab.layout.view_mut(first).unwrap().set_zoom_factor(1.25);
        let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
        assert_eq!(tab.layout.view(second).unwrap().zoom_factor(), 1.25);
        app.tab_manager
            .set_tabs(vec![tab, WorkspaceTab::untitled()], 0);
        let settings = app.state.app_settings.clone();
        apply(&mut app, second, ZoomChange::Scale(2.0));
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0]
                .layout
                .view(first)
                .unwrap()
                .zoom_factor(),
            1.25
        );
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0]
                .layout
                .view(second)
                .unwrap()
                .zoom_factor(),
            2.5
        );
        apply(&mut app, second, ZoomChange::Reset);
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0]
                .layout
                .view(second)
                .unwrap()
                .zoom_factor(),
            1.0
        );
        assert_eq!(
            app.tab_manager.tabs.as_slice()[1]
                .layout
                .active_view()
                .unwrap()
                .zoom_factor(),
            1.0
        );
        assert_eq!(app.state.app_settings, settings);
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
            "text"
        );
    }
}
