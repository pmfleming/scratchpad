use crate::app::domain::SplitAxis;
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::ui::tile_header::TileAction;
use eframe::egui;
use egui_phosphor::regular::{ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP};

const DEFAULT_SPLIT_RATIO: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy)]
pub(super) struct SplitMenuItem {
    pub(super) label: &'static str,
    pub(super) icon: &'static str,
    pub(super) direction: SplitDirection,
}

pub(super) const SPLIT_MENU_ITEMS: &[SplitMenuItem] = &[
    SplitMenuItem {
        label: "Split Left",
        icon: ARROW_LEFT,
        direction: SplitDirection::Left,
    },
    SplitMenuItem {
        label: "Split Right",
        icon: ARROW_RIGHT,
        direction: SplitDirection::Right,
    },
    SplitMenuItem {
        label: "Split Up",
        icon: ARROW_UP,
        direction: SplitDirection::Up,
    },
    SplitMenuItem {
        label: "Split Down",
        icon: ARROW_DOWN,
        direction: SplitDirection::Down,
    },
];

pub(super) fn queue_split_action(actions: &mut Vec<TileAction>, direction: SplitDirection) {
    let (axis, new_view_first) = split_direction_parts(direction);
    actions.push(TileAction::Split {
        axis,
        new_view_first,
        ratio: DEFAULT_SPLIT_RATIO,
    });
}

fn split_direction_parts(direction: SplitDirection) -> (SplitAxis, bool) {
    match direction {
        SplitDirection::Left => (SplitAxis::Vertical, true),
        SplitDirection::Right => (SplitAxis::Vertical, false),
        SplitDirection::Up => (SplitAxis::Horizontal, true),
        SplitDirection::Down => (SplitAxis::Horizontal, false),
    }
}

pub(super) fn shortcut_tooltip_for_menu_label(ctx: &egui::Context, label: &str) -> Option<String> {
    match label {
        "Undo" => return Some(shortcut_tooltips::UNDO.to_owned()),
        "Redo" => return Some(shortcut_tooltips::REDO.to_owned()),
        _ => {}
    }
    let action = match label {
        "History" => ShortcutAction::OpenTextHistory,
        "Find" => ShortcutAction::OpenSearch,
        "Replace" => ShortcutAction::OpenReplace,
        "Right to Left" | "Left to Right" => ShortcutAction::ToggleReadingOrder,
        "Control Chars" => ShortcutAction::ToggleControlChars,
        "Promote Tile" => ShortcutAction::PromoteTileToTab,
        "Close Tile" => ShortcutAction::CloseTile,
        "Split Left" => ShortcutAction::SplitLeft,
        "Split Right" => ShortcutAction::SplitRight,
        "Split Up" => ShortcutAction::SplitUp,
        "Split Down" => ShortcutAction::SplitDown,
        _ => return None,
    };
    Some(shortcut_tooltips::action(ctx, action, label))
}

pub(super) fn should_activate_tile_on_secondary_click(
    secondary_clicked: bool,
    tile_is_active: bool,
) -> bool {
    secondary_clicked && !tile_is_active
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SPLIT_RATIO, SPLIT_MENU_ITEMS, SplitDirection, queue_split_action,
        shortcut_tooltip_for_menu_label, should_activate_tile_on_secondary_click,
        split_direction_parts,
    };
    use crate::app::domain::SplitAxis;
    use crate::app::shortcut_tooltips;
    use crate::app::ui::tile_header::TileAction;

    #[test]
    fn split_direction_parts_match_menu_labels() {
        assert_eq!(
            split_direction_parts(SplitDirection::Left),
            (SplitAxis::Vertical, true)
        );
        assert_eq!(
            split_direction_parts(SplitDirection::Right),
            (SplitAxis::Vertical, false)
        );
        assert_eq!(
            split_direction_parts(SplitDirection::Up),
            (SplitAxis::Horizontal, true)
        );
        assert_eq!(
            split_direction_parts(SplitDirection::Down),
            (SplitAxis::Horizontal, false)
        );
    }

    #[test]
    fn split_menu_exposes_each_direction_once() {
        let labels: Vec<_> = SPLIT_MENU_ITEMS.iter().map(|item| item.label).collect();
        assert_eq!(
            labels,
            vec!["Split Left", "Split Right", "Split Up", "Split Down"]
        );
    }

    #[test]
    fn queue_split_action_uses_default_split_ratio() {
        let mut actions = Vec::new();
        queue_split_action(&mut actions, SplitDirection::Up);

        match actions.as_slice() {
            [
                TileAction::Split {
                    axis,
                    new_view_first,
                    ratio,
                },
            ] => {
                assert_eq!(*axis, SplitAxis::Horizontal);
                assert!(*new_view_first);
                assert_eq!(*ratio, DEFAULT_SPLIT_RATIO);
            }
            _ => panic!("expected one split action"),
        }
    }

    #[test]
    fn shortcut_tooltip_lookup_covers_menu_commands() {
        assert_eq!(
            shortcut_tooltip_for_menu_label(&eframe::egui::Context::default(), "Undo"),
            Some(shortcut_tooltips::UNDO.to_owned())
        );
        assert!(
            shortcut_tooltip_for_menu_label(&eframe::egui::Context::default(), "Split Down",)
                .is_some_and(|tooltip| tooltip.ends_with(": Split Down"))
        );
        assert_eq!(
            shortcut_tooltip_for_menu_label(&eframe::egui::Context::default(), "No shortcut",),
            None
        );
    }

    #[test]
    fn inactive_tiles_activate_on_secondary_click_only() {
        assert!(should_activate_tile_on_secondary_click(true, false));
        assert!(!should_activate_tile_on_secondary_click(true, true));
        assert!(!should_activate_tile_on_secondary_click(false, false));
    }
}
