
# WMS Connect Converter

Bus-Consumer, der drei Event-Typen vom
[HAG Connect Bus](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform)
in WMS-kompatible `.dat`-Dateien überführt. Ziel ist die Konvertierung fertiger WMS-Inbound-Daten.

## Was der Converter tut

| Inbound-Event                                | Was wird geschrieben                                                           |
| -------------------------------------------- | ------------------------------------------------------------------------------ |
| `hag.events.internal.lager.asn.ready`        | 25-Feld-ASN `.dat` (ein Pallet pro Zeile)                                    |
| `hag.events.internal.lager.article.ready`    | 47-Feld-ART `.dat` (ein Artikel pro Zeile)                                   |
| `hag.events.internal.lager.order.ready`      | HL40/HL41/HL42(×N)/HL43-ORDER `.dat` (ein oder mehrere Aufträge pro Datei) |

Alle Dateien: pipe-delimited (`|`), CRLF zwischen den Zeilen, **kein**
trailing CRLF am Ende. Encoding ist einheitlich **UTF-8** für alle
Mandanten. Der Dateiname folgt ebenfalls einem einheitlichen Muster
über alle Mandanten:

```
{mandant}_{asn|art|order}_{YYYYMMDD}_{HHMMSS}_{seq:05}.dat
```

## Architektur

```
                    ┌──────────────────────────┐
                    │  HAG Connect Bus         │
                    │   (NATS JetStream,       │
                    │    hag-events stream)    │
                    └─────┬───────┬───────┬────┘
                          │       │       │
            internal.lager│ .asn  │.article│ .order .ready
                          ▼       ▼       ▼
                    drei durable Pull-Consumer
                    (wms-connect-inbound-{asn,article,order})
                          │       │       │
                          ▼       ▼       ▼
                    ┌──────────────────────────┐
                    │ wms-connect-converter    │
                    │                          │
                    │  ┌────────────────────┐  │
                    │  │ bus::Consumer      │  │
                    │  │   └ nats adapter   │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ envelope (serde)   │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ writer::{asn,art,  │  │
                    │  │         order}     │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ sink::{smb,        │  │
                    │  │       local_fs}    │  │
                    │  │  (UTF-8)           │  │
                    │  └────────────────────┘  │
                    └─────────────┬────────────┘
                                  │ smbclient put + rename
                                  ▼
                    ┌──────────────────────────┐
                    │ WMS-infiles-SMB-Share    │
                    │ (192.168.4.x, ein        │
                    │  gemeinsamer Ordner)     │
                    └──────────────────────────┘
```

### Bus-Abstraktion

`src/bus/mod.rs` definiert das `Consumer`-Trait mit einem
Handler-Callback (`Vec<u8>` → `AckOutcome`). Der aktuelle Adapter
ist `src/bus/nats.rs` (async-nats, JetStream-Pull-Consumer). Bei
Bus-Wechsel wird nur ein neuer Adapter (Kafka, Redpanda, …)
geschrieben — die Business-Logik in `src/handlers/` kennt nur das
Trait.

### Pro-Event-Pfad

1. **Consumer** liest Rohbytes, reicht sie an den Handler.
2. **Handler** deserialisiert `Envelope<T>` (serde), rendert den
   Datei-Content mit dem passenden `writer::*::render(...)`,
   bestimmt den Dateinamen aus dem Template und delegiert an den
   Sink.
3. **Sink** schreibt den gerenderten String UTF-8-kodiert. In Prod
   ist das `sink::smb` (shellt `smbclient` aus, siehe unten); für
   Dev/Tests existiert `sink::local_fs` als Fallback.
4. **Ack** zurück an den Bus.

Fehler (Parse-Fehler, falsche Feldzahl im Payload, Sink-IO-Fehler)
werden zu `AckOutcome::RejectToDlx` — der NATS-Adapter ackt
`AckKind::Term`, JetStream zählt die Message nicht gegen
`max_deliver` und der DLQ-Router (`SUBJECT_SCHEMA.md §6`) leitet
sie auf `hag.dlq.>` um.

## SMB-Sink — Produktion

Der Converter schreibt **direkt** auf den WMS-infiles-Share, der
Ziel-Server bleibt unangetastet (keine Mounts, keine zusätzliche
Software dort). Voraussetzung auf dem Host, der den Converter
fährt: `smbclient` (Ubuntu/Debian: `apt install smbclient`).

Ablauf pro Datei:

1. Konvertierter Inhalt wird an `smbclient` per stdin (`put -`)
   übergeben — keine lokale Zwischendatei.
2. Datei landet auf dem Share zuerst mit `.tmp`-Suffix.
3. `rename` benennt sie auf den Endnamen — der WMS-Watcher sieht
   den Endnamen erst, wenn die Datei vollständig ist (Atomarität).
4. Bei Non-Zero-Exit von `smbclient` (Auth, DNS, Berechtigungen,
   Disk full) → `AckOutcome::RejectToDlx`, stderr landet im Log.

Credentials gehen via Env (`USER`, `PASSWD`, optional `DOMAIN`)
an `smbclient`, nicht via argv — `ps -ef` sieht das Passwort
nicht.

Transiente Fehler (DNS-Hickup, kurzer Netzausfall, Lobster
gerade am Restart) führen zu Non-Zero-Exit von `smbclient`. Der
Sink wiederholt bis zu `retry_attempts`-mal mit linearem Backoff
(`attempt × 1s`). Erst danach landet die Message im DLX —
dauerhafte Auth- oder Konfig-Fehler fallen so trotzdem schnell
durch, transiente Hickups verschwinden geräuschlos.

```toml
[smb]
server   = "192.168.4.153"
port     = 445             # Default; 139 wäre NetBIOS
share    = "infiles"
# subdir = "wms"           # optional, default = Share-Root
username = "svc-wms-connect"
password = "..."           # besser per WMS_CONNECT__SMB__PASSWORD
retry_attempts = 3         # Default
# domain         = "HARTMANN"
# smbclient_bin  = "/usr/bin/smbclient"
```

Solange `[smb]` in der Config steht, gewinnt der SMB-Sink. Für
Dev/Tests kann man `[smb]` weglassen und `[paths].output_dir` für
ein lokales Verzeichnis nutzen (siehe unten).

## Tech-Stack

- Rust 1.95.0 (pinned via `rust-toolchain.toml`)
- `tokio` (full), `async-nats` 0.42 (JetStream Pull-Consumer)
- `serde` + `serde_json` (Event-Payloads spiegeln die Pydantic-
  Contracts aus hag-connect-platform 1.0.1)
- `figment` (TOML + Env-Variablen)
- `tracing` + `tracing-subscriber` (JSON-Log, täglich rollend)

## Getting Started

### Voraussetzungen

- Zugang zum NATS-Server der HAG-Connect-Plattform
  (dev: `nats://192.168.4.128:4222`); Service-User
  `svc-wms-connect-<env>` + Passwort kommen aus
  [`declare_users.py`](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform).
- Stream `hag-events-<env>` + drei durable Pull-Consumer
  (`wms-connect-inbound-{asn,article,order}-<env>`) müssen
  **vorher** per `declare_topology.py --apply` provisioniert sein —
  der Converter deklariert nichts selbst.
- Rust-Toolchain 1.95.0 (holt `rust-toolchain.toml` automatisch).

### Konfiguration

```bash
cp config/config.sample.toml config/config.toml
$EDITOR config/config.toml
```

Die echte `config.toml` steht in `.gitignore`. Credentials kommen
besser per Env-Variable — figment nimmt `WMS_CONNECT__`-präfixierte
Vars mit `__` als Sektions-Separator, z. B.
`WMS_CONNECT__AMQP__URL=amqp://svc-wms-connect-prod:...@broker/%2F`.

Ein einziges Ziel (Share + optionaler Subordner) für alle Mandanten
und alle drei Event-Typen; die Dateinamen-Templates sind ebenfalls
global. Die Mandantennummer aus dem Event fließt in den Dateinamen
ein, wird sonst aber nicht validiert — das Contract-Schema (JSON
Schema auf der Publisher-Seite) ist die einzige Schranke.

```toml
# Production: SMB-Ziel (siehe „SMB-Sink — Produktion" oben)
[smb]
server   = "192.168.4.153"
share    = "infiles"
username = "svc-wms-connect"
password = "..."

# Dev/Tests: lokales Verzeichnis statt SMB
# [paths]
# output_dir = "out"

[filenames]
asn   = "{mandant}_asn_{date:%Y%m%d}_{time:%H%M%S}_{seq:05}.dat"
art   = "{mandant}_art_{date:%Y%m%d}_{time:%H%M%S}_{seq:05}.dat"
order = "{mandant}_order_{date:%Y%m%d}_{time:%H%M%S}_{seq:05}.dat"
```

**Dateinamen-Platzhalter** (siehe `src/handlers/mod.rs::render_filename`):

| Platzhalter             | Quelle                                                                        |
| ----------------------- | ----------------------------------------------------------------------------- |
| `{mandant}`           | Mandantennummer aus dem Event                                                 |
| `{date:FORMAT}`       | `chrono::Local::now().format(FORMAT)` (z. B. `%Y%m%d`)                    |
| `{time:FORMAT}`       | wie oben (z. B.`%H%M%S`)                                                    |
| `{seq}`, `{seq:05}` | Pro-Mandant in-memory Counter, optional zero-padded                           |
| `{count}`             | Anzahl Rows (ART) bzw. Orders (ORDER) im Event — im Default-Schema ungenutzt |
| `{trigger}`           | ART-only:`trigger_artikelnr` aus dem Event — im Default-Schema ungenutzt   |
| `{auftragsnr}`        | ORDER-only:`hl40[2]` des ersten Auftrags — im Default-Schema ungenutzt     |

Die letzten drei Platzhalter sind erhalten geblieben, damit in
Sonderfällen ein abweichendes Template möglich ist. Default sollte
das einheitliche Schema oben bleiben.

### Bauen und starten

```bash
cargo build --release
./target/release/wms-connect-converter
# oder während Entwicklung
cargo run --release
```

Der Prozess bleibt stehen und konsumiert, bis er per SIGINT (`Ctrl+C`)
beendet wird.

### Tests + Lint

```bash
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

Die Tests decken ab:

- Envelope-Round-Trip gegen die drei Event-Schemas.
- Writer-Output byte-identisch gegen Prod-Sample-Zeilen
  (z. B. `520_art_20260414_102107_00001_84572.dat`).
- HL40/41/42/43-Struktur + HL43-Auto-Ableitung aus `hl40[2]`.
- Mandant-520-vs.-800-Feld-14/15-Swap im ASN.
- UTF-8-Roundtrip mit Umlauten (Sink-Layer).
- Filename-Template-Rendering incl. ALDI-Datumsformat.

## Runtime-Verhalten

### Start-Sequenz

1. Config laden + Logging aufsetzen.
2. NATS-Verbindung mit Retry (10 Versuche × 3 s Pause).
3. Drei Consumer-Tasks spawnen, je ein Handler pro durable
   Pull-Consumer.
4. Warten auf `SIGINT`.

### Fehlerpfade

| Situation                                           | Outcome                                                                       |
| --------------------------------------------------- | ----------------------------------------------------------------------------- |
| Parse-Fehler (JSON kaputt, Pflichtfeld fehlt)       | `AckKind::Term` → `hag.dlq.>`, Log `parse error`                            |
| Falsche Feldzahl (z. B. `rows[i]` hat 23 statt 24)  | `AckKind::Term` → `hag.dlq.>`                                               |
| Ziel-Ordner nicht schreibbar                        | `AckKind::Term` → `hag.dlq.>`, Log mit Path                                 |
| Bus-Disconnect                                      | async-nats reconnected automatisch; Subscription-Stream endet, Task loggt `beendet` |

DLQ läuft im `hag-events-dlq`-Stream — siehe Plattform-Repo
`SUBJECT_SCHEMA.md §6` für Inspect/Replay.

## Bekannte Grenzen (Iteration 1)

Bewusst klein gehalten; offen für Folge-Tickets:

- **Seq-Counter in-memory**: beginnt nach Neustart wieder bei 1.
  Bei lückenlosen Nummernkreisen: Counter aus dem Zielordner
  inferieren (einmaliger Scan beim Start) oder persistieren
  (SQLite/Sidecar).
- **Keine Rückmeldung vom WMS**: Ablehnungen/Parsing-Fehler
  seitens xLogix werden nicht erkannt. Ein
  `hag.events.internal.lager.ingest.rejected`-Event wäre möglich,
  aktuell aber nicht geplant.
- **ART-Schema ist 47 Felder fix**. Felder 42-47 (palGewicht,
  anzImGebinde, inhaltEinzelteil, materialGruppe, gebindeGewicht,
  kartonsProPalett) sind bei Schenk typisch leer, werden aber
  bei ALDI-Mandanten real gefüllt — Publisher müssen sie
  trotzdem immer als leere Strings mitschicken. Die 2026-04-
  Schenk-Prod-Samples hatten nur 41 Felder, weil Lobster
  trailing-empty-Pipes abschneidet; wir schneiden nichts ab.
- **SMB läuft via `smbclient`-Subprozess pro Datei**. Bei sehr
  hohen Eventraten (>10/s) wäre ein nativer SMB-Client (z. B.
  `pavao`) effizienter; aktuell vernachlässigbar.

## Related

- **[HAG Connect Platform](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform)** —
  Event-Contracts, Topologie-Scripts, rmq-test-suite. Die drei
  hier konsumierten Events sind dort unter `contracts/schemas/events/`
  versioniert (v0.7.0).
- **[Schenk WE Export](https://gitlab.hartmannag.de/it-hartmann/schenk_we_export)** —
  Publisher für `hag.events.internal.lager.asn.ready`. Publisher
  für `hag.events.internal.lager.{article,order}.ready` sind noch
  TBD (Ersatz für die heutigen
  Lobster-Profile).

## Lizenz

Intern — Hartmann AG.
