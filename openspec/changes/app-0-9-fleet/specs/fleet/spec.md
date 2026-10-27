## ADDED Requirements

### Requirement: Fleet probes
Anvil SHALL probe each fleet entry by its TCP port or HTTP address and show state, latency and version, refreshing while the view is open.

#### Scenario: Port closed
- **WHEN** the entry's TCP port refuses the connection
- **THEN** the entry shows "not answering" with the system's error text

### Requirement: Owner-written commands
Anvil SHALL run an entry's update or rollback command only after showing the command verbatim and getting a confirmation, and SHALL report the exit code and the tail of the output.
