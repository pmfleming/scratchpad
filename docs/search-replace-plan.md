This evolved plan transforms the initial search concept into a robust, service-oriented architecture. It prioritizes **state stability**, **atomic operations**, and **cross-pane consistency** to ensure the search experience feels like a native part of the OS, rather than a bolted-on UI element.

---

# Pro-Grade Search & Replace Implementation Plan (V2)

## 1. System Architecture: The "Provider" Model
To support multi-pane workspaces, search must be decoupled from individual widgets. We introduce a **Service Layer** that manages the relationship between the UI and the data.

### The Search Provider Trait
Instead of hard-coding search logic for text buffers, we use a trait. This allows the same UI to eventually search terminal outputs, file trees, or PDF views.
* **Decoupling:** The UI doesn't know *what* it’s searching, only that it has a list of `SearchTarget` objects.
* **Atomic Edits:** Providers must support "Transaction" blocks to ensure "Replace All" doesn't leave the document in a partially corrupted state if an error occurs.

### Unified Search Session
A single `SearchSession` struct lives at the Workspace level.
```rust
pub struct SearchSession {
    pub query: String,
    pub replacement: String,
    pub options: SearchOptions,
    pub scope: SearchScope,
    pub results: Vec<SearchMatch>,
    pub active_index: Option<usize>,
    pub status: SearchStatus, // e.g., Idle, Searching, NoMatches, Error(String)
}
```

---

## 2. Advanced Scope & Context Awareness
We move beyond simple "Tab" scopes to include intent-based filtering:

* **`SelectionOnly`:** If text is selected when `Ctrl+F` is hit, the scope defaults to the selection.
* **`ActiveContext`:** Searches the focused buffer/tile.
* **`OpenBuffers`:** Searches all tabs currently loaded in memory.
* **`Project/Workspace`:** (Future-proofing) Triggers a background thread to grep through files on disk that aren't currently open.

---

## 3. High-Performance Execution Logic

### The "Virtual Coordinate" Strategy
To prevent the "moving target" problem (where replacing a 3-letter word with a 10-letter word shifts all subsequent match indices), the engine uses a **Reverse-Order Edit Stack**.
1.  **Collect:** Identify all `SearchMatch` ranges.
2.  **Filter:** Identify which matches are targeted (Active, Selected, or All).
3.  **Sort:** Sort ranges by start-index in **descending order**.
4.  **Apply:** Mutate the buffer from the bottom up. This ensures that every index remains valid for the duration of the operation.

### Coordinate Normalization
All internal matching uses **Absolute Character Offsets**. 
* Avoids UTF-8 byte-slicing errors.
* Simplifies communication between the search engine (Regex/Plaintext) and the `egui` text layout engine.

---

## 4. UX & Interaction Model

### The Responsive Search Strip
* **Non-Modal:** The strip should not block the editor. Users can click back into the code, type, and see highlights update in real-time.
* **Focus Management:** `Ctrl+F` always pulls focus to the input. `Esc` returns focus to the last active cursor position in the editor.
* **Visual Hierarchy:** * **Main Match:** High-contrast background (e.g., Bright Orange).
    * **Passive Matches:** Low-contrast border or subtle highlight (e.g., Dim Yellow).
    * **Selection Matches:** A "Multi-cursor" indicator (e.g., Vertical bar at each match).

### Multi-Cursor Promotion
This is the "Killer Feature" for efficiency:
* **`Ctrl+D` (Add Next):** The current `active_index` match is added to a `PersistentSelection` list. The `active_index` then increments to the next match.
* **Batch Editing:** Once promoted, these matches behave like standard cursors. Typing "Hello" replaces all selected matches simultaneously.

---

## 5. Implementation Roadmap

### Phase 1: The Core Engine (Robustness)
* Implement `Regex` and `CaseInsensitive` matching logic.
* Build the `Transaction` wrapper for safe string replacements.
* **Test Suite:** Validate matching against Unicode/Emojis and empty strings.

### Phase 2: Reactive UI (Fluidity)
* Build the `egui` search strip with immediate-mode feedback.
* Implement "Scroll-to-Match": Ensure that navigating matches automatically centers the editor on the result.

### Phase 3: Global Operations (Scale)
* Implement `Cross-Buffer` replacement logic.
* Add a "Summary" toast (e.g., *"Replaced 42 occurrences across 3 tabs"*).

---

## 6. Definition of Done (Quality Gates)

| Criterion | Requirement |
| :--- | :--- |
| **Undo Integrity** | "Replace All" must be reversible with a single `Ctrl+Z`. |
| **Index Stability** | Replacing "A" with "AAAAA" must not break subsequent highlights. |
| **Focus Flow** | Seamless keyboard transition between Search Input -> Editor -> Search Input. |
| **Performance** | Sub-16ms latency for highlight updates on a 50k line file. |
| **Zero-State** | Graceful handling of "No results found" (e.g., red text/shake animation). |