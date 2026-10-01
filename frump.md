# Frump

This is the frump.md file for the Frump project. Form more info about Frump
visit the [Frump Project's](https://github.com/sologub/frump) home page.

## Team

* Ruslan Sologub <sologub@gmail.com> - Developer

## Tasks

### Task 6 - Implement task subject parsing

### Task 7 - Define task properties and specify the predefined ones

### Task 8 - Implement task properties parsing

### Task 9 - Implement multiline task properties

### Task 10 - Add some usage scenarios

Describe how adding and modifying tasks should be combined with git commits for
a better history visualization later.

### Task 11 - Rebuild the web board with React and efficient updates

Replace the manual DOM UI with a polished React interface. Fix closing the task view after opening consecutive tasks and stop repeatedly transferring all task bodies on large boards. Preserve board editing, Markdown, grouping, filtering, column layout, keyboard navigation, drafts and notifications.

Plan: component-owned task lifecycle and explicit URL state; embedded offline frontend assets; cached file-metadata snapshots with conditional board summaries and task bodies on demand; virtualized task lists; behavioral browser regressions and complete Rust validation. Keep board rules in the shared backend authority.

Implemented React and TypeScript components with Radix dialogs, TanStack virtualization, Markdown write and preview modes, responsive board and list views, theme and display controls, independent task drafts, legacy browser-storage migration and notification forms. Task selection has one explicit route state. Closing after consecutive selections returns to the board, and a delayed save cannot reopen an abandoned view.

The web read model now owns file-metadata invalidation, compact summaries, incremental updates, conditional responses, full-text search and lazy task-body reads. The old DOM application is removed. The legacy document API preserves full data, including duplicate records; the editor refuses ambiguous task numbers. Browser saves compare the original task under the shared board write lock before writing. Task bodies keep the CLI size semantics.

Validation: 143 Rust tests, 8 frontend unit tests and 19 real-browser tests passed. TypeScript and production asset builds passed; Clippy with warnings denied, Rust formatting, release compilation and verified Rust packaging passed. Both original defects were reproduced with behavioral tests before the replacement. The large-body idle test transferred 994 JSON bytes over 4.5 seconds with two empty refreshes. Tests also cover sharded-file changes, a 2000-task virtualized column, external edits, drafts, full-body search, drag and drop, mobile notifications, hidden tabs, legacy preferences and delayed saves. Desktop, mobile, dark and task-panel renders were inspected. The installed release serves embedded HTML, JavaScript, CSS and compact summaries successfully. The updated command is installed in the user command directory.

Knowledge learning audit: verified prevention guidance is recorded in frump/web/board-read-model, frump/web/task-selection-and-drafts, frump/web/concurrent-editing, frump/web/frontend-build-and-controls and frump/engineering/scoped-knowledge-inspection. This covers the original read and navigation defects, asynchronous save and draft risks, accessible control labels, root-anchored Cargo package includes and scoped knowledge inspection. No verified reusable mistake remains unrepresented.

Self-review: UI state and the cached read model are local to their owning modules, shared board rules remain authoritative, and complete Markdown text survives reads and saves. Source and embedded assets are committed together.

PASS

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T16:59:27Z

