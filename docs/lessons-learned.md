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
