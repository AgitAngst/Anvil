## ADDED Requirements

### Requirement: Account repositories
Anvil SHALL list the repositories of the signed-in GitHub account (owner and organizations) using the token from the Windows credential store, and SHALL show for each one whether it is public or private and whether it is already on disk.

#### Scenario: Token present
- **WHEN** the owner opens "Add project" on the "From GitHub" tab and a token exists
- **THEN** the repositories are listed, connected ones are marked "Connected", local clones found by origin are marked "On disk", the rest offer "Clone"

#### Scenario: No token or rate limit
- **WHEN** there is no token or GitHub refuses the request
- **THEN** the tab shows a plain-language message with the next step ("Add a token in Settings → Forge") and no list

### Requirement: Manual add
Anvil SHALL add a project from any local folder and clone a repository given as a URL or `owner/name`.

#### Scenario: Clone by address
- **WHEN** the owner enters `owner/name` and presses "Clone"
- **THEN** `git clone` runs in the clone folder with visible progress, the project appears in the list on success, and an error shows the reason on failure; the button is unavailable while cloning

### Requirement: Auto-connect
Anvil SHALL connect repositories that already lie in the search folders without any action, and SHALL tell the owner about new account repositories without cloning them by itself.

#### Scenario: New repository on the account
- **WHEN** the account list contains a repository that is neither connected nor on disk
- **THEN** the sidebar shows "New: N" that opens the "From GitHub" tab

### Requirement: Visibility mark
Anvil SHALL keep a public/private mark per project with a GitHub origin, taken from the API and overridable by hand.

#### Scenario: Offline
- **WHEN** GitHub is unreachable
- **THEN** the last known mark is shown, or "unknown" if there is none; a manual value wins over the API value
