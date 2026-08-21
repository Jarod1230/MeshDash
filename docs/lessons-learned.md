# Lessons Learned

Was uns Zeit gekostet hat, damit es das nicht noch einmal tut.

**Wann hier etwas hingehört:** Wenn dich etwa eine Stunde etwas gekostet hat,
das eine Notiz verhindert hätte. Nicht erst, wenn es „wichtig genug" wirkt —
die Einträge, die am meisten sparen, wirken beim Schreiben meist trivial.

**Gilt ausdrücklich auch für KI-Agenten.** Ein Agent startet ohne Gedächtnis;
diese Datei ist das Gedächtnis.

## Format

```markdown
## JJJJ-MM-TT — Kurze Überschrift

**Kontext:** Was wurde versucht.
**Problem:** Was passierte, und woran es lag.
**Konsequenz:** Was daraus folgt — konkret genug, um danach zu handeln.
**Belege:** Links, Commits, Issues.
```

Neue Einträge kommen nach unten. Einträge werden nicht gelöscht; wenn etwas
überholt ist, wird es als überholt markiert und der Grund dazugeschrieben.

---

## 2026-08-16 — Die veröffentlichte Protokolldoku widerspricht sich beim Framing

**Kontext:** Beim Aufsetzen des Projekts sollte das Wire-Format des
MeshCore-Companion-Protokolls belegt werden, statt es zu raten.

**Problem:** Die verfügbaren Quellen sagen beim Framing **nicht dasselbe**:

- Eine Beschreibung nennt die Richtungs-Marker mit vertauschten Werten,
  eine andere Stelle derselben Recherche beschreibt sie andersherum
  (`>` = `0x3E` = 62 ausgehend, `<` = `0x3C` = 60 eingehend). Beide Aussagen
  stammen aus demselben Rechercheschritt.
- Die Dokumentation auf `docs.meshcore.io` beschreibt **ausschließlich BLE** und
  sagt dort ausdrücklich, es gebe *kein* Längenpräfix — bei BLE begrenzt die
  Characteristic den Frame. Für Serial und TCP gilt das nicht: dort ist ein
  Längenpräfix nötig, weil ein Bytestrom keine Frame-Grenzen kennt.

Wer die BLE-Beschreibung auf Serial überträgt, baut einen Decoder, der nie
synchronisiert. Wer die Marker vertauscht, baut einen, der nie etwas findet.

**Konsequenz:**

1. **Framing ist transportabhängig.** BLE und Serial/TCP sind getrennt zu
   behandeln. Nicht das eine aus dem anderen ableiten.
2. ~~**Vor Schritt 2 der Roadmap** ist die Richtung der Marker an echter Hardware
   oder am Firmware-Quellcode zu verifizieren.~~ **Erledigt am 2026-08-16** am
   Firmware-Quellcode — und die in diesem Eintrag als „plausibler" bezeichnete
   Variante B war die **falsche**. Siehe den Eintrag weiter unten.
3. Ein `Unknown(u8)`-Fallback für jeden Opcode-Bereich ist Pflicht, kein
   Komfort. Die Doku ist nachweislich unvollständig.
4. Der Recherchestand mit Quellen liegt in
   [`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md).

**Belege:** siehe Quellenabschnitt in der Recherchedatei.

---

## 2026-08-16 — „Startklar" hieß Rahmen, nicht Code

**Kontext:** Beim Aufsetzen des Repositories wurde die Aufforderung, das Projekt
„vollumfänglich startklar" zu machen, als Auftrag gelesen, gleich eine
lauffähige Anwendung zu bauen. Tatsächlich gemeint war der Projektrahmen —
Dokumentation, Konventionen, Struktur —, damit die Entwicklung *beginnen* kann.

**Problem:** Es entstand Implementierungscode, bevor Architektur und Zuschnitt
schriftlich festgehalten waren. Der Code musste zurückgenommen werden.

**Konsequenz:** „Startklar", „Setup" und „Grundgerüst" sind mehrdeutig. Im
Zweifel vorher klären, ob ausführbarer Code gemeint ist oder der Rahmen darum.
Für dieses Repository gilt die Reihenfolge aus [`roadmap.md`](roadmap.md):
Entscheidung dokumentieren, dann bauen.

---

## 2026-08-16 — Die JS-Werkzeugkette bricht woanders, als sie meldet

**Kontext:** Aufbau des Frontend-Gerüsts (Schritt 1 der Roadmap) mit pnpm,
Vite und ESLint 9.

**Problem:** Zwei Stellen, an denen ein grün aussehender Schritt einen späteren
kaputt macht:

1. **pnpm führt Postinstall-Skripte seit Version 10 nicht mehr automatisch aus.**
   `pnpm install` meldet den Fund nur als beiläufige Warnung („Ignored build
   scripts: esbuild"), der Exit-Code ist 0. Ohne dieses Postinstall fehlt die
   esbuild-Binary, und der Fehler taucht erst bei `vite build` auf — also in
   einem anderen Schritt, im Zweifel erst in der CI. ~~Behoben mit
   `pnpm.onlyBuiltDependencies` in `web/package.json`.~~ **Überholt seit
   pnpm 11**, siehe den letzten Eintrag dieser Datei — die Einstellung steht
   jetzt als `allowBuilds` in `web/pnpm-workspace.yaml`.

2. **`eslint-plugin-react-hooks` v7 exportiert zwei Config-Formate nebeneinander.**
   `configs['recommended-latest']` ist noch die alte eslintrc-Form mit `plugins`
   als Array; die Flat-Config liegt unter `configs.flat['recommended-latest']`.
   Wer die naheliegende Variante nimmt, bekommt eine Fehlermeldung über das
   Flat-Config-Format, die nicht sagt, welches Plugin schuld ist.

**Konsequenz:**

- Nach jeder Änderung an den Frontend-Abhängigkeiten **einmal wirklich bauen**,
  nicht nur installieren. `just check-web` deckt genau das ab.
- Warnungen von `pnpm install` lesen, auch wenn der Exit-Code 0 ist.
- Bei ESLint-Plugins prüfen, ob es einen `configs.flat`-Zweig gibt, bevor man
  den Top-Level-Export einbindet.

**Belege:** `web/package.json`, `web/eslint.config.js` (Kommentar an der Stelle),
PR #2.

---

## 2026-08-16 — Die plausiblere Quelle war die falsche

**Kontext:** Auflösung der offenen Framing-Frage aus dem ersten Eintrag oben.
Die Recherche hatte zwei einander widersprechende Marker-Richtungen ergeben und
Variante B als „ausführlicher und plausibler" eingestuft — mit dem ausdrücklichen
Zusatz, dass plausibel nicht verifiziert heißt.

**Problem:** Verifiziert wurde am Firmware-Quellcode. Ergebnis: **Variante A ist
richtig**, App → Radio ist `0x3C`, Radio → App ist `0x3E`. Die ausführlichere,
detailreichere, überzeugender formulierte Darstellung war schlicht falsch.

Hätte jemand nach Plausibilität entschieden statt nach Beleg, wäre ein Decoder
entstanden, der auf einem Bytestrom **nie** synchronisiert — und der Fehler wäre
erst beim ersten Hardwaretest aufgefallen, weit entfernt von seiner Ursache.

**Konsequenz:**

1. **Ausführlichkeit ist kein Wahrheitskriterium.** Bei widersprüchlichen
   Quellen entscheidet nicht die überzeugendere Darstellung, sondern die
   höhere Verifikationsstufe. Notfalls bleibt die Frage offen.
2. **Der Firmware-Quellcode war in zwanzig Minuten gelesen.** Die Hürde, eine
   Frage auf Stufe `SOURCE` zu klären, wurde vorher deutlich überschätzt: Zwei
   Dateien im Repository `meshcore-dev/MeshCore` unter `src/helpers/` haben alle
   vier offenen Framing-Fragen beantwortet. Das ist billiger als jede weitere
   Runde Dokumentationsvergleich — **erst den Quellcode, dann die Doku.**
3. **Die Firmware ist die Node-Seite, MeshDash die App-Seite.** Beim Lesen von
   `writeFrame()`/`checkRecvFrame()` sind Sende- und Empfangsrichtung
   spiegelverkehrt zu unserer. Eine naheliegende Fehlerquelle beim Übernehmen.
4. Referenzimplementierungen haben Schutzmaßnahmen, die die Firmware nicht hat
   (Längen-Plausibilität beim Resync, Überspringen von Konsolenmüll vor dem
   Marker). Sie zusätzlich zu lesen lohnt sich, auch wenn der Quellcode die
   Frage schon beantwortet hat.

**Belege:** Abschnitt „Framing" in
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md)
mit Commit-genauen Links auf `ArduinoSerialInterface.cpp`,
`SerialWifiInterface.cpp`, `BaseSerialInterface.h` und `serial_cx.py`.

---

## 2026-08-16 — Eine gepinnte CI-Version verbirgt einen kaputten Arbeitsplatz

**Kontext:** `just check` sollte vor einer Fertigmeldung laufen. Der Rust-Teil
war grün, `check-web` brach ab — auf einem Stand, den die CI als grün meldet.

**Problem:** Die CI war auf `pnpm/action-setup` mit `version: 10` gepinnt,
lokal war pnpm 11 installiert. Dazwischen liegen zwei Brüche:

1. **pnpm 11 liest das `pnpm`-Feld in `package.json` nicht mehr.** Es meldet das
   als Warnung und ignoriert den Inhalt.
2. **`onlyBuiltDependencies` gibt es in pnpm 11 nicht mehr.** Es wurde zusammen
   mit vier verwandten Optionen durch eine `allowBuilds`-Map ersetzt. Die
   Einstellung ließ sich also nicht bloß verschieben, sie musste übersetzt
   werden.

Ergebnis: `pnpm install` brach mit `ERR_PNPM_IGNORED_BUILDS` ab, und damit jeder
Frontend-Schritt. Die CI merkte nichts davon, weil sie eine Version fuhr, für
die die alte Schreibweise noch galt. Der Fehler war nicht neu — er war nur
unsichtbar, solange niemand lokal ein aktuelles pnpm hatte.

**Konsequenz:**

1. **Eine gepinnte Werkzeugversion in der CI ist eine Aussage über die
   Vergangenheit, kein Beleg für den aktuellen Stand.** Grüne CI bei rotem
   Arbeitsplatz heißt: Die Versionen sind auseinandergelaufen, nicht: Der
   Arbeitsplatz ist falsch eingerichtet.
2. **Version in CI und Einstellungsformat gehören zusammen gepflegt.** Die CI
   fährt jetzt pnpm 11, passend zu `allowBuilds` in `web/pnpm-workspace.yaml`.
   Ein Kommentar an beiden Stellen sagt das.
3. Beim nächsten pnpm-Sprung zuerst prüfen, ob Einstellungen *umbenannt* wurden
   — nicht nur, ob sie *umgezogen* sind. Bei diesem Sprung war beides der Fall.
4. Offen und bewusst nicht mitgemacht: ein `packageManager`-Feld würde die
   Version an einer einzigen Stelle festlegen und dieses Auseinanderlaufen
   künftig verhindern. Das ist ein eigener Vorschlag, kein Teil dieser Behebung.

**Belege:** [pnpm 11 Breaking Changes](https://pnpm.io/blog/releases/11.0) —
Entfall des `pnpm`-Felds und Ersatz von `onlyBuiltDependencies` durch
`allowBuilds`; `web/pnpm-workspace.yaml`, `.github/workflows/ci.yml`.

---

## 2026-08-16 — Clippy nimmt Tests nicht von selbst aus

**Kontext:** Die ersten Tests des Projekts entstanden, für den Frame-Codec in
`meshdash-proto`. Ein Test darf und soll `unwrap()` verwenden: Wenn die
Erwartung bricht, ist der Panic genau das gewünschte Ergebnis.

**Problem:** Der Workspace setzt `unwrap_used` und `expect_used` auf `warn`,
und die CI fährt `-D warnings`. Ein Kommentar in der `Cargo.toml` behauptete,
Tests seien „über die eigenen Test-Ausnahmen des Werkzeugs" ausgenommen. Das
stimmt nicht: Clippy prüft Testcode genauso, solange nicht
`allow-unwrap-in-tests` in einer `clippy.toml` steht — und die gab es nicht.

Aufgefallen ist es erst jetzt, weil vorher schlicht kein Test existierte. Die
Konfiguration sah ein halbes Projekt lang richtig aus, ohne je zu greifen.

**Konsequenz:**

1. `clippy.toml` mit `allow-unwrap-in-tests` und `allow-expect-in-tests` liegt
   jetzt im Wurzelverzeichnis, der irreführende Kommentar in `Cargo.toml` ist
   korrigiert.
2. **Eine Lint-Regel ist erst belegt, wenn Code existiert, der sie auslösen
   würde.** Bei Konfiguration, die auf künftigen Code zielt, gilt dasselbe wie
   bei Protokollwerten: unbewiesen ist nicht bewiesen.

**Belege:** `clippy.toml`, `[workspace.lints.clippy]` in `Cargo.toml`.

---

## 2026-08-16 — Eine falsch gestellte Frage bleibt unbeantwortbar

**Kontext:** Anheben der Opcodes von Stufe `DOKU` auf `SOURCE`, nach demselben
Muster wie zuvor beim Framing.

**Problem:** Zwei Dinge, die über den bereits notierten Vorrang des Quellcodes
hinausgehen:

1. **Die Dokumentation war nicht nur lückenhaft, sondern in Namen falsch.**
   Sie kannte 9 Kommandos, die Firmware definiert 58. Und sie nannte Konstanten,
   die es so nicht gibt: `RESP_CODE_ERROR` heißt `RESP_CODE_ERR`,
   `CMD_GET_BATTERY` heißt `CMD_GET_BATT_AND_STORAGE`, `PUSH_CODE_ACK` heißt
   `PUSH_CODE_SEND_CONFIRMED`. Die **Zahlenwerte** stimmten durchweg — wer nach
   Namen sucht statt nach Werten, findet die Stelle im Quellcode trotzdem nicht.

2. **Eine unserer offenen Fragen war falsch gestellt.** Sie lautete: „Ab welcher
   Firmware-Version kommen die V3-Nachrichtenvarianten?" Die Antwort ist: an der
   Firmware liegt es nicht. Die App teilt in `CMD_DEVICE_QUERY` mit, welche
   Protokollversion sie versteht, und die Firmware richtet sich danach. Wir
   hatten die Kontrollrichtung umgedreht angenommen und deshalb nach etwas
   gesucht, das es nicht gibt.

**Konsequenz:**

1. **Offene Fragen zu Fremdsystemen enthalten Annahmen — die Annahme prüfen,
   bevor man die Frage beantwortet.** „Ab welcher Version…" setzt voraus, dass
   die Gegenseite entscheidet. Wäre die Frage so recherchiert worden, wie sie
   dastand, hätte die Suche nichts ergeben und der Eintrag wäre offen geblieben.
2. **Namen aus Dokumentation sind schwächere Belege als Zahlen.** Beim Abgleich
   mit Quellcode nach dem Wert suchen, nicht nach der Bezeichnung.
3. Ergänzt den Eintrag „Die plausiblere Quelle war die falsche" — dort ging es um
   widersprüchliche Angaben, hier um unvollständige und falsch benannte.

**Belege:** Abschnitte „Opcodes" und „Protokollversionen" in
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md),
mit Verweis auf `MyMesh.cpp` und die Verzweigungen auf `app_target_ver`.

---

## 2026-08-16 — Backoff gehört an den Verlust, nicht an den Fehlversuch

**Kontext:** Wiederverbindung im `Link`. Der Backoff war an das Öffnen der
Verbindung geknüpft: Scheitert `connect`, wird gewartet und die Wartezeit
verdoppelt. Gelingt `connect`, geht es sofort weiter.

**Problem:** Der Fall „Verbinden **gelingt**, die Verbindung stirbt aber sofort
danach" war dabei nicht abgedeckt. Dann lief:

```
connect ok → lesen → Fehler → connect ok → lesen → Fehler → …
```

ohne jede Wartezeit dazwischen — eine Endlosschleife bei voller CPU-Last. Im
Test fiel das sofort auf, weil der Testablauf den Executor blockierte und
**alle** Tests des Crates gleichzeitig hingen, nicht nur der neue. Der erste
Verdacht galt deshalb dem Testaufbau, nicht dem Produktionscode.

In Betrieb wäre es ein defektes Kabel oder ein Node in einer Neustartschleife
gewesen — also genau der Fall, für den es die Wiederverbindung überhaupt gibt.

**Konsequenz:**

1. **Gewartet wird nach jedem Verbindungsverlust, nicht nur nach einem
   gescheiterten Verbindungsversuch.** Ein erfolgreiches `connect` ist keine
   funktionierende Verbindung.
2. **Die Wartezeit wird nur zurückgesetzt, wenn tatsächlich etwas übertragen
   wurde.** Sonst setzt eine flatternde Verbindung den Backoff bei jedem
   Durchlauf zurück und wächst nie — die Deckelung liefe ins Leere.
3. **Hängen alle Tests eines Crates gleichzeitig, ist die Ursache oft eine
   Endlosschleife ohne `await`**, nicht der zuletzt geänderte Test. Bei einem
   einzelnen Ausführungsstrang blockiert eine solche Schleife alles.
4. Ein Test, der beim Umbau plötzlich hängt statt fehlzuschlagen, prüft
   womöglich noch das alte Verhalten. Hier wartete einer auf das Ende des
   Aktors — das es nach der Änderung bewusst nicht mehr gibt.

**Belege:** `Interruption::Disconnected { made_progress }` in
`crates/meshdash-core/src/link.rs`, Tests `keeps_running_when_the_connection_dies`
und `backs_off_further_with_every_failed_attempt`.

---

## 2026-08-17 — Ein Broadcast ohne Rückschau bestraft die Startreihenfolge

**Kontext:** Das erste Fachmodul (`system`) hört auf dem Event-Bus mit und
schreibt Verbindungsereignisse fort. Alle Unit- und Modultests waren grün.

**Problem:** Im echten Betrieb blieb der Status **dauerhaft** auf „nicht
verbunden", obwohl der Node erreichbar war. Das Protokoll zeigte den Grund auf
die Millisekunde:

```
.485982  connected to node over TCP      ← Link meldet NodeConnected
.486422  module started                  ← Modul abonniert erst jetzt
```

Der Kern verdrahtete in der Reihenfolge Link → Module. Der Bus hält
**keine Rückschau** (so dokumentiert und so gewollt), also ging das erste
`NodeConnected` an null Zuhörer und war verloren. Der Zustand wäre erst nach dem
nächsten Verbindungsabbruch richtig geworden — bei einem stabilen Node also nie.

**Kein Test hat das gefunden**, weil jeder Test das Ereignis selbst
veröffentlicht, nachdem er abonniert hat. Genau die Reihenfolge, die in der
Anwendung falsch war, kam im Test nicht vor.

**Konsequenz:**

1. **Wer auf einem Broadcast ohne Rückschau lauscht, muss vor dem Erzeuger
   bereitstehen.** Der Link wird deshalb mit `link::prepare` gebaut und erst
   nach `registry.start_all` mit `.start()` losgelassen. Die Reihenfolge steht
   damit im Typ, nicht in einem Kommentar.
2. **Startreihenfolgen sind eine eigene Fehlerklasse.** Sie zeigen sich nicht in
   Komponententests, weil dort jeder Test seine Welt selbst aufbaut — meist in
   der richtigen Reihenfolge.
3. **Ein Durchstichlauf mit echtem Prozess findet, was Tests nicht finden.** Das
   ist inzwischen der dritte Fehler dieser Art (siehe die Busy-Loop weiter oben
   und die Rangfolge von Routing und Authentifizierung). Nach einem fertigen
   Feature einmal wirklich starten, nicht nur `cargo test`.

**Belege:** `link::prepare` und `PreparedLink` in
`crates/meshdash-core/src/link.rs`, Test
`a_prepared_link_stays_quiet_until_started`; die Verdrahtung in
`crates/meshdash-server/src/main.rs`.

---

## 2026-08-18 — Eine Route auf `/` greift im eingehängten Router nicht

**Kontext:** Das Modul `messages` bot seine Liste unter `/api/v1/messages/` an,
also mit einer Route auf `/` innerhalb des Modul-Routers. Alle Tests waren grün.

**Problem:** Im laufenden Dienst antwortete der Pfad mit `404`. Die Module
davor waren nicht betroffen, weil ihre Routen Namen tragen — `/status`,
`/contacts`. Erst das erste Modul mit einer Wurzelroute fiel darauf herein.

**Kein Test hat es gefunden**, weil die Modultests die Speicher- und
Abholfunktionen direkt aufrufen und den Router gar nicht durchlaufen. Geprüft
war also, dass die Daten stimmen — nicht, dass sie abrufbar sind.

**Konsequenz:**

1. **Modulrouten bekommen einen Namen.** `/received` statt `/`. Das ist auch
   fachlich besser: Sobald es Senden gibt, wäre `/` mehrdeutig gewesen.
2. Der Vertrag in [`module-system.md`](module-system.md) sagt das jetzt
   ausdrücklich, damit es nicht jedes neue Modul selbst herausfindet.
3. **Ein Modul ist erst geprüft, wenn seine Route abgerufen wurde.** Daten in
   der Datenbank nützen nichts, wenn der Weg dorthin nicht erreichbar ist.

**Belege:** `routes()` in `crates/meshdash-modules/src/messages/mod.rs`.

---

## 2026-08-18 — Ein Haken ist eine Aussage, keine Zusammenfassung

**Kontext:** Nach dem vierten Modul galt Schritt 6 der Roadmap als erledigt.
Alle vier Punkte trugen ein Häkchen, die Einschränkungen standen daneben.

**Problem:** Drei der vier Module waren gegenüber ihrer **eigenen
Anforderung** unvollständig — `nodes` ohne Nachbarn, `messages` ohne Kanäle und
ohne Senden, `telemetry` ohne Empfangsqualität. Über der Liste steht „jedes für
sich abgeschlossen".

Schlimmer als das Häkchen war, **wie** es gesetzt wurde: Beim Abhaken wurde die
Beschreibung mitgeschrieben. Aus „Kontakte **und Nachbarn** mit Verlauf" wurde
„Kontakte mit Erst- und Letztsichtung" — eine Beschreibung des Erledigten
anstelle der Anforderung. Die Lücke war danach nicht mehr sichtbar, sondern nur
noch der Nachsatz „fehlt noch", der wie ein Zusatzwunsch klingt statt wie ein
offener Teil des Auftrags. Aufgefallen ist es erst durch Nachfrage.

Nebenbei fiel dabei eine falsche Verknüpfung auf: Die fehlende Empfangsqualität
war als „braucht CayenneLPP" abgelegt. Der SNR steckt aber in den Nachrichten,
die `messages` ohnehin abholt — erreichbar über ein Ereignis auf dem Bus. Eine
vermeintliche Blockade, die keine war.

**Konsequenz:**

1. **Beim Abhaken die Anforderung stehen lassen.** Was erledigt ist, kommt
   dazu; die ursprüngliche Formulierung bleibt, damit die Lücke sichtbar ist.
2. **Teilweise erledigt bekommt Unterpunkte, kein Häkchen.** Ein Haken auf der
   obersten Ebene heißt „vollständig", sonst heißt er nichts.
3. **Bevor etwas als blockiert gilt, den Weg dorthin prüfen.** „Braucht ein
   Fremdformat" war hier schlicht falsch.

**Belege:** Schritt 6 in [`roadmap.md`](roadmap.md), verglichen mit der
ursprünglichen Fassung im ersten Commit derselben Datei.

---

## 2026-08-19 — Ein Kommentar, der die Gefahr benennt, schützt nicht vor ihr

**Kontext:** `drain_messages()` holt Nachrichten einzeln ab, bis der Node
`RESP_CODE_NO_MORE_MESSAGES` meldet. Mitten in der Schleife stand der Hinweis,
eine unlesbare Nachricht dürfe den Ablauf nicht anhalten — „the node would
otherwise offer the same frame forever".

**Problem:** Genau dieses „forever" war unbehandelt. Der Kommentar zog nur die
halbe Konsequenz: Parse-Fehler wurden übersprungen, aber der Fall „der Node
antwortet dauerhaft mit etwas, das gar keine Nachricht ist" hatte keinen
Ausgang. Beim Live-Test gegen einen nachgestellten Node waren das **376.315
Anfragen in 16 Sekunden** — gegen echte Hardware hieße das, den Funk-Node mit
rund 24.000 Frames pro Sekunde zu fluten.

Alle Tests des Moduls waren dabei grün. Sie stellten Nodes nach, die sich an das
Protokoll halten.

**Konsequenz:**

1. **Wer eine Endlosgefahr im Kommentar benennt, muss sie im Code beenden.** Ein
   Kommentar ist eine Notiz, kein Abbruchkriterium.
2. **Jede Schleife, deren Ausgang von einer Gegenstelle abhängt, bekommt
   zusätzlich eine eigene Obergrenze.** Die Gegenstelle ist genau das, was sich
   nicht an Erwartungen halten muss.
3. **Ein Zweig `Some(_) => {}` ist eine Entscheidung, keine Auslassung.** Er
   sagt „jede andere Antwort behandeln wir wie die erwartete", und das ist
   selten gemeint.

**Belege:** Issue #37 im Repository. Gefunden beim Live-Test zu den Adverts, in
einem ganz anderen Modul — wieder durch das laufende Binary, nicht durch die
Testsuite.

---

## 2026-08-19 — Eine Warteschlange kann mehr als eine Sorte enthalten

**Kontext:** Kanalnachrichten sollten ergänzt werden. Die Erwartung war ein
eigener Weg — eigener Push, eigenes Kommando, eigener Abrufer.

**Problem:** Es gibt keinen eigenen Weg. `onChannelMessageRecv()` legt die
Nachricht in **dieselbe** Offline-Queue wie eine Direktnachricht und kündigt sie
mit demselben `PUSH_CODE_MSG_WAITING` an. Sie kommt über dasselbe
`CMD_SYNC_NEXT_MESSAGE` herein, nur mit anderem Opcode.

Das heißt: Der bestehende Abrufer war nicht bloß unvollständig, er wäre an der
ersten Kanalnachricht **stehen geblieben** — seit der Absicherung vom selben Tag
bricht er bei einem unerwarteten Opcode ab. Ein Node mit aktiven Kanälen hätte
ab der ersten Kanalnachricht keine Direktnachrichten mehr geliefert bekommen.
Aufgefallen ist das nur, weil vor dem Bauen die Firmwarestelle gelesen wurde.
Aus der Opcode-Tabelle allein geht es nicht hervor.

**Konsequenz:**

1. **Bevor ein zweiter Weg gebaut wird, prüfen, ob es wirklich zwei sind.**
   Getrennte Opcodes bedeuten nicht getrennte Kanäle.
2. **Wer eine Warteschlange leert, muss wissen, was alles darin liegen darf.**
   Eine Erlaubnisliste von Opcodes ist nur so gut wie die Kenntnis der Quelle.
3. Eine Absicherung gegen Unerwartetes kann eine Lücke **verschärfen**: Vorher
   wäre die Kanalnachricht übersprungen worden, nachher hält sie den Abruf an.
   Beides ist falsch, das zweite fällt schneller auf — aber nur, wenn jemand
   hinsieht.

**Belege:** `onChannelMessageRecv()` und `onChannelDataRecv()` in `MyMesh.cpp`,
Commit `d929643`; festgehalten in
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md).

---

## 2026-08-19 — Standangaben altern still, und die schlimmste steht ganz vorn

**Kontext:** Nach Abschluss von Schritt 6 eine Prüfung des Gesamtstands.

**Problem:** Sechs Stellen behaupteten noch den Stand von Schritt 1 bis 3.
`CLAUDE.md` schrieb „Gerüst steht … Funktionalität gibt es noch keine — kein
Protokoll-Codec, keine Datenbank, keine Route, kein Modul", der `README` zählte
Transport, Datenbank, API und Module als „noch nicht vorhanden" auf, und drei
Crate-Kopfkommentare sagten „Scaffolding only. Nothing is implemented yet".

Der gefährlichste Satz war ein anderer: **„Die Opcodes sind es nicht"** —
gemeint war „nicht verifiziert" — direkt neben Regel 1 in `CLAUDE.md`, und
gleichlautend in `meshdash-proto/src/lib.rs`. Zu dem Zeitpunkt waren alle 110
Opcodes am Firmware-Quellcode belegt. Wer das las, musste die vorhandene
Tabelle für geraten halten und hätte sie entweder nochmals verifiziert oder,
schlimmer, als unzuverlässig behandelt.

Diese Sätze standen jeweils **in der Datei, die man zuerst liest**: die
Arbeitsanweisung für Agenten, der Einstieg für Menschen, der Kopfkommentar
eines Crates. Kein einziger der zwölf PRs davor hat sie mitgezogen, weil jeder
für sich stimmig war — die Pflegepflichten in `CLAUDE.md` nennen ADR,
Lessons Learned, Modultabelle, Konfiguration und CHANGELOG, aber keine
Standangabe.

**Konsequenz:**

1. **Wer einen Roadmap-Schritt abschließt, prüft die Standangaben mit**:
   `CLAUDE.md`, `README.md` und der Kopfkommentar jedes berührten Crates.
2. **Eine Aussage über Verifikationsstand ist eine Protokollaussage.** Sie
   veraltet genauso still wie ein falscher Offset und richtet ähnlichen
   Schaden an — nur in die andere Richtung.
3. Regel 5 hat eine Kehrseite: Nichts als fertig melden, was nicht läuft —
   aber auch nichts als unfertig stehen lassen, was läuft.

**Belege:** Vergleich der genannten Dateien mit `docs/roadmap.md` nach dem
Merge von Schritt 6.

---

## 2026-08-20 — Ein leerer Zweig altert mit dem Modul

**Kontext:** Nachlese der Gesamtprüfung. Vier der fünf Abonnenten des
Event-Bus melden verlorene Ereignisse (`RecvError::Lagged`) als Warnung, das
Telemetriemodul verschluckte sie stillschweigend.

**Problem:** Der leere Zweig war einmal **richtig**. Das Modul reagierte nur auf
`NodeConnected`, um sofort eine Batteriemessung zu nehmen; ging so ein Ereignis
verloren, kam beim nächsten Verbindungsaufbau das nächste, und der Fünf-Minuten-
Takt lieferte ohnehin weiter. Es gab nichts zu melden.

Mit der Empfangsqualität hängt seit dem 2026-08-19 eine **Messreihe** an diesem
Kanal. Ein verlorenes Ereignis ist jetzt ein fehlender Messwert — eine Lücke in
einer Kurve, die vollständig aussieht. Betroffen ist genau der Fall, in dem sie
am meisten aussagt: viel Funkverkehr, viele Ereignisse, voller Puffer.

Beim Einbauen wurde der Zweig nicht angefasst, weil er nicht im Weg stand. Das
ist die Falle: Nicht der Code hat sich geändert, sondern das, was von ihm
abhängt.

**Konsequenz:**

1. **Wer einen Ereignistyp neu abonniert, prüft die Fehlerzweige derselben
   Schleife mit.** Was bei einem seltenen Steuerereignis vertretbar war, ist es
   bei einer Messreihe nicht mehr.
2. **Ein leerer `Err`-Zweig braucht eine Begründung im Kommentar**, sonst ist
   nach dem nächsten Umbau nicht mehr erkennbar, ob er gedacht oder vergessen
   ist.
3. Ein stiller Datenverlust ist schlimmer als ein lauter Fehler: Die Kurve
   zeigt Werte, nur nicht alle.

**Belege:** `telemetry/mod.rs` gegen die drei anderen Module und
`server/src/events.rs`, die alle warnen.

---

## 2026-08-20 — „Wird verworfen" und „wird abgeschnitten" sind nicht dasselbe

**Kontext:** Bei der Nachprüfung blieb ein Wert übrig, den ich nicht selbst
belegen konnte: `MAX_FRAME_SIZE = 176`. Die Quelle stand im Code
(`BaseSerialInterface.h`), die Datei lag aber nicht vor.

**Problem:** Der Wert stimmte. Der Satz daneben nicht:

> The firmware **drops** anything larger instead of splitting it.

`ArduinoSerialInterface::checkRecvFrame()` verwirft nichts. Sie liest weiter,
behält die ersten 176 Byte und kürzt die Länge:
`if (_frame_len > MAX_FRAME_SIZE) _frame_len = MAX_FRAME_SIZE;`. Der Node
verarbeitet also einen **verstümmelten** Rahmen, als wäre er vollständig.

Der Unterschied entscheidet, wem die Sorgfalt obliegt. Bei „wird verworfen"
wäre ein zu langer Rahmen ein lauter Fehlschlag, den man bemerkt. Tatsächlich
ist es ein stiller Datenfehler: gekürzter Text, oder — bei einem Rahmen mit
Feldern hinter dem Text — Unsinn in den hinteren Feldern. Die Grenze
durchzusetzen ist damit Pflicht des Absenders, nicht Absicherung des Node.

Bemerkenswert ist, wo der Fehler saß: nicht im Wert, sondern in der **Prosa**
neben einem korrekt belegten Wert. Regel 1 verlangt eine Quelle für den Wert;
sie hat nicht verhindert, dass die Erklärung daneben eine Vermutung war.

**Konsequenz:**

1. **Auch die Aussage über das Verhalten braucht einen Beleg**, nicht nur die
   Zahl. „Wird abgelehnt", „wird ignoriert", „wird gekürzt" sind drei
   verschiedene Verträge.
2. **Eine nicht einsehbare Quelldatei ist eine offene Stelle**, kein erledigter
   Beleg — auch wenn der Dateiname im Kommentar steht. Nachholen, sobald sie
   erreichbar ist; hier ging es über die GitHub-API in zwei Minuten.

**Belege:** `ArduinoSerialInterface.{h,cpp}` und `BaseSerialInterface.h`,
MeshCore Commit `d929643`.

---

## 2026-08-20 — Etwas mit CSS zu verstecken heißt nicht, dass es weg ist

**Kontext:** Ausbau der Oberfläche (Schritt 7). Die Kopfzeile sollte auf
schmalen Schirmen zweizeilig sein, auf breiten einzeilig.

**Problem:** Der erste Versuch war, die Bedienelemente zweimal zu setzen — eine
Fassung mit `sm:hidden`, eine mit `hidden sm:flex`. Sieht in beiden Breiten
richtig aus. Aufgefallen ist es nur, weil ein Test scheiterte: „Found multiple
elements with the role button and name /hellen Ansicht/".

Der Test hatte recht, und zwar nicht bloß technisch. `display: none` blendet für
das Auge aus, aber im Dokument stehen zwei Schaltflächen mit derselben
Beschriftung. Wer mit einer Bildschirmleseausgabe arbeitet, hört sie doppelt und
weiß nicht, welche gemeint ist; wer mit der Tastatur navigiert, tabbt eventuell
durch tote Elemente.

Die Lösung war kein zweites Element, sondern eine Anordnung: ein einziger Satz
Bedienelemente, per `order` und `flex-wrap` mal in die erste, mal in die zweite
Zeile gesetzt.

**Konsequenz:**

1. **Responsive Varianten dupliziert man nicht, man ordnet sie um.** `order`,
   `flex-wrap` und Grid-Bereiche kosten nichts und lassen das Dokument in Ruhe.
2. **Ein Test, der „mehrere Elemente gefunden" meldet, ist ein Fund, kein
   Hindernis.** Der naheliegende Reflex — genauer selektieren — hätte den
   Mangel zugedeckt.
3. Beim Prüfen im Browser gehören schmale Breiten dazu: Die Fehler dieser
   Sitzung — Knoten außerhalb der Zeichenfläche, überlappende Beschriftungen,
   eine vierzeilige Navigation — waren allesamt in keiner Testausgabe zu sehen.

---

## 2026-08-20 — Der erste echte Node hat zwei Protokollfehler in zehn Minuten gezeigt

**Kontext:** Bis hierher lief alles gegen nachgestellte Nodes. Dann hing zum
ersten Mal ein echter Companion am USB — ein Xiao S3 WIO mit Firmware v1.17.0.

**Problem:** Die Kontaktliste kam sofort und sah falsch aus. Von 25 Kontakten
zeigte MeshDash bei 22 einen Pfad aus 64 Nullbytes, bei einem „64 Stationen".
Dahinter steckten **zwei** Fehler, beide in derselben Zeile Code:

1. `0xFF` in `out_path_len` ist `OUT_PATH_UNKNOWN` — „kein Weg bekannt". Auf die
   Feldbreite begrenzt wurde daraus eine Reise über 64 Stationen.
2. Schlimmer: Das Byte ist **gar keine Länge**. Die unteren sechs Bit zählen die
   Stationen, die oberen zwei sagen, wie viele Bytes jede belegt. `64` heißt
   deshalb *null* Stationen mit Zwei-Byte-Hashes, nicht vierundsechzig.

Beides warf keinen Fehler. Beides ergab Zahlen, die man einer Oberfläche glaubt.

**Warum kein Test das fand:** Die Mocks stammen von mir, und sie bildeten meine
eigene Annahme ab — in jedem Testfall stand im Längenbyte genau die Zahl der
Bytes, die ich danach erwartete. Ein Mock kann eine falsche Annahme nicht
widerlegen, er bestätigt sie. Die Quelle hätte es gekonnt: `isValidPathLen()`
stand die ganze Zeit in `Packet.cpp`, drei Zeilen lang, und niemand hat sie
gelesen, weil das Feld „path_len" heißt und nach einer Länge aussieht.

**Konsequenz:**

1. **Ein Feldname ist keine Spezifikation.** Wo die Firmware eine eigene
   Prüf- oder Schreibfunktion für ein Feld hat (`isValidPathLen`, `writePath`),
   ist die zu lesen — nicht die Deklaration.
2. **Selbstgebaute Testdaten prüfen den Parser, nicht die Annahme.** Sie sind
   trotzdem richtig; sie beweisen nur weniger, als es aussieht. Was sie nicht
   ersetzen, ist ein Blick auf echte Bytes.
3. **Ein Hardwaretest gehört früher.** Zehn Minuten mit einem echten Node haben
   mehr Protokollfehler gefunden als sechs Wochen Mock-Tests. Nicht weil die
   Tests schlecht wären, sondern weil sie eine andere Frage beantworten.

**Belege:** `Packet::isValidPathLen()` und `Packet::writePath()` in
`src/Packet.cpp`, `OUT_PATH_UNKNOWN` in `src/helpers/ContactInfo.h`, MeshCore
Commit `d929643`; festgehalten in
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md).

---

## 2026-08-20 — Zwei benachbarte Felder, zwei Einheiten

**Kontext:** `RESP_CODE_SELF_INFO` auslesen — die Antwort, die ein Node auf die
Anmeldung gibt. Frequenz und Bandbreite stehen dort direkt nebeneinander, beide
als `u32`, beide von der Firmware mit demselben Ausdruck geschrieben:

```c
uint32_t freq = _prefs.freq * 1000;
uint32_t bw   = _prefs.bw * 1000;
```

**Problem:** Gleicher Ausdruck, verschiedene Einheiten. `_prefs.freq` ist ein
Float in **Megahertz**, `_prefs.bw` einer in **Kilohertz**. Auf der Leitung
steht deshalb Kilohertz neben Hertz. Beide Felder als Hertz zu lesen ergibt
keinen Fehler — es ergibt eine Frequenz von 870 kHz, ein Band, auf dem niemand
ein Mesh betreibt.

Aufgefallen ist es nur, weil das Prüfprogramm den Wert **ausgedruckt** hat und
`869618 Hz` sich falsch las. Ein Test gegen selbstgebaute Bytes hätte
bestätigt, was ich ohnehin annahm; die Zahl auf dem Bildschirm hat
widersprochen.

**Konsequenz:**

1. **Bei Zahlen mit physikalischer Einheit gehört die Einheit in den Feldnamen**
   — `frequency_khz`, nicht `frequency`. Ein falsch benanntes Feld pflanzt sich
   durch jede Umrechnung fort, die darauf aufbaut.
2. **Der Ausdruck in der Firmware sagt nichts über die Einheit.** `x * 1000`
   verrät nicht, was `x` war. Die Deklaration verrät es — hier ein
   `constrain(_prefs.freq, 150.0f, 2500.0f)`, das nur in Megahertz Sinn ergibt.
3. **Werte einmal ansehen, nicht nur zusichern.** Ein `assert_eq!` prüft gegen
   die eigene Erwartung. Ein ausgedruckter Wert prüft gegen die Wirklichkeit.

**Belege:** `NodePrefs.h` (`float freq`, `float bw`), `MyMesh.cpp` Zeilen 943
und 1072, MeshCore Commit `d929643`; am Gerät gelesen als `869618` / `62500`.

---

## 2026-08-21 — Zwei grüne Testsuiten, ein Feature, das nichts tat

**Kontext:** Absender auflösen. `nodes` sollte jeden Kontakt als Ereignis
veröffentlichen, `messages` sollte zuhören und daraus Namen für die
Sechs-Byte-Präfixe der Nachrichten bilden.

**Problem:** Die Empfängerseite war gebaut und getestet — sechs Testfälle, alle
grün, darunter der Kollisionsfall und das Umbenennen. Die **Senderseite fehlte
vollständig**: Ein Ersetzen im Quelltext hatte seinen Anker nicht gefunden und
war wirkungslos durchgelaufen.

Beide Modul-Testsuiten blieben grün, weil jede nur ihr eigenes Modul
registriert. Die Tests von `messages` schickten das Ereignis selbst — sie
prüften, dass das Modul richtig zuhört, und konnten gar nicht bemerken, dass
niemand spricht. Aufgefallen ist es erst am echten Node: 25 Kontakte in der
Datenbank, null aufgelöste Absender.

**Konsequenz:**

1. **Was über den Bus von Modul zu Modul geht, braucht einen Test mit beiden
   Modulen.** Dafür gibt es jetzt `tests/module_coupling.rs`. Ein Test, der nur
   eine Seite einer Kopplung prüft, prüft die Kopplung nicht — er prüft eine
   Hälfte und suggeriert das Ganze.
2. **Ein Ersetzen ohne Zusicherung ist ein stiller Fehlschlag.** Jedes
   skriptgesteuerte Ersetzen im Quelltext gehört mit einer Prüfung versehen,
   dass der Anker existierte.
3. Der Modulschnitt macht solche Lücken wahrscheinlicher, nicht seltener: Genau
   weil die Module nichts voneinander wissen, merkt keines, wenn das andere
   schweigt.

**Belege:** `messages_senders` blieb leer, während `nodes_contacts` 25 Zeilen
hatte; nach dem Nachtragen von `announce_contact()` waren es 25 zu 25.
