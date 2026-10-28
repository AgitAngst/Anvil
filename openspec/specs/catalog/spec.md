# catalog Specification

## Purpose
TBD - created by archiving change app-1-0-catalog. Update Purpose after archive.

## Requirements

### Requirement: Program catalog
Anvil SHALL list the programs of connected projects with the installed version, the latest GitHub release and a state, and SHALL install or update one program or all programs of a state from the releases.

#### Scenario: Update all
- **WHEN** the owner presses "Update all"
- **THEN** every installed program with a newer release is installed from that release

### Requirement: Moving to another PC
Anvil SHALL export the repositories of connected projects as a text list and SHALL clone a pasted list one by one into the clone folder, skipping repositories that already lie on disk.

#### Scenario: Pasted list on a new PC
- **WHEN** the owner pastes a list of repositories and presses "Connect from the list"
- **THEN** the repositories not yet on disk are cloned one by one and appear as projects
