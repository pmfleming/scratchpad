use scratchpad::app::domain::WorkspaceTab;

#[test]
fn untitled_workspace_tab_wraps_untitled_buffer() {
    let tab = WorkspaceTab::untitled();

    assert_eq!(tab.buffer.name, "Untitled");
    assert!(tab.buffer.path.is_none());
}
