# Networking & IPC

## Transport
- Linux: Unix socket
- Windows: TCP localhost:19735

## IPC Security
- Buffer size limit: 10MB max on socket reads
- Response size validation on client side
- Input validation on all commands

## Adding a New IPC Command
1. Define struct in `src/types/commands.rs`
2. Implement `Executable` trait
3. Add to `parse_command()` in `src/utils/commands.rs`
4. Add handler in daemon
