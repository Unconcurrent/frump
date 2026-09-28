# Frump Tutorial: Getting Started

Welcome to Frump! This tutorial will walk you through setting up and using Frump for task management in your project.

## What You'll Learn

- Setting up a Frump project
- Creating and managing tasks
- Working with your team
- Using git integration
- Advanced features

## Prerequisites

- Basic command-line knowledge
- Git installed (for history features)
- Rust and Cargo installed (to build Frump)

## Part 1: Installation and Setup

### Authority workflow commands

Every frump option has a long name, and the only unnamed argument a command takes is the task number. Use `frump init` to create an empty board, `frump unset N --property NAME` to remove stale properties, and `frump update N --append TEXT` to retain review or validation evidence. `--notify assignee` also sends the raw text to the task's assignee through Metateam after saving, or to the whole crew when the task has no assignee; `--notify all` broadcasts it to the whole Metateam crew. Dependency properties use `Depends On: 1, 2`; run `frump validate`, `frump depends-on`, `frump dependents`, and `frump ready` to work safely with the dependency graph. `frump next --set 1 2` activates an ordered todo plan: only task 1 may leave `todo`, and completing it removes it from the plan. New statuses and similar new task subjects produce warnings but remain user-controlled.

### Step 1: Build Frump

```bash
# Clone the repository (if you haven't already)
git clone https://github.com/sologub/frump
cd frump

# Build the project
cargo build --release

# The binary is now at target/release/frump
# Optionally, add it to your PATH or create an alias
alias frump='path/to/frump/target/release/frump'
```

### Step 2: Create Your First Frump Project

Let's create a simple web application project:

```bash
# Create and enter your project directory
mkdir my-web-app
cd my-web-app

# Initialize git (important for Frump's history features)
git init

# Create a basic frump.md file
cat > frump.md << 'EOF'
# My Web Application

A simple web application for learning Frump.

## Team

* Alice Johnson <alice@example.com> - Lead Developer
* Bob Smith <bob@example.com> - Backend Developer

## Tasks
EOF

# Commit the initial file
git add frump.md
git commit -m "Initial frump.md setup"
```

### Step 3: Verify Everything Works

```bash
# List tasks (should be empty)
frump list

# Validate the file
frump validate
```

Expected output:
```
No tasks found.
```

```
✓ File structure is valid
✓ All task IDs are unique
✓ Task IDs are sequential
✓ Validation complete: 0 tasks, 2 team members
```

## Part 2: Creating Your First Tasks

### Step 4: Add Some Tasks

Let's add tasks for building our web app:

```bash
# Add a basic task (assigned to first team member by default)
frump add --subject "Set up project structure"

# Add a feature with description
frump add --type Feature --subject "Create user registration" \
  --body "Users should be able to register with email and password"

# Add a bug (even though we haven't started coding yet!)
frump add --type Bug --subject "Fix login redirect issue" \
  --body "Users are redirected to wrong page after login" \
  --assignee "Bob Smith"

# Add a task with status
frump add --subject "Write API documentation" \
  --status "planned" \
  --assignee "Alice Johnson"
```

### Step 5: View Your Tasks

```bash
# List all tasks
frump list
```

Expected output:
```
Task 1 - Set up project structure
  Assigned to: Alice Johnson

Feature 2 - Create user registration
  Assigned to: Alice Johnson

Bug 3 - Fix login redirect issue
  Assigned to: Bob Smith

Task 4 - Write API documentation
  Status: planned
  Assigned to: Alice Johnson
```

### Step 6: Commit Your Changes

Remember, Frump works best with git:

```bash
git add frump.md
git commit -m "Add initial project tasks"
```

## Part 3: Managing Tasks

### Step 7: Working on a Task

Let's say Alice starts working on task 1:

```bash
# Update the status
frump set 1 --property Status --value working

# Check the change
frump show 1
```

Output:
```
### Task 1 - Set up project structure

Status: working
Assigned To: Alice Johnson
```

### Step 8: Adding Details to a Task

Add more information as you work:

```bash
# Set priority
frump set 1 --property Priority --value high

# Set due date
frump set 1 --property "Due Date" --value "2025-12-01"

# Update the body with more details
frump update 1 --body "Set up project structure including:
- Initialize npm project
- Configure webpack
- Set up testing framework"

# View the updated task
frump show 1
```

### Step 9: Completing a Task

When you finish a task:

```bash
# Mark it done and commit, so history keeps the final text
frump set 1 --property Status --value done
frump commit --message "Finish task 1"

# Close the task
frump close 1

# Commit with a descriptive message
frump commit --message "Complete task 1: Project structure set up"
```

## Part 4: Filtering and Searching

### Step 10: Filter Tasks

```bash
# See only bugs
frump list --type Bug

# See what Alice is working on
frump list --assignee "Alice Johnson"

# See all tasks with status "working"
frump list --status working

# Combine filters: bugs assigned to Bob
frump list --type Bug --assignee "Bob Smith"
```

### Step 11: Search for Tasks

```bash
# Search in task subjects and bodies
frump search --text "registration"

# Also print the start of each matching body
frump search --text "login" --show-body
```

### Step 12: Get Statistics

```bash
# See project overview
frump stats
```

Output:
```
Task Statistics

Total tasks: 3

By Type:
  Feature: 1
  Bug: 1
  Task: 1

By Status:
  working: 1
  (no status): 2

By Assignee:
  Alice Johnson: 2
  Bob Smith: 1
```

## Part 5: Git Integration

### Step 13: View Task History

```bash
# See the complete history of a task
frump history 1
```

Output shows all commits that affected task 1:
```
History for Task 1:

✓ Created by Alice Johnson on 2025-11-24 10:00
  Commit: a3f8b901
  Message: Add initial project tasks

• Modified by Alice Johnson on 2025-11-24 10:15
  Commit: c2e4d678
  Message: Update task 1 status to working

✗ Deleted by Alice Johnson on 2025-11-24 11:30
  Commit: 9f2b3c45
  Message: Complete task 1: Project structure set up
```

### Step 14: View Closed Tasks

```bash
# See all tasks you've completed
frump list --closed
```

Output:
```
Closed tasks:

Task 1 - Set up project structure

Total: 1 closed tasks
```

## Part 6: Advanced Features

### Step 15: Create a Template

Let's create a template for bug reports:

```bash
# Create bug template
frump template add --name bug --subject "Fix {component} bug" \
  --type Bug \
  --body "Bug found in {component}.

Steps to reproduce:
{steps}

Expected behavior:
{expected}

Actual behavior:
{actual}"

# List your templates
frump template list
```

### Step 16: Use the Template

Now when you find a bug:

```bash
# See what the template needs
frump template show --name bug

# Create a task from it; every {placeholder} needs a --fill
frump add --template bug \
  --fill component=login \
  --fill steps="Log in with a new account" \
  --fill expected="The dashboard opens" \
  --fill actual="The settings page opens"
```

The template file `.frump_templates.json` sits next to `frump.md`; commit it so the team shares the templates.

### Step 17: Bulk Operations

Let's say you finish multiple tasks at once:

```bash
# First, mark them as done
frump set 2 --property Status --value done
frump set 3 --property Status --value done
frump set 4 --property Status --value done

# Close all done tasks at once (nothing closes if one of them is not ready)
frump bulk close --with-status done

# Commit
frump commit --message "Close all completed tasks"
```

### Step 18: Export Your Tasks

Backup or share your tasks:

```bash
# Export to JSON
frump export --to backup.json

# Export to CSV for spreadsheet
frump export --format csv --to tasks.csv
```

## Part 7: Team Collaboration

### Step 19: Simulate a Merge Conflict

In real projects with multiple team members, you might get ID conflicts:

```bash
# Create a scenario:
# 1. Create a branch
git checkout -b feature/new-tasks

# 2. Add a task
frump add --subject "Implement caching" --type Feature

# 3. Go back to main and add a different task
git checkout main
frump add --subject "Add error handling" --type Task

# 4. Merge the branch
git merge feature/new-tasks
# If both tasks got ID 5, we have a conflict!
```

### Step 20: Resolve Conflicts

```bash
# Check for conflicts
frump validate

# If duplicate numbers are found, renumber them
frump renumber-duplicates --commit

# Verify everything is good
frump validate
frump list
```

## Part 8: Daily Workflow

### A Typical Day with Frump

**Morning:**
```bash
# See what you're working on
frump list --assignee "Your Name" --status working

# Review all open tasks
frump list --status open
```

**During Development:**
```bash
# Found a bug while coding
frump add --type Bug --subject "Null pointer in user profile" \
  --body "Error occurs when user has no avatar" \
  --status "working"

# Start working on planned task
frump set 7 --property Status --value working

# Update task as you learn more
frump update 7 --body "Additional details discovered during implementation..."
```

**Code Review:**
```bash
# Teammate asks for a bug fix
frump add --type Bug --subject "Memory leak in connection pool" \
  --assignee "Teammate Name"

# Update task based on review comments
frump set 10 --property Priority --value high
frump set 10 --property "Review Status" --value "changes requested"

# Record the review and tell the assignee
frump update 10 --append "Changes requested: see the review notes" --notify assignee
```

**End of Day:**
```bash
# Complete finished tasks (their Status is done and committed)
frump close 8
frump close 9

# Commit your changes
frump commit --message "Update tasks: completed #8 and #9"

# See tomorrow's work
frump list --status working
```

## Best Practices

### 1. Commit Frequently
Every time you modify frump.md, commit it with a clear message:
```bash
frump commit --message "Add task 15: Implement notification system"
```

### 2. Use Descriptive Task Names
- Good: "Fix memory leak in WebSocket connection handler"
- Bad: "Fix bug"

### 3. Add Context in Task Body
```bash
frump add --subject "Optimize database queries" \
  --body "Current queries are slow for large datasets.
Target: < 100ms for 1M records.
Focus on user search and report generation."
```

### 4. Keep Tasks Actionable
Each task should be something one person can complete:
- Good: "Add password reset email template"
- Bad: "Improve security" (too vague)

### 5. Use Properties Consistently
Decide on property names as a team:
- Status values: open, working, review, done, blocked
- Priority values: low, medium, high, critical

### 6. Regular Cleanup
```bash
# Weekly: close completed tasks
frump bulk close --with-status done
frump commit --message "Weekly cleanup: close completed tasks"

# Monthly: export for records
frump export --to "archive/tasks-$(date +%Y-%m).json"
```

## Next Steps

Now that you've completed the tutorial:

1. **Read the [USAGE.md](USAGE.md)** for complete command reference
2. **Review [README.md](README.md)** for concepts and philosophy

## Common Questions

### Q: Can I have multiple frump.md files?
Yes! Use `--board`:
```bash
frump --board docs/tasks.md list
frump --board backend/tasks.md show 5
```

### Q: What if I don't use git?
Frump works without git, but you'll lose:
- Task history (`frump history`)
- Closed task list (`frump list --closed`)
- Automatic ID conflict prevention

### Q: Can I customize the task types?
Yes! Use any type you want:
```bash
frump add --type Chore --subject "Update dependencies"
frump add --type Documentation --subject "Write API guide"
```

### Q: How do I recover a deleted task?
Check git history:
```bash
# See closed tasks, in their last committed state
frump list --closed

# View when it was closed
frump history <task_id>

# Restore from git if needed
git log --all -- frump.md
git show <commit-hash>:frump.md > frump.md.old
# Copy the task back manually
```

### Q: Can I automate frump?
Yes! Frump is scriptable:
```bash
# Add tasks from a script
for feature in feature1 feature2 feature3; do
  frump add --type Feature --subject "Implement $feature"
done

# Export for CI/CD
frump export --to tasks.json
# Process tasks.json in your pipeline
```

## Troubleshooting

### Tasks Not Showing Up
```bash
# Check file location
pwd
ls frump.md

# Validate file
frump validate
```

### Git History Not Working
```bash
# Make sure you're in a git repo
git status

# Make sure frump.md is committed
git log -- frump.md
```

### Property Validation Errors
Property keys must:
- Start with uppercase letter
- Be maximum 3 words

```bash
# Wrong:
frump set 1 --property priority --value high           # lowercase
frump set 1 --property "expected completion date" --value "2025-12-31"  # 3+ words

# Right:
frump set 1 --property Priority --value high
frump set 1 --property "Due Date" --value "2025-12-31"
```

## Congratulations!

You've completed the Frump tutorial. You now know how to:
- ✅ Set up a Frump project
- ✅ Create and manage tasks
- ✅ Use git integration
- ✅ Filter and search tasks
- ✅ Use advanced features
- ✅ Work effectively in a team

Happy task management! 🎉
