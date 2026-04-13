#![forbid(unsafe_code)]

use scratchpad::app::domain::TabManager;

#[test]
fn reorder_tab_updates_shared_order_and_active_index() {
    let mut manager = TabManager::default();
    manager.tabs[0].buffer.name = "one.txt".to_owned();
    manager.create_untitled_tab();
    manager.tabs[1].buffer.name = "two.txt".to_owned();
    manager.create_untitled_tab();
    manager.tabs[2].buffer.name = "three.txt".to_owned();
    manager.active_tab_index = 1;

    assert!(manager.reorder_tab(0, 2));

    let ordered_names = manager
        .tabs
        .iter()
        .map(|tab| tab.buffer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ordered_names, vec!["two.txt", "three.txt", "one.txt"]);
    assert_eq!(manager.active_tab_index, 0);
}
