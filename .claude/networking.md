# Networking & IPC

## Transport
TCP `127.0.0.1:19735`. Length-prefixed (u32 LE) JSON for both directions.

The same TCP loop serves the GUI's `make_request_sync` and the `cli` subcommand. Since the engine is embedded in the GUI process, the GUI is talking to itself over loopback — overhead is negligible and it kept the existing IPC code intact.

## IPC Security
- Server reads cap at 64 KiB / 5 s timeout (in `daemon_run::handle_connection`).
- Max 32 args per request (`MAX_IPC_ARGS`).
- Client response cap: 1 MiB (in `daemon::send_and_receive`).
- All paths validated via `validate_audio_path` / `validate_path_within` (rejects symlinks, NTFS ADS, escape paths).
- `Request` is `#[serde(deny_unknown_fields)]`.

## Adding a New IPC Command
1. Add a variant to `DaemonCommand` in `src/types/commands.rs`
2. Handle it in `DaemonCommand::execute`
3. Add a wire-name match arm in `parse_command()` in `src/utils/commands.rs`
4. Add a `Request::*` constructor in `src/types/socket.rs` if callers need one
5. (CLI) add a clap variant in `src/utils/cli_run.rs`
6. (GUI) wire `make_request_sync(...)` where needed
