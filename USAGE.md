# Frump Usage Guide

Complete reference for using the Frump task management CLI.

## Table of Contents

- [Installation](#installation)
- [Command-line rules](#command-line-rules)
- [Getting Started](#getting-started)
- [Core Commands](#core-commands)
- [Task Management](#task-management)
- [Filtering and Search](#filtering-and-search)
- [Git Integration](#git-integration)
- [Templates](#templates)
- [Bulk Operations](#bulk-operations)
- [Import/Export](#importexport)
- [Duplicate Task Numbers](#duplicate-task-numbers)
- [Examples and Workflows](#examples-and-workflows)

## Installation

```bash
cargo build --release
# Binary will be at target/release/frump
```

Frump automatically finds `frump.md` (or a `frump/` board directory) in the current directory or a parent directory. Use `--board` only to select a different board.

## Command-line rules

- Every option has a long name, and there are no short flags. Help is `--help`.
- A command takes at most one unnamed argument, and it is always the task number: `frump show 12`, `frump set 12 --property Status --value done`.
- The same idea has the same name in every command: `--type`, `--status`, `--assignee`, `--subject`, `--body`, `--property`, `--value`, `--from`, `--to`.
- `--board PATH` names the board: a board file, or a board directory. It works before or after the command name.

## Sharded board layout and migration

Frump can store each task in its own Markdown file. Convert a single-file board with `frump migrate`; a successful migration replaces `frump.md` with the `frump/` directory. The command stages and verifies the new board before publishing it, and refuses an existing destination.

```text
frump/
  general.md
  tasks/
    1.md
    2.md
```

`general.md` contains the project header and Team section. Each task file contains exactly one normal task heading, body, and properties; the heading, not the file name, gives the task number. When invoked with `--board frump`, Frump writes only changed task files rather than rewriting the full board. `--board frump/general.md` names the same board. Files in `tasks/` that are not `.md` files are never touched.

```bash
frump --board frump.md migrate
frump --board frump list
```

`migrate --to DIR` writes the board directory somewhere else. Frump links the Git history of `name/` to the `name.md` it replaced, so a custom destination starts without the old board's history: its closed task numbers can be given out again, and `frump history` starts at the migration. Frump warns when this happens.

## Web board

Start a local Kanban editor for the board:

```bash
frump web
# Open http://127.0.0.1:3000

# Use a different board or port
frump web --board project-tasks.md --port 4000
```

The board edits the Markdown file directly and refreshes automatically when another tool modifies it. The Delete button follows the same rules as `frump close`: the task must be done and its prerequisites satisfied. When the last commit (HEAD) does not hold the task's current text, the page shows that text in a notice that stays until you dismiss it.

## Authority workflow

```bash
# Declare comma-separated prerequisites in the existing Markdown property format
frump set 12 --property "Depends On" --value "3, 7"
frump validate                 # rejects unknown, malformed and cyclic dependencies
frump depends-on 12            # show the prerequisite tree
frump dependents 7             # show active consumers of task 7
frump ready                    # show unfinished tasks whose prerequisites are satisfied

# Append durable task evidence without overwriting the existing body
frump update 12 --append "Review rejected: reason and next proof"
frump unset 12 --property Status
```

A prerequisite is satisfied when its task is on the board with `Status: done`, or when the task was closed: it is no longer on the board and Git history has it. An unknown number or an entry that is not a number is never satisfied. `frump close` requires `Status: done` and satisfied prerequisites. A new status remains allowed, but emits a warning so project vocabulary does not drift accidentally. `frump add` warns when similar existing tasks are found but still creates the task.

Frump automatically maintains `Last Updated` whenever it creates or changes a task. Property values are compact metadata and may be at most 40 bytes; put longer evidence, reports, and rationale in the body. `--append` adds a dated update line (`Update 2026-09-28T08:20:47Z - data`) and, when invoked by a Metateam crew member, records that member's name. `--notify assignee` then sends the raw appended text with `metateam crew message` to the crew members named in the task's `Assigned To` value (comma-separated names each get it), or to all crew members when the task has no assignee. `--notify all` sends it with `metateam crew message all`, whoever the assignee is. Normal web Save deliberately replaces the body with exactly the text in the editor.

Initialize a new board explicitly:

```bash
frump init --board frump.md --title "My Project"
```

Commit only the task board with a short message:

```bash
frump commit --message "Record completed validation"
```

The React interface offers **Board** and **List** views. Group columns by `Status`
or any other property, search the complete task text, and filter by type or
assignee. Board progress appears beside the heading, showing the percentage and
completed/total task count for the entire board. Use **Display → Completed tasks
only** to focus on finished tasks.
Reorder groups by dragging their headers, or focus a header and press Space,
arrow keys, then Space to finish (Escape cancels). Collapse groups and use
**Display** for compact cards, hiding empty columns, or resetting the layout.
The footer theme button switches between dark and light. Select **System** in
**Display → Theme** to follow the operating system. Preferences,
filters, column layouts, and scroll positions are saved per board in the browser.

Opening a task docks its details beside the board; the divider resizes the panel
by drag or arrow keys and a double click resets it. Expand the task view from its
header, or use the previous and next controls to navigate tasks. On small screens
the task opens as a dialog. The open task and notification panel are in the URL,
so a reload restores the view. Switching tasks keeps a single selection; closing
always returns to the board. Saving keeps the task open. Unsaved drafts are kept
independently per task when switching, closing, or reloading. **Discard draft**
explicitly removes one. Keyboard: `/` search, `n` new task, `Alt` plus arrow keys
to move a focused card between columns, `Escape` to close the task view.

Cards show short body excerpts. The unified description editor renders Markdown
while editing, with controls for headings, bold, italic, strike, lists,
checklists, quotes, code, links, images, and tables. Paste Markdown or type
Markdown shortcuts such as `## ` and `- ` at the start of a line. Ctrl+B, Ctrl+I,
and Ctrl+K format selections or add links; Ctrl+Enter saves (Command shortcuts
also work). Lists continue on Enter and nest with Tab; Tab outside a list moves
focus normally. Opening or saving an untouched body preserves its original
Markdown exactly. HTML, comments, and reference definitions remain editable
as source blocks and survive edits to nearby text. Type, status, and property fields offer
existing values and accept new ones. `Last updated` sorting places the newest
tasks first and tasks without timestamps last.

The browser loads compact summaries, receives only changed summaries, and
downloads a full body only when its task is opened or changed. An unchanged
refresh returns no JSON body; polling pauses while the tab is hidden. Large
columns and lists render only visible rows. Search runs against complete task
bodies on the server without downloading those bodies to the browser.

An external change updates a clean task view and preserves a dirty draft with
**Reload from file** and **Keep my draft** choices. Saving also checks the original
task under the board write lock, so an edit arriving between refresh and save
cannot silently overwrite newer text. A draft of a removed task can be saved as
a new task, including after a reload. **Close completed task** follows the same
rules as `frump close`. **Notify**, on a card or in its details, sends a message
about that task to a crew member through Metateam.

## Getting Started

Frump works with a `frump.md` file in your project directory. This file contains:
- A header section
- A team section listing team members
- A tasks section with all active tasks

### Basic frump.md Structure

```markdown
# My Project

Project description goes here.

## Team

* John Doe <john@example.com> - Lead Developer
* Jane Smith <jane@example.com> - QA Engineer

## Tasks

### Task 1 - Implement user authentication

Add login and registration functionality.
Status: working
Assigned To: John Doe

### Bug 2 - Fix navigation bug

The menu doesn't close on mobile.
Status: open
Assigned To: Jane Smith
```

## Core Commands

### list - List tasks

List the tasks on the board.

```bash
# List all tasks
frump list

# Filter by task type
frump list --type Bug

# Filter by status
frump list --status working

# Filter by assignee
frump list --assignee "John Doe"

# Combine filters
frump list --type Bug --status open --assignee "Jane Smith"

# Filter authority metadata, find missing values, and sort
frump list --property "Evidence Kind=test" --missing Review
frump list --sort last-updated --desc
frump list --sort "Depends On"

# Machine-readable filtered results (properties keep their order in the task file)
frump list --status todo --format json

# Closed tasks: Git history has them and the board no longer does
frump list --closed
frump list --closed --type Bug --format json
```

**Example output:**
```
Task 1 - Implement user authentication
  Status: working
  Assigned to: John Doe

Bug 2 - Fix navigation bug
  Status: open
  Assigned to: Jane Smith
```

### show - Show task details

Display complete information about a specific task.

```bash
frump show <task_id>
```

**Example:**
```bash
$ frump show 1

### Task 1 - Implement user authentication

Add login and registration functionality.
Authentication should support email/password and OAuth.

Status: working
Assigned To: John Doe
Priority: high
```

### add - Add a new task

Create a new task on the board.

```bash
# Simple task (defaults to type "Task")
frump add --subject "Fix typo in README"

# Specify task type
frump add --type Bug --subject "Login button not working"
frump add --type Feature --subject "Add dark mode support"

# Add with body text
frump add --subject "Refactor database code" --body "Current code is hard to maintain"

# Assign to someone
frump add --subject "Write tests" --assignee "Jane Smith"

# Set status
frump add --subject "Deploy to staging" --status "ready"

# Combine all options
frump add --type Feature --subject "Add export feature" \
  --body "Users want to export their data as CSV" \
  --assignee "John Doe" \
  --status "planning"

# From a template (see Templates)
frump add --template bug --fill component=parser --fill description="crash on empty input"
```

**Note:** The add command automatically:
- Gives the task a number above every number on the board and in its Git history, so a closed task's number is never given out again
- Assigns to the first team member if no assignee is specified

Whenever a task receives a new `Assigned To` value, Frump saves the board first and then announces `<type> <id> is assigned to <assignee>.` to the new assignee as `frump`; a comma-separated value notifies each name. Metateam is optional: when the `metateam` command is not on `PATH`, the announcement is skipped and the assignment still succeeds. When Metateam cannot deliver the announcement, for example because the assignee is not a crew member, Frump prints a warning and the assignment still succeeds; the web board shows the same warning next to the saved task. An assignee of `all`, `all-crews` or `all-hosts` is a Metateam broadcast target, so the announcement goes to all of those agents.

## Task Management

### close - Close a task

Remove a done task from the board. Its prerequisites must be satisfied.

```bash
frump close <task_id>
```

**Example:**
```bash
$ frump close 5
Closed Task 5 - Fix typo in README

Remember to commit this change with a descriptive message.
```

Git state never stops a close. When the last commit does not hold the task exactly as it is now (a new task, uncommitted edits, or a board outside Git), `close` prints a warning on stderr with the complete task text, so you can paste it back. When no commit has the task at all, the warning also says its number can be given out again and names the tasks that now depend on an unknown number. Commit before closing to keep the final text in history.

**Tip:** Closed tasks remain in git history and can be listed with `frump list --closed`.

### assign - Assign a task

Change or set the assignee for a task.

```bash
frump assign <task_id> --assignee <name>
```

**Examples:**
```bash
frump assign 3 --assignee "John Doe"
frump assign 7 --assignee "Jane Smith"
```

### set - Set a property

Set or update any property on a task. Property values are limited to 40 bytes; use the task body for longer text.

```bash
frump set <task_id> --property <name> --value <value>
```

**Examples:**
```bash
# Set status
frump set 1 --property Status --value working
frump set 5 --property Status --value done

# Set priority
frump set 2 --property Priority --value high

# Set custom property (must be Capitalized, max 3 words)
frump set 3 --property "Due Date" --value "2025-12-31"
frump set 4 --property "Estimated Hours" --value 8
```

**Property Rules:**
- Must start with an uppercase letter
- Maximum 3 words
- Common properties: Status, Priority, Tags, "Assigned To", "Due Date"

### unset - Remove a property

```bash
frump unset <task_id> --property <name>
```

### update - Update task content

Modify a task's subject or body. At least one change is required.

```bash
# Update subject only
frump update <task_id> --subject "New subject text"

# Update body only
frump update <task_id> --body "New detailed description"

# Clear a body deliberately; an empty --body value is refused
frump update <task_id> --clear-body

# Update both
frump update <task_id> \
  --subject "Updated title" \
  --body "Updated description"

# Append dated evidence without replacing the existing body
frump update <task_id> --append "Validation passed after rebuild"

# Append dated evidence and notify the task's assignee through Metateam
frump update <task_id> --append "Validation passed after rebuild" --notify assignee

# Append dated evidence and notify the whole Metateam crew
frump update <task_id> --append "Validation passed after rebuild" --notify all
```

### next - Manage the ordered todo plan

`## Next` stores the ordered IDs permitted to leave `todo`. `next --set` replaces the plan; with no options, `next` prints the current order; `--clear` removes the whole plan. An empty plan preserves the existing unrestricted workflow; once populated, a `todo` task that is not first cannot move to another status. Moving a task to `done` automatically removes it from the plan.

```bash
frump next --set 12 15 18
frump next
frump next --clear
frump set 12 --property Status --value working
frump set 12 --property Status --value done
```

`--body` replaces the body exactly and refuses empty or whitespace-only values, so an accidental empty shell expansion cannot erase the record. Use `--clear-body` for an intentional removal. `--append` creates a dated update and adds the current Metateam crew member when that identity is available. `--notify` needs `--append`; it sends the raw appended text after saving the task. `--notify assignee` sends it to the crew members named in the task's `Assigned To` value (comma-separated names each get it), or to all crew members when the task has no assignee. `--notify all` sends it to all Metateam crew members, whoever the assignee is. When the `metateam` command is not on `PATH`, the update is saved and the message is skipped. When Metateam cannot deliver the message, Frump prints a warning and the update stays saved.

**Example:**
```bash
$ frump update 3 --subject "Implement OAuth authentication"
Updated subject for task 3
```

## Filtering and Search

### search - Search tasks

Search for tasks by keyword in subject and body.

```bash
frump search --text "authentication"

# Also print the start of each matching task's body
frump search --text "authentication" --show-body
```

**Example output:**
```
Found 2 similar task(s) matching 'auth':

Task 1 - Implement user authentication
  Similarity: 100% (subject)
Feature 8 - Add OAuth support
  Similarity: 100% (body)
```

### stats - Show statistics

Display summary statistics about your tasks.

```bash
frump stats
```

**Example output:**
```
Task Statistics

Total tasks: 12

By Type:
  Task: 7
  Bug: 3
  Feature: 2

By Status:
  working: 5
  open: 4
  done: 2
  (no status): 1

By Assignee:
  John Doe: 7
  Jane Smith: 5

Closed tasks: 8
```

### validate - Validate the board

Check the board for issues. `validate` exits with an error when any check fails, so scripts can rely on its exit code.

```bash
frump validate
```

**Checks performed:**
- File structure is valid
- All task IDs are unique
- Dependencies resolve and are acyclic (a closed prerequisite is valid)
- Task IDs are sequential (warns about gaps)
- Team member emails are valid

**Example output:**
```
✓ File structure is valid
✓ All task IDs are unique
✓ Dependencies resolve and are acyclic
⚠ ID gaps found (possibly closed tasks):
  ID 3
  IDs 5-7

✓ Validation complete: 10 tasks, 3 team members
```

## Git Integration

Frump reads the Git history of the board itself, from the repository that contains it, whatever directory you run it from. For a board directory `name/`, the history of the `name.md` it was migrated from counts as the same board.

### history - Show task history

View the commits that changed a task. A commit counts only when it changed that task's own text.

```bash
frump history <task_id>
```

**Example output:**
```
History for Task 5:

✓ Created by John Doe on 2025-11-20 14:30
  Commit: a3f8b901
  Message: Add authentication task

• Modified by Jane Smith on 2025-11-21 09:15
  Commit: c2e4d678
  Message: Update task priority

✗ Deleted by John Doe on 2025-11-24 16:45
  Commit: 9f2b3c45
  Message: Close completed authentication task
```

**Note:** Requires a board inside a git repository.

### list --closed - List closed tasks

Show all tasks that have been removed from the board but exist in history, in their last committed state.

```bash
frump list --closed
```

**Example output:**
```
Closed tasks:

Task 3 - Initial project setup
Bug 5 - Fix login validation
Task 7 - Write documentation

Total: 3 closed tasks
```

## Templates

Templates help you quickly create tasks with predefined structure. They are stored in `.frump_templates.json` in the directory that contains the board.

### template add - Create a template

```bash
frump template add --name <name> --subject <subject_template> \
  --type <task_type> \
  --body <body_template> \
  --property KEY=VALUE
```

Use `{placeholder}` syntax for variables. A placeholder name is letters, digits, `_` or `-`; any other text in braces stays as written. `--property` may repeat.

**Examples:**
```bash
# Bug report template
frump template add --name bug --subject "Fix {component} issue" \
  --type Bug \
  --body "Issue found in {component}: {description}" \
  --property Priority=high

# Feature template
frump template add --name feature --subject "Add {feature} support" \
  --type Feature \
  --body "Users requested: {description}"
```

### add --template - Create a task from a template

```bash
frump add --template bug --fill component=parser --fill description="crash on empty input"
```

Each `{placeholder}` is filled once, and a fill value is never expanded again. A placeholder without `--fill`, a `--fill` that no placeholder uses, and a repeated `--fill` key are errors. `--subject` cannot be combined with `--template`; `--type`, `--body`, `--assignee` and `--status` override the template (an overridden body is not filled). The assignee comes from `--assignee`, then the template's `Assigned To`, then the board's first team member.

### template list - List templates

```bash
frump template list
```

**Example output:**
```
Available templates:

bug (Bug)
  Subject: Fix {component} issue
  Body: Issue found in {component}: {description}

feature (Feature)
  Subject: Add {feature} support
  Body: Users requested: {description}
```

### template show - Show template details

```bash
frump template show --name <name>
```

### template remove - Delete a template

```bash
frump template remove --name <name>
```

## Bulk Operations

Perform operations on multiple tasks at once. `--with-status` and `--with-type` select the tasks.

### bulk close - Close tasks by status

Close all tasks with a status. Each task must pass the `frump close` rules; when one fails, nothing is closed.

```bash
frump bulk close --with-status <status>
```

**Example:**
```bash
$ frump bulk close --with-status done
Closed Task 3 - Write documentation
Closed Bug 5 - Fix login validation

Closed 2 task(s) with status 'done'
```

### bulk assign - Assign tasks by type

Assign all tasks of a specific type to someone.

```bash
frump bulk assign --with-type <task_type> --assignee <name>
```

**Example:**
```bash
$ frump bulk assign --with-type Bug --assignee "Jane Smith"
Assigned 7 task(s) of type 'Bug' to Jane Smith
```

### bulk set - Set property by status

Set a property on all tasks with a specific status.

```bash
frump bulk set --with-status <status> --property <name> --value <value>
```

**Example:**
```bash
$ frump bulk set --with-status working --property Priority --value high
Set Priority = high on 3 task(s) with status 'working'
```

## Import/Export

### export - Export tasks

Export tasks to JSON or CSV format. `--to` never writes the board or a file inside it.

```bash
# Export to JSON (stdout)
frump export

# Export to JSON file
frump export --to tasks.json

# Export to CSV
frump export --format csv --to tasks.csv
```

**JSON format** includes:
- Header text
- Team members with emails and roles
- All tasks with full details, properties in the order of the task file

**CSV format** includes:
- One row per task
- Columns: ID, Type, Subject, Body, Status, Assignee

### import - Import tasks

Import tasks from a JSON export. `--from` is never written: Frump refuses a source that is the board, a file inside it, or another name for it.

```bash
# Replace all tasks of the board (careful!)
frump import --from tasks.json

# Restore into a new board file
frump import --from tasks.json --to restored.md

# Merge with existing tasks (adds new IDs)
frump import --from tasks.json --merge
```

`--to` names the board to import into; it cannot be combined with `--board`. A replace restores a board and does not announce assignments.

**Note:** When merging, imported tasks get new numbers above the board and its history, and `Depends On` entries that point inside the imported file are renumbered with them. An entry that points outside the file refuses the whole import. Each new task with an assignee is announced, as `add` does.

## Duplicate Task Numbers

When merging git branches, task IDs may conflict if both branches added tasks with the same ID. `frump validate` reports duplicate numbers and exits with an error.

### renumber-duplicates - Give duplicates new numbers

```bash
# Renumber and save changes
frump renumber-duplicates

# Renumber and commit the board automatically
frump renumber-duplicates --commit
```

**Example:**
```bash
$ frump renumber-duplicates --commit
✓ Resolved 2 duplicate task ID(s):

  5 → 12: Implement notifications
  8 → 13: Update dependencies

✓ Changes committed
```

**How it works:**
- Keeps the first occurrence of each duplicate ID
- Gives later duplicates new numbers above the board and its history
- With `--commit`, commits only the board, as `frump commit` does

## Examples and Workflows

### Daily Development Workflow

```bash
# Start your day - see what you're working on
frump list --assignee "Your Name" --status working

# Add a new task you discovered
frump add --type Bug --subject "Login timeout not working" --body "Users get logged out too quickly"

# Update task status as you work
frump set 5 --property Status --value working

# Close completed tasks
frump commit --message "Finish task 3"
frump close 3
frump commit --message "Close task 3: Completed user authentication"

# End of day - see what's left
frump stats
```

### Code Review Workflow

```bash
# Reviewer adds issues found
frump add --type Bug --subject "Missing null check in login" --assignee "Developer Name"
frump add --type Task --subject "Add tests for edge cases" --assignee "Developer Name"

# Developer fixes and tracks progress
frump list --assignee "Developer Name" --status open
frump set 10 --property Status --value working
# ... fix the issue ...
frump set 10 --property Status --value done
frump close 10
```

### Team Handoff Workflow

```bash
# Reassign all your tasks to someone else
frump list --assignee "Your Name"  # See what you have
frump bulk assign --with-type Task --assignee "Other Person"

# Or reassign specific task
frump assign 7 --assignee "Other Person"
```

### Release Planning Workflow

```bash
# See all features planned
frump list --type Feature

# Mark features as ready for next release
frump bulk set --with-status done --property Release --value v2.0

# Export for external tracking
frump export --to v2.0-tasks.json
```

### Merge Conflict Resolution

```bash
# After merging branches
frump validate                          # reports duplicate task numbers

# If duplicates are found
frump renumber-duplicates --commit

# Verify everything is clean
frump validate
frump list
```

## Tips and Best Practices

1. **Commit Often**: Commit board changes regularly so you can track task history.

2. **Use Descriptive Commit Messages**: Your commit messages become part of task history.
   ```bash
   frump commit --message "Add task 15: Implement dark mode"
   frump commit --message "Update task 7: Change priority to high"
   frump commit --message "Close task 12: Feature completed and tested"
   ```

3. **Leverage Filtering**: Use filters to focus on relevant tasks.
   ```bash
   frump list --status working  # What's in progress
   frump list --type Bug        # All bugs
   ```

4. **Use Templates**: Create templates for common task types to ensure consistency.

5. **Regular Validation**: Run `frump validate` periodically to catch issues early.

6. **Close Tasks Promptly**: Close tasks when done to keep your list manageable.
   ```bash
   frump close 5
   ```

7. **Check History**: Use `frump history` to understand how a task evolved.

8. **Export for Backup**: Periodically export to JSON for backup.
   ```bash
   frump export --to backup-$(date +%Y%m%d).json
   ```

## Board Location

By default, frump looks for `frump.md` or a `frump/` board directory in the current directory and its parents. You can name a different board:

```bash
frump --board path/to/custom.md list
frump --board docs/tasks.md show 5
frump --board frump list
```

## Getting Help

```bash
# General help
frump --help

# Command-specific help
frump list --help
frump add --help
frump bulk --help
```

## Troubleshooting

### "Task ID must be positive"
- Task IDs start at 1, not 0
- This usually means a parsing error

### "Property key must start with uppercase letter"
- Property names must be capitalized: `Status`, not `status`
- Max 3 words: `Due Date` ✓, `Expected Completion Date` ✗

### "Task history requires a board inside a Git repository"
- `history` needs Git; `add`, `close`, `list --closed` and `stats` also use it when it is there
- Initialize git: `git init`

### "Failed to read the board in commit ..."
- A commit in the board's history holds a task that does not parse, or a board path of the wrong kind
- History reads only task content (task files, or the Tasks section of a single file), so a committed mistake in the header, Team or Next section does not cause this
- Frump stops rather than treat an unreadable history as empty, because that could give out a used task number

### "Failed to read frump.md"
- Make sure frump.md exists in your directory
- Or name the board: `frump --board path/to/frump.md`

### Duplicate IDs after merge
- Run `frump validate`
- Run `frump renumber-duplicates --commit`

## See Also

- [README.md](README.md) - Project overview and concepts
