# ADR-0007: Module tauschen Daten über ein generisches Ereignis aus

- **Status:** Angenommen
- **Datum:** 2026-08-19
- **Betrifft:** `meshdash-core` (`event`), alle Module

## Kontext

Das Telemetriemodul soll die Empfangsqualität über die Zeit führen. Der dafür
nötige SNR steht in den Nachrichten, die das Nachrichtenmodul abholt — er kommt
also über eine Leitung herein, die einem anderen Modul gehört.

[`module-system.md`](../module-system.md) verbietet den kurzen Weg: Kein Modul
liest die Tabellen eines anderen, keins ruft ein anderes auf, und keins darf
voraussetzen, dass ein anderes überhaupt läuft. Erlaubt ist, dass „das
besitzende Modul sie als Ereignis veröffentlicht".

Nur gab es dieses Ereignis nicht. `AppEvent` kannte bis hierher ausschließlich
Dinge, die der **Kern** feststellt: Verbindung auf, Verbindung zu, ein Frame kam
unaufgefordert herein. Etwas, das ein Modul zu sagen hat, hatte keinen Platz.

Das ist die erste Stelle, an der zwei Module überhaupt Daten teilen müssen. Was
hier entschieden wird, gilt für jede weitere.

## Entscheidung

`AppEvent` bekommt eine generische Variante:

```rust
AppEvent::Module {
    module: String,          // wer es veröffentlicht hat
    kind: String,            // was es ist, innerhalb dieses Moduls
    data: serde_json::Value, // die Nutzlast, vom Modul bestimmt
}
```

Der Kern transportiert sie und liest sie nicht. Er kennt weder die zulässigen
`kind`-Werte noch den Aufbau von `data`; beides gehört dem veröffentlichenden
Modul und wird in dessen Dokumentation beschrieben.

Ein empfangendes Modul filtert auf `module` **und** `kind` und behandelt eine
Nutzlast, die nicht zu seinen Erwartungen passt, wie eine unbekannte: es
überspringt sie, statt zu scheitern.

## Begründung

Der Kern bleibt fachlich blind — die Eigenschaft, auf der das ganze Modulsystem
beruht. Ein `AppEvent::MessageReceived { snr, .. }` hätte den Kern gezwungen zu
wissen, was eine Nachricht ist und welche Felder sie hat. Damit wäre die Regel
„Fachlichkeit gehört nicht in `meshdash-core`" beim ersten Anwendungsfall
gebrochen worden, und jede weitere Kopplung hätte eine weitere Variante
gefordert.

Die Kopplung bleibt lose in beide Richtungen. Das Nachrichtenmodul weiß nicht,
ob jemand zuhört; das Telemetriemodul weiß nicht, ob jemand sendet. Beide
laufen einzeln. Genau das verlangt `module-system.md`.

`serde_json::Value` ist ohnehin im Kern vorhanden, weil die API JSON spricht.
Die Variante kostet damit keine neue Abhängigkeit.

## Verworfene Alternativen

**Eine typisierte Variante je Anwendungsfall** (`AppEvent::MessageReceived`,
`AppEvent::AdvertHeard`, …) — der Kern hätte jede Fachlichkeit mitwachsen
müssen, die je ein Modul veröffentlicht. Ein neues Modul hieße dann: Kern
ändern. Das ist die Kopplung, die das Modulsystem gerade vermeidet.

**Ein zweiter, modul-eigener Bus** — mehr Infrastruktur für dieselbe
Zustellung, plus die Frage, welcher Bus wofür zuständig ist. Der vorhandene
Broadcast leistet es bereits.

**Das empfangende Modul liest mit, was das andere liest** — also `telemetry`
holt die Nachrichten selbst ab. Zwei Module, die dieselbe Warteschlange leeren,
nehmen sich gegenseitig die Nachrichten weg: Der Node händigt jede genau einmal
aus. Das wäre kein Entwurfsfehler auf Papier, sondern stiller Datenverlust im
Betrieb.

**Ein direkter Aufruf oder ein `JOIN` über Modulgrenzen** — ausdrücklich
verboten in `module-system.md`, und es würde `telemetry` von `nodes` abhängig
machen. Module müssen einzeln abschaltbar bleiben.

## Folgen

- Ereignisse zwischen Modulen sind **nicht typgeprüft**. Der Preis für den
  blinden Kern: Ein Tippfehler in `kind` fällt erst zur Laufzeit auf. Deshalb
  ist die Nutzlast jedes veröffentlichten Ereignisses im jeweiligen Modul zu
  dokumentieren, und der Empfänger überspringt, was er nicht versteht.
- Der Ereignisstrom unter `/api/v1/events` gibt diese Ereignisse mit aus. Was
  ein Modul veröffentlicht, ist damit auch im Browser sichtbar — das ist
  erwünscht, verlangt aber, dass **keine Geheimnisse** in `data` landen.
- `AppEvent` verliert `Eq`, weil `serde_json::Value` es nicht hat. `PartialEq`
  bleibt und genügt den Tests.
