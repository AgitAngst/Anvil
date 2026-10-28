## ADDED Requirements

### Requirement: Program store
Anvil SHALL list the programs found in the releases of the account's repositories and connected projects, with the installed version, the latest release and a state, and SHALL install, update or uninstall a program from its release without cloning the repository.

#### Scenario: Install without cloning
- **WHEN** the owner presses "Install" on a program whose repository is not on disk
- **THEN** the release archive is downloaded, its SHA-256 is checked against the release's `SHA256SUMS`, the program is installed, and progress shows on the card

#### Scenario: Repository without releases
- **WHEN** a repository has no release with a Windows archive by the convention
- **THEN** it is hidden by default and listed under "Show without releases"

### Requirement: Update all
Anvil SHALL update every installed program that has a newer release when the owner presses "Update all".

#### Scenario: Several programs are outdated
- **WHEN** two installed programs have newer releases and the owner presses "Update all"
- **THEN** both are downloaded, verified and installed one after another
