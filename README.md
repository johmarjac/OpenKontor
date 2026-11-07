# OpenKontor

Freie Software für Rechnungen und Buchhaltung kleiner Firmen – als eigener Server
mit beliebig vielen Arbeitsplätzen, oder als Einzelplatz.

![Übersicht](docs/screenshot-uebersicht.png)

| Rechnung mit E-Rechnung und Skonto | Belegeingang mit Texterkennung |
|---|---|
| ![Rechnung](docs/screenshot-rechnung.png) | ![Belegeingang](docs/screenshot-belegeingang.png) |

> OpenKontor ist ein unabhängiges Open-Source-Projekt. Es steht in keiner Verbindung zu
> Haufe-Lexware; „Lexware“ ist eine Marke ihres Inhabers und wird hier nur zum Vergleich genannt.
> Die Software ersetzt keine Steuerberatung – bitte Auswertungen vor der Abgabe prüfen.

**Warum „OpenKontor“?** Ein *Kontor* ist das alte Wort für die Schreibstube eines Kaufmanns –
der Raum, in dem Rechnungen geschrieben, Bücher geführt und Zahlungen verbucht wurden. Bekannt
sind die Kontore der Hanse in Bergen, London oder Brügge; im Norden sagt man bis heute „Kontor“
für Büro. Das Wort kommt über das niederländische *kantoor* vom französischen *comptoir*
(Zahltisch), dahinter steckt das lateinische *computare*, „rechnen“. *Open* steht für freie,
quelloffene Software: ein offenes Kontor für jede kleine Firma.

**Herunterladen:** fertige Programme für Windows (x64 und ARM), macOS (Apple Silicon und
Intel), Linux (x64 und ARM) sowie ein Docker-Image für den Server gibt es unter
[Releases](https://github.com/johmarjac/OpenKontor/releases).

**Selbst bauen:** [Rust](https://rustup.rs) installieren, dann im Projektordner

    cargo run --release                  # Arbeitsplatz öffnen (Demo, Einzelplatz oder Server)
    cargo run --release -- --server      # Server im selben Programm, ohne Fenster

Den Server für den Dauerbetrieb einrichten: [SERVER-INSTALLATION.md](SERVER-INSTALLATION.md).

## Funktionen

- Kontakte (Kunden, Lieferanten) und Artikel/Leistungen
- Angebote, Rechnungen, Gutschriften mit 19 %, 7 % und 0 % Umsatzsteuer, Rabatten,
  Kleinunternehmerregelung (§ 19 UStG) und fortlaufenden Nummernkreisen
- Fertige Belege bleiben unverändert (Änderung nur über Gutschrift oder Kopie)
- Angebot → Rechnung mit einem Klick; „Als bezahlt buchen“ legt die Einnahme automatisch an
- Einnahmen und Ausgaben mit Konten im Stil SKR03, CSV-Export für den Steuerberater
- Auswertungen: Einnahmen-Überschuss-Rechnung, Umsatzsteuer-Voranmeldung (Kennzahlen),
  offene Posten mit Mahnstufe, Umsatz je Kunde
- PDF-Export direkt aus dem Programm (A4, Anschriftfeld für Fensterumschläge nach DIN 5008,
  mehrseitig mit Seitenzahlen) – ohne Browser oder Zusatzprogramme
- Protokoll: jede Änderung mit Zeit und Benutzer
- Benutzer mit Rollen (Administrator, Buchhaltung, Nur lesen), Datensicherung und -wiederherstellung
- E-Mail-Versand von Angeboten, Rechnungen, Gutschriften und Mahnungen (Zahlungserinnerung bis letzte Mahnung, mit Gebühren) samt PDF, eigene E-Mail-Texte auf Deutsch und Englisch, Versandverlauf am Beleg
- **E-Rechnungen ausstellen:** ZUGFeRD/Factur-X (PDF/A-3 mit eingebetteten Daten, Profil EN 16931)
  oder XRechnung 3.0 (XML, z. B. für Behörden mit Leitweg-ID) – pro Kunde einstellbar, beim
  E-Mail-Versand automatisch angehängt; fehlende Pflichtangaben werden am Beleg angezeigt.
  Geprüft mit dem offiziellen KoSIT-Validator (XRechnung) und Mustang/veraPDF (ZUGFeRD, PDF/A-3b).
- Belegeingang für Eingangsrechnungen: E-Rechnungen (XRechnung, ZUGFeRD) werden exakt
  gelesen, PDFs und Fotos per Texterkennung; Vorschläge für Lieferant, Beträge, Steuer und
  Kategorie; Originale unverändert archiviert; automatischer Abruf aus einem Sammelpostfach
- Optional: Anmeldung mit Microsoft 365 (Single Sign-on über Microsoft Entra ID)

## Zwei Programme

| Programm            | Wofür                                                                 |
|---------------------|-----------------------------------------------------------------------|
| `openkontor`        | Arbeitsplatz mit Fenster (Windows, macOS, Linux-Desktop)              |
| `openkontor-server` | Reines Konsolenprogramm ohne Fenster – für Linux-Server, systemd, Docker |

Der Server braucht keine Grafikbibliotheken. Auf dem Linux-Server nur ihn bauen:

    cargo build --release --no-default-features --bin openkontor-server

Die Arbeitsplätze baut man mit `cargo build --release`.

## Datenhaltung mit PostgreSQL

Der Server speichert alles in PostgreSQL. Jede Änderung wird in einer Transaktion
geschrieben, erst danach sehen die Arbeitsplätze sie. Ist die Datenbank kurz nicht
erreichbar, wird die Änderung mit einer Fehlermeldung abgelehnt (nichts geht halb verloren);
danach verbindet sich der Server von selbst neu.

Datenbank anlegen (einmalig):

    sudo -u postgres psql -c "CREATE USER openkontor WITH PASSWORD 'geheim';"
    sudo -u postgres psql -c "CREATE DATABASE openkontor OWNER openkontor;"

Server starten – die Tabellen werden beim ersten Start automatisch angelegt:

    openkontor-server --db postgres://openkontor:geheim@localhost/openkontor

oder über Umgebungsvariablen (praktisch für systemd und Docker):

    OPENKONTOR_DATABASE_URL=postgres://openkontor:geheim@localhost/openkontor
    OPENKONTOR_PORT=8470

Weitere Optionen: `openkontor-server --help` (`--port`, `--bind`, `--data`, `--name`).

Tabellen:

| Tabelle          | Inhalt                                                       |
|------------------|--------------------------------------------------------------|
| `contacts`       | Kunden und Lieferanten                                       |
| `articles`       | Artikel und Leistungen (Preise in Cent)                      |
| `documents`      | Angebote, Rechnungen, Gutschriften (Kopfdaten, Status)       |
| `document_lines` | Positionen der Belege                                        |
| `bookings`       | Einnahmen und Ausgaben (Beträge in Cent)                     |
| `change_log`     | Protokoll aller Änderungen – wird nie gelöscht               |
| `users`          | Benutzer, Rollen, Passwort-Hashes (gesalzen)                 |
| `settings`       | Firmendaten, Nummernkreise, interne Zähler                   |

Datensicherung: regelmäßig `pg_dump openkontor > sicherung.sql`. Zusätzlich schreibt der
Server einmal am Tag eine lesbare Kopie aller Daten als JSON in den Ordner `Sicherungen`
(Ort mit `--data` wählbar).

Die Verbindung zur Datenbank läuft unverschlüsselt; die Datenbank sollte daher auf demselben
Rechner oder im internen Netz liegen.

Ohne `--db` nutzt der Server eine JSON-Datei (`server.json`) – gedacht zum Ausprobieren
und für den Server, den ein Arbeitsplatz mit „Server starten“ im eigenen Fenster startet.

## Server installieren

Schritt für Schritt in **[SERVER-INSTALLATION.md](SERVER-INSTALLATION.md)**. Kurzfassung für
Debian/Ubuntu – ein Befehl im Projektordner:

    sudo ./deploy/install-server.sh

Alternativ mit Docker: `cd deploy && docker compose up -d`.

## Sprachen

Die Oberfläche gibt es auf Deutsch und Englisch (Anmeldebildschirm oben rechts oder
Einstellungen → Sprache). Angebote, Rechnungen und Gutschriften entstehen in der Sprache
des Kontakts („Sprache der Belege“), mit passenden Zahlen- und Datumsformaten.
Weitere Sprachen: in `src/i18n.rs` eine Spalte ergänzen; ein Test meldet fehlende Texte.

## Mitgelieferte Dateien

- Schrift **Liberation Sans** (SIL Open Font License, `assets/fonts/LICENSE-LiberationSans.txt`) –
  wird in ZUGFeRD-PDFs eingebettet, wie es das Archivformat PDF/A verlangt.
- Farbprofil **sRGB** (CC0, `assets/icc/LICENSE-sRGB.txt`) für PDF/A.

## Sicherheit

Ein neuer Datenbestand hat den Benutzer `admin` mit Passwort `admin` – bitte sofort unter
Einstellungen → Benutzer & Passwort ändern.

Die Arbeitsplätze sprechen HTTP mit dem Server. Für den Zugriff über das Internet den Server
hinter einen Proxy mit HTTPS (z. B. Caddy oder nginx) stellen und `--bind 127.0.0.1` nutzen.

## Was noch fehlt (Stand: November 2026)

Verglichen mit einer kaufmännischen Komplettlösung wie Lexware Premium/Pro, nach Wichtigkeit:

**1. Gesetzlich bald nötig**
- ~~E-Rechnungen ausstellen~~ – erledigt (ZUGFeRD und XRechnung). Noch offen: Versand über
  das Peppol-Netz bzw. Upload in Behördenportale (bisher per E-Mail oder als Datei).
- **GoBD-Datenexport für die Betriebsprüfung** („Z3“, Format nach GDPdU) und eine
  Verfahrensdokumentation als Vorlage.

**2. Großer Nutzen im Alltag**
- **Bankanbindung:** Kontoauszüge abrufen (FinTS/HBCI) oder importieren (CAMT, CSV) und
  Zahlungen automatisch offenen Rechnungen und Belegen zuordnen. Bisher wird jede Zahlung von
  Hand gebucht.
- **DATEV-Export** für die Steuerberatung (Buchungsstapel mit Belegbildern) statt nur CSV.
- **Wiederkehrende Rechnungen** (z. B. Wartungsverträge), Abschlags- und Teilrechnungen,
  Teilzahlungen.
- **Kassenbuch** für Bargeld (GoBD-konform, mit Tagesabschluss).
- **Eigenes Logo und Briefpapier** auf den PDFs.

**3. Steuern und Buchhaltung vertiefen**
- **Anlagenverzeichnis mit Abschreibungen (AfA)** – gehört auch zur Einnahmen-Überschuss-Rechnung.
- **Anlage EÜR** und **Umsatzsteuer-Voranmeldung direkt an ELSTER** übermitteln (bisher
  werden die Kennzahlen angezeigt und von Hand übertragen).
- **EU-Geschäfte:** Reverse-Charge, innergemeinschaftliche Lieferungen, Zusammenfassende Meldung,
  Fremdwährungen.
- **Doppelte Buchführung** mit vollständigem Kontenrahmen (SKR 03/04), Bilanz und GuV – nötig
  für bilanzierende Firmen (z. B. GmbH). Bisher: Einnahmen-Überschuss-Rechnung.

**4. Weitere Bausteine von Lexware**
- **Lohn und Gehalt** (Lohnabrechnung, Meldungen an Krankenkassen und Finanzamt).
- **Warenwirtschaft:** Lagerbestand, Bestellungen, Auftragsbestätigungen, Lieferscheine.
- **Mehrere Firmen (Mandanten)** auf einem Server.

**5. Technik und Sicherheit**
- HTTPS direkt im Server, Zwei-Faktor-Anmeldung, feinere Rechte (z. B. nur Rechnungen).
- Abruf des Microsoft-365-Postfachs mit Microsofts eigener Anmeldung (bisher Passwort).
- Programmpakete (Installer für Windows/macOS, Linux-Paket) und automatische Updates.

**Noch nicht unter echten Bedingungen erprobt:** Texterkennung von Scans und Fotos
(Tesseract), echte Mail-Anbieter beim Versand und beim Sammelpostfach, Anmeldung bei einem
echten Microsoft-365-Mandanten, das Linux-Installationsskript, das Docker-Image und Windows.

## Lizenz

OpenKontor steht unter der **GNU Affero General Public License 3.0** (oder neuer), siehe
[LICENSE](LICENSE). Sie dürfen die Software frei nutzen, verändern und weitergeben. Wer eine
veränderte Fassung anderen anbietet – auch nur als Server über das Netzwerk –, muss den
geänderten Quelltext ebenfalls unter dieser Lizenz zur Verfügung stellen.

Mitgelieferte Fremdbestandteile: Schrift Liberation Sans (SIL Open Font License 1.1),
sRGB-Farbprofil (CC0). Die verwendeten Rust-Bibliotheken stehen unter MIT- bzw.
Apache-2.0-ähnlichen Lizenzen.

## Neue Version veröffentlichen

Auf GitHub unter **Actions → „Release erstellen“ → „Run workflow“** die Versionsnummer
eingeben (z. B. `0.2.0`). Alternativ ein Tag setzen: `git tag v0.2.0 && git push origin v0.2.0`.

GitHub baut dann alle Pakete (Windows x64/ARM, macOS Universal, Linux x64/ARM), legt die
Release-Seite mit Prüfsummen an und veröffentlicht das Docker-Image
`ghcr.io/johmarjac/openkontor-server`.
