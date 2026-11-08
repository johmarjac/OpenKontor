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
- Benutzer mit Rollen (Administrator, Buchhaltung, Vertrieb, Belegerfassung, Nur lesen),
  Zwei-Faktor-Anmeldung (Authenticator-App), mehrere Firmen (Mandanten) auf einem Server,
  Datensicherung und -wiederherstellung
- E-Mail-Versand von Angeboten, Rechnungen, Gutschriften und Mahnungen (Zahlungserinnerung bis letzte Mahnung, mit Gebühren) samt PDF, eigene E-Mail-Texte auf Deutsch und Englisch, Versandverlauf am Beleg
- **E-Rechnungen ausstellen:** ZUGFeRD/Factur-X (PDF/A-3 mit eingebetteten Daten, Profil EN 16931)
  oder XRechnung 3.0 (XML, z. B. für Behörden mit Leitweg-ID) – pro Kunde einstellbar, beim
  E-Mail-Versand automatisch angehängt; fehlende Pflichtangaben werden am Beleg angezeigt.
  Geprüft mit dem offiziellen KoSIT-Validator (XRechnung) und Mustang/veraPDF (ZUGFeRD, PDF/A-3b).
- Belegeingang für Eingangsrechnungen: E-Rechnungen (XRechnung, ZUGFeRD) werden exakt
  gelesen, PDFs und Fotos per Texterkennung; Vorschläge für Lieferant, Beträge, Steuer und
  Kategorie; Originale unverändert archiviert; automatischer Abruf aus einem Sammelpostfach
- Optional: Anmeldung mit Microsoft 365 (Single Sign-on über Microsoft Entra ID)
- **Auftragsabwicklung:** Angebot → Auftragsbestätigung → Lieferschein (ohne Preise, mit
  Empfangsbestätigung) → Rechnung; Abschlags- und Schlussrechnungen (Abschläge werden
  abgezogen); Teilzahlungen; wiederkehrende Rechnungen (monatlich bis jährlich, der Server
  erstellt sie selbst)
- **Lager:** Bestand je Artikel, Mindestbestand mit Nachbestell-Hinweis, Bestellungen an
  Lieferanten („Ware erhalten“ bucht ein), Lieferscheine/Rechnungen buchen aus, Inventur
- **Bank:** Kontoauszüge als CAMT (XML) oder CSV einlesen; Zahlungen werden offenen
  Rechnungen (auch mit Skonto oder als Teilzahlung) und Eingangsbelegen automatisch
  zugeordnet, der Rest mit wenigen Klicks gebucht
- **Kassenbuch** mit laufendem Bestand, Prüfung gegen negativen Bestand und Tagesabschluss
  (danach gesperrt, GoBD)
- **Anlagenverzeichnis** mit Abschreibung (linear nach Monaten, GWG, Sammelposten), fließt in
  die EÜR ein
- **EU-Geschäfte:** innergemeinschaftliche Lieferung, Reverse Charge, Ausfuhr, Leistungen
  außerhalb der EU – mit Hinweisen auf der Rechnung, passender E-Rechnung,
  UStVA-Kennzahlen 41/21/43/45, § 13b für eingekaufte Leistungen (46/47/67) und
  **Zusammenfassende Meldung**
- **Anlage EÜR** (gruppiert wie das Formular, als PDF), **doppelte Buchführung** mit
  SKR 03 oder SKR 04, Umbuchungen, **GuV und Bilanz** (als PDF)
- **Exporte:** DATEV-Buchungsstapel mit Originalbelegen, GoBD-Datenexport für die
  Betriebsprüfung (index.xml + CSV), Vorlage der Verfahrensdokumentation (PDF)
- Eigenes **Logo** auf allen PDFs
- **Handy-Version:** Der Server liefert unter seiner Adresse eine Web-App für Smartphone und
  Tablet (Beleg fotografieren, offene Rechnungen, Ausgabe erfassen, Kontakte) – auf den
  Startbildschirm legbar, ohne App-Store

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

Weitere Optionen: `openkontor-server --help` (`--port`, `--bind`, `--data`, `--name`,
`--tls-cert`, `--tls-key`).

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
| `settings`       | Firmendaten, Nummernkreise, interne Zähler, Kassenabschlüsse |
| `receipts`, `receipt_files` | Belegeingang und Originaldateien                  |
| `records`        | Bankumsätze, Anlagen, wiederkehrende Rechnungen, Lagerbewegungen, Umbuchungen (JSON) |

Neuere Felder einer Zeile stehen in der Spalte `extra` (JSON), damit Updates keine
Tabellenänderungen brauchen. Weitere Firmen (Mandanten) bekommen je ein eigenes Schema
(`ok_<name>`) in derselben Datenbank.

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

Die Arbeitsplätze sprechen HTTP mit dem Server – oder HTTPS, wenn der Server mit
`--tls-cert` und `--tls-key` gestartet wird (z. B. mit einem Zertifikat von Let's Encrypt).
Alternativ einen Proxy mit HTTPS (Caddy, nginx) davorstellen und `--bind 127.0.0.1` nutzen.

Weitere Schutzmaßnahmen: Zwei-Faktor-Anmeldung je Benutzer (Einstellungen → Sicherheit),
Sperre nach acht falschen Passwörtern für fünf Minuten, Rollen mit abgestuften Rechten,
Microsoft-365-Postfächer ohne gespeichertes Passwort (Anmeldung als App).

## Was noch fehlt (Stand: November 2026)

Die frühere Liste ist fast vollständig umgesetzt. Offen bleiben Punkte, die eine amtliche
Zulassung, einen Vertrag mit einem Anbieter oder Fachwissen brauchen, das man nicht
selbst prüfen kann:

- **Lohn und Gehalt:** Lohnabrechnungen und die Meldungen an Krankenkassen und Finanzamt
  dürfen nur von zertifizierter Software verschickt werden (ITSG-Prüfung, ELStAM-Zugang).
  Empfehlung: Lohnbüro oder Steuerberatung; die Lohnkosten als Ausgabe „4120 Löhne und
  Gehälter“ buchen.
- **Direkte Übermittlung an ELSTER** (Umsatzsteuer-Voranmeldung, Anlage EÜR, ZM an das BZSt):
  braucht die amtliche Bibliothek ERiC und eine Registrierung als Hersteller. Bis dahin
  zeigt OpenKontor alle Kennzahlen; man trägt sie in „Mein ELSTER“ bzw. im BZStOnline-Portal ein.
- **Kontoabruf direkt bei der Bank (FinTS/HBCI):** braucht eine Produkt-Registrierung bei
  der Deutschen Kreditwirtschaft. Bis dahin: Umsätze im Online-Banking als CAMT oder CSV
  herunterladen und einlesen.
- **Versand über das Peppol-Netz:** braucht einen Vertrag mit einem Peppol-Zugangsanbieter.
  E-Rechnungen gehen bis dahin per E-Mail oder als Datei (Upload in Behördenportale).
- **Fremdwährungen:** Rechnungen sind in Euro.
- **Eigene Store-App** für Android/iOS: Die Handy-Version im Browser deckt den Alltag ab;
  eine Store-App bräuchte Entwicklerkonten bei Apple und Google.
- **Automatische Installation von Updates:** Das Programm meldet neue Versionen und öffnet
  die Download-Seite; installiert wird mit dem Installer bzw. Paket.
- **Signierte Programme:** Windows und macOS warnen beim ersten Start, weil die Programme
  nicht mit einem kostenpflichtigen Zertifikat signiert sind (Windows ca. 200–400 € im Jahr,
  Apple-Entwicklerkonto 99 $ im Jahr).

Machbar, aber noch nicht eingebaut (nach Nutzen für den Umstieg und den Alltag):

1. **Datenübernahme:** Kontakte, Artikel und offene Posten aus Lexware oder anderen
   Programmen per CSV einlesen – wichtig für den Umstieg.
2. **SEPA:** Lieferantenrechnungen als Sammelüberweisung (pain.001) ans Online-Banking
   geben, Kunden per Lastschrift einziehen (pain.008, mit Mandatsverwaltung).
3. **Mahnlauf:** alle überfälligen Rechnungen auf einmal mahnen (heute einzeln je Rechnung).
4. **Liquiditätsvorschau:** Kontostand der nächsten Wochen aus offenen Rechnungen,
   Eingangsbelegen und wiederkehrenden Zahlungen.
5. **Projekte und Zeiterfassung:** Stunden erfassen und in Rechnungen übernehmen.
6. **Revisionssichere Ablage:** Belege im S3-Speicher zusätzlich gegen Löschen sperren
   (Object Lock).

**Bitte vor dem Echtbetrieb mit der Steuerberatung abstimmen:** Kontenzuordnung SKR 03/04
im DATEV-Export, Steuerschlüssel, Bilanzgliederung und die Vorlage der
Verfahrensdokumentation. Die Rechenregeln sind getestet, ersetzen aber keine Fachberatung.

**Noch nicht unter echten Bedingungen erprobt:** Texterkennung von Scans und Fotos
(Tesseract), echte Mail-Anbieter beim Versand und beim Sammelpostfach (auch die
Microsoft-365-Anmeldung als App), Anmeldung bei einem echten Microsoft-365-Mandanten,
DATEV-Import beim Steuerberater, GoBD-Export in einer Prüfsoftware (IDEA), Kontoauszüge
verschiedener Banken, die Belegablage in S3, FTP/FTPS und SFTP, die Installationspakete
(Windows-Installer, DMG, .deb), das Linux-Installationsskript, das Docker-Image und Windows.

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
