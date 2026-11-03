# History navigation contract

Every file has an edit tree rooted at an empty document. Opening a nonempty file
records its contents as the first insertion. **Now** is the selected node and is
visible at the root, at intermediate edits, and at the end of a branch.

- Undo moves Now to its parent. Redo follows the selected branch.
- Editing after undo creates a new branch and preserves every old continuation.
- Clicking a History row selects the state after that edit. Hover never edits.
- The scrubber shows the selected file's single branch, including the empty root.
- History groups all files' branch paths. Recently edited files include closed
  files; navigation and opening a file do not count as edits.
- A seek is atomic: replay conflicts leave both text and Now unchanged.
- Escape and outside clicks dismiss History without reverting the selected state.
- Opening History focuses navigation. An automatically opened scrubber leaves
  focus in the editor. Typing returns focus without losing the first event.
- Navigation never saves the source file; saved/dirty state follows selected text.
- Now, the selected branch, and all changes survive closing and restarting.
- Accepting external file changes records a replacement at Now, preserving branches.
- Forget history explicitly removes a file's branches and starts a new tree from
  its current contents. It never changes the source file.

There are no separate saved versions, snapshot visits, return states, or automatic
history expiry. Internal content caches are implementation details.
