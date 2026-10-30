use super::{render_file_actions, render_recently_closed_files};
use crate::app::{
    app_state::{
        ScratchpadApp, settings_controller,
        test_support::{test_app, wait_for_background_io},
        workspace::lifecycle,
    },
    domain::{BufferState, WorkspaceTab},
    services::{
        file_service::FileService,
        settings_store::{FileOpenDisposition, RecentlyClosedFile},
    },
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::time::{Duration, Instant};

#[test]
fn open_file_submenu_contains_closed_files_without_a_separate_menu_item() {
    for recent_files in [true, false] {
        for with_entry in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let mut app = test_app(root.path());
            settings_controller::set_recent_files_enabled(&mut app, recent_files);
            if with_entry {
                let mut buffer =
                    BufferState::new("closed note".into(), "retained text".into(), None);
                buffer.is_dirty = true;
                app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
                lifecycle::perform_close_tab(&mut app, 0);
            }
            let mut harness = Harness::builder()
                .with_context_setup(|ctx| {
                    let mut fonts = egui::FontDefinitions::default();
                    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
                    ctx.set_fonts(fonts);
                })
                .with_size(egui::vec2(800.0, 600.0))
                .build_ui_state(
                    |ui, app| {
                        ui.set_max_width(super::TAB_CONTEXT_MENU_WIDTH);
                        render_file_actions(ui, app, 0, Some(0), true, true, true);
                    },
                    app,
                );
            harness.run();
            harness.get_by_role_and_label(
                egui::Role::Button,
                if recent_files {
                    "Open File"
                } else {
                    "Open File Here"
                },
            );
            assert!(harness.query_by_label("Recently Closed").is_none());
            assert!(harness.query_by_label("closed note").is_none());
            // The first split-row caret belongs to Open File; Save follows it.
            harness
                .get_all_by_role_and_label(egui::Role::Button, egui_phosphor::regular::CARET_RIGHT)
                .next()
                .expect("Open File submenu caret")
                .click();
            harness.run_steps(2);
            let label = if with_entry {
                "closed note"
            } else {
                "No recently closed files"
            };
            wait_for_menu_entry(&mut harness, label);
            assert!(harness.query_by_label("Recently Closed").is_none());
            if with_entry {
                harness
                    .get_by_role_and_label(egui::Role::Button, label)
                    .click_accesskit();
                harness.run_steps(2);
                wait_for_background_io(harness.state_mut());
                assert!(
                    harness
                        .state()
                        .tab_manager
                        .tabs
                        .as_slice()
                        .iter()
                        .flat_map(WorkspaceTab::buffers)
                        .any(|buffer| buffer.text() == "retained text")
                );
            }
        }
    }
}

#[test]
fn recently_closed_menu_has_single_action_rows_and_reopens_retained_and_disk_entries() {
    for (disposition, missing) in [
        (FileOpenDisposition::NewTab, false),
        (FileOpenDisposition::CurrentTab, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        let other = root.path().join("other.txt");
        std::fs::write(&path, "disk").unwrap();
        std::fs::write(&other, "other disk").unwrap();
        let mut app = test_app(root.path());
        settings_controller::set_file_open_disposition(&mut app, disposition);
        let mut buffer = BufferState::new("note.txt".into(), "disk".into(), Some(path.clone()));
        buffer.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
        buffer.replace_text("unsaved".into());
        buffer.is_dirty = true;
        buffer.is_settings_file = true;
        let id = buffer.temp_id.clone();
        app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
        lifecycle::perform_close_tab(&mut app, 0);
        let disk = FileService::build_buffer_from_file_content(
            &path,
            FileService::read_file(&path).unwrap(),
            Some(FileService::read_disk_state(&path).unwrap()),
        );
        app.tab_manager.set_tabs(vec![WorkspaceTab::new(disk)], 0);
        app.state
            .recently_closed_files
            .push_back(RecentlyClosedFile {
                path: other.clone(),
                closed_at_unix_seconds: None,
            });
        if missing {
            std::fs::remove_file(&path).unwrap();
        }
        let mut harness = Harness::builder()
            .with_context_setup(|ctx| {
                let mut fonts = egui::FontDefinitions::default();
                egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
                ctx.set_fonts(fonts);
            })
            .with_size(egui::vec2(1000.0, 700.0))
            .build_ui_state(|ui, app| render_recently_closed_files(ui, app, 0), app);
        wait_for_menu_entry(&mut harness, "note.txt");
        assert_closed_menu_layout(&harness, missing);
        harness
            .get_by_role_and_label(egui::Role::Button, "note.txt")
            .click_accesskit();
        harness.run_steps(2);
        wait_for_background_io(harness.state_mut());
        let app = harness.state();
        let recovered = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(WorkspaceTab::buffers)
            .find(|buffer| buffer.temp_id == id)
            .unwrap();
        assert_eq!(recovered.text(), "unsaved");
        assert_eq!(recovered.name, "note.txt (recovered)");
        assert!(recovered.path.is_none());
        assert!(recovered.path_key.is_none());
        assert!(!recovered.is_settings_file);
        assert!(recovered.disk_state.is_none());
        assert!(recovered.disk_content_digest.is_none());
        assert!(
            app.state
                .persistence
                .session_store
                .closed_tabs()
                .unwrap()
                .is_empty()
        );
        harness
            .get_by_role_and_label(egui::Role::Button, "other.txt")
            .click_accesskit();
        harness.run_steps(2);
        wait_for_background_io(harness.state_mut());
        assert!(
            harness
                .state()
                .tab_manager
                .find_tab_by_path(&other)
                .is_some()
        );
        if missing {
            assert!(!path.exists());
        } else {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "disk");
        }
    }
}

fn assert_closed_menu_layout(harness: &Harness<'_, ScratchpadApp>, missing: bool) {
    assert_eq!(
        harness
            .get_all_by_role_and_label(egui::Role::Button, "note.txt")
            .count(),
        1,
        "archive path must not be duplicated as disk-only entry"
    );
    assert_eq!(harness.get_all_by_role(egui::Role::Button).count(), 2);
    for removed in ["Pin", "Archived text", "Files on disk"] {
        assert!(harness.query_by_label(removed).is_none());
    }
    let painted = |label: &str| {
        harness
            .output()
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    Some(text.pos)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("missing painted label: {label}"))
    };
    assert_eq!(painted("note.txt").x, painted("other.txt").x);
    painted(egui_phosphor::regular::DOT);
    painted(if missing {
        egui_phosphor::regular::FILE_X
    } else {
        egui_phosphor::regular::FILE
    });
}

fn wait_for_menu_entry(harness: &mut Harness<'_, ScratchpadApp>, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.query_by_label(label).is_none() {
        assert!(Instant::now() < deadline, "menu did not show {label}");
        std::thread::sleep(Duration::from_millis(1));
        harness.run_steps(1);
    }
}
