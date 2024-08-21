# http-horse 🐴

[![Crates.io](https://img.shields.io/crates/v/http-horse.svg)](https://crates.io/crates/http-horse)

HTTP Hot Reload Server for HTML, CSS, JavaScript/TypeScript and WebAssembly web development.

Please note that this software currently runs exclusively on macOS. 🍎💻

There's [an open issue (#1)](https://github.com/ctsrc/http-horse/issues/1) for supporting
other operating systems including Linux and FreeBSD but work on that is not expected
to happen anytime soon.

## Usage

Have an out-dir that you want to serve, e.g. `./example_web_project/out/`.

### Serve out-dir

Serve the out-dir. In this case:

```zsh
RUST_LOG=debug cargo run -- ./example_web_project/out/
```

The log output will tell you the address and port for the two servers that `http-horse` runs;
one server for the status page, and one server for the project page.

For example:

```text
[…]
2023-10-29T05:06:49.278038Z  INFO http_horse: Status pages will be served on http://[::1]:59917
2023-10-29T05:06:49.278089Z  INFO http_horse: Project pages will be served on http://[::1]:59918
[…]
```

Open the status page and the project page in your web browser.

### Edit a web source file

Make a change to one or more of the HTML, CSS, JS, or other web files.

In the case of the example web files included with `http-horse` you find them
in `example_web_project/www/` under the root of the repo.

```zsh
$EDITOR ./example_web_project/www/index.htm
```

### Build edited project

In the example project we use a Makefile. However, you can use `http-horse`
with any kind of build system, and it will hot reload the page in the browser for
you when the build system changes any of the relevant files in the out-dir.

```zsh
cd example_web_project/
make
```

### Look at project page

Observe in the browser that the pages from your project which you have open
in your browser will hot reload when the build system makes relevant changes
in the out-dir.

## Advanced usage

Instead of manually invoking the build system, you can tell `http-horse`
where the source files are, and what command to run in order to run the build system.

(Implementation of this feature has not yet started.)

Example:

```zsh
RUST_LOG=debug cargo run -- -c "make" -d example_web_project/ -C example_web_project/www/ example_web_project/out/
```

where:

```text
RUST_LOG=debug cargo run -- -c "make" -d example_web_project/ -C example_web_project/www/ example_web_project/out/

                               ^         ^                       ^                        ^
  -c specifies build command  -'         |                       |                        |
     to run when changes are             `------------.          |                        |
     made in source dir.                              |          |                        |
                                                      |          |                        |
  -d specifies working dir to run build command in.  -'          |                        |
                                                                 |                        |
  -C specifies source dir to watch for changes.  ----------------'                        |
                                                                                          |
  Positional argument specifies out-dir to watch for changes.  ---------------------------'
```

So:

* The `-c` parameter specifies the build command to run when changes are made in source dir.
* The `-d` parameter specifies the working directory to run the build command in.
* The `-C` parameter specifies source dir to watch for changes.
* The positional argument after all flags and options have been provided specifies out-dir to watch for changes.
