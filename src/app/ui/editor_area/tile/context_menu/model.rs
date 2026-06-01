use crate::app::domain::SplitAxis;
use crate::app::shortcut_tooltips;
use crate::app::ui::tile_header::TileAction;
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

pub(super) fn shortcut_tooltip_for_menu_label(label: &str) -> Option<&'static str> {
    match label {
        "Undo" => Some(shortcut_tooltips::UNDO),
        "Redo" => Some(shortcut_tooltips::REDO),
        "History" => Some(shortcut_tooltips::HISTORY),
        "Find" => Some(shortcut_tooltips::FIND),
        "Replace" => Some(shortcut_tooltips::REPLACE),
        "Right to Left" => Some(shortcut_tooltips::RIGHT_TO_LEFT),
        "Left to Right" => Some(shortcut_tooltips::LEFT_TO_RIGHT),
        "Control Chars" => Some(shortcut_tooltips::CONTROL_CHARS),
        "Promote Tile" => Some(shortcut_tooltips::PROMOTE_TILE),
        "Close Tile" => Some(shortcut_tooltips::CLOSE_TILE),
        "Split Left" => Some(shortcut_tooltips::SPLIT_LEFT),
        "Split Right" => Some(shortcut_tooltips::SPLIT_RIGHT),
        "Split Up" => Some(shortcut_tooltips::SPLIT_UP),
        "Split Down" => Some(shortcut_tooltips::SPLIT_DOWN),
        _ => None,
    }
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
            shortcut_tooltip_for_menu_label("Undo"),
            Some(shortcut_tooltips::UNDO)
        );
        assert_eq!(
            shortcut_tooltip_for_menu_label("Split Down"),
            Some(shortcut_tooltips::SPLIT_DOWN)
        );
        assert_eq!(shortcut_tooltip_for_menu_label("No shortcut"), None);
    }

    #[test]
    fn inactive_tiles_activate_on_secondary_click_only() {
        assert!(should_activate_tile_on_secondary_click(true, false));
        assert!(!should_activate_tile_on_secondary_click(true, true));
        assert!(!should_activate_tile_on_secondary_click(false, false));
    }
}
