## ADDED Requirements

### Requirement: Module manifest
Anvil SHALL describe each installable server by a manifest (identifier, name, release source per architecture, service name, ports, contract commands, settings form, actions) and SHALL reject a manifest that lacks an identifier, a release source or the `install` command, naming the missing field.

#### Scenario: Incomplete manifest
- **WHEN** a manifest file has no `install` command
- **THEN** the module is not offered and the settings list the file with the reason "no install command"

### Requirement: Hidden modules
Anvil SHALL show a module marked `hidden` only while the "Hidden modules" switch in the settings is on.

#### Scenario: Switch off
- **WHEN** a manifest has `hidden = true` and the switch is off
- **THEN** the module is absent from the install choice and from the machine's module list

### Requirement: Machine key pinning
Anvil SHALL send nothing to a machine, including a password, before the owner confirmed the SSH host key fingerprint, and SHALL refuse to connect, with a separate dialog, when a confirmed key changes.

#### Scenario: Changed key
- **WHEN** a machine presents a host key different from the confirmed one
- **THEN** no credentials are sent and the dialog shows the old and the new fingerprint

### Requirement: Secrets outside the config
Anvil SHALL store passwords, key passphrases and release tokens only in the system credential store and SHALL NOT write them to the settings file or the work log.

#### Scenario: Token in output
- **WHEN** a command output contains `token=<value>`
- **THEN** the work log shows the value replaced by an ellipsis

### Requirement: Install by contract
Anvil SHALL install a module by uploading the verified release file to a temporary directory on the machine, running the manifest's `install` command with the form values, and removing the temporary directory afterwards.

#### Scenario: Checksum mismatch
- **WHEN** the downloaded file does not match its entry in `SHA256SUMS`
- **THEN** nothing is uploaded and the task fails with "checksum mismatch"

### Requirement: Verbatim commands
Anvil SHALL show any command beyond the manifest's contract commands (actions, console) verbatim and run it only after confirmation.

#### Scenario: Confirmed action
- **WHEN** the owner presses an action marked `confirm` and confirms
- **THEN** the command runs and the work log shows its output and exit code

### Requirement: Status from the machine
Anvil SHALL take a module's state from the output of its `status --json` command and SHALL show unknown fields of that object as extra lines.

#### Scenario: Service stopped
- **WHEN** `status --json` reports `"running": false`
- **THEN** the module card shows "stopped" and offers "Start"

### Requirement: Instances
Anvil SHALL, for a module with `instances = true`, give each instance its own service, user, data directory and ports, and SHALL refuse a port that another instance uses or the machine already listens on.

#### Scenario: Busy port
- **WHEN** the owner asks for a port that is listened on
- **THEN** the form says which port is busy and nothing is installed
