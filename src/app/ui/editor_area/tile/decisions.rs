use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TileFocusRequest {
    ConsumeRequestedFocus,
    RequestEditorFocus,
    None,
}

pub(super) fn context_menu_attach_policy(
    editor_rect: Option<egui::Rect>,
    pointer_pos: Option<egui::Pos2>,
) -> bool {
    editor_rect.is_none_or(|rect| pointer_pos.is_none_or(|pos| !rect.contains(pos)))
}

pub(super) fn tile_focus_request(
    request_focus: bool,
    request_editor_focus: bool,
) -> TileFocusRequest {
    if request_focus {
        TileFocusRequest::ConsumeRequestedFocus
    } else if request_editor_focus {
        TileFocusRequest::RequestEditorFocus
    } else {
        TileFocusRequest::None
    }
}

pub(super) fn scrollbar_visibility_for_drag_active(
    drag_active: bool,
) -> egui::scroll_area::ScrollBarVisibility {
    if drag_active {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TileFocusRequest, context_menu_attach_policy, scrollbar_visibility_for_drag_active,
        tile_focus_request,
    };
    use eframe::egui;

    #[test]
    fn context_menu_attach_policy_skips_tile_menu_inside_editor_rect() {
        let editor_rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 60.0));

        assert!(!context_menu_attach_policy(
            Some(editor_rect),
            Some(egui::pos2(40.0, 30.0))
        ));
        assert!(context_menu_attach_policy(
            Some(editor_rect),
            Some(egui::pos2(100.0, 30.0))
        ));
        assert!(context_menu_attach_policy(Some(editor_rect), None));
        assert!(context_menu_attach_policy(
            None,
            Some(egui::pos2(40.0, 30.0))
        ));
    }

    #[test]
    fn tile_focus_request_prioritizes_consuming_existing_request() {
        assert_eq!(
            tile_focus_request(true, true),
            TileFocusRequest::ConsumeRequestedFocus
        );
        assert_eq!(
            tile_focus_request(false, true),
            TileFocusRequest::RequestEditorFocus
        );
        assert_eq!(tile_focus_request(false, false), TileFocusRequest::None);
    }

    #[test]
    fn scrollbar_visibility_hides_scrollbars_during_tab_drag() {
        assert_eq!(
            scrollbar_visibility_for_drag_active(true),
            egui::scroll_area::ScrollBarVisibility::AlwaysHidden
        );
        assert_eq!(
            scrollbar_visibility_for_drag_active(false),
            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
        );
    }
}
