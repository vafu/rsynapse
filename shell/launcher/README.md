# rsynapse

**rsynapse** is a modular and extensible launcher backend for the Linux operating system, implemented in Rust. It is designed to operate as a headless daemon that manages data sources through a dynamic plugin system. The service communicates with any user interface via its D-Bus API, thereby establishing a clear separation between backend logic and frontend presentation.

The system is architected for performance and efficiency.

## ⚠️ Disclaimer

This software is currently under active development and should be considered a **Work In Progress**. The architecture, D-Bus API, plugin interface, and installation procedures are subject to frequent and substantial changes. Use it at your own risk.

## Features

* **Modular Architecture**: The system is extensible with dynamically loaded plugins that are compiled as shared libraries (`.so`).
* **D-Bus API**: A language-agnostic interface is provided for UI clients.
* **Centralized Execution**: The daemon is responsible for handling command execution, which enables stateful features such as command history (WIP).
* **rsynapse-cli**: A simple CLI to test things. More here [rsynapse-cli](./rsynapse-cli/)

### Current Plugins

* **Application Launcher**: Indexes and provides fuzzy-search capabilities for `.desktop` files from standard XDG directories.
* **Shell Executor**: Validates and executes shell commands.
* **Calculator**: Evaluates mathematical expressions.
* **History**: Stores and provides access to previously executed commands (WIP).

## Installation

The following instructions are for a manual, user-local installation.

### 1. Prerequisites

Ensure the Rust toolchain is installed.

### 2. Install From The Rsynapse Workspace

From the Rsynapse workspace root, run the local installer:

```bash
./install/local.sh
```

This installs release binaries under `~/.local/bin`, installs launcher plugins
under `~/.local/lib/rsynapse/plugins`, updates D-Bus activation files, and
updates the user systemd units for the shell UI surfaces.

### 3. Manual Executable Install

The `cargo install` command compiles and copies individual executables to a
chosen local root.

```bash
cargo install --path rsynapse-daemon --locked --force --root ~/.local
cargo install --path rsynapse-cli --locked --force --root ~/.local
```

**Note**: Ensure that the `~/.local/bin` directory is included in your shell's `PATH` environment variable. If it is not, add the following line to your shell configuration:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### 4. Install Plugins

The plugin libraries (`.so` files) must be copied to a dedicated directory.

```bash
# Create the plugin directory
mkdir -p ~/.local/lib/rsynapse/plugins/

# Copy the compiled plugins
cp target/release/*.so ~/.local/lib/rsynapse/plugins/
```

### 5. Configure D-Bus Activation

To enable D-Bus to start the daemon automatically on demand, install the
repo-owned activation files:

```bash
./install/local.sh
```

Once this configuration is in place, the daemon no longer needs to be started manually. Any client connecting to the `org.rsynapse.Engine` service will trigger D-Bus to launch it automatically.


## D-Bus API (Unstable)

rsynapse communicates with UI clients via a D-Bus interface on the session bus. This API is designed to be simple and stable.

* **Service Name**: `org.rsynapse.Engine`
* **Object Path**: `/org/rsynapse/Engine1`
* **Interface Name**: `org.rsynapse.Engine1`

### Methods

#### `Search(query: String) -> Array<Struct>`

Takes a search query and returns a sorted list of matching results from all active plugins.

* **Arguments**:
    * `query` (`s`): The user-inputted search term.
* **Returns** (`a(ssss)`): An array of structs. Each struct represents a single result item with the following fields:
    * `id` (`s`): A unique identifier for the result item, used for the `Execute` method.
    * `title` (`s`): The main text to be displayed.
    * `description` (`s`): Sub-text or a description of the item.
    * `icon` (`s`): XDG icon name.

#### `Execute(id: String)`

Instructs the daemon to execute the action associated with a specific result item and, if applicable, add it to the history.

* **Arguments**:
    * `id` (`s`): The unique identifier of the item to execute, as received from a `Search` call.
* **Returns**: None.
