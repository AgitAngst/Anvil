# demo Specification

## Purpose
TBD - created by archiving change kit-1-0-demo. Update Purpose after archive.

## Requirements

### Requirement: Gallery in the kit
The kit SHALL provide the gallery of all elements and animations as `anvil_ui::demo` behind the off-by-default feature `demo`, so that any program may embed it and the standalone example uses the same code.

#### Scenario: Other programs
- **WHEN** a program depends on `anvil-ui` without the `demo` feature
- **THEN** the gallery code is not compiled and nothing in its behaviour changes

### Requirement: Demo window in Anvil
Anvil SHALL open the gallery in a separate window from the command palette and the menu, with its own theme, language and accent, and SHALL restore the owner's settings when the window closes.

#### Scenario: Switching theme inside the demo
- **WHEN** the owner switches the theme or language in the demo window and closes it
- **THEN** the main window shows the theme, language and accent saved in `anvil.toml`
