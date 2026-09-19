# ADR-0021: Warnungen dort, wo man hinsieht — für beobachtete Knoten

- **Status:** Angenommen
- **Datum:** 2026-09-19
- **Betrifft:** neues Modul `alerts`, Karte, Stufe D der Roadmap

## Kontext

Stufe D sah `alerts` vor: eine Warnung, wenn ein Knoten ausfällt, auf der Karte
an dem Knoten, um den es geht. Offen war, wohin eine Warnung **sonst noch**
geht. Der Betreiber hat am 2026-09-19 entschieden: **nirgendwohin** — auf die
Karte und als Benachrichtigung im Browser, solange MeshDash offen ist. Kein
Webhook, keine E-Mail, keine Nachricht ins Mesh.

Zwei Fragen bleiben, bevor sich etwas bauen lässt:

1. **Welche Knoten?** Das Mesh kennt jeden Knoten, der je einmal gehört wurde —
   auch einen Repeater hundert Kilometer entfernt, den eine Überreichweite an
   einem Abend hereingetragen hat. Er schweigt ab dem nächsten Morgen. Wer auf
   jeden schweigenden Knoten warnt, warnt dauernd und damit gar nicht.
2. **Woran merkt man „still"?** Das Modul darf die Tabellen von `nodes` nicht
   lesen ([`module-system.md`](../module-system.md)).

## Entscheidung

Das Modul `alerts` warnt nur für Knoten, die der Betreiber **ausdrücklich
beobachtet**, und dafür, dass der **eigene Node getrennt** ist. Ein beobachteter
Knoten gilt als still, wenn von ihm länger als `silent_after_hours` kein Advert
gehört wurde. Das Modul merkt sich selbst, wann es einen beobachteten Knoten
zuletzt gehört hat — aus den Adverts auf dem Ereignisbus, die ihren Absender
mit vollem Schlüssel nennen.

Warnungen werden mit Beginn und Ende festgehalten und als Ereignis
veröffentlicht. Zugestellt werden sie nur über den Ereignisstrom an den Browser.

## Begründung

**Beobachten statt alles.** Welche Knoten für einen Betreiber zählen — seine
eigenen Repeater, die Brücke ans Festland —, weiß nur er. Jede Regel, die das
errät („alles vom Typ Repeater", „was öfter als dreimal gehört wurde"), irrt
in beide Richtungen und wird zu Rauschen. Eine Liste, die er selbst führt, ist
eine eigene Frage mit eigenen Daten: genau die Bedingung, unter der laut
`module-system.md` ein Modul entsteht.

**Nur Adverts als Lebenszeichen.** Sie nennen ihren Absender mit vollem
Schlüssel — der Push `PUSH_CODE_ADVERT`/`PUSH_CODE_NEW_ADVERT` ebenso wie ein
gehörtes Advert (Protokollrecherche, 2026-09-19). Ein Pfadeintrag nennt nur ein
Präfix von ein bis drei Byte; bei einem Byte wäre „gehört" für jeden Zwilling
wahr, und eine Warnung, die ausbleibt, weil ein anderer Knoten dasselbe erste
Byte hat, ist schlimmer als keine. Repeater senden ihr Advert regelmäßig von
sich aus; die Voreinstellung von 24 Stunden lässt dafür Luft.

**Ohne Fremddienst.** MeshDash läuft dort, wo es keinen Uplink gibt — ein
Webhook oder eine E-Mail würden genau dann ausbleiben, wenn es darauf ankäme.
Was der Browser zeigt, braucht nur die Verbindung zu MeshDash selbst.

## Verworfene Alternativen

**Jeder Repeater warnt.** Ohne Pflege sofort nutzbar, aber jede Überreichweite
hinterließe eine Dauerwarnung. Nach einer Woche Betrieb stünden auf der Karte
mehr Warnungen als Knoten, die zählen.

**„Zuletzt gehört" aus der API von `nodes` lesen.** Zulässig wäre eine
ausdrückliche Schnittstelle. Sie müsste aber für jeden Kontakt sagen, wann er
zuletzt gehört wurde — auch aus Pfaden, also mit Präfix-Unschärfe — und würde
`alerts` an die Kontaktliste des Nodes koppeln. Die Adverts auf dem Bus sagen
dasselbe für die beobachteten Knoten, ohne diese Kopplung.

**In der Oberfläche berechnen.** Die Karte hat Kontakte samt letzter Sichtung
schon im Speicher. Eine Warnung gäbe es dann aber nur, solange jemand
hinsieht, und niemand wüsste, seit wann ein Knoten still ist — der Beginn
einer Warnung ist die halbe Aussage.

## Konsequenzen

**Positiv:** Keine Warnung, um die niemand gebeten hat. Der Beginn einer
Stille steht fest, auch wenn in der Zwischenzeit niemand hingesehen hat.

**Negativ:** Ein Knoten muss erst beobachtet werden, bevor er warnen kann.
Wer ihn nicht markiert, bekommt nichts. Ein Knoten, der weiterleitet, aber
kein Advert mehr sendet, gilt nach der Frist als still — seltener Fall, aber
die Warnung wäre dann falsch.

**Zu beachten:** Außerhalb des offenen Browsers erfährt niemand etwas. Das ist
gewollt und steht so in der Oberfläche. Akkustand als Warnung ist nicht Teil
dieses Schritts.

## Wann diese Entscheidung neu zu prüfen ist

Wenn MeshDash ohne ständig offenen Browser betrieben werden soll — dann fehlt
ein Zustellweg, und die Frage aus Stufe D ist neu zu stellen. Oder wenn sich
zeigt, dass beobachtete Knoten regelmäßig still gemeldet werden, obwohl sie in
Pfaden auftauchen.
