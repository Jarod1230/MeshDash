# ADR-0013: Den eigenen Node verortet der Betreiber

- **Status:** Angenommen
- **Datum:** 2026-08-26
- **Betrifft:** Modul `system`, Stufe B der Roadmap
- **Ergänzt:** [ADR-0012](0012-positionen-nur-aus-dem-mesh.md). Die
  Entscheidung dort gilt unverändert; dieser ADR zieht die Grenze, an der sie
  endet.

## Kontext

ADR-0012 hat das Eingabefeld für Koordinaten abgeschafft: Eine Position auf der
Karte kommt aus dem Mesh oder gar nicht. Damit bleibt eine Frage offen, die
Stufe B stellt — **woher kommt der erste Punkt?**

Ein Mesh, in dem niemand seine Position meldet, hat keinen Anker. Die
Triangulation aus Stufe D setzt verortete Nachbarn voraus; ohne einen einzigen
Bezugspunkt schwebt jede Schätzung frei. Und der eine Knoten, über den
MeshDash tatsächlich Bescheid weiß, ist der eigene: Wo er steht, weiß der
Betreiber, und niemand sonst kann es ihm sagen. Das Protokoll sieht genau
dafür `CMD_SET_ADVERT_LATLON` vor.

## Entscheidung

**Der Betreiber kann diesem einen Node sagen, wo er steht** — über
`PUT /api/v1/system/position`, in Grad. MeshDash gibt das an den Node weiter,
der Node übernimmt es und trägt es von da an in seinem Advert.

**Für alle anderen Knoten bleibt ADR-0012 unangetastet.** Es gibt kein Feld,
in das jemand die Position eines fremden Knotens schreibt.

**Was gezeichnet wird, kommt weiterhin aus dem Mesh.** Auch dieser Node
erscheint auf der Karte über sein Advert, nicht über die Eingabe. Der Weg
lautet: setzen — der Node sendet ein Advert — das Mesh hört es — die Karte
zeichnet es.

**Das Setzen sendet nichts.** Es ist eine Einstellung am Gerät. Ins Mesh kommt
sie mit dem nächsten Advert, und den löst ein Mensch aus.

## Begründung

**Eine Einstellung am Gerät ist keine Anmerkung an der Karte.** Genau das war
der Einwand in ADR-0012: Ein Handeintrag sitzt im Bestand der Beobachtungen und
sieht aus wie eine. Hier ist es umgekehrt — die Eingabe verlässt MeshDash und
wird zur Aussage des Node über sich selbst. Sie kommt durch dieselbe Tür
zurück wie jede andere Position und altert, widerspricht und veraltet genauso.

**Die drei Einwände aus ADR-0012 greifen hier nicht.** Sie zielen alle auf die
Menge: Fünfzig Knoten von Hand zu verorten ist Arbeit, die niemand pflegt.
Einen Node zu verorten ist einmalige Arbeit an dem einen Gerät, das man selbst
in der Hand hält — und sie verschwindet nicht, wenn die Triangulation kommt,
sondern wird deren Fundament.

**Der Node ist ohnehin die Quelle.** Verschwiege MeshDash die Einstellung,
setzte der Betreiber sie mit einem anderen MeshCore-Client — und MeshDash
zeigte das Ergebnis, ohne es erklären zu können.

## Verworfene Alternativen

**Gar nicht setzen, auf einen Node mit GPS warten.** Die meisten Boards haben
keins, und ein Repeater auf einem Dach braucht auch keins: Er bewegt sich nicht.

**Beim Setzen sofort ein Advert senden.** Bequem, aber es sendet ungefragt in
ein geteiltes Band. Wer eine Koordinate um eine Stelle korrigiert, löst damit
eine Flutung im ganzen Mesh aus. Advert bleibt eine eigene Handlung.

**Die Position in MeshDash halten und nur beim Verbinden an den Node schicken.**
Zwei Wahrheiten über denselben Wert, die auseinanderlaufen, sobald jemand den
Node anderweitig umstellt. Der Node ist die Quelle, MeshDash schreibt nur, was
er bestätigt hat.

## Folgen

- Modul `system` bekommt `PUT /api/v1/system/position`. Die Antwort des Node
  entscheidet: Nur ein bestätigtes `RESP_CODE_OK` wird gespeichert.
- Die gespeicherte Zeile in `system_self` wird beim nächsten Sitzungsbeginn
  ohnehin durch das ersetzt, was der Node über sich sagt.
- Die Systemseite bekommt beides an einem Ort: Position setzen und Advert
  senden. Getrennte Knöpfe, weil es getrennte Handlungen sind.
- Ein Anker für die Triangulation aus Stufe D existiert damit — genau einer,
  aber ohne ihn gäbe es keinen.
