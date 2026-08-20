# ADR-0008: Bausteine des Frontends

- **Status:** Angenommen
- **Datum:** 2026-08-20
- **Betrifft:** `web/`, Schritt 7 der Roadmap

## Kontext

[ADR-0001](0001-tech-stack.md) legt React, TypeScript, Vite und Tailwind fest
und begründet React ausdrücklich mit dem Ökosystem für Dashboard-Bausteine.
Offen blieb, welche dieser Bausteine tatsächlich eingebunden werden.

Für Schritt 7 sind vier Fragen zu beantworten: Navigation, Datenabruf,
Diagramme und der Umgang mit dem Bearer-Token aus
[ADR-0006](0006-authentifizierung.md).

Zwei Randbedingungen prägen die Antworten:

**Das Frontend wird ins Binary eingebettet.** Jedes Kilobyte liegt dauerhaft im
ausgelieferten Artefakt und wird auf einem Raspberry Pi ausgeliefert.

**MeshDash läuft in Netzen ohne Internet.** Ein LoRa-Mesh existiert gerade
dort, wo keine Infrastruktur ist; der Dienst läuft auf einem Gerät im lokalen
Netz. Alles, was zur Laufzeit von außen nachgeladen würde, fehlt dann.

## Entscheidung

**Navigation: `react-router-dom`.** Verlauf, Tiefenlinks und die aktive
Markierung sind Detailarbeit, die man selbstgebaut zweimal falsch macht.

**Datenabruf: eigene Hooks, keine Bibliothek.** Die Oberfläche aktualisiert
sich über den Ereignisstrom, nicht durch Abfragen im Takt.

**Diagramme: eigenes SVG.** Zwei Zeitreihen mit je einer Linie.

**Schriften: ausschließlich Systemstapel.** Keine Web-Fonts, weder nachgeladen
noch eingebettet.

**Token: `localStorage`.** Einmal eingeben, dauerhaft angemeldet.

## Begründung

**Datenabruf.** Eine Bibliothek wie TanStack Query verwaltet einen Cache und
entscheidet anhand von Alter und Fokus, wann neu geladen wird. MeshDash weiß
es genauer: Der Ereignisstrom sagt, *dass* sich etwas geändert hat. Neben dem
Strom stünde ein zweiter Begriff von Aktualität, und beide müssten aufeinander
abgestimmt werden. Ein Hook, der lädt und auf ein Ereignis hin erneut lädt,
ist weniger Code als diese Abstimmung.

**Diagramme.** Achsen, Skalierung und ein Tooltip für eine Linie sind
überschaubar. Eine Diagrammbibliothek wiegt rund das Anderthalbfache des
heutigen Bundles — für Funktionen, die hier niemand braucht. Die Entscheidung
ist umkehrbar: Sobald Karten oder gestapelte Flächen dazukommen, ist sie neu
zu treffen.

**Schriften.** Das ist die Randbedingung, die am leichtesten übersehen wird.
Ein Verweis auf Google Fonts sähe im Entwicklungsnetz gut aus und fiele
ausgerechnet dort aus, wofür MeshDash gebaut ist — im Netz ohne Uplink. Fonts
mitzuliefern wäre möglich, kostet aber je Schnitt einige zehn Kilobyte im
Binary. Der Systemstapel ist auf jeder Zielplattform vorhanden, lädt sofort und
kostet nichts.

**Token in `localStorage`.** Ausdrücklich gegen den ursprünglichen Vorschlag
(`sessionStorage`) entschieden: MeshDash ist ein Dashboard, das man dauerhaft
offen hat, und ein Token, das bei jedem Neustart neu einzugeben ist, landet
erfahrungsgemäß in einer Textdatei neben dem Bildschirm — das wäre schlechter
als beides.

Die Abwägung ist damit bewusst getroffen, nicht übersehen: Bei einer
XSS-Lücke ist ein Token aus `localStorage` abgreifbar und überdauert das
Schließen des Browsers. Daraus folgt eine Pflicht für alles Weitere: **keine
Fremdinhalte ungefiltert ins DOM.** Nachrichtentexte, Node-Namen und
Kanalnamen kommen über Funk von fremden Geräten und sind niemals als Markup zu
behandeln. React entschärft das von sich aus, solange niemand
`dangerouslySetInnerHTML` benutzt — was in diesem Projekt damit ausgeschlossen
ist.

## Verworfene Alternativen

**TanStack Query** — siehe oben: ein zweiter Begriff von Aktualität neben dem
Ereignisstrom.

**Eigener Router** — rund 150 Zeilen, kein Gewinn. Der Verlauf des Browsers ist
kein Ort für Eigenbau.

**Recharts** — verworfen für heute, nicht für immer.

**Web-Fonts von einem CDN** — bricht im Netz ohne Uplink, also genau im
Einsatzfall. Auch ein Ausfall ohne Fehlermeldung ist ein Ausfall.

**Token nur im Arbeitsspeicher** — sicherste Variante auf dem Papier, verleitet
im Alltag zum Zettel.

## Folgen

- `dangerouslySetInnerHTML` ist im gesamten Frontend unzulässig. Wo Text von
  einem fremden Gerät stammt, wird er als Text gerendert.
- Kommt später ein Diagrammtyp dazu, den eigenes SVG nicht trägt, ist diese
  Entscheidung durch einen neuen ADR abzulösen.
- Das Bundle bleibt klein genug, dass es ins Binary passt, ohne dass jemand
  darüber nachdenken muss.
