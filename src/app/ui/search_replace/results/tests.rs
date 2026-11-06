use super::*;
use crate::app::{
    app_state::test_support::test_app,
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use egui_kittest::{Harness, kittest::Queryable};

struct State {
    app: ScratchpadApp,
    groups: Vec<SearchResultGroup>,
    cull: bool,
    offset: f32,
    height: f32,
    rendered: usize,
    focused: Option<usize>,
}

fn harness(cull: bool, offset: f32) -> (tempfile::TempDir, Harness<'static, State>) {
    let root = tempfile::tempdir().unwrap();
    let state = State {
        app: test_app(root.path()),
        cull,
        offset,
        height: 0.0,
        rendered: 0,
        focused: None,
        groups: (0..200)
            .map(|index| SearchResultGroup {
                tab_index: index,
                buffer_id: index as u64 + 1,
                buffer_label: if index == 50 {
                    "長い名前λ".repeat(100)
                } else {
                    format!("file-{index}")
                },
                tab_label: format!("file-{index}"),
                first_match_index: index * 3,
                total_match_count: 3,
                active: index == 0,
            })
            .collect(),
    };
    let harness = Harness::builder()
        .with_size(egui::vec2(620.0, 360.0))
        .with_context_setup(|ctx| {
            apply_editor_fonts(
                ctx,
                &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
                4.0,
            )
            .unwrap();
        })
        .build_ui_state(
            |ui, state: &mut State| {
                let mut actions = SearchStripActions::default();
                let mut height = None;
                let mut rendered = 0;
                let output = egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .vertical_scroll_offset(state.offset)
                    .show(ui, |ui| {
                        for (index, group) in state.groups.iter().enumerate() {
                            let (measured, painted) = show_result_group(
                                ui,
                                &mut state.app,
                                index,
                                group,
                                None,
                                &mut actions,
                                if state.cull { height } else { None },
                            );
                            height = measured.or(height);
                            rendered += usize::from(painted);
                            if index + 1 < state.groups.len() {
                                ui.add_space(SEARCH_RESULT_FILE_SPACING);
                            }
                        }
                    });
                state.height = output.content_size.y;
                state.rendered = rendered;
                if actions.focused_file_match_index.is_some() {
                    state.focused = actions.focused_file_match_index;
                }
            },
            state,
        );
    (root, harness)
}

#[test]
fn culling_preserves_scroll_extent_with_unicode_labels_and_expanded_groups() {
    for offset in [0.0, 3000.0, 12000.0] {
        let (_root, mut culled) = harness(true, offset);
        let (_reference_root, mut reference) = harness(false, offset);
        for h in [&mut culled, &mut reference] {
            let expanded_id = search_result_group_expanded_id(&h.state().groups[20]);
            h.ctx
                .data_mut(|data| data.insert_persisted(expanded_id, true));
            h.run_steps(3);
        }
        assert!(
            (culled.state().height - reference.state().height).abs() < 1.0,
            "scroll extent differs at {offset}: {} vs {}",
            culled.state().height,
            reference.state().height
        );
        assert!(
            culled.state().rendered < 20,
            "rendered {} groups",
            culled.state().rendered
        );
        assert_eq!(reference.state().rendered, 200);
    }
}

#[test]
fn visible_group_remains_clickable_after_culling() {
    let (_root, mut h) = harness(true, 0.0);
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::Button, "file-1")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().focused, Some(3));
}
