# OpenKontor-Server installieren

Der Server ist ein reines Konsolenprogramm (`openkontor-server`) ohne Fenster. Er läuft auf
jedem Linux-Server (auch ohne Bildschirm), speichert alles in PostgreSQL und wird von den
Arbeitsplätzen (`openkontor`) über das Netzwerk angesprochen.

Es gibt drei Wege – von „ein Befehl“ bis „alles von Hand“. Am einfachsten auf
Debian/Ubuntu ist das fertige Paket von der Release-Seite:

    sudo apt install ./openkontor_0.1.0-1_amd64.deb
    sudo systemctl enable --now openkontor-server

Es legt den Benutzer `openkontor`, den Datenordner `/var/lib/openkontor` und die
Einstellungsdatei `/etc/openkontor/server.env` an (dort z. B. die PostgreSQL-Adresse eintragen).

---

## Weg 1: Installationsskript (Debian / Ubuntu) – empfohlen

Auf dem Server, im Projektordner:

    sudo ./deploy/install-server.sh

Das Skript erledigt alles:

1. installiert PostgreSQL und die Werkzeuge zum Bauen,
2. baut `openkontor-server` (nur der Server, ohne Grafikbibliotheken),
3. legt den Systembenutzer `openkontor` und die Datenbank `openkontor` mit zufälligem Passwort an,
4. schreibt die Zugangsdaten nach `/etc/openkontor/server.env` (nur für root und den Dienst lesbar),
5. richtet den Dienst `openkontor-server` ein, der beim Hochfahren automatisch startet.

Am Ende zeigt es die Adresse, mit der sich die Arbeitsplätze verbinden, z. B.
`http://192.168.1.20:8470`. Anderer Port: `sudo PORT=9000 ./deploy/install-server.sh`.

Ein **Update** ist dasselbe: neue Version holen und das Skript erneut starten.
Daten und Passwörter bleiben erhalten.

---

## Weg 2: Docker

Im Ordner `deploy` zuerst in `docker-compose.yml` das Passwort `bitte-aendern` (zweimal)
ersetzen, dann:

    cd deploy
    docker compose up -d

Das startet PostgreSQL und den Server; Port 8470 ist nach außen offen.

---

## Weg 3: Von Hand

### 1. Programm bauen

Auf dem Linux-Server (oder einem gleichartigen Rechner):

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    cargo build --release --no-default-features --bin openkontor-server
    sudo cp target/release/openkontor-server /usr/local/bin/

### 2. Datenbank anlegen

    sudo apt install postgresql
    sudo -u postgres psql -c "CREATE USER openkontor WITH PASSWORD 'geheim';"
    sudo -u postgres createdb -O openkontor openkontor

Die Tabellen legt der Server beim ersten Start selbst an.

### 3. Im Vordergrund ausprobieren

    openkontor-server --db postgres://openkontor:geheim@localhost/openkontor

Ausgabe:

    OpenKontor Server läuft auf 0.0.0.0:8470 – Daten: PostgreSQL localhost/openkontor
    Neuer Datenbestand – erste Anmeldung: Benutzer „admin“, Passwort „admin“ – bitte sofort ändern.

Beenden mit Strg+C. Jede Änderung ist zu diesem Zeitpunkt schon gespeichert.

### 4. Als Dienst einrichten

    sudo useradd --system --home /var/lib/openkontor --create-home openkontor
    sudo mkdir -p /etc/openkontor
    echo "OPENKONTOR_DATABASE_URL=postgres://openkontor:geheim@localhost/openkontor" | sudo tee /etc/openkontor/server.env
    sudo chmod 640 /etc/openkontor/server.env && sudo chown root:openkontor /etc/openkontor/server.env
    sudo cp deploy/openkontor-server.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now openkontor-server

---

## Optionen

| Option            | Umgebungsvariable          | Bedeutung                                         |
|-------------------|----------------------------|---------------------------------------------------|
| `--db URL`        | `OPENKONTOR_DATABASE_URL`  | PostgreSQL-Zugang                                 |
| `--port N`        | `OPENKONTOR_PORT`          | Port für die Arbeitsplätze (Standard 8470)        |
| `--bind ADRESSE`  |                            | Netzwerkadresse (Standard 0.0.0.0 = alle)         |
| `--data ORDNER`   |                            | Ordner für die tägliche JSON-Sicherung            |
| `--name NAME`     |                            | Name, den die Arbeitsplätze anzeigen              |
| `--help`          |                            | Hilfe                                             |

## Im Alltag

| Was                       | Befehl                                                   |
|---------------------------|----------------------------------------------------------|
| Läuft der Server?         | `systemctl status openkontor-server`                     |
| Meldungen ansehen         | `journalctl -u openkontor-server -f`                     |
| Neu starten               | `sudo systemctl restart openkontor-server`               |
| Datenbank sichern         | `sudo -u postgres pg_dump openkontor > sicherung.sql`    |
| Sicherung zurückspielen   | `sudo -u postgres psql openkontor < sicherung.sql`       |

Zusätzlich legt der Server täglich eine lesbare JSON-Kopie in `/var/lib/openkontor/Sicherungen` ab.

## Arbeitsplätze verbinden

Programm `openkontor` öffnen → „Mit dem Firmenserver verbinden“ → Serveradresse
(z. B. `http://192.168.1.20:8470`), Benutzer `admin`, Passwort `admin` → Anmelden.
Danach unter **Einstellungen → Benutzer & Passwort** das Passwort ändern und weitere
Benutzer anlegen.

Falls eine Firewall läuft: `sudo ufw allow 8470/tcp`.

## E-Mail-Versand (Angebote, Rechnungen, Mahnungen)

Eingerichtet wird er im Programm unter **Einstellungen → E-Mail** (als Administrator):
Mailserver (Vorlagen für Microsoft 365, Google, IONOS, STRATO, GMX, WEB.DE, Telekom),
Absender, optional „Antwort an“ und eine Blindkopie fürs Archiv; danach „Test-E-Mail senden“.
Bei einem Server verschickt **der Server** alle E-Mails – das Passwort wird dort gespeichert
und nie an die Arbeitsplätze übertragen. Der Server braucht dafür Zugang zum Mailserver
(meist Port 587 oder 465 nach außen).

Hinweise: Bei Microsoft 365 muss für das Absender-Postfach „Authentifiziertes SMTP“ im
Microsoft-365-Admin-Center erlaubt sein. Bei Google wird ein App-Passwort benötigt.

## Belegeingang (Eingangsrechnungen, Texterkennung, Sammelpostfach)

Eingangsrechnungen landen im Bereich **Belegeingang** – per „Dateien hinzufügen …“, per
Ziehen in das Programmfenster oder automatisch aus einem Sammelpostfach. Der Server

- liest **E-Rechnungen** (XRechnung in UBL- oder CII-Form, ZUGFeRD/Factur-X) exakt aus,
- liest bei **PDFs mit Text** den Text direkt,
- erkennt **Scans und Fotos** mit der freien Texterkennung Tesseract,
- schlägt Lieferant (aus den Kontakten), Rechnungsnummer, Datum, Fälligkeit, Betrag,
  Umsatzsteuer und die beim Lieferanten zuletzt genutzte Kategorie vor,
- bewahrt die **Originaldatei unverändert** auf (mit SHA-256-Prüfsumme; in PostgreSQL in
  der Tabelle `receipt_files`, ohne Datenbank im Ordner `Belege`).

Nach dem Prüfen genügt „Als Ausgabe buchen“. Doppelte Dateien werden erkannt.

**Texterkennung:** Das Installationsskript und das Docker-Image installieren sie mit.
Von Hand: `sudo apt install tesseract-ocr tesseract-ocr-deu poppler-utils`. Beim Start
meldet der Server, ob die Texterkennung bereit ist. E-Rechnungen und PDFs mit Text
funktionieren auch ohne sie.

**Sammelpostfach:** Unter Einstellungen → E-Mail → „Sammelpostfach für Belege“ ein
eigenes Postfach eintragen (z. B. `belege@firma.de`, IMAP mit SSL/TLS auf Port 993;
Microsoft 365: `outlook.office365.com`, Google: `imap.gmail.com` mit App-Passwort).
Der Server ruft es im eingestellten Abstand ab, übernimmt die Anhänge ungelesener E-Mails
(PDF, XML, Fotos; eingebettete Logos werden ignoriert) und markiert die E-Mails als gelesen.
Lieferanten können ihre Rechnungen dann einfach an diese Adresse schicken.

## Optional: Anmeldung mit Microsoft 365 (Single Sign-on)

Mitarbeitende melden sich dann mit ihrem Microsoft-365-Konto an („Mit Microsoft 365
anmelden“ im Anmeldebildschirm) – ohne eigenes OpenKontor-Passwort. Die normale Anmeldung
mit Benutzer und Passwort funktioniert weiterhin. Ohne die folgenden Einstellungen bleibt
die Funktion einfach aus.

### 1. App in Microsoft Entra registrieren (einmalig, als Microsoft-365-Administrator)

1. [entra.microsoft.com](https://entra.microsoft.com) → **Anwendungen → App-Registrierungen → Neue Registrierung**
2. Name: `OpenKontor`; unterstützte Kontotypen: **Nur Konten in diesem Organisationsverzeichnis**
3. Umleitungs-URI: Plattform **Öffentlicher Client/nativ (mobil und Desktop)**, Adresse `http://localhost`
4. Registrieren. Auf der Übersichtsseite stehen die beiden Werte, die der Server braucht:
   **Anwendungs-ID (Client-ID)** und **Verzeichnis-ID (Mandanten-ID)**.
5. Ein geheimer Clientschlüssel ist **nicht** nötig (die Arbeitsplätze nutzen das sichere
   PKCE-Verfahren). Berechtigungen: die vorgegebenen `openid`, `profile`, `email` genügen.
6. Optional: Unter **Unternehmensanwendungen → OpenKontor → Eigenschaften** „Zuweisung
   erforderlich“ einschalten und unter „Benutzer und Gruppen“ festlegen, wer überhaupt darf.

### 2. Server einstellen

In `/etc/openkontor/server.env` ergänzen und den Dienst neu starten:

    OPENKONTOR_SSO_TENANT=<Verzeichnis-ID>
    OPENKONTOR_SSO_CLIENT_ID=<Anwendungs-ID>
    # optional: Kolleginnen und Kollegen beim ersten Anmelden automatisch anlegen
    # OPENKONTOR_SSO_AUTO_ROLE=lesen        (oder: buchhaltung)

    sudo systemctl restart openkontor-server

Oder als Optionen: `--sso-tenant … --sso-client-id … [--sso-auto-role lesen]`.

### 3. Wer darf hinein?

- **Ohne** `AUTO_ROLE` (Standard, empfohlen): Der Administrator legt unter
  Einstellungen → Benutzer & Passwort den Benutzer mit der Microsoft-Anmeldeadresse als
  Namen an (z. B. `max@firma.de`) und hakt „Nur mit Microsoft 365 anmelden“ an.
- **Mit** `AUTO_ROLE`: Jede Person Ihrer Microsoft-365-Organisation wird beim ersten
  Anmelden mit dieser Rolle angelegt; der Administrator kann die Rolle danach ändern.
  Administratorrechte werden nie automatisch vergeben.

Der Server prüft jeden Anmeldenachweis selbst: Signatur mit den veröffentlichten
Schlüsseln von Microsoft, Organisation (Mandant), Anwendung, Gültigkeitsdauer und eine
Einmal-Zufallszahl. Nachweise aus anderen Organisationen oder für andere Anwendungen
werden abgelehnt. Der Server braucht dafür Internetzugang zu `login.microsoftonline.com`.

## Mehrere Firmen (Mandanten)

Ein Server kann die Buchhaltung mehrerer Firmen führen, jede mit eigenen Daten, Benutzern,
Nummernkreisen und E-Mail-Einstellungen. Ein Administrator legt eine neue Firma im
Arbeitsplatz unter **Einstellungen → Mandanten** an und ist dort mit seinem jetzigen
Passwort Administrator. Bei der Anmeldung erscheint dann eine Auswahl „Firma“.

Gespeichert wird jede weitere Firma in einem eigenen PostgreSQL-Schema (`ok_<name>`) bzw.
ohne Datenbank im Ordner `Mandanten/<name>` neben `server.json`.

## Handy-Version

Unter der Adresse des Servers (z. B. `http://192.168.1.20:8470/`) öffnet jeder Browser auf
Smartphone oder Tablet eine schlanke OpenKontor-Version: Belege fotografieren (der Server
liest sie aus), offene Rechnungen ansehen und als bezahlt buchen, Ausgaben erfassen,
Kontakte nachschlagen. Über „Zum Startbildschirm hinzufügen“ wird daraus ein App-Symbol.
Benutzer mit der Rolle **Belegerfassung** sehen dort nur „Beleg“.
Über das Internet bitte nur mit HTTPS (siehe unten).

## Zwei-Faktor-Anmeldung

Jeder Benutzer kann unter **Einstellungen → Sicherheit** einen Code aus einer
Authenticator-App (Microsoft Authenticator, Google Authenticator, FreeOTP …) zusätzlich
zum Passwort verlangen lassen: QR-Code scannen, angezeigten Code eingeben, fertig. Hat
jemand sein Handy verloren, schaltet ein Administrator es unter Einstellungen → Benutzer &
Passwort („2-Faktor aus“) wieder ab. Nach acht falschen Passwörtern ist der Benutzername
fünf Minuten gesperrt.

## Microsoft-365-Postfächer ohne Passwort

Microsoft erlaubt für IMAP und SMTP keine einfachen Passwörter mehr. OpenKontor meldet sich
deshalb als App an (Einstellungen → E-Mail bzw. Belegeingang → „Microsoft 365 ohne
Passwort“):

1. In Microsoft Entra eine App registrieren, unter „Zertifikate & Geheimnisse“ einen
   geheimen Schlüssel anlegen.
2. Unter „API-Berechtigungen“ → „Office 365 Exchange Online“ → Anwendungsberechtigungen
   `IMAP.AccessAsApp` (Belegeingang) bzw. `SMTP.SendAsApp` (Versand) hinzufügen und die
   Administratorzustimmung erteilen.
3. In der Exchange-PowerShell die App mit `New-ServicePrincipal` bekannt machen und ihr
   mit `Add-MailboxPermission` Zugriff auf das Postfach geben.
4. In OpenKontor Verzeichnis-ID, Anwendungs-ID und Schlüssel eintragen; Benutzername ist
   die Adresse des Postfachs, Server `outlook.office365.com` (IMAP) bzw.
   `smtp.office365.com` (SMTP).

## Zugriff über das Internet

**HTTPS direkt im Server:** Zertifikat und Schlüssel als PEM-Dateien angeben, z. B. von
Let's Encrypt:

    openkontor-server --db … --port 443 \
      --tls-cert /etc/letsencrypt/live/kontor.meine-firma.de/fullchain.pem \
      --tls-key  /etc/letsencrypt/live/kontor.meine-firma.de/privkey.pem

(oder `OPENKONTOR_TLS_CERT` / `OPENKONTOR_TLS_KEY` in `/etc/openkontor/server.env`). Die
Arbeitsplätze verbinden sich dann mit `https://kontor.meine-firma.de`.

**Oder mit Proxy:** den Server nur lokal lauschen lassen (`--bind 127.0.0.1`) und einen
Proxy mit HTTPS davorsetzen, z. B. mit Caddy:

    openkontor.meine-firma.de {
        reverse_proxy 127.0.0.1:8470
    }

Die Arbeitsplätze verbinden sich dann mit `https://openkontor.meine-firma.de`.
