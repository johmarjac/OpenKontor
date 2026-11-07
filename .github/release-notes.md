## OpenKontor {VERSION}

Freie Software für Rechnungen und Buchhaltung kleiner Firmen – als eigener Server mit
beliebig vielen Arbeitsplätzen, oder als Einzelplatz.

### Herunterladen

| System | Installieren | Ohne Installation |
|---|---|---|
| Windows (die meisten PCs) | `OpenKontor-{VERSION}-windows-x64-Setup.exe` | `OpenKontor-{VERSION}-windows-x64.zip` |
| Windows auf ARM (z. B. Surface Pro X) | `OpenKontor-{VERSION}-windows-arm64-Setup.exe` | `OpenKontor-{VERSION}-windows-arm64.zip` |
| macOS (Apple Silicon und Intel) | `OpenKontor-{VERSION}-macos-universal.dmg` | `OpenKontor-{VERSION}-macos-universal.zip` |
| Debian/Ubuntu (x86, 64 Bit) | `openkontor_{VERSION}-1_amd64.deb` | `OpenKontor-{VERSION}-linux-x64.tar.gz` |
| Debian/Ubuntu auf ARM (z. B. Raspberry Pi 4/5) | `openkontor_{VERSION}-1_arm64.deb` | `OpenKontor-{VERSION}-linux-arm64.tar.gz` |
| Server als Docker-Image | `ghcr.io/johmarjac/openkontor-server:{VERSION}` |

Jedes Paket enthält den **Arbeitsplatz** (`openkontor`, mit Fenster) und den **Server**
(`openkontor-server`, für die Konsole), dazu die Anleitungen.

### Erster Start

- **Windows:** ZIP entpacken, `openkontor.exe` starten. Erscheint „Der Computer wurde durch
  Windows geschützt“: „Weitere Informationen“ → „Trotzdem ausführen“ (das Programm ist noch
  nicht mit einem kostenpflichtigen Zertifikat signiert).
- **macOS:** ZIP öffnen, `OpenKontor.app` in den Ordner „Programme“ ziehen. Beim ersten Start
  mit Rechtsklick → „Öffnen“ bestätigen (noch keine Apple-Beglaubigung).
- **Debian/Ubuntu:** `sudo apt install ./openkontor_…deb`, danach den Server mit
  `sudo systemctl enable --now openkontor-server` starten.
- **Linux:** entpacken und `./openkontor` starten. Für den Server: siehe `SERVER-INSTALLATION.md`
  oder `sudo ./deploy/install-server.sh` im Quelltext.

Das Programm meldet beim Start, wenn es eine neuere Version gibt (abschaltbar unter
Einstellungen → Verbindung & Datensicherung).

### Prüfsummen

Die Datei `SHA256SUMS.txt` enthält die SHA-256-Prüfsummen aller Pakete.
