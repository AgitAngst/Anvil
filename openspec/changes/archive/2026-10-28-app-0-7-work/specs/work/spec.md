## ADDED Requirements

### Requirement: Progress of long tasks
Anvil SHALL show the plans written by the progress skill with a bar, remaining time estimate and stage states, and SHALL notify when a running plan becomes finished.

#### Scenario: Plan finishes
- **WHEN** a plan that was running in the previous reading is finished in the next one
- **THEN** a toast appears, and a Windows notification when the window is not in front

### Requirement: Claude sessions
Anvil SHALL list Claude sessions touched in the last day with project, first prompt and an activity mark, reading the files without changing them.

#### Scenario: Session touched an hour ago
- **WHEN** a Claude session file was changed within the last day
- **THEN** the session is listed with its project and first prompt, and marked "working" only if it changed within the last two minutes
