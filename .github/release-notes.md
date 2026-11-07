## OpenKontor {VERSION}

Freie Software für Rechnungen und Buchhaltung kleiner Firmen – als eigener Server mit
beliebig vielen Arbeitsplätzen, oder als Einzelplatz.

### Herunterladen

| System | Datei |
|---|---|
| Windows (die meisten PCs) | `OpenKontor-{VERSION}-windows-x64.zip` |
| Windows auf ARM (z. B. Surface Pro X) | `OpenKontor-{VERSION}-windows-arm64.zip` |
| macOS (Apple Silicon und Intel) | `OpenKontor-{VERSION}-macos-universal.zip` |
| Linux (x86, 64 Bit) | `OpenKontor-{VERSION}-linux-x64.tar.gz` |
| Linux auf ARM (z. B. Raspberry Pi 4/5, ARM-Server) | `OpenKontor-{VERSION}-linux-arm64.tar.gz` |
| Server als Docker-Image | `ghcr.io/johmarjac/openkontor-server:{VERSION}` |

Jedes Paket enthält den **Arbeitsplatz** (`openkontor`, mit Fenster) und den **Server**
(`openkontor-server`, für die Konsole), dazu die Anleitungen.

### Erster Start

- **Windows:** ZIP entpacken, `openkontor.exe` starten. Erscheint „Der Computer wurde durch
  Windows geschützt“: „Weitere Informationen“ → „Trotzdem ausführen“ (das Programm ist noch
  nicht mit einem kostenpflichtigen Zertifikat signiert).
- **macOS:** ZIP öffnen, `OpenKontor.app` in den Ordner „Programme“ ziehen. Beim ersten Start
  mit Rechtsklick → „Öffnen“ bestätigen (noch keine Apple-Beglaubigung).
- **Linux:** entpacken und `./openkontor` starten. Für den Server: siehe `SERVER-INSTALLATION.md`
  oder `sudo ./deploy/install-server.sh` im Quelltext.

### Prüfsummen

Die Datei `SHA256SUMS.txt` enthält die SHA-256-Prüfsummen aller Pakete.
