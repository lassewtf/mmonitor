# mmonitor

A macOS Apple Silicon monitor interface built around independent check programs.

## Build deployment package

```bash
./deploy.sh
```

The script builds both release binaries and creates the ignored `deploy/` directory containing:

```text
deploy/
├── mmonitor
├── check_mmonitor_memory
├── checks.toml.example
└── install.sh
```

## Install

Transfer the `deploy/` directory to the target Mac, then run:

```bash
cd deploy
./install.sh
```

The installer installs `monitoring-plugins` when needed, copies both binaries to `/opt/ma/mmonitor`, and creates `/opt/ma/mmonitor/checks.toml`. An existing configuration is preserved. Rust and the source repository are not required on the target Mac.

## Run

```bash
/opt/ma/mmonitor/mmonitor \
  --config /opt/ma/mmonitor/checks.toml \
  check system_disk cpu_load memory
```

The command writes structured JSON to standard output. Check exit codes are preserved but not interpreted as health assessments.

See [DESIGN.md](DESIGN.md) for the scope and architecture.
