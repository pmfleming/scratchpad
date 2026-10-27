use super::{BufferState, FileController, SavedBufferFixture, wait_for_background_io};
use crate::app::{
    app_state::{PendingReloadBufferAction, PendingReloadMode},
    services::background_io::LoadedPathResult,
};

#[test]
fn pending_loads_only_replace_the_requested_unchanged_buffer() {
    for mode in [
        PendingReloadMode::ExplicitReload,
        PendingReloadMode::AutoRefreshCleanBuffer,
        PendingReloadMode::ReopenWithEncoding,
    ] {
        for case in [
            "success",
            "anchor-only",
            "edit",
            "edit-then-clean",
            "rename",
            "closed",
            "new-baseline",
            "error",
            "wrong-path",
            "empty",
            "extra",
        ] {
            let mut fixture = SavedBufferFixture::new("tracked.txt", "disk", "memory");
            let buffer = fixture.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
            let action = PendingReloadBufferAction::new(buffer, mode).unwrap();
            match case {
                "anchor-only" => {
                    let snapshot = buffer.document_snapshot();
                    let tree = buffer.document_mut().piece_tree_mut();
                    tree.create_anchor(0, crate::app::domain::AnchorBias::Left);
                    assert_eq!(tree.generation(), snapshot.revision());
                }
                "edit" | "edit-then-clean" => {
                    buffer.replace_text("new edit".into());
                    buffer.is_dirty = case == "edit";
                }
                "rename" => buffer.set_path(Some(fixture.path.with_extension("renamed"))),
                "new-baseline" => buffer.disk_state = None,
                _ => {}
            }
            if case == "closed" {
                fixture.app.tab_manager.set_tabs(Vec::new(), 0);
            }
            let result = || LoadedPathResult {
                path: if case == "wrong-path" {
                    fixture.path.with_extension("wrong")
                } else {
                    fixture.path.clone()
                },
                disk_state: action.previous_disk_state.clone(),
                result: if case == "error" {
                    Err("read failed".into())
                } else {
                    Ok(BufferState::new(
                        "tracked.txt".into(),
                        "disk".into(),
                        Some(fixture.path.clone()),
                    ))
                },
            };
            let results = match case {
                "empty" => vec![],
                "extra" => vec![result(), result()],
                _ => vec![result()],
            };
            let id = action.buffer_id;
            FileController::apply_async_reload_buffer_result(&mut fixture.app, action, results);
            if case == "closed" {
                assert!(
                    fixture
                        .app
                        .tab_manager
                        .tabs
                        .as_slice()
                        .iter()
                        .all(|tab| tab.buffer_by_id(id).is_none())
                );
                continue;
            }
            let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
            let expected = match case {
                "success" | "anchor-only" => "disk",
                "edit" | "edit-then-clean" => "new edit",
                _ => "memory",
            };
            assert_eq!(buffer.text(), expected, "{mode:?}/{case}");
            if case == "success" {
                assert!(!buffer.is_dirty);
            }
            assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "disk");
        }
    }
}

#[test]
fn queued_reload_and_encoding_reopen_preserve_intervening_edits() {
    for encoding in [false, true] {
        for edit in [false, true] {
            let mut fixture = SavedBufferFixture::new("tracked.txt", "disk", "memory");
            let queue = |app: &mut super::ScratchpadApp| {
                if encoding {
                    FileController::reopen_buffer_with_encoding(app, 0, "UTF-8")
                } else {
                    FileController::reload_buffer_from_disk(app, 0)
                }
            };
            assert!(queue(&mut fixture.app));
            assert!(queue(&mut fixture.app), "pending request is deduplicated");
            if edit {
                let buffer = fixture.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
                buffer.replace_text("new edit".into());
                buffer.is_dirty = true;
            }
            wait_for_background_io(&mut fixture.app);
            let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
            assert_eq!(buffer.text(), if edit { "new edit" } else { "disk" });
            assert_eq!(buffer.is_dirty, edit);
        }
    }
}
