# Combined Search & Replace Plan

This document outlines the design for a unified search and replace interface that handles both finding patterns and replacing them within a single, cohesive UI component.

## 1. Objectives
- **Single Interface**: A unified bar for both Find and Replace to reduce UI clutter.
- **Real-time Feedback**: Highlight matches as the user types.
- **Efficiency**: Support "Replace" and "Replace All" directly from the search bar.
- **Non-blocking**: The UI should be an overlay that doesn't force a modal state.

## 2. Integrated UI Design

### The Search & Replace Bar
A single horizontal bar positioned at the top of the editor area (just below the tab strip).

- **Left Section (Find)**:
  - Magnifying glass icon.
  - "Find" text input (auto-focused).
  - Match counter (e.g., "4 / 12").
- **Middle Section (Replace)**:
  - "Replace" text input.
  - "Replace" button (replaces current match and moves to next).
  - "Replace All" button.
- **Right Section (Controls)**:
  - Arrow buttons (Up/Down) for navigation.
  - Toggle buttons for:
    - `Cc`: Case Sensitivity.
    - `W`: Whole Word.
    - `.*`: Regex (optional/future).
  - Close (X) button.

### Visual State
- **Active Match**: Distinctive highlight (e.g., bright orange/yellow).
- **Other Matches**: Subtle highlight (e.g., semi-transparent yellow).
- **Empty/No Matches**: The "Find" input border turns red.

## 3. Technical Implementation

### Shared State (`SearchState`)
```rust
struct SearchState {
    is_open: bool,
    find_query: String,
    replace_query: String,
    matches: Vec<TextRange>,
    active_index: Option<usize>,
    case_sensitive: bool,
    whole_word: bool,
}
```

### Logic Flow
1. **Find**: On query change, perform a regex or string search across the current tab's content. Store the byte ranges of all matches.
2. **Navigate**: Use the arrows or `Enter`/`Shift+Enter` to cycle through `active_index`.
3. **Replace**: 
   - Get the range of the `active_index`.
   - Splice the `replace_query` into the document string at that range.
   - Re-run the "Find" logic to refresh match positions.
4. **Replace All**: Iterate through all matches in reverse order (to preserve indices) and replace them.

## 4. Keyboard Shortcuts
- `Ctrl + F` / `Ctrl + H`: Open the combined Search & Replace bar.
- `Enter`: Find Next.
- `Shift + Enter`: Find Previous.
- `Alt + R`: Replace Current.
- `Alt + A`: Replace All.
- `Esc`: Close and clear highlights.

## 5. Implementation Roadmap
1. [ ] **UI Overlay**: Build the basic layout using `egui::Area` or a `TopBottomPanel` sub-section.
2. [ ] **Search Engine**: Implement the matching logic and range tracking.
3. [ ] **Editor Sync**: Connect search ranges to `egui::TextEdit` selection and scrolling.
4. [ ] **Replace Actions**: Add the logic for single and batch text replacement.
