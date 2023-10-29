# hot-reload-server

[![Crates.io](https://img.shields.io/crates/v/hot-reload-server.svg)](https://crates.io/crates/hot-reload-server)

Hot reloading HTTP server for HTML, CSS, JavaScript and WebAssembly web development.

## Usage

Have a out-dir that you want to serve, e.g. `./example/out/`.

### Serve out-dir

Serve the out-dir. In this case:

```zsh
RUST_LOG=debug cargo run -- ./example/out/
```

The log output will tell you the address and port for the two servers that `hot-reload-server` runs;
one server for the status page, and one server for the project page.

For example:

```text
[…]
2023-10-29T02:37:44.974101Z  INFO hot_reload_server: Status pages will be served on http://[::1]:58124
2023-10-29T02:37:44.974124Z  INFO hot_reload_server: Project pages will be served on http://[::1]:58125
[…]
```

Open the status page and the project page in your web browser.

### Edit a web source file

Make a change to one or more of the HTML, CSS, JS, or other web files.

In the case of the example web files included with `hot-reload-server` you find them
in `example/www/` under the root of the repo.

```zsh
$EDITOR ./example/www/index.htm
```

### Build edited project

In the example project we use a Makefile. However, you can use `hot-reload-server`
with any kind of build system and it will hot reload the page in the browser for
you when the build system changes any of the relevant files in the out-dir.

```zsh
cd example/www/
make
```

### Look at project page

Observe in the browser that the pages from your project which you have open
in your browser will hot reload when the build system makes relevant changes
in the out-dir.

## Advanced usage

Instead of manually invoking the build system, you can tell `hot-reload-server`
where the source files are, and what command to run in order to run the build system.

(Implementation of this feature has not yet started.)

Example:

```zsh
RUST_LOG=debug cargo run -- -c "make" -d example/ -C example/www/ example/out/
```

where:

```text
RUST_LOG=debug cargo run -- -c "make" -d example/ -C example/www/ example/out/
                               ^         ^           ^            ^- out-dir to watch for changes
                               |         |           `- source dir to watch for changes
                               |         `- the working directory to run the build command in
                               `- the build command to run when changes are made in source dir
```

So:

* The `-c` parameter specifies the build command to run when changes are made in source dir.
* The `-d` parameter specifies the working directory to run the build command in
* The `-C` parameter specifies source dir to watch for changes
* The positional argument after all flags and options have been provided specifies out-dir to watch for changes
