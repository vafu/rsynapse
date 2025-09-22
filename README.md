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

### 2. Compile Release Binaries

From the root of the project directory, build the optimized binaries:

```bash
cargo build --release
```

All compiled artifacts will be located in the `target/release/` directory.

### 3. Install Executables

The `cargo install` command compiles and copies the daemon and CLI executables to `~/.cargo/bin/`.

```bash
cargo install --path rsynapse-daemon
(optional) cargo install --path rsynapse-cli
```

**Note**: Ensure that the `~/.cargo/bin` directory is included in your shell's `PATH` environment variable. If it is not, add the following line to your `~/.bashrc` or `~/.zshrc` configuration file:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
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

To enable D-Bus to start the daemon automatically on demand, a service file must be created.

Create the file `~/.local/share/dbus-1/services/com.rsynapse.Launcher.service` with the following content:

```ini
[D-BUS Service]
Name=com.rsynapse.Launcher
Exec=/home/YOUR_USERNAME/.cargo/bin/rsynapse-daemon
```

Once this configuration is in place, the daemon no longer needs to be started manually. Any client connecting to the `com.rsynapse.Launcher` service will trigger D-Bus to launch it automatically.


## D-Bus API (Unstable)

rsynapse communicates with UI clients via a D-Bus interface on the session bus. This API is designed to be simple and stable.

* **Service Name**: `com.rsynapse.Launcher`
* **Object Path**: `/org/rsynapse/Launcher1`
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
