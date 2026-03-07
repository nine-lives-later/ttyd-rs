# ttyd - Share your terminal over the web with Rust

`ttyd-rs` is a high-performance web console application written in Rust. It allows you to seamlessly share your terminal over the web via WebSockets. It acts as a bridge between your local command-line interface and your web browser, utilizing `xterm.js` (https://xtermjs.org) for an authentic, rich terminal experience right inside your browser.

It is a Rust port based on the work of [ttyd](https://github.com/tsl0922/ttyd), many thanks to the original authors. The original still has more features than this Rust port.

## Features

- **Full Terminal Emulation:** Powered by `xterm.js`, supporting true color (256 colors/truecolor), full cursor support, and complex terminal apps like `tmux`, `vim`, and `htop`.
- **File Transfer:** Built-in ZMODEM (`rz`/`sz`) protocol and `trzsz` support directly over the web browser, featuring native drag-and-drop file upload.
- **Secure Authentication:** Features a robust token-based authentication system by default (`uuid v4`) ensuring your web shell is private.
- **Responsive UI:** Includes an elegant web frontend using Tailwind CSS with automatic system-aware Dark/Light mode theme switching.
- **Clipboard Integration:** Built-in `ClipboardAddon` alongside native web keyboard shortcuts (`Ctrl+Shift+C`/`Ctrl+Shift+V`).
- **Structured Logging:** Includes complete structured tracing (standard or JSON output).

## Quick Start

### Running from source

Ensure you have Node.js and Rust/Cargo installed. 

First, build the frontend web application:
```bash
./built_and_run.sh
```

By default, the server will bind to `127.0.0.1` on port `7681` with token authentication enabled.

### Docker

You can instantly deploy `ttyd-rs` via Docker. The included `Dockerfile` performs a multi-stage compilation for both the Node frontend and Rust backend.

```bash
./build_and_run_docker.sh
```

## Usage & Arguments

You can append any standard shell command to the end of the executable to specify what program to launch inside the web terminal. If omitted, it defaults to `bash`.

```bash
# Run htop inside the web console
ttyd-rs htop

# Run a python script
ttyd-rs python3 main.py
```

### CLI Options

```text
Usage: ttyd-rs [OPTIONS] [COMMAND_AND_ARGS]...

Arguments:
  [COMMAND_AND_ARGS]...  Command and arguments to run (default: bash)

Options:
  -p, --port <PORT>                Port to listen on [default: 7681]
  -b, --bind <BIND>                Interfaces to bind to. Can be specified multiple times [default: 127.0.0.1]
      --bind-all                   Bind to all interfaces (0.0.0.0 / [::])
      --log-json                   Log in JSON format
      --debug                      Enable debug logging
      --once                       Accept only one client and exit on disconnect (same as --max-clients=1 and --exit-no-conn)
      --max-clients <MAX_CLIENTS>  Limit the maximum clients which can connect at the same time (0 = no limit) [default: 10]
      --exit-no-conn               Exit after the last client disconnected
      --auth-mode <AUTH_MODE>      Authentication mode: unsafe (no authentication), token (requires token to login) [default: token] [possible values: unsafe, token]
      --auth-token <AUTH_TOKEN>    Token to use for authentication (if --auth-mode=token). If not provided, a random UUID will be generated
  -h, --help                       Print help
  -V, --version                    Print version
```

### Authentication 

By default, `ttyd-rs` uses a secure `token` based authentication. Upon startup, it will generate a secure UUID v4 token and output an access URL to the terminal:

```text
Access the console at: http://127.0.0.1:7681?token=e42e8361-12f8-43c7-a36e-c2cf073d45c4
```

Clicking the URL automatically injects the token into the web client and scrubs the URL bar. If you browse directly to `http://127.0.0.1:7681`, you will be met with a secure modal asking for your token.

To expose the terminal without a password/token, you must explicitly use the unsafe flag:
```bash
ttyd-rs --auth-mode unsafe
```

## Keyboard Shortcuts

Once connected in the browser, you can use the following custom keyboard shortcuts:
- <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>C</kbd>: Copy selection
- <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>V</kbd>: Paste from clipboard
- <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd>: Search terminal buffer
- <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>S</kbd>: Export terminal buffer to file
