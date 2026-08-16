# Glossar

MeshCore- und Projektbegriffe. Wer hier arbeitet, sollte den ersten Abschnitt
kennen — die Begriffe tauchen in Code, Protokoll und Oberfläche auf.

> Die MeshCore-Begriffe stammen aus der öffentlichen Dokumentation des Projekts.
> Wo etwas unsicher ist, steht es dabei. Korrekturen willkommen.

## MeshCore

**MeshCore**
: LoRa-Mesh-Firmware und -Protokoll. Anders als reine Broadcast-Ansätze arbeitet
MeshCore mit Pfaden über Repeater und mit Ende-zu-Ende-verschlüsselten
Direktnachrichten.

**Node**
: Ein Gerät im Mesh. Welche Rolle es spielt, hängt von der aufgespielten Firmware ab.

**Companion (Companion Radio)**
: Node-Rolle, die als Funkmodem für eine App dient. Er hat keine eigene
Bedienoberfläche, sondern spricht über Serial, TCP oder BLE mit einem Client.
**Das ist die Rolle, an die sich MeshDash anschließt.**

**Repeater**
: Node-Rolle, die Pakete weiterleitet und damit die Reichweite des Mesh
herstellt. Aus Betreibersicht das interessanteste Gerät — und das, das man
nicht ständig physisch erreichen kann. Repeater lassen sich über das Mesh
fernadministrieren, geschützt durch ein Passwort.

**Room Server**
: Node-Rolle, die Nachrichten für eine Gruppe vorhält, sodass Clients sie später
abholen können.

**Advert (Advertisement)**
: Paket, mit dem sich ein Node im Mesh bekannt macht — mit öffentlichem
Schlüssel, Name und optional Position. Die Grundlage dafür, dass MeshDash
überhaupt weiß, welche Nodes es gibt.

**Contact**
: Ein dem Node bekannter anderer Node, üblicherweise über einen Advert gelernt.
Wird im Companion gespeichert.

**Path**
: Die Folge von Repeatern, über die ein Paket ein Ziel erreicht. Kann sich
ändern; Pfadwechsel über die Zeit sind ein Diagnosesignal.

**Flood vs. Direct**
: Zwei Zustellarten. *Flood* verteilt breit ins Mesh, *Direct* nutzt einen
bekannten Pfad. Direct ist sparsamer, setzt aber einen gültigen Pfad voraus.

**Channel**
: Gemeinsamer Kanal mit geteiltem Schlüssel — alle mit demselben Schlüssel
lesen mit. Im Gegensatz zur Direktnachricht, die auf ein Schlüsselpaar geht.

**SNR / RSSI**
: Empfangsqualität. SNR (Signal-Rausch-Abstand, in dB) ist bei LoRa
aussagekräftiger als RSSI, weil LoRa noch unterhalb des Rauschens dekodiert.

**Companion-Protokoll**
: Das binäre Protokoll zwischen Client und Companion-Node. Siehe
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md).

**Frame**
: Eine abgegrenzte Übertragungseinheit auf der Leitung. Wie die Abgrenzung
zustande kommt, hängt vom Transport ab: Bei Serial und TCP steht ein
Richtungs-Marker mit Längenangabe davor, bei BLE begrenzt die Characteristic
den Frame. Der Inhalt eines Frames ist der Payload, und dessen erstes Byte ist
der Opcode.

**Opcode**
: Erstes Byte eines Protokoll-Payloads; legt fest, um welches Kommando, welche
Antwort oder welche Push-Meldung es sich handelt. Werte unter `0x80` sind
Antworten, ab `0x80` Pushes — daran erkennt der Link, was zu einer Anfrage
gehört und was der Node von sich aus meldet.

**Push**
: Meldung, die der Node von sich aus schickt, ohne dass ein Kommando sie
angefordert hat — etwa ein eingehender Advert.

## MeshDash

**Modul**
: Fachlich abgeschlossene Erweiterungseinheit mit eigenen Tabellen, Routen und
Oberfläche. Siehe [`module-system.md`](module-system.md).

**Link**
: Die Komponente, die genau eine Node-Verbindung besitzt: schickt Kommandos,
ordnet Antworten den Anfragen zu und verteilt Pushes auf den Event-Bus.

**Transport**
: Austauschbare Anbindung an den Node — Serial, TCP, später BLE, sowie Mock
für Tests.

**Event-Bus**
: Broadcast-Verteilung von Ereignissen an alle interessierten Module.

**AppEvent**
: Ein Ereignis auf dem Bus. Die interne Darstellung, nicht das rohe Protokollframe.

**Mock-Transport**
: Transport, der Frames aus einem Skript liefert statt aus Hardware. Ermöglicht
Entwicklung und Tests ohne Node.
