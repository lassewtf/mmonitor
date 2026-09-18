# mmonitor – Design

## Ziel

`mmonitor` liest lokale Monitoring-Checks aus und gibt deren Messwerte strukturiert zurück.

Das Projekt wird in Rust umgesetzt und unterstützt zunächst ausschließlich macOS auf Apple Silicon.

## Erster Umfang

Der erste Schnitt liest das Systemdateisystem `/`, die CPU-Systemlast, den Arbeitsspeicher und die installierte macOS-Version aus.

| Check-ID | Programm | Messwerte |
|---|---|---|
| `system_disk` | `check_disk` aus Homebrews `monitoring-plugins` | Gesamtgröße, belegter und verfügbarer Speicher in Bytes |
| `cpu_load` | `check_load` aus Homebrews `monitoring-plugins` | logische CPUs sowie Load Average und Load Average pro logischer CPU über 1, 5 und 15 Minuten |
| `memory` | mit `mmonitor` ausgeliefertes `check_mmonitor_memory` | gesamter, benutzter, verfügbarer, freier, wired und komprimierter Arbeitsspeicher sowie Swap in Bytes |
| `macos_version` | Apples `/usr/bin/sw_vers` | Produktname, Version und Build als Strings |

Netzwerk, Prozesse, weitere Dateisysteme und CPU-Auslastung in Prozent sind nicht enthalten.

## Keine Bewertung

`mmonitor` bewertet Messwerte nicht.

Insbesondere erzeugt es keine Zustände wie `ok`, `warning` oder `critical`. Der Exit-Code und die unveränderte Ausgabe des Check-Programms bleiben als technische Rohdaten erhalten, werden aber nicht fachlich interpretiert.

`check_disk` verlangt Schwellenwerte. Die Definition verwendet deshalb `-w 0% -c 0%`. Diese Werte dienen ausschließlich dazu, den Check auszuführen. `mmonitor` übernimmt die daraus entstehende Bewertung nicht.

## Externe Check-Programme

Die Dateisystem- und CPU-Checks stammen aus dem Homebrew-Paket `monitoring-plugins`:

```bash
brew install monitoring-plugins
```

Auf Apple Silicon liegen die Programme standardmäßig unter:

```text
/opt/homebrew/sbin/check_disk
/opt/homebrew/sbin/check_load
```

Der Memory-Check wird als eigene ausführbare Datei `check_mmonitor_memory` zusammen mit `mmonitor` unter `/opt/ma/mmonitor` installiert. Er bleibt ein unabhängig aufrufbares externes Check-Programm.

`deploy.sh` baut beide Rust-Binärdateien und erzeugt das Git-ignorierte Verzeichnis `deploy/`. Dieses enthält beide Binärdateien, `checks.toml.example` und `install.sh`.

`deploy/install.sh` benötigt weder Rust noch den Quellcode. Es installiert bei Bedarf `monitoring-plugins`, kopiert beide Binärdateien nach `/opt/ma/mmonitor` und legt eine fehlende Konfiguration unter `/opt/ma/mmonitor/checks.toml` an. Eine vorhandene Konfiguration bleibt unverändert.

`mmonitor` lädt keine Programme herunter und installiert keine Laufzeitabhängigkeiten.

Check-Programme werden direkt und ohne Shell gestartet. Programm und Argumente bleiben getrennt. Der konfigurierte Programmpfad muss absolut sein.

Beispieldefinition:

```toml
[storage]
path = "/opt/ma/mmonitor/data/mmonitor.sqlite3"

[checks.system_disk]
kind = "system_disk"
program = "/opt/homebrew/sbin/check_disk"
args = ["-w", "0%", "-c", "0%", "-p", "/"]
timeout_ms = 3000
interval_seconds = 300
store = "always"

[checks.cpu_load]
kind = "cpu_load"
program = "/opt/homebrew/sbin/check_load"
args = ["-r"]
timeout_ms = 3000
interval_seconds = 60
store = "always"

[checks.memory]
kind = "memory"
program = "/opt/ma/mmonitor/check_mmonitor_memory"
args = []
timeout_ms = 3000
interval_seconds = 60
store = "always"

[checks.macos_version]
kind = "macos_version"
program = "/usr/bin/sw_vers"
args = []
timeout_ms = 1000
interval_seconds = 3600
store = "on_change"
```

`check_load -r` liefert rohe und durch die erkannte Anzahl logischer CPUs geteilte Load-Average-Werte. Die CPU-Anzahl steht in seiner Zusammenfassung, nicht als eigener Performance-Datenwert.

## Architektur

Der Datenfluss ist:

```text
CLI
 │
 ▼
Monitor
 │
 ├── Check-Katalog
 │
 ▼
ProcessRunner
 │
 ▼
RawCheckResult
 │
 ├── NagiosParser ──► Normalizer
 │
 └── SwVersParser
          │
          ▼
     CheckResult
```

### Bibliotheks-Schnittstelle

`Monitor` bildet die kleine öffentliche Schnittstelle:

```rust
let monitor = Monitor::from_path("checks.toml")?;
let results = monitor.run(["system_disk", "cpu_load", "memory", "macos_version"])?;
```

Die Bibliothek erhält eine geordnete Liste angefragter Check-IDs. Eine ID führt genau ein externes Programm aus. Eine einzelne Anfrage ist eine Liste mit genau einer ID.

`Monitor` koordiniert Katalog, Ausführung, Parsing und Normalisierung. Die jeweilige Implementation bleibt in den zuständigen Modulen.

Konfigurationsfehler verhindern die gesamte Ausführung. Technische Ausführungs- oder Parsingfehler bleiben Ergebnisse des jeweiligen Checks. Ein solcher Fehler verhindert keine weiteren Ergebnisse.

Ergebnisse behalten die Reihenfolge der Anfrage. Der erste Schnitt führt Checks sequenziell aus. Parallelisierung folgt nur bei gemessenem Bedarf.

### Check-Katalog

`config.rs` lädt die TOML-Konfiguration und validiert vor der Ausführung:

- eindeutige Check-IDs,
- bekannte `kind`-Werte,
- absolute Programmpfade,
- gültige Timeouts,
- getrennte Argumentlisten.

Die ID adressiert eine Check-Instanz. `kind` bestimmt deren Normalisierung.

### Prozessausführung

`runner.rs` enthält den `ProcessRunner`. Er kennt weder Nagios noch konkrete Messwerte.

Er übernimmt:

- direkten Start ohne Shell,
- Timeout und Beenden des Prozesses,
- begrenztes Einlesen von `stdout` und `stderr`,
- Exit-Code und Laufzeit,
- unveränderte Rohdaten im Ergebnis.

### Nagios-Parsing

`nagios.rs` parst das Performance-Datenformat:

```text
'label'=value[unit];warn;crit;min;max
```

Warn- und Critical-Felder werden nicht als Bewertung übernommen.

### macOS-Versions-Parsing

`sw_vers.rs` liest `ProductName`, `ProductVersion` und `BuildVersion` aus der Ausgabe von `/usr/bin/sw_vers`. Fehlende, leere oder doppelte Pflichtfelder führen zu `invalid_output`.

Versionsdaten bleiben unveränderte String-Fakten:

- `os.name`,
- `os.version`,
- `os.build`.

### Normalisierung

`normalize.rs` überführt bekannte Nagios-Ausgaben in stabile Metriknamen. Ein einfacher `match` genügt:

```rust
match kind {
    CheckKind::SystemDisk => normalize_disk(metrics),
    CheckKind::CpuLoad => normalize_cpu_load(metrics, summary),
    CheckKind::Memory => normalize_memory(metrics),
}
```

Der erste Schnitt verwendet keine Traits oder Anbieteradapter.

## CLI

Die CLI ist eine dünne Schicht über der Bibliothek.

```bash
mmonitor --config checks.toml check system_disk
mmonitor --config checks.toml check system_disk cpu_load memory macos_version
mmonitor --config checks.toml collect
```

Die Standardausgabe ist JSON. Diagnostische Meldungen gehen nach `stderr`.

Die CLI beendet sich mit Exit-Code `0`, wenn alle angefragten Checks technisch erfolgreich waren. Mindestens ein technischer Ausführungs- oder Parsingfehler führt zu einem Exit-Code ungleich `0`. Fachliche Check-Bewertungen beeinflussen den CLI-Exit-Code nicht.

## Ergebnis

Ein erfolgreich ausgelesener Check liefert mindestens:

```json
{
  "id": "system_disk",
  "execution": "completed",
  "exit_code": 0,
  "metrics": [
    { "name": "filesystem.total", "value": 994631127040, "unit": "bytes", "mount": "/" },
    { "name": "filesystem.used", "value": 802577416192, "unit": "bytes", "mount": "/" },
    { "name": "filesystem.available", "value": 192053710848, "unit": "bytes", "mount": "/" }
  ],
  "stdout": "unveränderte Check-Ausgabe",
  "stderr": "",
  "duration_ms": 14
}
```

Die Bibliothek liest die Nagios-Performance-Daten hinter `|`. Bei `check_disk` enthält der Messwert den belegten Speicher und als Maximum die Gesamtgröße. Der verfügbare Speicher ergibt sich aus Gesamtgröße minus belegtem Speicher.

Bei `check_load -r` übernimmt die Bibliothek `load1`, `load5`, `load15`, `scaled_load1`, `scaled_load5` und `scaled_load15`. Sie liest zusätzlich die verwendete Anzahl logischer CPUs aus der Zusammenfassung. Load Average und CPU-Auslastung in Prozent bleiben unterschiedliche Messgrößen.

### Memory-Check

`check_mmonitor_memory` verwendet native macOS-Schnittstellen und keine Shell-Befehle:

- `host_statistics64(HOST_VM_INFO64)` für VM-Seitenzähler,
- `sysctl hw.memsize` für den physischen Gesamtspeicher,
- `sysctl vm.swapusage` für Swap,
- die zur Laufzeit ermittelte Page Size für die Umrechnung in Bytes.

Der Check verwendet `libc` und `mach2`. Er wird ausschließlich für `aarch64-apple-darwin` gebaut.

Die Messwerte verwenden folgende Definitionen:

```text
used = (internal - purgeable) + wired + compressed
available_non_compressed = active + inactive + free + speculative
available = available_non_compressed + compressed
system_free_percent = available_non_compressed / total × 100
```

`free_count` aus `vm_statistics64` enthält spekulative Seiten bereits. Die Implementation darf diese deshalb bei `available_non_compressed` nicht doppelt addieren.

`memory.used` und `memory.available` sind teilweise überlappende XNU-Sichten. Ihre Summe muss nicht `memory.total` ergeben.

Der Check liefert mindestens:

- `memory.total`,
- `memory.used`,
- `memory.available`,
- `memory.available_non_compressed`,
- `memory.free`,
- `memory.wired`,
- `memory.compressed`,
- `memory.system_free_percent`,
- `swap.total`,
- `swap.used`,
- `swap.free`.

Speicherwerte verwenden Bytes. `memory.system_free_percent` verwendet Prozent. Der Check gibt keine fachliche Bewertung aus.

## Periodische Sammlung und Historie

Der manuelle Befehl `check` wird durch `collect` ergänzt:

```text
check   führt die angeforderten Checks sofort aus und speichert nichts
collect führt nur fällige Checks aus und speichert deren Ergebnisse
```

Ein systemweiter LaunchDaemon startet `collect` jede Minute. `interval_seconds` bestimmt pro Check, ob eine Ausführung fällig ist.

### Speicherstrategie

Jeder Check besitzt eine Strategie `store`:

- `always` speichert jede Ausführung.
- `on_change` speichert nur semantische Änderungen.

Ohne explizite Angabe verwenden numerische Checks `always` und `macos_version` verwendet `on_change`. Die Standardintervalle entsprechen der Beispielkonfiguration.

Für `on_change` umfasst der Vergleich:

- normalisierte Metriknamen, Werte und Einheiten,
- normalisierte Faktennamen und Werte,
- technischen Ausführungszustand,
- technischen Fehler.

Laufzeit, Zeitstempel, rohe Ausgabe und Nagios-Bewertung beeinflussen den Vergleich nicht. Bei einer Änderung wird der vollständige Check-Snapshot gespeichert.

Der erste erfolgreiche Snapshot wird immer gespeichert. Technische Zustandswechsel und die Erholung nach einem Fehler werden ebenfalls gespeichert. Wiederholte identische Ergebnisse erzeugen keinen weiteren Historieneintrag.

### Aktueller Check-Zustand

`check_state` wird unabhängig von `store` bei jeder Ausführung aktualisiert:

```text
check_state
- check_id
- last_attempt_at
- last_success_at
- last_execution
- last_error
- last_snapshot
```

Damit bleibt sichtbar, wann ein unveränderter Check zuletzt tatsächlich ausgeführt wurde.

### SQLite

Die Datenbank liegt unter:

```text
/opt/ma/mmonitor/data/mmonitor.sqlite3
```

SQLite darf daneben seine WAL- und SHM-Dateien anlegen. WAL bedeutet Write-Ahead Log und ermöglicht robuste Transaktionen bei parallelen Lesezugriffen.

Die Historie verwendet mindestens:

```text
runs
- id
- check_id
- observed_at
- execution
- exit_code
- duration_ms
- error

samples
- run_id
- metric_name
- value
- unit

facts
- run_id
- fact_name
- value

rollups
- check_id
- metric_name
- unit
- resolution
- bucket_start
- minimum
- maximum
- sum
- count
- first
- last
```

Eine SQLite-basierte Collection-Sperre verhindert gleichzeitig laufende `collect`-Aufrufe.

### Verdichtung und Aufbewahrung

Messwerte werden aggregiert statt zufällig gelöscht:

```text
Rohdaten im Minutenraster  7 Tage
5-Minuten-Rollups          30 Tage
1-Stunden-Rollups          unbegrenzt
```

Rollups speichern Minimum, Maximum, Summe, Anzahl, ersten und letzten Wert. Fehlende Werte werden nicht interpoliert.

Die Verdichtung verarbeitet nur abgeschlossene Zeitfenster. Upsert, Rollup und Löschung laufen in einer Transaktion. Quelldaten werden erst nach erfolgreicher Speicherung des Ziel-Rollups gelöscht.

Fakten wie die macOS-Version werden nicht zeitlich aggregiert. `on_change` hält deren Änderungshistorie bereits klein.

## Systemweiter LaunchDaemon

Die periodische Sammlung verwendet einen LaunchDaemon, keinen LaunchAgent. Ein LaunchDaemon läuft auch ohne angemeldeten Benutzer.

Kennung und Datei:

```text
com.ma.mmonitor.collector
/Library/LaunchDaemons/com.ma.mmonitor.collector.plist
```

Der LaunchDaemon startet:

```text
/opt/ma/mmonitor/mmonitor --config /opt/ma/mmonitor/checks.toml collect
```

Er verwendet `RunAtLoad` und `StartInterval = 60`. Er läuft als dedizierter lokaler Dienstbenutzer und als gleichnamige Gruppe:

```text
UserName  = _mmonitor
GroupName = _mmonitor
```

`_mmonitor` besitzt:

- keine interaktive Anmeldung,
- kein Passwort,
- keine Admin- oder sudo-Rechte,
- `/var/empty` als Home-Verzeichnis,
- `/usr/bin/false` als Shell.

Der Installer legt Benutzer und Gruppe idempotent an. Vorhandene passende Identitäten werden weiterverwendet. Bei kollidierenden oder unerwarteten Eigenschaften bricht die Installation ab.

Dateirechte:

```text
/opt/ma/mmonitor/                     root:wheel             0755
/opt/ma/mmonitor/mmonitor            root:wheel             0755
/opt/ma/mmonitor/check_*             root:wheel             0755
/opt/ma/mmonitor/checks.toml         root:_mmonitor         0640
/opt/ma/mmonitor/data/               _mmonitor:_mmonitor    0750
/opt/ma/mmonitor/log/                _mmonitor:_mmonitor    0750
```

Die Datenbank und Logdateien werden bei Installation oder Aktualisierung weder überschrieben, gelöscht noch als `root` in ihren Metadaten verändert. Der Installer akzeptiert dort nur reguläre Dateien mit `_mmonitor:_mmonitor 0640` und bricht bei Symlinks, anderen Dateitypen oder abweichenden Eigentümern und Rechten ab. Neue Dateien erstellt `_mmonitor` mit Umask `027`. Binärdateien und Konfiguration bleiben `root`-verwaltet.

Das interne Ergebnismodell entspricht dieser Form:

```rust
struct CheckResult {
    id: String,
    execution: Execution,
    exit_code: Option<i32>,
    metrics: Vec<Metric>,
    facts: Vec<Fact>,
    stdout: String,
    stderr: String,
    duration_ms: u64,
    error: Option<String>,
}
```

Technische Ausführungszustände sind:

- `completed`
- `timed_out`
- `spawn_failed`
- `invalid_output`

## Projektstruktur

Ein einzelnes Cargo-Paket enthält Bibliothek und CLI:

```text
mmonitor/
├── Cargo.toml
├── Cargo.lock
├── DESIGN.md
├── README.md
├── checks.toml.example
├── com.ma.mmonitor.collector.plist
├── deploy.sh
├── install.sh
├── src/
│   ├── bin/
│   │   └── check_mmonitor_memory.rs
│   ├── lib.rs
│   ├── main.rs
│   ├── config.rs
│   ├── model.rs
│   ├── nagios.rs
│   ├── normalize.rs
│   ├── runner.rs
│   ├── storage.rs
│   └── sw_vers.rs
└── tests/
    └── cli.rs
```

Ein Cargo-Workspace, Daemon, Scheduler, asynchrones Rust-Laufzeitsystem, Anbieteradapter und Trait-Hierarchien sind für den ersten Schnitt nicht vorgesehen.

## Prüfung

Die automatisierte Prüfung verwendet ein kleines Testprogramm mit fester Nagios-Ausgabe. Sie prüft mindestens:

- Aufruf eines einzelnen Checks,
- geordnete Ausführung mehrerer Definitionen,
- Parsing von Gesamtgröße, belegtem und verfügbarem Dateisystemspeicher,
- Parsing von CPU-Anzahl, rohem und normiertem Load Average,
- Berechnung und Parsing aller festgelegten Memory- und Swap-Werte,
- Parsing von macOS-Produktname, Version und Build,
- Zurückweisung fehlender oder doppelter `sw_vers`-Pflichtfelder,
- Laufzeitabhängige Page Size ohne fest codierte 4- oder 16-KiB-Annahme,
- Timeout und fehlendes Programm,
- unveränderte Übernahme von Exit-Code, `stdout` und `stderr`,
- Abwesenheit einer fachlichen Bewertung,
- Speicherung vollständiger Check-Snapshots,
- `on_change` ohne unveränderte Historienkopien,
- Sperre gegen parallele Collector-Läufe,
- idempotente 5-Minuten- und Stunden-Rollups.

Optionale lokale Integrationstests dürfen `/opt/homebrew/sbin/check_disk` und `/opt/homebrew/sbin/check_load` verwenden. Die reguläre Testsuite darf Homebrew nicht voraussetzen.

Ein macOS-ARM64-Integrationstest führt `check_mmonitor_memory` ohne erhöhte Rechte aus und prüft positive Gesamtwerte sowie `swap.used + swap.free = swap.total`.
