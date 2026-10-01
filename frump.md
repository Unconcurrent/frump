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

Validation: 143 Rust tests, 8 frontend unit tests and 19 real-browser tests passed. TypeScript and production asset builds passed; Clippy with warnings denied, Rust formatting, release compilation and verified Rust packaging passed. Both original defects were reproduced with behavioral tests before the replacement. The large-body idle test transferred 994 JSON bytes over 4.5 seconds with two empty refreshes. Tests also cover sharded-file changes, a 2000-task virtualized column, external edits, drafts, full-body search, drag and drop, mobile notifications, hidden tabs, legacy preferences and delayed saves. Desktop, mobile, dark and task-panel renders were inspected. The installed release serves embedded HTML, JavaScript, CSS and compact summaries successfully. The updated command is installed system-wide at /usr/local/bin/frump with root ownership. The duplicate user installation has been removed. Normal command resolution and the embedded React assets, compact summaries and empty unchanged responses were verified from the system-wide executable.

Knowledge learning audit: verified prevention guidance is recorded in frump/web/board-read-model, frump/web/task-selection-and-drafts, frump/web/concurrent-editing, frump/web/frontend-build-and-controls and frump/engineering/scoped-knowledge-inspection. This covers the original read and navigation defects, asynchronous save and draft risks, accessible control labels, root-anchored Cargo package includes and scoped knowledge inspection. Installation ownership and duplicate executable prevention are also recorded in frump/engineering/system-installation. No verified reusable mistake remains unrepresented.

Self-review: UI state and the cached read model are local to their owning modules, shared board rules remain authoritative, and complete Markdown text survives reads and saves. Source and embedded assets are committed together.

PASS

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T16:59:27Z

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T17:04:49Z

### Task 12 - Unify Markdown editing and simplify board navigation

Create one polished Markdown editing surface with inline formatting and rendering. Reorder groups by dragging their headers instead of small arrow controls. Remove workspace creation and switching, retaining a fully hideable task navigation and team-filter sidebar with its restore icon in the board heading. Merge the topbar into the heading and footer. Plan: use a maintained Markdown-aware editor with React controls, preserve task body data and independent drafts; use a sortable group interaction with pointer and keyboard support; remove superseded topbar and workspace UI; persist sidebar visibility and group order; verify the existing UI and bandwidth regressions plus new editor, reorder and layout behavior. Build embedded assets and update the existing system-wide executable through the authorized root shell.

Update 2026-10-01T17:18:07Z - data

Captain clarified: remove the workspace feature entirely. Also fix dark mode, including the page background, theme persistence, system preference changes and all editor/dialog surfaces.

Update 2026-10-01T17:52:56Z - data

Implementation and self-review: replaced the separate Markdown writer/preview with one Tiptap editor and formatting toolbar. Body conversion is local to MarkdownEditor and richMarkdown; opening an untouched task retains its exact source, while comments, HTML and reference definitions remain editable and survive nearby changes. Removed the superseded renderer and textarea shortcut authority. Group reordering now belongs to sortable group headers with pointer, touch and keyboard support; removed reorder arrows and verified cancellation preserves an open task and resumes polling. Removed the workspace placeholder entirely, kept task/team navigation in a fully hideable sidebar with its restore icon in the board heading and a mobile drawer. Removed the topbar and moved board information, refresh and theme controls into the compact heading/footer. Fixed dark root/body backgrounds, readable controls and saved status, system preference changes, persisted choice and the initial loading theme. Validation: 143 Rust tests, 30 real-server browser tests, 7 frontend unit tests, TypeScript/build, formatting, clippy with warnings denied and verified cargo package passed. Full outputs retained in /tmp/frump-ui-rust-tests.log, /tmp/frump-ui-browser.log, /tmp/frump-ui-unit.log, /tmp/frump-ui-clippy.log and /tmp/frump-ui-package.log. Actual desktop, mobile and dark renders inspected. Large-board idle regression still transferred 994 JSON bytes over 4.5 seconds with empty unchanged responses. Updated the existing root-owned /usr/local/bin/frump through the retained authorized root shell; command resolution has no user-local copy. Restarted the existing board server at its same loopback address while preserving its directory and environment, then verified its actual rendered editor, dark background, sidebar controls, closing and empty idle responses. KB learning audit: created frump/web/markdown-editing for source preservation, isolated parser registration and keyboard-save ordering; extended frump/web/frontend-build-and-controls for theme startup/surfaces and group drag lifecycle. Existing frump/engineering/system-installation remains complete and was followed. All requested behavior is implemented and verified. PASS

Update 2026-10-01T17:55:35Z - data

Operational correction: the installed executable was correct, but restarting the user-managed foreground web server as a background replacement took over port 3000 and prevented the user from starting it again. Stopped only the verified replacement and confirmed port 3000 is free. Extended frump/engineering/system-installation to preserve user-managed server lifecycle and require an explicit request before restarting it. The user can now run frump web normally in the original terminal.

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T17:55:35Z

### Task 13 - Move board progress into the heading and remove the sidebar

Remove the entire task navigation sidebar and its restore controls. Place board progress horizontally beside the Project board heading, showing the percentage and completed/total count. Keep progress based on the whole board when filters are active, maintain completed filtering in Display, and fit narrow screens without a side rail. Update source, embedded assets, documentation and affected browser checks, validate the real server and install into the existing system-wide executable. Preserve the user-managed server lifecycle.

Update 2026-10-01T18:05:10Z - data

Implemented board progress as one horizontal summary immediately after the heading: progress label and meter, percentage, and completed/total count. Counts remain based on the entire board when filtering. Removed the complete sidebar, its mobile drawer, restore icon, visibility preference, and superseded styling. Completed filtering remains available in Display and assignee filtering is available in the toolbar at all viewport widths. Updated documentation and rebuilt the embedded assets. Validation: all 143 Rust tests, 30 browser checks and 7 frontend unit tests passed; TypeScript and release builds passed. Desktop and mobile renders inspected. The installed system-wide executable was separately checked on an isolated ephemeral server with 31 tasks and 18 completed: it displays 58% horizontally beside the heading, has no sidebar, and fits the mobile viewport. Updated the existing root-owned /usr/local/bin/frump through the retained authorized root shell. User-managed servers were left running; only the isolated smoke server was stopped. Logs are in /tmp/frump-heading-rust.log, /tmp/frump-heading-browser.log, /tmp/frump-heading-unit.log, /tmp/frump-heading-build.log and /tmp/frump-heading-release.log. Self-review: the heading owns the progress presentation; obsolete sidebar authority is removed completely. KB learning audit: no new reusable failure was discovered. Existing frump/web/frontend-build-and-controls and frump/engineering/system-installation cover the validation, asset installation and preservation of user-managed server lifecycle and were followed. PASS

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T18:05:10Z

### Bug 14 - Prevent restored-draft warnings when viewing untouched tasks

Goal: viewing, selecting, resizing, expanding, saving, or closing an untouched task must preserve the original Markdown and must not create an unsaved draft. Diagnose editor update events, reproduce the issue with Markdown whose serialization changes its spelling, then suppress non-document updates at the editor boundary. Preserve real text and formatting edits, existing draft recovery, and concurrent-edit baselines. Verify browser behavior, frontend checks and full Rust tests, install the release system-wide without restarting user-owned servers, and record reusable findings in the owning project knowledge rule.

Update 2026-10-01T18:24:03Z - data

Cause: the rich editor enabled/disabled effect called setEditable with its default emitUpdate flag. That emits an update event with an unchanged document, causing the form to serialize noncanonical Markdown and persist it as an apparent user draft. The boundary now disables these state-only events and accepts only document-changing root or appended transactions. Exact original source remains intact until a real text or formatting change. Genuine drafts and concurrent-edit baselines are preserved. Existing stored drafts lack provenance, so they are not deleted by normalization heuristics; Reload from file clears an already-cached false draft and now stays clean after reload. Regression evidence: the untouched setext/underscore/plus-list browser test failed before the patch and passes after it. Coverage exercises selection, expand/dock, close/reopen, reload, byte-exact save, previous false-draft cleanup, and formatting-only draft recovery. Validation: all 143 Rust tests, 32 browser tests and 7 frontend unit tests passed, as did TypeScript, frontend and release builds. The root-owned /usr/local/bin/frump was updated using the existing authorized root shell; both regression scenarios passed against that installed release on an isolated test server. No user-managed server was restarted. Full logs are /tmp/frump-draft-red.log, /tmp/frump-draft-browser.log, /tmp/frump-draft-recovery.log, /tmp/frump-draft-unit.log, /tmp/frump-draft-rust.log, /tmp/frump-draft-build.log, /tmp/frump-draft-release.log and /tmp/frump-draft-installed.log. Self-review: serialization authority remains local to actual document edits; UI lifecycle changes do not modify the form, file, or draft. KB learning audit: patched frump/web/markdown-editing with the established failure, transaction guard, lifecycle regression requirements and preservation of drafts with unknown edit provenance. Existing frump/engineering/system-installation remains the authority for installation and user-owned server lifecycle. PASS

Assigned To: Ruslan Sologub
Status: done
Last Updated: 2026-10-01T18:24:09Z

