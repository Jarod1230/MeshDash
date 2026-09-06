# Frontend

Wie die Oberfläche geschnitten ist und wo etwas Neues hingehört. Das Gegenstück
zu [`module-system.md`](module-system.md), das dasselbe für das Backend tut.

**Lies das vor der ersten Änderung an `web/`.** Die Karte ist die Leitansicht
([ADR-0011](decisions/0011-karte-als-leitansicht.md)) — wer das nicht weiß,
baut eine weitere Seite neben die Karte statt einer Ebene auf ihr, und das
fällt erst im Review auf.

## Die vier Bereiche

```
web/src/
├── app/       Die Hülle: Routen, Blende, Overlay, Thema, Token-Abfrage
├── ground/    Die Karte — die Fläche, auf der alles liegt
├── modules/   Je Fachlichkeit eine Seite, aus der Registry gehängt
├── lib/       Was mehrere Bereiche brauchen: API, Ereignisstrom, Zeit, Präfixe
└── ui/        Bausteine ohne Fachwissen: Kurve, Band, Zustände, Suchfeld
```

Die Abhängigkeitsrichtung ist **`app` → `ground` → `modules` → `lib`/`ui`**.
Rückwärts nicht: `lib` weiß nichts von der Karte, `ui` weiß nichts von
MeshCore.

Eine Ausnahme ist gewollt und benannt: `ground` liest Typen aus
`modules/nodes/types.ts`, weil die Karte dieselben Kontakte zeichnet, die das
Modul auflistet. Das ist derselbe Fall wie im Backend — ein Client darf
mehrere öffentliche APIs lesen.

## Die drei Schichten

Aus [ADR-0011](decisions/0011-karte-als-leitansicht.md), und der Grund, warum
`app` und `ground` getrennt sind:

| Schicht | Was | Wo |
| --- | --- | --- |
| 0 | Die Fläche mit Kacheln, Knoten, Verbindungen, Paketen | `ground/Ground.tsx` |
| 1 | Bedienung als Overlay in den Ecken | `app/App.tsx`, `Overlay` |
| 2 | Kontexttafel bei Auswahl — Knoten oder Verbindung | `ground/NodePanel.tsx`, `ground/LinkPanel.tsx` |
| 3 | Die vollen Seiten, als Blende über allem | `app/App.tsx`, `Shutter` → `modules/` |

**Die Fläche liegt außerhalb der Routen und wird nie ausgehängt.** Genau das
lässt den Ausschnitt jeden Ausflug in die Tiefe überleben. Wer das ändert,
bricht die Leitansicht — siehe
[ADR-0014](decisions/0014-die-adresse-bleibt-ein-pfad.md).

## Wo kommt Neues hin?

| Was du bauen willst | Wohin |
| --- | --- |
| Eine neue Seite zu einer Fachlichkeit | `modules/<name>/`, plus **eine** Zeile in `modules/index.ts` |
| Etwas, das auf der Karte liegt | `ground/` — als Ebene, nicht als Seite |
| Rechnung ohne Browser (Projektion, Auflösung, Ableitung) | eigene `.ts` neben ihrer Verwendung, mit eigenen Tests |
| Ein Baustein ohne Fachwissen | `ui/` |
| Etwas, das zwei Bereiche brauchen | `lib/` |

**Rechnung gehört in eine eigene Datei, nicht in die Komponente.** Projektion,
Kachelabdeckung, Auflösung eines Präfixes, Ableitung der Nachbarn: alles reine
Funktionen, alle ohne Browser prüfbar. Das ist der Grund, warum die Karte
überhaupt Tests hat — jsdom zeichnet nicht.

## Ein Modul in der Oberfläche

`modules/index.ts` ist die einzige Registrierung. Ein Eintrag:

```ts
export const beispielModule: UiModule = {
  id: 'beispiel',
  title: 'Beispiel',
  summary: 'Was diese Seite beantwortet',
  path: '/beispiel',
  component: BeispielPage,
};
```

Die Hülle hängt daraus Reiter und Route. Eine Seite darf eigene Unterrouten
haben (`/knoten/:key`) — die Hülle kennt sie nicht, sie hängt einen Platzhalter
an den Pfad.

**Ein UI-Modul braucht kein Backend-Modul.** `settings` ist eine Seite über
Kernfunktionen und hat kein Gegenstück in `meshdash-modules`.

## Regeln, die hier gelten

**Der Browser deutet keine Nutzlasten.** Ein Paket zu lesen heißt, das
Protokoll zu kennen; dieses Wissen liegt im Dienst. `lib/pushes.ts` liest
genau ein Byte weit — den Opcode —, um zu wissen, welche Seite sich
aktualisieren soll. Alles Weitere kommt dekodiert über
`AppEvent::Module`.

**Was nicht belegt ist, wird nicht gezeichnet.** Ein Präfix, auf das mehrere
Knoten passen, benennt niemanden (`lib/prefix.ts`). Eine Verbindung ohne Beleg
bekommt keine Linie. Eine geschätzte Position gibt es nicht.

**Leere Zustände sagen, warum sie leer sind.** „Keine Verbindungen" ist eine
Aussage über das Mesh; „noch kein Weg belegt, und so entsteht einer" ist eine
über das Beobachtete. Nur die zweite ist wahr.

**Der Zustand steht in der Adresse.** Ansicht als Pfad, Auswahl und
Verfeinerung als Abfrageparameter — `?knoten=`, `?verbindung=`,
`?verbindungen=aus`. Ein Link muss öffnen, was ein Klick öffnet.

**Kein Zeitlesen im Rendern.** `Date.now()` während des Renderns macht eine
Komponente unrein; dafür gibt es `lib/useNow.ts`, und für Animationen eine Uhr
im Zustand. Was davon abhängt, ob der Tab sichtbar ist, gehört nicht an
`requestAnimationFrame` — siehe `lessons-learned.md`, 2026-08-28.

**Eigene Bausteine, wenige Abhängigkeiten.** Keine Komponentenbibliothek, keine
Kartenbibliothek, kein Datenabruf-Paket — begründet in
[ADR-0008](decisions/0008-frontend-bausteine.md) und
[ADR-0015](decisions/0015-eigene-zeichnung-statt-leaflet.md). Wer eine
Abhängigkeit einziehen will, schreibt vorher einen ADR.

## Datenabruf

`lib/useResource.ts` lädt einen Pfad und hält ihn aktuell,
`lib/usePagedResource.ts` blättert. Kein Cache-Paket: Frische entscheidet hier
nicht die Uhr, sondern der Ereignisstrom — eine Seite sagt über
`useLiveReload`, worauf sie reagiert, und lädt dann neu.

`lib/useLiveEvent` ist die Ausnahme für den Fall, dass das Ereignis **selbst**
die Daten ist: ein Paket, das über die Karte läuft, existiert eine Sekunde und
wird nie abgerufen.

## Prüfen

`pnpm test` läuft mit jsdom. Das heißt:

- **Reine Funktionen sind der Ort für die Zusicherungen.** Projektion,
  Kachelrechnung, Nachbarableitung: dort liegen die aussagekräftigen Tests.
- **jsdom zeichnet nicht.** Es gibt kein Layout, keine Größen, keine Frames.
  Was von Darstellung abhängt, prüft man im Browser — und sagt dazu, wenn man
  es nicht getan hat.
- **Auf Nachladen warten, nicht direkt lesen.** `findBy…` statt `getBy…`, wo
  etwas erst nach einem Abruf erscheint. Eine schnellere Maschine hat das schon
  einmal grün gemacht und die CI rot.
