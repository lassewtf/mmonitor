# mmonitor – Design

## Ziel

`mmonitor` liest lokale Monitoring-Checks aus und gibt deren Messwerte strukturiert zurück.

Das Projekt wird in Rust umgesetzt und unterstützt zunächst ausschließlich macOS auf Apple Silicon.

## Erster Umfang

Der erste Schnitt liest nur das Systemdateisystem `/` aus.

| Check-ID | Programm | Messwerte |
|---|---|---|
| `system_disk` | `check_disk` aus Homebrews `monitoring-plugins` | Gesamtgröße, belegter und verfügbarer Speicher in Bytes |

CPU, Load Average, Arbeitsspeicher, Swap, Netzwerk, Prozesse und weitere Dateisysteme sind nicht enthalten.

## Keine Bewertung

`mmonitor` bewertet Messwerte nicht.

Insbesondere erzeugt es keine Zustände wie `ok`, `warning` oder `critical`. Der Exit-Code und die unveränderte Ausgabe des Check-Programms bleiben als technische Rohdaten erhalten, werden aber nicht fachlich interpretiert.

`check_disk` verlangt Schwellenwerte. Die Definition verwendet deshalb `-w 0% -c 0%`. Diese Werte dienen ausschließlich dazu, den Check auszuführen. `mmonitor` übernimmt die daraus entstehende Bewertung nicht.

## Externes Check-Programm

Der Check stammt aus dem Homebrew-Paket `monitoring-plugins`:

```bash
brew install monitoring-plugins
```

Auf Apple Silicon liegt das Programm standardmäßig unter:

```text
/opt/homebrew/sbin/check_disk
```

`mmonitor` lädt keine Programme herunter und installiert keine Laufzeitabhängigkeiten.

Check-Programme werden direkt und ohne Shell gestartet. Programm und Argumente bleiben getrennt. Der konfigurierte Programmpfad muss absolut sein.

Beispieldefinition:

```toml
[checks.system_disk]
program = "/opt/homebrew/sbin/check_disk"
args = ["-w", "0%", "-c", "0%", "-p", "/"]
timeout_ms = 3000
```

## Bibliothek

Die Bibliothek erhält Check-Definitionen und eine geordnete Liste angefragter Check-IDs. Eine ID führt genau ein externes Programm aus.

Dieselbe Operation unterstützt einen oder mehrere Checks. Eine einzelne Anfrage ist eine Liste mit genau einer ID.

Ergebnisse behalten die Reihenfolge der Anfrage. Ein technischer Fehler eines Checks verhindert keine weiteren Ergebnisse.

Der erste Schnitt darf Checks sequenziell ausführen. Parallelisierung folgt nur bei gemessenem Bedarf.

## CLI

Die CLI ist eine dünne Schicht über der Bibliothek.

```bash
mmonitor --config checks.toml check system_disk
```

Die Standardausgabe ist JSON. Diagnostische Meldungen gehen nach `stderr`.

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
│   ├── lib.rs
│   ├── main.rs
│   ├── model.rs
│   └── runner.rs
└── tests/
    └── cli.rs
```

Ein Cargo-Workspace, Anbieteradapter und Trait-Hierarchien sind für den ersten Schnitt nicht vorgesehen.

## Prüfung

Die automatisierte Prüfung verwendet ein kleines Testprogramm mit fester Nagios-Ausgabe. Sie prüft mindestens:

- Aufruf eines einzelnen Checks,
- geordnete Ausführung mehrerer Definitionen,
- Parsing von Gesamtgröße, belegtem und verfügbarem Speicher,
- Timeout und fehlendes Programm,
- unveränderte Übernahme von Exit-Code, `stdout` und `stderr`,
- Abwesenheit einer fachlichen Bewertung.

Ein optionaler lokaler Integrationstest darf `/opt/homebrew/sbin/check_disk` verwenden. Die reguläre Testsuite darf Homebrew nicht voraussetzen.
