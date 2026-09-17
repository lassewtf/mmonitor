# mmonitor

A macOS Apple Silicon monitor interface built around independent check programs.

## Build

```bash
brew install monitoring-plugins
cargo build --release
```

Copy `checks.toml.example` and point `checks.memory.program` to the built or installed `check_mmonitor_memory` binary.

## Run

```bash
./target/release/mmonitor \
  --config checks.toml \
  check system_disk cpu_load memory
```

The command writes structured JSON to standard output. Check exit codes are preserved but not interpreted as health assessments.

See [DESIGN.md](DESIGN.md) for the scope and architecture.
