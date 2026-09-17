# mmonitor – Design

## Ziel

`mmonitor` liest lokale Monitoring-Checks aus und gibt deren Messwerte strukturiert zurück.

Das Projekt wird in Rust umgesetzt und unterstützt zunächst ausschließlich macOS auf Apple Silicon.

## Erster Umfang

Der erste Schnitt liest das Systemdateisystem `/`, die CPU-Systemlast und den Arbeitsspeicher aus.

| Check-ID | Programm | Messwerte |
|---|---|---|
| `system_disk` | `check_disk` aus Homebrews `monitoring-plugins` | Gesamtgröße, belegter und verfügbarer Speicher in Bytes |
| `cpu_load` | `check_load` aus Homebrews `monitoring-plugins` | logische CPUs sowie Load Average und Load Average pro logischer CPU über 1, 5 und 15 Minuten |
| `memory` | mit `mmonitor` ausgeliefertes `check_mmonitor_memory` | gesamter, benutzter, verfügbarer, freier, wired und komprimierter Arbeitsspeicher sowie Swap in Bytes |

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

Der Memory-Check wird als eigene ausführbare Datei `check_mmonitor_memory` zusammen mit `mmonitor` installiert. Er bleibt ein unabhängig aufrufbares externes Check-Programm.

`mmonitor` lädt keine Programme herunter und installiert keine Laufzeitabhängigkeiten.

Check-Programme werden direkt und ohne Shell gestartet. Programm und Argumente bleiben getrennt. Der konfigurierte Programmpfad muss absolut sein.

Beispieldefinition:

```toml
[checks.system_disk]
kind = "system_disk"
program = "/opt/homebrew/sbin/check_disk"
args = ["-w", "0%", "-c", "0%", "-p", "/"]
timeout_ms = 3000

[checks.cpu_load]
kind = "cpu_load"
program = "/opt/homebrew/sbin/check_load"
args = ["-r"]
timeout_ms = 3000

[checks.memory]
kind = "memory"
program = "/opt/homebrew/bin/check_mmonitor_memory"
args = []
timeout_ms = 3000
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
 ▼
NagiosParser
 │
 ▼
Normalizer
 │
 ▼
CheckResult
```

### Bibliotheks-Schnittstelle

`Monitor` bildet die kleine öffentliche Schnittstelle:

```rust
let monitor = Monitor::from_path("checks.toml")?;
let results = monitor.run(["system_disk", "cpu_load", "memory"]);
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

### Normalisierung

`normalize.rs` überführt bekannte Check-Ausgaben in stabile Metriknamen. Ein einfacher `match` genügt:

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
mmonitor --config checks.toml check system_disk cpu_load memory
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

Das interne Ergebnismodell entspricht dieser Form:

```rust
struct CheckResult {
    id: String,
    execution: Execution,
    exit_code: Option<i32>,
    metrics: Vec<Metric>,
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
├── src/
│   ├── bin/
│   │   └── check_mmonitor_memory.rs
│   ├── lib.rs
│   ├── main.rs
│   ├── config.rs
│   ├── model.rs
│   ├── nagios.rs
│   ├── normalize.rs
│   └── runner.rs
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
- Laufzeitabhängige Page Size ohne fest codierte 4- oder 16-KiB-Annahme,
- Timeout und fehlendes Programm,
- unveränderte Übernahme von Exit-Code, `stdout` und `stderr`,
- Abwesenheit einer fachlichen Bewertung.

Optionale lokale Integrationstests dürfen `/opt/homebrew/sbin/check_disk` und `/opt/homebrew/sbin/check_load` verwenden. Die reguläre Testsuite darf Homebrew nicht voraussetzen.

Ein macOS-ARM64-Integrationstest führt `check_mmonitor_memory` ohne erhöhte Rechte aus und prüft positive Gesamtwerte sowie `swap.used + swap.free = swap.total`.
