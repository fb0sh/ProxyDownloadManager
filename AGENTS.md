
# AGENTS.md

## AI Development Workflow

This section defines how AI agents should approach development tasks in this project.
The Release Management Policy below governs version bump and release decisions.

---

### 1. Task Entry: Clarify Before Coding

Before making any changes:

- **State your understanding** of what needs to change and why
- **List files you expect to modify** — let the user confirm before you start
- **If requirements are ambiguous**, ask clarifying questions, don't guess
- **If a simpler approach exists**, propose it

Exception: trivial changes (typo fix, single-line config) can skip this step.

---

### 2. Implementation Rules

- Follow [CLAUDE.md](./CLAUDE.md) behavioral guidelines: Think Before Coding, Simplicity First, Surgical Changes, Goal-Driven Execution
- Each change should be **minimal and focused** — one concern per commit
- Touch **only the files needed** for the task
- If you discover unrelated issues during work, **mention them but don't fix without asking**

---

### 3. Before Requesting Commit

Run the following verification locally:

```bash
# Rust
cargo check                    # type-check backend
cargo test                     # run backend tests

# TypeScript / Frontend
npx tsc --noEmit              # type-check frontend
pnpm test                     # run frontend tests
```

✅ All must pass before presenting changes to the user.

---

### 4. Changelog Discipline

Every user-facing change MUST update `CHANGELOG.md`:

- Before requesting commit, add an entry under `## [Unreleased]`
- Use the format:
  ```
  ### Added       — new features
  ### Fixed       — bug fixes
  ### Changed     — modifications to existing functionality
  ### Removed     — removed features
  ### Docs        — documentation changes
  ```
- Each entry should be a **single line** describing what changed and why
- See `CHANGELOG.md` for examples

**The change log entry is part of the change.** Don't skip it.

---

### 5. Commit Convention

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add download rate limiter
fix: handle connection timeout on slow proxies
docs: update browser extension installation guide
refactor: extract chunk computation into engine/chunk.rs
chore: bump version to 0.4.1
```

Scope is optional but encouraged when the change is scoped to one area:

```
fix(macos): deploy browser extensions to Application Support
feat(extension): add context menu for video/audio links
```

---

### 6. Authorization

The AI agent MUST:

- **Ask before committing** — do not commit without the user's explicit approval
- **Show the diff** or a summary of what will be committed
- **Ask before pushing** — do not push without confirmation (unless the task explicitly says "commit and push")

Exception: if the user says "git add commit push" or equivalent, proceed directly.

---

### 7. Version String Sync

When bumping the version for a release, update ALL of these to match:

| File | Field |
|------|-------|
| `src-tauri/Cargo.toml` | `version` |
| `src-tauri/tauri.conf.json` | `version` |
| `package.json` | `version` |

Without a release PR, the version should only change when the user explicitly asks.

---

### 8. Error Recovery

If a change breaks something:

1. **Revert immediately** — undo the last change, don't stack fixes
2. **Re-analyze** — what was the actual root cause?
3. **Try again** — with a corrected approach

Do not attempt more than 2 fix iterations without escalating to the user.

---

## Release Management Policy

This document defines how AI agents should decide whether a change requires a new software release.

The AI agent MUST NOT arbitrarily bump versions.
Version changes must follow the release workflow described below.

---

# 1. Release Philosophy

A release represents a user-facing milestone.

A Pull Request (PR) is a code change.
A Release is a collection of validated changes delivered to users.

Do NOT create a release for every PR.

The normal workflow is:

```

Feature/Fix PR
|
v
Merge into main
|
v
Release decision
|
v
Release PR
|
v
Git Tag
|
v
Published Release

```

---

# 2. When to Consider a Release

The AI agent should evaluate user requests and recent changes.

A release SHOULD be considered when one or more conditions are met:

## Feature Release

Recommend a MINOR version bump when:

- A new user-visible feature is added
- A new module/component is introduced
- Existing capability is significantly expanded
- New API functionality is added without breaking compatibility

Example:

```

1.4.0 -> 1.5.0

```

Examples:

- Add Worker management
- Add Plugin/Mod system
- Add new scanning engine
- Add new UI module


---

## Bugfix Release

Recommend a PATCH version bump when:

- Fixing bugs
- Improving stability
- Fixing crashes
- Fixing incorrect behavior
- Security fixes that do not change APIs

Example:

```

1.5.0 -> 1.5.1

```

Examples:

- Fix WebSocket reconnect issue
- Fix incorrect configuration parsing
- Fix UI display issue


---

## Breaking Release

Recommend a MAJOR version bump when:

- Existing APIs are incompatible
- Configuration format changes incompatibly
- Database migration breaks old data
- Protocol changes require all clients to update

Example:

```

1.x.x -> 2.0.0

```


---

# 3. AI Decision Rules

When the user says things like:

## "这个功能完成了吗？"

The AI should answer:

- Whether the feature is complete
- Whether it is release-worthy
- Suggested version impact

Example:

```

该功能属于新增能力。

建议：
Release required: YES
Version type: MINOR
Reason:

* Adds new user-facing capability

```

---

## "我修了几个bug"

The AI should evaluate:

- Number of fixes
- Severity
- User impact

Example:

```

当前修改属于稳定性修复。

建议：
Release required: YES
Version type: PATCH

```

---

## "准备上线"

The AI should:

1. Review recent changes
2. Generate release checklist
3. Recommend version
4. Suggest creating Release PR


---

# 4. Release PR Rules

A Release PR should ONLY contain release-related changes.

Allowed:

```

CHANGELOG.md
version files
release notes
documentation updates

```

Not allowed:

```

new features
bug fixes
refactoring
large code changes

```

Example:

```

Release v1.5.0

Changes:

* Update version
* Update CHANGELOG
* Update documentation

```

---

# 5. Changelog Rules

Every release should contain:

```

## vX.Y.Z

### Features

* New features

### Fixes

* Bug fixes

### Improvements

* Performance or UX improvements

### Breaking Changes

* Incompatible changes

```

---

# 6. Release Checklist

Before recommending release:

The AI agent should verify:

- [ ] All related PRs merged
- [ ] Tests passed
- [ ] Documentation updated if needed
- [ ] CHANGELOG updated
- [ ] Version number follows SemVer
- [ ] No unfinished features remain

---

# 7. Version Rules

Follow Semantic Versioning:

```

MAJOR.MINOR.PATCH

```

Rules:

```

PATCH:
Bug fixes only

MINOR:
Backward-compatible features

MAJOR:
Breaking changes

```

During 0.x development:

```

0.MINOR.PATCH

```

is preferred.

Example:

```

0.5.0
0.6.0
0.6.1
1.0.0

```

---

# 8. AI Output Format

When evaluating release decisions, use:

```

Release Decision:

Required:
YES / NO

Suggested Version:
x.y.z

Version Type:
MAJOR / MINOR / PATCH

Reason:

* item 1
* item 2

Recommended Action:

* Create Release PR
* Update CHANGELOG
* Create Git Tag

```

---

# 9. Important Restrictions

The AI agent must NOT:

- Automatically bump versions after normal PRs
- Create releases without user confirmation
- Mix feature changes into Release PR
- Skip changelog updates
- Use arbitrary version numbers

The AI agent should act as a release manager assistant, not an automatic publisher.

