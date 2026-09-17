# mmonitor

A macOS Apple Silicon monitor interface built around independent check programs.

## Build

```bash
./install.sh
```

The installer builds both binaries, installs `monitoring-plugins` when needed, and installs everything under `/opt/ma/mmonitor`. An existing configuration is preserved.

## Run

```bash
/opt/ma/mmonitor/mmonitor \
  --config /opt/ma/mmonitor/checks.toml \
  check system_disk cpu_load memory
```

The command writes structured JSON to standard output. Check exit codes are preserved but not interpreted as health assessments.

See [DESIGN.md](DESIGN.md) for the scope and architecture.
