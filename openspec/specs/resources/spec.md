# resources Specification

## Purpose
TBD - created by archiving change app-0-8-resources. Update Purpose after archive.

## Requirements

### Requirement: Resources view
Anvil SHALL show free space on the disks that matter, the SMART data Windows provides for physical disks without thresholds, the size of each project's build folder with a clean action, and the estimated GitHub Actions minutes of private repositories for the current month against a configurable monthly quota.

#### Scenario: No wear data
- **WHEN** Windows returns health only
- **THEN** the disk shows its health and a note that wear and temperature need administrator rights
