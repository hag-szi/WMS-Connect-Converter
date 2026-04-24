# WMS Connect Converter

Bus-Consumer, der drei Event-Typen vom HAG-Connect-Bus in
Lobster-kompatible `.dat`-Dateien auf einem Zielordner/Share
überführt:

| Event | Ausgabedatei (per Mandant konfigurierbar) |
|-------|-------------------------------------------|
| `wms.asn-ready` | `{mandant}_asn_YYYYMMDD_HHMMSS_NNNNN.dat` |
| `wms.art-ready` | `{mandant}_art_YYYYMMDD_HHMMSS_NNNNN_{trigger}.dat` |
| `wms.order-ready` | mandant-spezifisch (siehe `config/config.sample.toml`) |

Alle Dateien: pipe-delimited, CRLF, kein trailing CRLF am Ende
(verbatim zur Lobster-Ausgabe). Encoding wird pro Mandant gewählt
(ISO-8859-1 Default, UTF-8 für 510).

## Architektur

- `src/bus/` — dünnes `Consumer`-Interface; heutige Implementierung
  ist lapin (RabbitMQ). Bei späterem Bus-Wechsel (Kafka/Redpanda/…)
  wird nur der Adapter getauscht.
- `src/envelope.rs` — serde-Structs, die die Pydantic-Contracts aus
  `hag-connect-platform/contracts/schemas/events/*.yaml` spiegeln.
- `src/writer/` — ein Renderer je Event-Typ (ASN 25-Feld, ART 41-Feld,
  ORDER HL40/41/42/43).
- `src/sink/` — schreibt die Bytes in den Zielordner; SMB-Mount ist
  Deployment-Sache (auf dem Host via cifs-utils / systemd-mount).
- `src/mandant.rs` — Registry aus der Config (Dateinamen-Templates,
  Encoding, Zielpfad je Mandant).
- `src/handlers/` — drei Task-Handler, je ein Queue/Writer/Sink-Dreiklang.

## Lokal starten

```bash
cp config/config.sample.toml config/config.toml     # Passwörter eintragen
cp config/mandants.sample.toml config/mandants.toml # bei Bedarf anpassen
cargo run --release
```

Die Topologie (Exchange, Queues, Bindings, User) wird **nicht**
vom Converter deklariert — das macht `contracts/scripts/declare_topology.py`
im HAG-Connect-Repo.

## Tests

```bash
cargo test --release
cargo clippy --release --all-targets -- -D warnings
cargo fmt --check
```

## Lizenz

Intern — Hartmann AG.
