# guard Specification

## Purpose
TBD - created by archiving change app-0-6-guard. Update Purpose after archive.

## Requirements

### Requirement: Rule checks per project
Anvil SHALL check each connected project against the owner's rules and show the findings with a severity (violation, warning, note) on the project's "Rules" tab, in the sidebar and in the overview.

#### Scenario: Public repository with a listed word
- **WHEN** a public repository contains a word from the owner's word list in a tracked text file or in one of the last 300 commit messages
- **THEN** the finding names the word and the place (`file:line` or the commit) as a violation

#### Scenario: Version scheme
- **WHEN** a project's version is 0.10 or higher in the 0.x series
- **THEN** a violation says that after 0.9.x comes 1.0.0

### Requirement: Optional release block
Anvil SHALL block the release step of the wizard while a project has violations when the owner has turned the block on, and SHALL only warn otherwise.

#### Scenario: Block on
- **WHEN** the block is on and the project has at least one violation
- **THEN** the publish button is unavailable and a banner points to the Rules tab

### Requirement: Word list outside git
The word list SHALL live in a file next to `anvil.toml` that is not part of any repository.

#### Scenario: Word file next to the settings
- **WHEN** the owner opens the word file from Settings → Forge
- **THEN** the file `guard-words.txt` next to `anvil.toml` is created if missing and opened, and it is not tracked by any repository
