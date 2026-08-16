# ADR-0004: Dokumentation auf Deutsch, Code auf Englisch

- **Status:** Angenommen
- **Datum:** 2026-08-16
- **Betrifft:** das gesamte Projekt

## Kontext

MeshDash entsteht als Werkzeug für ein konkretes, deutschsprachig betriebenes
Mesh, liegt aber unter GPL-3.0 öffentlich auf GitHub. Damit stellt sich die
Sprachfrage — und sie einmal falsch zu beantworten ist teuer, weil ein
nachträglicher Wechsel den gesamten Bestand betrifft.

Die beiden Enden: Alles auf Englisch maximiert die Reichweite, kostet aber
Präzision, wenn die Beteiligten Deutsch denken. Alles auf Deutsch ist präziser
für sie, macht den Code aber für Außenstehende ungewohnt und passt nicht zu den
Bibliotheken, gegen die programmiert wird.

## Entscheidung

**Deutsch:** Dokumentation, ADRs, Issues, Pull-Request-Beschreibungen,
Oberflächentexte.

**Englisch:** Code, Bezeichner, Code-Kommentare, Commit-Messages, Log-Ausgaben,
Fehlermeldungen im Code, Dateinamen.

## Begründung

- **Die Dokumentation richtet sich an die Beteiligten.** Sie wird in der Sprache
  präziser, in der die Beteiligten denken — gerade bei Architekturbegründungen,
  wo es auf Nuancen ankommt. MeshCore hat zudem eine große deutschsprachige
  Nutzerschaft; deutsche Dokumentation ist hier kein Nachteil.
- **Code lebt in einem englischen Umfeld.** Bezeichner stehen neben `tokio`,
  `axum` und `serde`. Ein deutscher Bezeichner mitten in einer englischen
  Signatur ist ein Bruch, und gemischte Sprache in einem Bezeichner
  (`node_zaehler`) ist schlechter als beides.
- **Commit-Messages sind Werkzeugoberfläche.** Conventional Commits, `feat`,
  `fix`, Changelog-Erzeugung — das Ökosystem ist englisch. Eine deutsche
  Commit-Message zwischen englischen Präfixen wirkt inkonsistent.
- **Protokollbegriffe bleiben ohnehin englisch.** `Advert`, `Repeater`,
  `Companion`, `Path` werden nicht übersetzt — sie sind Fachbegriffe der
  Firmware. Deutsche Fließtexte mit englischen Fachbegriffen sind normal und
  gut lesbar; das [Glossar](../glossary.md) fängt den Rest ab.

## Verworfene Alternativen

**Alles auf Englisch.** Maximale Reichweite, keine Trennlinie, keine
Grenzfälle. Ernsthaft erwogen und knapp verworfen: Die Qualität der
Architekturdokumentation ist für dieses Projekt wichtiger als die Zahl
potenzieller internationaler Mitwirkender — ein selbst betriebenes Dashboard
für ein regionales Mesh gewinnt kaum durch englische ADRs.

**Alles auf Deutsch, inklusive Code.** Verworfen: erzwingt Übersetzungen von
Fachbegriffen, die keine guten deutschen Entsprechungen haben, und bricht mit
jeder Bibliothek im Projekt.

**Zweisprachige Dokumentation.** Verworfen: Zwei Fassungen laufen auseinander,
und die schlechter gepflegte ist schlimmer als gar keine.

## Konsequenzen

**Positiv:** präzise Dokumentation für die Beteiligten; Code, der sich in sein
Ökosystem einfügt; klare Regel ohne Einzelfallentscheidungen.

**Negativ:** Die Reichweite unter internationalen Mitwirkenden ist geringer.
Wer Dokumentation *und* Code anfasst, wechselt in einem Pull Request die
Sprache. Zwischenformen brauchen eine Festlegung — sie steht in
[`../conventions.md`](../conventions.md).

**Zu beachten:** Oberflächentexte sind Deutsch, aber nicht hart im Code
verdrahtet werden sollten sie trotzdem nicht — wer später Mehrsprachigkeit
will, soll nicht jede Komponente anfassen müssen. Das ist keine Anforderung für
den Start, aber ein Grund, Texte nicht wahllos zu verstreuen.

## Wann diese Entscheidung neu zu prüfen ist

- Wenn regelmäßig englischsprachige Beiträge eingehen. Dann ist die Reichweite
  real und nicht mehr hypothetisch.
- Wenn MeshDash über den ursprünglichen Zweck hinaus von Fremden betrieben wird.
