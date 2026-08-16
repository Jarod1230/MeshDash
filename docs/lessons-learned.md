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
2. **Vor Schritt 2 der Roadmap** ist die Richtung der Marker an echter Hardware
   oder am Firmware-Quellcode zu verifizieren. Es sind zwei Bytes — aber sie
   blockieren alles Weitere.
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

## 2026-08-16 — Eine gepinnte CI-Version verbirgt einen kaputten Arbeitsplatz

**Kontext:** `just check` sollte vor einer Fertigmeldung laufen. Der Rust-Teil
war grün, `check-web` brach ab — auf einem Stand, den die CI seit Wochen als
grün meldet.

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
