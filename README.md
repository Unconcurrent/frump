# Frump

Distributed task management tool based on Git and Markdown. Manage tasks alongside your code with full version history.

Frump automatically discovers `frump.md` from the current directory upward, so commands work from project subdirectories without an explicit file option.

## Why Frump?

**Problem**: Traditional task management tools keep tasks separate from code, making it hard to see the full project picture from git history alone. Teams also lack truly distributed task collaboration.

**Solution**: Frump stores tasks as Markdown in your git repository. When you clone a repo, you get code, current tasks, and complete task history. Tasks can be branched, merged, and versioned just like code.

## Philosophy

**No tools required**: Frump uses simple conventions in a `frump.md` file. You can read and edit tasks with any text editor.

**Git-native**: Tasks live in git alongside code. `git pull` gets new tasks, `git push` shares your changes. Task history is preserved in git commits.

**Simple & readable**: Everything is plain Markdown. No databases, no servers, just files and git.

## Quick Start

### Installation

```bash
# Build from source
cargo build --release

# Binary will be at target/release/frump
# Optionally copy to your PATH
cp target/release/frump /usr/local/bin/
```

### Create your first project

```bash
cd my-project
git init

# Create initial frump.md
cat > frump.md << 'EOF'
# My Project

## Team

* Your Name <you@example.com> - Lead Developer

## Tasks
EOF

git add frump.md
git commit -m "Initialize frump"
```

### Basic commands

```bash
# Add a task
frump add --subject "Implement user authentication"

# List all tasks
frump list

# View task details
frump show 1

# Mark task as working, then done
frump set 1 --property Status --value working
frump set 1 --property Status --value done

# Close completed task
frump close 1
frump commit --message "Complete task 1"
```

### Local Kanban board

```bash
frump web
# Open http://127.0.0.1:3000
```

The board edits `frump.md` directly and refreshes when the file changes externally. Its Delete button follows the same rules as `frump close`.

### Authority workflow

Frump supports `Depends On: 1, 2` task properties, validates dependency graphs, and provides `depends-on`, `dependents`, and `ready` queries; a prerequisite is satisfied when it is done or closed. Use `update --append` to preserve review and validation evidence in the durable task record; it adds a UTC timestamp and, when available, the Metateam crew member. `update --append ... --notify assignee` additionally sends the raw appended text to the task's assignee after saving it, or to all Metateam crew members when the task has no assignee. `--notify all` sends it to all Metateam crew members, whoever the assignee is. Assignment announcements go to the new assignee. `Last Updated` is maintained automatically for every created or changed task. Properties are compact metadata limited to 40 bytes; place longer evidence in the body. Use `unset` to remove stale properties. `close` requires a `done` task whose prerequisites are satisfied; when Git does not have the task's current text, it prints that text so it can be restored. Use `frump next --set 12 15` to activate an ordered todo plan: only its front task may leave `todo`, and a completed task is removed automatically. `frump list --property "Review=approved" --missing Evidence --sort last-updated --desc --format json` provides filtered machine-readable task data. Metateam is optional: when the `metateam` command is not on `PATH`, Frump saves the change and skips the notification instead of failing. A notification that Metateam cannot deliver, for example to an assignee who is not a crew member, prints a warning; the saved change stands.

Use `frump commit --message "short message"` to stage and commit only the task board; it never pushes.

Every option has a long name, there are no short flags, and a command takes at most one unnamed argument: the task number. `--board PATH` names the board when it is not found from the current directory.

### Sharded storage

Frump reads two board layouts. A single `frump.md` file keeps the whole board in one file, which suits small boards. A directory board keeps the shared `frump/general.md` and one Markdown file per task under `frump/tasks/`, so an edit rewrites only the file that changed instead of the entire board — the layout to use for a heavily edited board, where rewriting everything on every change is unnecessary write volume on the SSD. `frump migrate` converts a single file into the directory layout, and `--board frump` selects the migrated board.

## CLI Reference

### Task Management

```bash
# Add tasks
frump add --subject "Task subject"
frump add --type Bug --subject "Fix login issue" --body "Details here"
frump add --subject "New feature" --assignee "John Doe" --status working

# List and filter
frump list                              # All tasks
frump list --type Bug                   # Only bugs
frump list --status working             # Tasks with status "working"
frump list --assignee "John Doe"        # Assigned to John

# View and modify
frump show 5                                        # Show task details
frump assign 5 --assignee "Jane Smith"              # Reassign task
frump set 5 --property Priority --value high        # Set property
frump unset 5 --property Priority                   # Remove property
frump update 5 --subject "New title"                # Update subject
frump update 5 --append "Evidence" --notify assignee  # Dated update plus message
frump close 5                                       # Close task
```

### Search & Analysis

```bash
frump search --text "authentication"          # Search subjects and bodies
frump search --text "login" --show-body       # Also print the start of each body
frump stats                                   # Show statistics
frump validate                                # Check board integrity (exit code 1 on failure)
```

### Git Integration

```bash
frump history 5                    # Show task lifecycle from git
frump list --closed                # List all closed tasks
```

### Advanced Features

```bash
# Templates
frump template add --name bug --subject "Fix {component} issue" --type Bug
frump add --template bug --fill component=parser
frump template list

# Bulk operations
frump bulk close --with-status done
frump bulk assign --with-type Bug --assignee "QA Team"
frump bulk set --with-status done --property Release --value v2.0

# Import/Export
frump export --to backup.json
frump export --format csv --to tasks.csv
frump import --from backup.json --merge
```

### Conflict Resolution

When merging branches with conflicting task IDs:

```bash
frump validate                          # Reports duplicate numbers
frump renumber-duplicates --commit      # Give duplicates new numbers and commit
```

## Format Specification

### File Structure

```markdown
# Project Title

Brief description of the project.

## Team

* John Doe <john@example.com> - Lead Developer
* Jane Smith <jane@example.com> - QA Engineer

## Tasks

### Task 1 - Implement authentication

Add login and registration with email/password.

Status: working
Assigned To: John Doe
Priority: high

### Bug 2 - Fix navigation

The menu doesn't close properly on mobile devices.

Status: open
Assigned To: Jane Smith
```

### Task Format

```
### <Type> <ID> - <Subject>

<Body text - optional, multiple lines>

<Property>: <Value>
<Property>: <Value>
```

**Task ID**: Positive integer, unique across all history. CLI automatically assigns next available ID.

**Task Type**: Task, Bug, Issue, Feature, or custom. Default: Task.

**Subject**: Brief description (not a title, don't capitalize like a title).

**Properties**: Name-value pairs. Names must be Capitalized and max 3 words. Common properties:
- `Status`: open, working, review, done, blocked
- `Assigned To`: Team member name
- `Priority`: low, medium, high
- `Tags`: Comma-separated tags
- `Due Date`: Target date

## Common Workflows

### Daily Development

```bash
# Morning: check your tasks
frump list --assignee "Your Name" --status working

# Found a bug while coding
frump add --type Bug --subject "Null pointer in profile page"

# Update task as you work
frump set 5 --property Status --value working

# Complete task
frump set 5 --property Status --value done
frump close 5
frump commit --message "Complete task 5: Add user profile"
```

### Code Review

```bash
# Reviewer adds issues
frump add --type Bug --subject "Missing error handling" --assignee "Developer"

# Developer addresses issue
frump set 10 --property Status --value working
# ... make fixes ...
frump set 10 --property Status --value done
frump close 10
frump commit --message "Fix task 10: Add error handling"
```

### Release Planning

```bash
# See all planned features
frump list --type Feature

# Mark done features for release
frump bulk set --with-status done --property Release --value v2.0

# Export release tasks
frump export --to release-v2.0.json
```

### Merge Conflict Resolution

```bash
# After merging branches
git merge feature/new-feature

# Check for ID conflicts
frump validate
# ✗ Found duplicate task IDs: [TaskId(5), TaskId(8)]

# Auto-resolve
frump renumber-duplicates --commit
# ✓ Resolved 2 duplicate task ID(s):
#   5 → 12: New feature
#   8 → 13: Bug fix
```

## Features

### ✅ Implemented

- **Strong typing** with validation (TaskId, PropertyKey, Email)
- **20+ commands** for complete task management
- **Git integration** with history tracking and conflict prevention
- **Smart ID assignment** that checks git history
- **Templates** for reusable task patterns
- **Bulk operations** for batch modifications
- **Import/Export** (JSON and CSV formats)
- **Conflict resolution** for merge scenarios
- **Metateam integration**: task notifications and assignment announcements ([metateam.ai](https://metateam.ai))
- **Property-based testing** with 60+ tests passing
- **Comprehensive docs** (USAGE.md, TUTORIAL.md)

### Architecture

```
src/
├── domain/        # Core types (Task, Team, Properties)
├── parser/        # Markdown parsing/serialization
├── git/           # Git history integration
├── export/        # JSON/CSV import/export
└── templates/     # Task template system
```

**Type system**: Validated newtypes prevent invalid states at compile time.

**Testing**: 56 unit tests + 4 doctests with property-based round-trip testing.

## Documentation

- **[USAGE.md](USAGE.md)**: Complete command reference with examples
- **[TUTORIAL.md](TUTORIAL.md)**: Step-by-step walkthrough for new users

## Examples

### Statistics Output

```
$ frump stats
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

By Assignee:
  John Doe: 7
  Jane Smith: 5

Closed tasks: 8
```

### Task History

```
$ frump history 5
History for Task 5:

✓ Created by John Doe on 2025-11-24 10:00
  Commit: a3f8b901
  Message: Add authentication task

• Modified by Jane Smith on 2025-11-24 11:30
  Commit: c2e4d678
  Message: Update priority to high

✗ Deleted by John Doe on 2025-11-24 15:45
  Commit: 9f2b3c45
  Message: Complete authentication feature
```

## Best Practices

1. **Commit frequently**: Every frump.md change should be committed with a clear message
2. **Descriptive subjects**: "Fix memory leak in WebSocket handler" not "Fix bug"
3. **Add context**: Use task body for implementation details and requirements
4. **Use filters**: `frump list --status working` to focus on active tasks
5. **Close promptly**: Close tasks when done to keep the list clean
6. **Check history**: Use `frump history` to understand task evolution

## Why "frump"?

Because with git they make a lovely couple.

## Contributing

The CLI is in `src/main.rs`. The rules the CLI and the web board share (board path, task numbers, prerequisites, closing, input protection) are in `src/board.rs`; Git history is in `src/git`, board files in `src/storage.rs` and `src/parser`, and the web board in `src/web.rs`. Run `cargo test` before sending a change.

## License

MIT
