use super::{render_file_actions, render_recently_closed_files};
use crate::app::{
    app_state::{
        settings_controller,
        test_support::{test_app, wait_for_background_io},
        workspace::lifecycle,
    },
    domain::{BufferState, WorkspaceTab},
    services::{file_service::FileService, settings_store::FileOpenDisposition},
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::time::{Duration, Instant};

#[test]
fn recently_closed_row_follows_open_file_and_matches_menu_styling() {
    for recent_files in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        settings_controller::set_recent_files_enabled(&mut app, recent_files);
        let mut harness = Harness::builder()
            .with_context_setup(|ctx| {
                let mut fonts = egui::FontDefinitions::default();
                egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
                ctx.set_fonts(fonts);
            })
            .with_size(egui::vec2(700.0, 600.0))
            .build_ui_state(
                |ui, app| {
                    ui.set_max_width(super::TAB_CONTEXT_MENU_WIDTH);
                    render_file_actions(ui, app, 0, Some(0), true, true, true);
                },
                app,
            );
        harness.run();
        let text = |label: &str| {
            harness
                .output()
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == label
                    {
                        Some(text)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("missing painted text: {label}"))
        };
        let open = text(if recent_files {
            "Open File"
        } else {
            "Open File Here"
        });
        let recent = text("Recently Closed");
        let rename = text("Rename");
        assert!(open.pos.y < recent.pos.y && recent.pos.y < rename.pos.y);
        assert_eq!(recent.pos.x, open.pos.x);
        assert_eq!(recent.pos.x, rename.pos.x);
        assert_eq!(
            recent.galley.job.sections[0].format.font_id,
            open.galley.job.sections[0].format.font_id
        );
        let icon = text(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE);
        let folder = text(egui_phosphor::regular::FOLDER_OPEN);
        assert_eq!(icon.pos.x, folder.pos.x);
        assert_eq!(
            icon.galley.job.sections[0].format.font_id,
            folder.galley.job.sections[0].format.font_id
        );
        assert!(harness.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text() == egui_phosphor::regular::CARET_RIGHT
                    && (text.pos.y - recent.pos.y).abs() < 5.0)
        }));
        harness
            .get_by_role_and_label(egui::Role::Button, "Recently Closed")
            .click();
        harness.run_steps(2);
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.query_by_label("No recently closed files").is_none() {
            assert!(Instant::now() < deadline, "submenu did not open");
            std::thread::sleep(Duration::from_millis(1));
            harness.run_steps(1);
        }
    }
}

#[test]
fn recently_closed_menu_pins_recovers_and_opens_disk_only_entries() {
    for (action, disposition, suffix) in [
        (
            "note.txt · Recoverable tab · unsaved",
            FileOpenDisposition::NewTab,
            "recovered",
        ),
        ("Archived text", FileOpenDisposition::CurrentTab, "archived"),
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
        let token = app.state.persistence.session_store.closed_tabs().unwrap()[0]
            .token
            .clone();
        let disk = FileService::build_buffer_from_file_content(
            &path,
            FileService::read_file(&path).unwrap(),
            Some(FileService::read_disk_state(&path).unwrap()),
        );
        app.tab_manager.set_tabs(vec![WorkspaceTab::new(disk)], 0);
        app.state.recently_closed_files.push_back(other.clone());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_ui_state(|ui, app| render_recently_closed_files(ui, app, 0), app);
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.query_by_label("Pin").is_none() {
            assert!(Instant::now() < deadline, "archive query did not finish");
            std::thread::sleep(Duration::from_millis(1));
            harness.run_steps(1);
        }
        assert!(
            harness
                .query_by_role_and_label(egui::Role::Button, "note.txt")
                .is_none(),
            "archive path must not be duplicated as disk-only entry"
        );
        for expected in [true, false] {
            harness.get_by_label("Pin").click_accesskit();
            harness.run_steps(2);
            let policy = &harness.state().state.app_settings.history.recovery;
            assert_eq!(policy.pinned_archives.contains(&token), expected);
        }
        harness
            .get_by_role_and_label(egui::Role::Button, action)
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
        assert_eq!(recovered.name, format!("note.txt ({suffix})"));
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
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "disk");
    }
}
