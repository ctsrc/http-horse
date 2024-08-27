# http-horse 🐴

[![Crates.io](https://img.shields.io/crates/v/http-horse.svg)](https://crates.io/crates/http-horse)

`http-horse` is a HTTP Hot Reload Server designed for web development.

With `http-horse`, your web pages stay current as you code – automatically refreshing
HTML, CSS, JavaScript/TypeScript, and WebAssembly giving you a tighter feedback loop
so you can stay in the zone.

**Note:** `http-horse` currently only supports macOS 🍎💻.
Support for other operating systems (Linux, FreeBSD) is planned but not yet available.
[Track the progress in issue #1](https://github.com/ctsrc/http-horse/issues/1).

## Table of Contents

- [Installation](#installation)
- [Building `http-horse` from git repo sources](#building-http-horse-from-git-repo-sources)
- [Usage](#usage)
  - [Basic Usage](#basic-usage)
  - [Automatic Browser Launch](#automatic-browser-launch)
  - [Status Web-UI Color Schemes](#status-web-ui-color-schemes)
  - [Editing your Project Source Files](#editing-your-project-source-files)
  - [Rebuilding your Project](#rebuilding-your-project)
  - [Viewing Changes](#viewing-changes)
- [Future Enhancements](#future-enhancements)
  - [Tighter Integration with Existing Build Systems](#tighter-integration-with-existing-build-systems)
  - [Modular Web Development Platform](#modular-web-development-platform)
    - [Key Features](#key-features)
    - [Customizable Themes and Plugins](#customizable-themes-and-plugins)
- [License](#license)

## Installation

In the future, pre-built binaries will be provided for installation.

For now, please build from git repo sources (described in the next section),
or use cargo to install the latest release from crates.io:

```zsh
cargo install -f http-horse
```

## Building `http-horse` from git repo sources

Ensure you have [Rust](https://www.rust-lang.org/) installed on your macOS system.
Then, you can clone this repository and build the application using Cargo:

```zsh
git clone https://github.com/ctsrc/http-horse.git
cd http-horse
cargo build --release
```

## Usage

### Basic Usage

To serve a directory containing your web project's output files,
use the following command:

```zsh
RUST_LOG=debug cargo run --release -- ./example_web_project/out/
```

This command starts `http-horse`, which will serve both a status page
and your project's pages. The output will provide the URLs for both servers:

```text
2023-10-29T05:06:49.278038Z  INFO http_horse: Status pages will be served on http://[::1]:59917
2023-10-29T05:06:49.278089Z  INFO http_horse: Project pages will be served on http://[::1]:59918
```

Open these URLs in your web browser to view the status and project pages.

### Automatic Browser Launch

To automatically open the status and project pages in your default web browser,
use the `--open` option (`-o` for short):

```zsh
RUST_LOG=debug cargo run --release -- --open ./example_web_project/out/
```

### Status Web-UI Color Schemes

The status web-UI supports five built-in color schemes:

- **Midnight Purple** (Dark Mode)
- **Slate Green** (Dark Mode)
- **Abyss Blue** (Dark Mode)
- **Graphite & Copper** (Dark Mode, default)
- **Crimson & Charcoal** (Dark Mode)

You can select a color scheme using the `--color-scheme` (`-c`) option. For example:

```zsh
RUST_LOG=debug cargo run --release -- -c crimson-and-charcoal --open ./example_web_project/out/
```

The corresponding argument values for the available color schemes are as follows:

- `midnight-purple`
- `slate-green`
- `abyss-blue`
- `graphite-and-copper`
- `crimson-and-charcoal`

### Editing your Project Source Files

To make changes to your project, edit your project source files
using your favorite code editor. For example:

```zsh
$EDITOR ./example_web_project/www/index.htm
```

### Rebuilding your Project

After editing, rebuild your project.

In the following example, a Makefile is used, but `http-horse` is compatible
with any build system. All that is required is that your build system outputs
the built files into some directory that `http-horse` can then serve from
and watch for changes.

```zsh
cd example_web_project/
make
```

See [`example_web_project/GNUmakefile`](example_web_project/GNUmakefile)
for a very basic sample makefile that copies an index html source file
from `example_web_project/www/` into `example_web_project/out/` without
making any changes to it. The principle remains the same although in
the real world you would usually have your build system make some
kind of transformation on the source file or source files when
producing output files.

### Viewing Changes

When the project is rebuilt, the project pages that you have
open in your browser will automatically reload to reflect the changes.

## Future Enhancements

### Tighter Integration with Existing Build Systems

`http-horse` aims to support more advanced use cases, such as automatically
running build commands when source files change. This feature is planned
for future releases.

Example of the intended usage (feature not yet implemented):

```zsh
RUST_LOG=debug cargo run --release -- -x "make" -C example_web_project/ -w example_web_project/www/ example_web_project/out/
```

Explanation of parameters in form of an ASCII "diagram":

```text
RUST_LOG=debug cargo run --release -- -x "make" -C example_web_project/ -w example_web_project/www/ example_web_project/out/

                                         ^         ^                       ^                        ^
  -x defines build command to run  ------'         '--.                    |                        |
     when changes are detected in                     |                    |                        |
     the source dir.                                  |                    |                        |
                                                      |                    |                        |
  -C specifies working dir to run build command in.  -'                    |                        |
                                                                           |                        |
  -w indicates source dir to watch for changes.  --------------------------'                        |
                                                                                                    |
  Positional argument specifies out-dir to watch for changes.  -------------------------------------'
```

Put in a bulleted list:

- `-x`: Defines the build command to run when changes are detected in the source directory.
  * The build command can be the name of a single command (such as, `"make"`), but it can
    also include any parameters that you want to pass to the build command.
    E.g.: `"make -B -d"`
- `-C`: Specifies the working directory in which the build command is to be executed.
- `-w`: Indicates the directory to watch for source file changes.
  * This argument can be repeated multiple times if multiple different source directories
    are to be watched, provided that the build command (`-x` argument) and build
    working directory (`-C` argument) remains the same for all of these source directories.
- Positional argument: Specifies the output directory to monitor for changes.

### Modular Web Development Platform

As `http-horse` evolves, it will transition into a more comprehensive web development platform,
maintaining its core strength in serving and hot-reloading web projects while expanding its capabilities
to include modularity, customization, and extensibility.

Future versions of `http-horse` will be designed as a modular platform that developers can extend
through themes and plugins. This modularity will allow users to build highly customizable and feature-rich
web applications with minimal effort.

#### Key Features

- **Modular Architecture:** The platform will be designed to support various modules (such as plugins and themes)
  that can be dynamically loaded and unloaded as needed, allowing users to tailor the platform to their
  specific needs without unnecessary bloat.
- **Plugin System:** A new plugin architecture will be introduced, where plugins will be distributed
  as WebAssembly (WASM) modules. These plugins will be sandboxed for security and will interact
  with the core platform via a stable API. This system will allow developers to easily add
  or extend functionality without recompiling the core platform

#### Customizable Themes and Plugins

- **Themes:** Support for loadable, customizable themes will be added, allowing users to dynamically
  alter the appearance and layout of their web projects. These themes will be easy to apply and modify,
  offering flexibility without the need for extensive coding.
- **Plugins:** A powerful plugin system will be introduced, enabling the addition of new features
  and integrations to web projects without modifying the core codebase. Plugins will be managed
  through a centralized marketplace, ensuring they are secure, optimized, and easy to install.

## License

`http-horse` is licensed under the ISC License. See the [`LICENSE`](LICENSE) file for details.
