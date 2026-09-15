# ADR-0019: Bereiche statt Punkte — was Verortung hier sein kann

- **Status:** Angenommen
- **Datum:** 2026-09-10
- **Betrifft:** Stufe D der [Roadmap](../roadmap.md), Karte, Module `traffic`
  und `nodes`
- **Präzisiert:** [ADR-0012](0012-positionen-nur-aus-dem-mesh.md), das
  Triangulation als „den vorgesehenen Weg" für Knoten ohne gemeldete Position
  benennt. Diese Entscheidung sagt, was daraus wird.

## Kontext

Die Roadmap führt „Knoten triangulieren" als letzten großen Punkt der Stufe D.
Die Voraussetzungen sollten damals aus Stufe B (verortete Anker) und Stufe C
(gemessene Verbindungen) kommen. Beides steht jetzt — Zeit, die Sache genau
anzusehen, bevor jemand sie baut.

### Was Triangulation braucht

Für einen unverorteten Knoten U: **Entfernungsschätzungen zu mindestens drei
bekannten Positionen.** Jede davon braucht eine Signalmessung zwischen U und
einem verorteten Knoten sowie ein Modell, das Signal in Entfernung übersetzt.

### Was MeshDash tatsächlich hat

| Quelle | Was sie misst | Wo gemessen |
| --- | --- | --- |
| `traffic_packets.snr` / `.rssi` | Empfang eines gehörten Pakets | **nur am eigenen Node**, und nur für den letzten Abschnitt |
| `nodes_trace_hops.snr` | Empfang je Abschnitt eines Weges | zwischen fremden Stationen — aber nur, wenn jemand eine Wegmessung ausgelöst hat |
| `traffic_links` | **dass** zwei Stationen einander hören | überall, ohne Zutun |
| `telemetry_signal_samples` | Empfangsqualität über die Zeit | ohne Angabe, von wem — die Quelle ist „direkt" oder „Kanal", kein Knoten |

Das heißt: **Es gibt genau einen messenden Anker — den eigenen Node.**
Wegmessungen liefern Fremdmessungen, aber nur sporadisch, nur entlang bekannter
Routen und nur, wenn jemand dafür sendet.

### Warum ein Anker nicht reicht

Eine Entfernung von einem Punkt ergibt einen Ring, keinen Ort. Und der Ring ist
selbst zweifelhaft: Aus Empfangsstärke eine Entfernung zu schätzen setzt einen
bekannten Pfadverlust voraus. Bei LoRa hängt der an Gelände, Antennenhöhe und
Sendeleistung der **Gegenstelle** — alles unbekannt, alles je Knoten
verschieden. Am eigenen Mesh gemessen: derselbe Knoten kam mit −7 dBm und mit
−24 dBm an, bei praktisch gleichem SNR.

Wer daraus eine Entfernung rechnet, rechnet aus einer Unbekannten.

## Entscheidung

**Es gibt keine geschätzten Positionen als Punkte.** Kein Knoten bekommt einen
Ort, den niemand gemeldet hat.

**Stattdessen: der Bereich, in dem er liegen muss.** Aus den Hörbeziehungen
folgt eine harte Aussage ohne jedes Signalmodell — wenn U und ein verorteter
Knoten A einander hören, liegt U **innerhalb der Reichweite von A**. Mit
mehreren verorteten Nachbarn ist es der Schnitt dieser Bereiche.

**Die Reichweite wird nicht angenommen, sondern gemessen.** Sie ist die größte
Entfernung zwischen zwei *verorteten* Knoten, die einander nachweislich hören.
Damit steht keine erfundene Konstante im Code, sondern eine Beobachtung aus
genau diesem Mesh — und sie wird besser, je mehr das Mesh hergibt.

**Gezeigt wird der Bereich als Bereich**, nicht als Punkt in seiner Mitte. Ein
Knoten mit einem Nachbarn bekommt eine Scheibe von vielen Kilometern, und die
sieht auch so aus. Wer daraus keinen Ort ablesen kann, soll auch keinen ablesen.

**Ohne verorteten Nachbarn gibt es nichts.** Der Knoten bleibt in der Zählung
„n Knoten melden keine Position", wie bisher.

## Begründung

**Eine Aussage über Reichweite ist wahr, eine über Entfernung geraten.** „A hört
U" heißt sicher, dass U in Reichweite von A ist. Wie weit genau, sagt es nicht —
und genau dort hört die Aussage auf, statt in eine Zahl weitergerechnet zu
werden.

**Der Schnitt wird eng, wo das Mesh dicht ist.** Drei verortete Nachbarn in
einem Stadtgebiet ergeben eine brauchbare Fläche; einer auf dem Land ergibt
eine nutzlose, und dann sieht man das. Die Darstellung passt sich der Datenlage
an, statt überall gleich sicher auszusehen — dasselbe Prinzip wie bei der
Grundfläche, die zwischen Geografie und Ringen wechselt.

**Es kostet nichts.** Hörbeziehungen entstehen aus dem Zuhören. Kein Paket muss
gesendet werden, um einen Knoten einzugrenzen — im Unterschied zu jeder Lösung
über Wegmessungen.

**Es bleibt gültig, wenn später doch gemessen wird.** Kommen eines Tages
Fremdmessungen in Menge, verengt sich der Bereich; die Darstellung ändert sich
nicht, nur ihre Fläche. Ein Punkt hätte umgebaut werden müssen.

## Verworfene Alternativen

**Triangulation aus Empfangsstärke, wie ursprünglich vorgesehen.** Ein
messender Anker, ein unbekannter Pfadverlust je Gegenstelle. Das Ergebnis sähe
aus wie eine Messung und wäre eine Erfindung — genau das, was
[ADR-0012](0012-positionen-nur-aus-dem-mesh.md) verhindern wollte, nur mit
mehr Rechnung davor.

**Wegmessungen ausweiten, um Fremdmessungen zu bekommen.** Technisch der
sauberste Weg zu echten Entfernungsdaten — und er sendet. Jede Messung belegt
Sendezeit im gemeinsamen Band, und für flächendeckende Abdeckung bräuchte es
viele. Steht als Möglichkeit, nicht als Voreinstellung.

**Eine Reichweite als Konstante annehmen** (etwa „LoRa schafft 5 km"). Falsch
in jeder Richtung: im Stadtgebiet zu großzügig, auf freier Sicht zu knapp. Die
gemessene Spanne des eigenen Mesh ist die einzige Zahl, die hier etwas bedeutet.

**Den Mittelpunkt des Bereichs als Position anbieten.** Der Kompromiss, der
alles kaputtmacht: Auf einer Karte mit fünfzig Punkten liest niemand einem
Punkt seine Herkunft an. Genau dieselbe Begründung wie in ADR-0012.

## Folgen

- Die Ableitung ist reine Rechnung und gehört neben `lib/prefix.ts` und
  `lib/neighbours.ts` — sie liest mehrere öffentliche APIs, wie die Karte auch.
- **Die gemessene Reichweite ist eine Zahl mit Bedeutung** und gehört
  angezeigt: Sie sagt dem Betreiber, wie weit sein Mesh nachweislich trägt.
- Die Kartenebene zeigt Bereiche unter den Knoten, nicht zwischen ihnen. Sie
  gehört hinter einen eigenen Schalter — eine Fläche über der halben Karte ist
  nichts, was ständig im Weg sein soll.
- **Was fehlt, um enger zu werden:** Signalmessungen von fremden Ankern. Der
  Weg dorthin ist die Wegmessung, und der kostet Sendezeit. Diese Entscheidung
  schließt ihn nicht aus, sie setzt ihn nur nicht voraus.
- `telemetry_signal_samples` merkt sich **nicht**, von wem gemessen wurde. Für
  eine spätere Verfeinerung wäre das nachzuholen; für diese Entscheidung wird
  es nicht gebraucht.
