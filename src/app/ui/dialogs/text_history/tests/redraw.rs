use super::*;

#[test]
fn distant_history_edits_stay_visible_while_history_or_scrubber_keeps_focus() {
    for mode in ["This file", "Timeline", "Scrubber"] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app(root.path());
        let mut buffer = BufferState::new("long.txt".into(), "unchanged line\n".repeat(240), None);
        let end = buffer.document().piece_tree().len_chars();
        let cursor = CursorRange::one(crate::app::domain::CharCursor::new(end));
        for insert in ["before", "after"] {
            let offset = buffer.document().piece_tree().len_chars();
            buffer
                .replace_char_ranges_with_undo(&[(offset..offset, insert.into())], cursor, cursor)
                .unwrap();
        }
        buffer.take_text_history_event();
        app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
        if mode == "Scrubber" {
            app.state.dialogs.text_history.close();
            app.state.dialogs.history_scrubber.open();
        }
        let mut h = harness(app);
        h.run_steps(3);
        let focus = if mode == "Scrubber" {
            scrubber::focus_id()
        } else {
            focus_id()
        };
        if mode == "Scrubber" {
            h.get_by_label("Previous change (←)").click();
        } else if mode == "Timeline" {
            h.get_by_label("Timeline").click();
            h.run_steps(3);
            h.key_press(egui::Key::End);
        } else {
            h.key_press(egui::Key::ArrowLeft);
        }
        h.run_steps(5);
        let tab = h.state().tab_manager.active_tab().unwrap();
        assert_eq!(tab.active_buffer().document().history_now(), 2);
        assert!(text(h.state()).ends_with("before"));
        assert!(h.ctx.memory(|m| m.has_focus(focus)));
        assert!(!tab.layout.active_view().unwrap().editor_has_focus);
        assert_eq!(tab.layout.active_view().unwrap().cursor_reveal_mode(), None);
        assert!(h.output().platform_output.ime.is_none());
        assert!(
            visible_document_text(&h),
            "Tile went blank during history navigation ({mode})"
        );
        h.key_press(egui::Key::Escape);
        h.run_steps(5);
        assert!(visible_document_text(&h));
    }
}

fn visible_document_text(h: &Harness<'_, ScratchpadApp>) -> bool {
    h.output().shapes.iter().any(|s| {
        matches!(&s.shape, egui::Shape::Text(t)
            if t.pos.x < 400.0 && t.galley.text().contains("unchanged line")
            && t.galley.rect.translate(t.pos.to_vec2()).intersects(s.clip_rect))
    })
}
