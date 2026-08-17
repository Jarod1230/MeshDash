# ADR-0006: Einzelnes Token, und kein ungeschützter Start nach außen

- **Status:** Vorschlag
- **Datum:** 2026-08-17
- **Betrifft:** `meshdash-server`, `meshdash-core` (Konfiguration), Frontend

## Kontext

Schritt 5 der [Roadmap](../roadmap.md) sieht „optionale Authentifizierung" vor
und verlangt dafür ausdrücklich einen ADR. Das Konfigurationsfeld `auth.token`
existiert bereits, wird aber von nichts ausgewertet.

Was auf dem Spiel steht, steht in [`../../SECURITY.md`](../../SECURITY.md):
**MeshDash ist eine Administrationsoberfläche.** Wer Zugriff hat, kann über den
angeschlossenen Node ins Mesh senden und — sobald das Modul existiert —
Repeater fernkonfigurieren. Die Datenbank enthält Nachrichtenverläufe,
Kontakte und Positionen im Klartext.

Randbedingungen:

- **Ein Betreiber, ein Mesh.** [`architecture.md`](../architecture.md) schließt
  Mandantenfähigkeit aus. Es gibt niemanden, dem unterschiedliche Rechte zu
  geben wären.
- **Dauerbetrieb auf kleiner Hardware**, typischerweise ein Raspberry Pi neben
  dem Node. Kein Administrator, der täglich draufschaut.
- **Die Voreinstellung ist heute schon sicher:** `bind` steht auf
  `127.0.0.1:8080`, ohne Token. Unerreichbar von außen, also unproblematisch.
- **Das Frontend ist ein Browser-Dashboard.** Was gewählt wird, muss ein Browser
  praktisch mitschicken können.

Die Gefahr ist nicht die Voreinstellung, sondern der Moment, in dem jemand
`bind` aufmacht, um von einem anderen Rechner zuzugreifen — und dabei nicht
daran denkt, dass damit alles offensteht.

## Entscheidung

**Ein einzelnes Bearer-Token aus der Konfiguration.** Ist `auth.token` gesetzt,
verlangt jede Anfrage unter `/api/v1/` einen passenden
`Authorization: Bearer …`-Header; andernfalls antwortet der Server mit `401`
im vereinbarten Fehlerformat.

**MeshDash startet nicht, wenn es nach außen lauscht und keinen Schutz hat.**
Ist `bind` keine Loopback-Adresse und kein Token gesetzt, bricht der Start mit
einer erklärenden Meldung ab — es sei denn, der Betreiber erklärt das
ausdrücklich mit einer neuen Option `auth.allow_unauthenticated = true`.

Das Frontend fragt das Token einmal ab und schickt es fortan mit.

## Begründung

- **Ein Geheimnis reicht für einen Betreiber.** Nutzerkonten lösen ein Problem,
  das dieses Projekt laut Architektur nicht hat. Was niemand braucht, muss auch
  niemand pflegen — und Sessionverwaltung, Passwort-Hashing und CSRF-Schutz
  sind Fläche, auf der Fehler passieren.
- **Der gefährliche Fall wird aktiv verhindert, nicht nur dokumentiert.** Ein
  Hinweis in `SECURITY.md` hilft dem nicht, der ihn nie liest. Ein Dienst, der
  sich weigert, ungeschützt ins Netz zu gehen, schon. Die Ausnahmeoption hält
  den legitimen Fall offen, verlangt aber eine bewusste Handlung.
- **Bearer-Token statt Basic-Auth**, weil der Browser bei Basic-Auth einen
  eigenen Dialog zeigt, den man nicht gestalten und aus dem man sich nicht
  abmelden kann.
- **Es passt zu beiden Nutzungsarten.** Ein Skript setzt den Header ohnehin;
  das Dashboard speichert das Token nach der Eingabe.

## Verworfene Alternativen

**Gar keine eigene Authentifizierung — der Reverse-Proxy macht das.** Das
stärkste Gegenargument, denn ein Proxy kann mehr als wir je bauen werden: TLS,
OIDC, mTLS, Sperrlisten. `SECURITY.md` empfiehlt den Proxy ohnehin.

Verworfen als *alleinige* Lösung, weil sie den Unfall nicht verhindert: Wer
`bind` aufmacht, ohne einen Proxy davorzusetzen, hätte keinerlei Schutz und
bekäme es nicht gesagt. Der Proxy-Fall bleibt über
`auth.allow_unauthenticated` ausdrücklich möglich — er wird also nicht
verboten, sondern nur zur bewussten Entscheidung gemacht.

**Benutzer und Passwort mit Sitzungen.** Vertraut, browsertauglich,
rotierbar. Verworfen: Ohne Mandanten gibt es genau einen Nutzer, und dann ist
ein Benutzername ein Feld ohne Inhalt. Der Preis wäre Passwort-Hashing, ein
Sitzungsspeicher, Ablauf, CSRF-Schutz — und jede dieser Stellen kann falsch
gebaut werden. Für ein Geheimnis, das ohnehin in einer Konfigurationsdatei
steht, ist das kein Gewinn an Sicherheit, sondern nur an Umfang.

**HTTP Basic Auth.** Am wenigsten Code, jeder Browser kann es. Verworfen: Der
Anmeldedialog ist nicht gestaltbar, eine Abmeldung ist praktisch nicht
vorgesehen, und die Zugangsdaten gehen bei jeder Anfrage mit — ohne TLS also
dauerhaft im Klartext.

**Token erzwingen, immer.** Auch auf Loopback. Verworfen: Es macht den
Erstkontakt unnötig sperrig — wer MeshDash lokal ausprobiert, müsste erst ein
Geheimnis erfinden. Der Wert wäre gering, weil auf Loopback nur erreichbar ist,
wer ohnehin auf der Maschine ist.

**Nur warnen statt den Start zu verweigern.** Verworfen: Eine Warnung im
Protokoll sieht niemand, der den Dienst als Systemdienst startet. Genau in dem
Fall wäre sie nötig.

## Konsequenzen

**Positiv:** Der wahrscheinlichste Fehlgriff — nach außen öffnen und
Authentifizierung vergessen — ist nicht mehr möglich, ohne ihn ausdrücklich zu
wollen. Der Aufwand bleibt klein: ein Vergleich pro Anfrage, kein Zustand.

**Negativ:** Ein Token in einer Konfigurationsdatei ist ein Klartextgeheimnis.
Wer die Datei lesen kann, kommt hinein — allerdings kann er dann laut
`SECURITY.md` ohnehin die Datenbank lesen. Rotation heißt: Datei ändern und neu
starten. Es gibt keine Abmeldung, kein Sperren einzelner Geräte.

**Ohne TLS bleibt das Token abhörbar.** Dieser ADR ersetzt den Reverse-Proxy
nicht, er sichert nur den Fall ab, in dem keiner da ist.

**Zu beachten:**

- `auth.allow_unauthenticated` ist eine neue Option und gehört nach
  [`../configuration.md`](../configuration.md).
- Der Vergleich des Tokens muss **zeitkonstant** erfolgen, sonst verrät die
  Antwortdauer das Geheimnis Zeichen für Zeichen.
- Ein abgelehnter Zugriff gehört protokolliert — aber ohne das gesendete Token.
- Der WebSocket-Strom braucht dieselbe Prüfung. Browser können bei
  WebSocket-Verbindungen keine eigenen Header setzen; dafür ist ein eigener Weg
  nötig (Token beim Verbindungsaufbau), der beim Bau des WebSockets zu
  entscheiden ist.
- Loopback heißt `127.0.0.0/8` und `::1`. `0.0.0.0` ist **nicht** Loopback,
  auch wenn es lokal erreichbar ist.

## Wann diese Entscheidung neu zu prüfen ist

- **Sobald mehrere Personen Zugriff brauchen** — etwa ein Verein mit mehreren
  Betreibern. Dann fehlt eine Zuordnung, wer was getan hat, und Nutzerkonten
  werden zur richtigen Antwort.
- **Sobald das `admin`-Modul Repeater-Passwörter speichert.** Dann wiegt ein
  einzelnes geteiltes Geheimnis schwerer, und eine zweite Stufe für heikle
  Aktionen ist zu erwägen.
- Wenn sich zeigt, dass die Startverweigerung Betreiber häufiger behindert als
  schützt.
