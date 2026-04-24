
# WMS Connect Converter

Bus-Consumer, der drei Event-Typen vom
[HAG Connect Bus](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform)
in WMS-kompatible `.dat`-Dateien überführt. Ziel ist die Konvertierung fertiger WMS-Inbound-Daten.

## Was der Converter tut

| Inbound-Event       | Was wird geschrieben                                                           |
| ------------------- | ------------------------------------------------------------------------------ |
| `wms.asn-ready`   | 25-Feld-ASN `.dat` (ein Pallet pro Zeile)                                    |
| `wms.art-ready`   | 47-Feld-ART `.dat` (ein Artikel pro Zeile)                                   |
| `wms.order-ready` | HL40/HL41/HL42(×N)/HL43-ORDER `.dat` (ein oder mehrere Aufträge pro Datei) |

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
                    │   (RabbitMQ heute,       │
                    │    austauschbar)         │
                    └─────┬───────┬───────┬────┘
                          │       │       │
                 wms.asn- │   wms.│   wms.order-ready
                  ready   │   art-│       │
                          │  ready│       │
                          ▼       ▼       ▼
                    ┌──────────────────────────┐
                    │ wms-connect-converter    │
                    │                          │
                    │  ┌────────────────────┐  │
                    │  │ bus::Consumer      │  │
                    │  │   └ lapin adapter  │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ envelope (serde)   │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ writer::{asn,art,  │  │
                    │  │         order}     │  │
                    │  └────────────────────┘  │
                    │  ┌────────────────────┐  │
                    │  │ sink::local_fs     │  │
                    │  │  (UTF-8)           │  │
                    │  └────────────────────┘  │
                    └─────────────┬────────────┘
                                  │
                                  ▼
                    ┌──────────────────────────┐
                    │ WMS-infiles-Zielordner   │
                    │ (ein gemeinsamer Share,  │
                    │  lokal oder CIFS-Mount)  │
                    └──────────────────────────┘
```

### Bus-Abstraktion

`src/bus/mod.rs` definiert das `Consumer`-Trait mit einem
Handler-Callback (`Vec<u8>` → `AckOutcome`). Der aktuelle Adapter
ist `src/bus/rabbitmq.rs` (lapin). Bei Bus-Wechsel wird nur ein
neuer Adapter (Kafka, Redpanda, …) geschrieben — die Business-
Logik in `src/handlers/` kennt nur das Trait.

### Pro-Event-Pfad

1. **Consumer** liest Rohbytes, reicht sie an den Handler.
2. **Handler** deserialisiert `Envelope<T>` (serde), rendert den
   Datei-Content mit dem passenden `writer::*::render(...)`,
   bestimmt den Dateinamen aus dem Template und delegiert an den
   Sink.
3. **Sink** schreibt den gerenderten String UTF-8-kodiert per
   `std::fs::write`.
4. **Ack** zurück an den Bus.

Fehler (Parse-Fehler, falsche Feldzahl im Payload, Sink-IO-Fehler)
werden zu `AckOutcome::RejectToDlx` und die Message landet in der
Dead-Letter-Queue — **nicht** Requeue, damit defekte Payloads
nicht endlos rezirkulieren.

## Was passiert **nach** dem `.dat`-Schreiben?

Der Converter ist bei der lokalen Datei fertig. Der Transfer in
das WMS ist Deployment-Sache:

- **CIFS-Mount auf dem Host** (empfohlen, nahtloser Ersatz für
  Lobster): `output_dir` zeigt auf einen `systemd`-gemounteten
  SMB-Share, das WMS pollt wie gewohnt.
- **Watcher + Upload-Agent**: Converter schreibt lokal, ein
  separater Dienst (`inotify` / Cron) kopiert via `smbclient` /
  `rsync` / `curl`.
- **Eigenes Sink-Impl**: ein `SmbSink` / `SftpSink` hinter dem
  bestehenden `OutputSink`-Trait, Schalter pro Mandant in der
  Config.

Siehe auch Punkte unter *Bekannte Grenzen* unten.

## Tech-Stack

- Rust 1.95.0 (pinned via `rust-toolchain.toml`)
- `tokio` (full), `lapin` 2.5, `tokio-{executor,reactor}-trait`
- `serde` + `serde_json` (Event-Payloads spiegeln die Pydantic-
  Contracts aus hag-connect-platform 0.7.0)
- `figment` (TOML + Env-Variablen)
- `tracing` + `tracing-subscriber` (JSON-Log, täglich rollend)

## Getting Started

### Voraussetzungen

- Zugang zur RabbitMQ-Instanz der HAG-Connect-Plattform.
  Service-User `svc-wms-connect` + Passwort kommen aus
  [`declare_users.py`](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform).
- Topologie (Exchange `hag.events`, Queues
  `wms-connect.inbound.{asn,art,order}`, Bindings) muss
  **vorher** per `declare_topology.py` provisioniert sein — der
  Converter deklariert nichts selbst.
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

Ein einziger Zielordner für alle Mandanten und alle drei Event-Typen;
die Dateinamen-Templates sind ebenfalls global. Die Mandantennummer
aus dem Event fließt in den Dateinamen ein, wird sonst aber nicht
validiert — das Contract-Schema (JSON Schema auf der Publisher-
Seite) ist die einzige Schranke.

```toml
[paths]
output_dir = "/mnt/wms-infiles"

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
2. AMQP-Verbindung mit Retry (10 Versuche × 3 s Pause).
3. Drei Consumer-Tasks spawnen, je ein Handler pro Queue.
4. Warten auf `SIGINT`.

### Fehlerpfade

| Situation                                           | Outcome                                                                      |
| --------------------------------------------------- | ---------------------------------------------------------------------------- |
| Parse-Fehler (JSON kaputt, Pflichtfeld fehlt)       | Reject → DLX, Log `parse error`                                           |
| Falsche Feldzahl (z. B.`rows[i]` hat 40 statt 41) | Reject → DLX                                                                |
| Ziel-Ordner nicht schreibbar                        | Reject → DLX, Log mit Path                                                  |
| Bus-Disconnect                                      | lapin reconnected automatisch; Consumer-Stream endet, Task loggt `beendet` |

DLX sammelt auf `hag.events.dlx` — von dort per `rmq-test-suite`
oder Management-UI inspizieren.

## Bekannte Grenzen (Iteration 1)

Bewusst klein gehalten; offen für Folge-Tickets:

- **Kein Atomic-Rename**: wir schreiben `xxx.dat` direkt. Für
  CIFS-Ziele evtl. `*.dat.tmp` → `rename(…)`-Pattern nachrüsten.
- **Seq-Counter in-memory**: beginnt nach Neustart wieder bei 1.
  Bei lückenlosen Nummernkreisen: Counter aus dem Zielordner
  inferieren (einmaliger Scan beim Start) oder persistieren
  (SQLite/Sidecar).
- **Keine Rückmeldung vom WMS**: Ablehnungen/Parsing-Fehler
  seitens xLogix werden nicht erkannt. Ein
  `wms.ingest-rejected`-Event wäre möglich, aktuell aber nicht
  geplant.
- **ART-Schema ist 47 Felder fix**. Felder 42-47 (palGewicht,
  anzImGebinde, inhaltEinzelteil, materialGruppe, gebindeGewicht,
  kartonsProPalett) sind bei Schenk typisch leer, werden aber
  bei ALDI-Mandanten real gefüllt — Publisher müssen sie
  trotzdem immer als leere Strings mitschicken. Die 2026-04-
  Schenk-Prod-Samples hatten nur 41 Felder, weil Lobster
  trailing-empty-Pipes abschneidet; wir schneiden nichts ab.
- **Kein SMB-Sink** — siehe *Was passiert nach dem `.dat`-Schreiben?*.

## Related

- **[HAG Connect Platform](https://gitlab.hartmannag.de/it-hartmann/hag-connect-platform)** —
  Event-Contracts, Topologie-Scripts, rmq-test-suite. Die drei
  hier konsumierten Events sind dort unter `contracts/schemas/events/`
  versioniert (v0.7.0).
- **[Schenk WE Export](https://gitlab.hartmannag.de/it-hartmann/schenk_we_export)** —
  Publisher für `wms.asn-ready`. Publisher für `wms.art-ready`
  und `wms.order-ready` sind noch TBD (Ersatz für die heutigen
  Lobster-Profile).

## Lizenz

Intern — Hartmann AG.
