pub(crate) const CLOSE_SEARCH: &str = "ESC: Close Search";
pub(crate) const CLOSE_TAB: &str = "CTRL+W: Close Tab";
pub(crate) const CLOSE_TILE: &str = "CTRL+SHIFT+W: Close Tile";
pub(crate) const COPY: &str = "CTRL+C: Copy";
pub(crate) const COPY_PATH: &str = "CTRL+SHIFT+C: Copy Path";
pub(crate) const CONTROL_CHARS: &str = "CTRL+ALT+C: Control Chars";
pub(crate) const CUT: &str = "CTRL+X: Cut";
pub(crate) const ENCODING: &str = "CTRL+SHIFT+E: Encoding";
pub(crate) const FIND: &str = "CTRL+F: Find";
pub(crate) const HIDE_TAB_LIST: &str = "CTRL+SHIFT+B: Hide Tab List";
pub(crate) const HISTORY: &str = "CTRL+SHIFT+H: History";
pub(crate) const NEW_TAB: &str = "CTRL+N: New Tab";
pub(crate) const OPEN_FILE: &str = "CTRL+O: Open File";
pub(crate) const OPEN_FILE_HERE: &str = "CTRL+SHIFT+O: Open File Here";
pub(crate) const PASTE: &str = "CTRL+V: Paste";
pub(crate) const PIN_TAB_LIST: &str = "CTRL+SHIFT+B: Pin Tab List";
pub(crate) const PROMOTE_ALL_FILES: &str =
    "CTRL+SHIFT+T: Promote each file in this workspace to its own tab";
pub(crate) const PROMOTE_TILE: &str = "CTRL+T: Promote Tile";
pub(crate) const REDO: &str = "CTRL+Y: Redo";
pub(crate) const RENAME: &str = "F2: Rename";
pub(crate) const REPLACE: &str = "CTRL+H: Replace";
pub(crate) const REPLACE_ALL_MATCHES: &str = "ALT+ENTER: Replace all matches";
pub(crate) const REPLACE_CURRENT_MATCH: &str = "CTRL+ENTER: Replace current match";
pub(crate) const REVEAL_IN_EXPLORER: &str = "CTRL+SHIFT+R: Reveal In Explorer";
pub(crate) const OPEN_CONTAINING_FOLDER: &str = "CTRL+SHIFT+R: Open Containing Folder";
pub(crate) const RIGHT_TO_LEFT: &str = "CTRL+ALT+R: Right to Left";
pub(crate) const LEFT_TO_RIGHT: &str = "CTRL+ALT+R: Left to Right";
pub(crate) const SAVE: &str = "CTRL+S: Save";
pub(crate) const SAVE_AS: &str = "CTRL+SHIFT+S: Save As";
pub(crate) const SEARCH: &str = "CTRL+F: Search";
pub(crate) const SEARCH_MATCH_CASE: &str = "ALT+C: Case Sensitive";
pub(crate) const SEARCH_MODE_REGEX: &str = "ALT+R: Regex";
pub(crate) const SEARCH_NEXT_MATCH: &str = "F3: Next Match";
pub(crate) const SEARCH_PREVIOUS_MATCH: &str = "SHIFT+F3: Previous Match";
pub(crate) const SEARCH_SCOPE_ALL_TABS: &str = "ALT+4: Search All Open Files";
pub(crate) const SEARCH_SCOPE_CURRENT_FILE: &str = "ALT+2: Search Current File";
pub(crate) const SEARCH_SCOPE_CURRENT_TAB: &str = "ALT+3: Search All Files on This Tab";
pub(crate) const SEARCH_SCOPE_SELECTION: &str = "ALT+1: Search Selected Text";
pub(crate) const SEARCH_SCOPE_SELECTION_DEFAULT: &str =
    "ALT+1: Search Selected Text (auto-selected)";
pub(crate) const SEARCH_WHOLE_WORD: &str = "ALT+W: Whole Word";
pub(crate) const SELECT_ALL: &str = "CTRL+A: Select All";
pub(crate) const SETTINGS: &str = "CTRL+,: Settings";
pub(crate) const STATUS: &str = "CTRL+SHIFT+M: Status";
pub(crate) const SPLIT_TILE: &str =
    "CTRL+SHIFT+ARROWS: Split Tile\nDrag left/right/up/down to split in that direction";
pub(crate) const SPLIT_DOWN: &str = "CTRL+SHIFT+DOWN: Split Down";
pub(crate) const SPLIT_LEFT: &str = "CTRL+SHIFT+LEFT: Split Left";
pub(crate) const SPLIT_RIGHT: &str = "CTRL+SHIFT+RIGHT: Split Right";
pub(crate) const SPLIT_UP: &str = "CTRL+SHIFT+UP: Split Up";
pub(crate) const UNDO: &str = "CTRL+Z: Undo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_tooltips_put_the_shortcut_first() {
        for tooltip in [
            OPEN_FILE,
            OPEN_FILE_HERE,
            SAVE,
            SAVE_AS,
            SEARCH,
            FIND,
            REPLACE,
            HISTORY,
            ENCODING,
            STATUS,
            COPY_PATH,
            CONTROL_CHARS,
            REVEAL_IN_EXPLORER,
            RIGHT_TO_LEFT,
            HIDE_TAB_LIST,
            NEW_TAB,
            CLOSE_TAB,
            PROMOTE_TILE,
            RENAME,
        ] {
            assert!(tooltip.contains(": "));
            assert!(
                tooltip.starts_with("CTRL+")
                    || tooltip.starts_with("ESC")
                    || tooltip.starts_with("F2")
            );
        }
    }
}
