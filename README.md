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
├── com.ma.mmonitor.collector.plist
└── install.sh
```

## Install

Transfer the `deploy/` directory to the target Mac, then run:

```bash
cd deploy
./install.sh
```

The installer:

- installs `monitoring-plugins` when needed,
- creates the locked service account and group `_mmonitor`,
- installs the binaries and configuration under `/opt/ma/mmonitor`,
- preserves an existing configuration and database,
- installs and starts the system LaunchDaemon `com.ma.mmonitor.collector`.

Rust and the source repository are not required on the target Mac.

## Run checks manually

```bash
/opt/ma/mmonitor/mmonitor \
  --config /opt/ma/mmonitor/checks.toml \
  check system_disk cpu_load memory macos_version
```

Manual checks write structured JSON to standard output and do not persist results. Check exit codes are preserved but not interpreted as health assessments.

## Collect due checks

```bash
sudo -u _mmonitor /opt/ma/mmonitor/mmonitor \
  --config /opt/ma/mmonitor/checks.toml \
  collect
```

The LaunchDaemon invokes this command every minute. Per-check intervals and storage strategies determine which results are written to `/opt/ma/mmonitor/data/mmonitor.sqlite3`.

See [DESIGN.md](DESIGN.md) for the scope and architecture.
