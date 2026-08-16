# Recherche

Erkenntnisse über Fremdsysteme, auf die sich MeshDash stützt — vor allem
MeshCore. Getrennt von der Architekturdokumentation gehalten, weil hier
**Fremdwissen** steht, das wir nicht kontrollieren und das falsch sein kann.

## Regeln

1. **Jede Aussage braucht eine Quelle.** Link, Firmware-Version oder
   Commit-Referenz. Ohne Quelle ist es keine Erkenntnis, sondern eine Vermutung.
2. **Vermutungen werden als solche gekennzeichnet.** Ein eigener Abschnitt
   „Offene Fragen" ist besser als ein Wert, der sicher aussieht und es nicht ist.
3. **Verifikationsstufe dazuschreiben.** Es macht einen Unterschied, ob ein Wert
   aus einer Dokumentationsseite stammt oder aus einem Hexdump echter Hardware.
4. **Datieren.** Fremdsysteme ändern sich. Ein undatierter Stand ist wertlos.

## Verifikationsstufen

| Stufe | Bedeutung |
| --- | --- |
| `HARDWARE` | An echter Hardware beobachtet. Firmware-Version notiert. |
| `SOURCE` | Aus dem Firmware-Quellcode gelesen. |
| `REFERENZ` | Aus einer funktionierenden Fremdimplementierung übernommen. |
| `DOKU` | Aus veröffentlichter Dokumentation. Kann veraltet oder falsch sein. |
| `VERMUTUNG` | Abgeleitet, nicht belegt. Darf nicht in Code ohne Fallback. |

## Bestand

- [`meshcore-companion-protocol.md`](meshcore-companion-protocol.md) —
  Wire-Format und Opcodes des Companion-Protokolls. Das Framing für Serial und
  TCP steht auf Stufe `SOURCE`, sämtliche Opcodes weiterhin auf `DOKU`.
