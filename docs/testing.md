# Teststrategie

Die zentrale Randbedingung: **Ein MeshCore-Node ist keine Testvoraussetzung.**
Niemand hat in der CI Hardware am USB-Port, und wer am Projekt mitarbeitet,
soll das auch ohne Funkgerät können. Alles, was sich nur mit Hardware prüfen
lässt, ist faktisch ungetestet.

## Ebenen

### `meshdash-proto` — Byte-Tests

Die wichtigste Ebene. Reine Funktionen von Bytes nach Struktur, ohne I/O,
also mit gewöhnlichen Unit-Tests vollständig abdeckbar.

Pflicht für jede Frame-Art:

- **Round-Trip:** kodieren, dekodieren, Gleichheit prüfen.
- **Feste Byte-Arrays:** ein bekannter Frame aus echtem Verkehr, als Konstante
  im Test, mit Herkunftsangabe im Kommentar. Das ist die einzige Prüfung, die
  eine falsche Annahme über das Wire-Format tatsächlich aufdeckt.
- **Abgeschnittene Frames:** dürfen einen Fehler liefern, aber nicht panicken.
- **Unbekannte Opcodes:** müssen als `Unknown` durchkommen, nicht verworfen werden.
- **Überzählige Bytes:** ein Frame, dem Daten folgen, darf den Decoder nicht
  aus dem Tritt bringen.

Fuzzing des Decoders ist angedacht — der Decoder verarbeitet Fremdeingaben und
ist damit der natürliche Kandidat.

### `meshdash-transport` — Mock-Transport

Der Mock-Transport implementiert dasselbe `Transport`-Trait wie Serial und TCP
und spielt ein Skript ab. Er ist **Bestandteil der Architektur**, kein
Testbehelf: Ohne ihn lässt sich weder der Link noch ein Modul prüfen, und
das Frontend hat keine Datenquelle.

Ein Skript ist eine Folge von Schritten: `Emit` liefert einen Frame, `Drop`
lässt die Verbindung abreißen. Nach einem `Drop` schlagen weitere Zugriffe fehl,
bis erneut verbunden wird — dann läuft das Skript hinter der Abbruchstelle
weiter. So lässt sich ein abgezogenes USB-Kabel nachstellen, ohne eines zu
haben. Läuft das Skript aus, endet die Verbindung ebenfalls mit einem Fehler,
statt auf ewig zu warten; ein Test soll hängen können, aber nicht schweigend.

Skripte werden derzeit im Code zusammengesetzt. Sie aus Dateien zu laden ist
möglich und sinnvoll, sobald es aufgezeichneten Verkehr gibt — siehe
„Fixtures" unten.

Damit prüfbar:

- Antwortkorrelation im `Link`
- Verhalten bei Verbindungsabbruch und Wiederverbindung
- Zeitüberschreitung, wenn der Node nicht antwortet
- Pushes, die zwischen Kommando und Antwort eintreffen

### `meshdash-core` und Module — Integrationstests

- SQLite in-memory pro Test, Migrationen laufen echt durch.
- Synthetische Ereignisse auf den Event-Bus, dann den Datenbankzustand prüfen.
- Modulrouten über den zusammengebauten Router aufrufen, nicht die Handler
  direkt — sonst bleiben Routing und Serialisierung ungeprüft.
- Migrationen mindestens einmal gegen eine bestehende Datenbank testen, nicht
  nur gegen eine leere.

### Frontend

- Komponententests gegen gemockte API-Antworten.
- Ein Ende-zu-Ende-Test über den kompletten Durchstich — Mock-Transport,
  Backend, Browser — ist das Ziel, sobald Schritt 6 der Roadmap steht.

## Fixtures

Aufgezeichneter Verkehr gehört unter `fixtures/`, mit einer Notiz zur Herkunft:
welche Firmware-Version, welche Hardware, wann aufgenommen. Ein Fixture ohne
Herkunftsangabe ist wertlos, weil man später nicht beurteilen kann, ob es noch
gilt.

**Vor dem Ablegen prüfen:** Aufgezeichneter Verkehr kann Nachrichteninhalte,
öffentliche Schlüssel und Positionen enthalten. Was in ein öffentliches
Repository geht, ist zu anonymisieren.

## Was die CI prüfen wird

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm lint && pnpm typecheck && pnpm test && pnpm build
```

Was hier durchfällt, wird nicht gemergt.

## Gegen ein echtes Gegenüber laufen lassen

**Das ist die wirksamste Prüfung in diesem Projekt**, und keine automatisierte
ersetzt sie. Die Liste dessen, was hier ausschließlich so gefunden wurde, steht
in [`lessons-learned.md`](lessons-learned.md) und ist lang: eine
Ereignisschleife, die auf den Node wartete (zweimal), ein Kommando, das ohne
Node ewig hängt, ein leerer Zeitparameter, der 400 auslöst, eine falsch
dokumentierte Pfadbreite, ein Text, der sich selbst widersprach, abgebrochene
Kachelabrufe, eingefrorene Pakete auf der Karte, ein Zoom, der am Trackpad
fliegt.

Alle diese Fehler hatten grüne Tests.

### Warum Mocks sie nicht finden

Ein Mock-Transport antwortet in Mikrosekunden. Genau das versteckt jeden
Fehler, dessen Bedingung **Wartezeit** ist — eine blockierte Schleife fällt
nur auf, wenn das Gegenüber sich Zeit lässt. Dasselbe gilt für alles, was von
Darstellung abhängt: jsdom zeichnet nicht, hat kein Layout und keine Frames.

### Die Werkzeuge, die sich bewährt haben

- **Ein selbstgebauter Node aus dreißig Zeilen Python**, der genau ein Kommando
  beantwortet — und zwar langsam. Damit wurde der Sitzungsstart-Fehler gefunden.
- **Ein Mitleser am Ereignisstrom**, der Pushes zählt und rohe Pakete
  dekodiert. Damit wurde Stufe A gegen echten Funkverkehr bestätigt.
- **Ein lokaler Kachelserver**, der nummerierte Testkacheln ausliefert. Prüft
  die ganze Kette, ohne die Position eines echten Mesh an einen Fremden zu
  schicken.
- **Der Blick in den Browser**, mit gezielten Fragen statt Raten: Kommt die
  Anfrage an? Steht das Element da? Ist es sichtbar? Lässt sich das Bild
  dahinter noch laden? Der letzte Schritt war einmal die Antwort.

Die Skripte dazu gehören nicht ins Repository — sie sind Wegwerfware für einen
Befund. Was bleibt, ist der Befund in `lessons-learned.md`.

### Was dabei zu sagen ist

**Sag, was du nicht geprüft hast.** „Am Gerät ausprobiert" und „die Rechnung
ist getestet" sind zwei verschiedene Aussagen, und die zweite als die erste zu
verkaufen ist die einzige Form von Unehrlichkeit, die in diesem Projekt teuer
wird. Ein PR, der sagt „die Bewegung ist reine Interpolation und an drei
Punkten getestet, gesehen habe ich sie nicht", ist mehr wert als einer, der
„funktioniert" behauptet.

## Was bewusst ungetestet bleibt

Ehrlichkeit an dieser Stelle ist besser als Scheinabdeckung:

- **Echte Funkstrecken.** Reichweite, Störungen, Pfadwechsel im Feld lassen
  sich nicht automatisiert prüfen.
- **Verhalten echter Firmware.** Der Mock spielt unsere *Annahme* über die
  Firmware nach. Weicht die Annahme ab, sind die Tests grün und die Software
  falsch — genau deshalb gilt die Regel, Protokollwerte nicht zu raten.
- **Serielle Hardware-Eigenheiten.** Latenzen, Puffergrößen, USB-Reset-Verhalten.
