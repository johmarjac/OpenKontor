//! Exports for the tax advisor and the tax office: DATEV booking batch (with the receipt
//! files), the GoBD data export for a tax audit ("Z3", description standard of the
//! former GDPdU) and a template of the procedure documentation (Verfahrensdokumentation).

use crate::accounting::account_in;
use crate::model::*;
use chrono::{Datelike, NaiveDate};

// ---- ZIP ---------------------------------------------------------------------------------

/// A small ZIP writer (deflate), enough for export packages.
pub struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl Default for Zip {
    fn default() -> Self {
        Self::new()
    }
}

impl Zip {
    pub fn new() -> Zip {
        Zip { out: Vec::new(), central: Vec::new(), count: 0 }
    }

    pub fn add(&mut self, name: &str, data: &[u8]) {
        use std::io::Write;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).expect("in memory");
        let packed = e.finish().expect("in memory");
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() { (8, &packed) } else { (0, data) };
        let now = chrono::Local::now().naive_local();
        let dos_time = ((now.hour() as u16) << 11) | ((now.minute() as u16) << 5) | (now.second() as u16 / 2);
        let dos_date = (((now.year() - 1980).max(0) as u16) << 9) | ((now.month() as u16) << 5) | now.day() as u16;
        let offset = self.out.len() as u32;
        let name_bytes = name.as_bytes();
        // Bit 11: file names are UTF-8.
        let flags: u16 = 1 << 11;
        let mut local = Vec::new();
        local.extend_from_slice(&0x04034b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes());
        local.extend_from_slice(&flags.to_le_bytes());
        local.extend_from_slice(&method.to_le_bytes());
        local.extend_from_slice(&dos_time.to_le_bytes());
        local.extend_from_slice(&dos_date.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(body.len() as u32).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name_bytes);
        self.out.extend_from_slice(&local);
        self.out.extend_from_slice(body);

        let c = &mut self.central;
        c.extend_from_slice(&0x02014b50u32.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&flags.to_le_bytes());
        c.extend_from_slice(&method.to_le_bytes());
        c.extend_from_slice(&dos_time.to_le_bytes());
        c.extend_from_slice(&dos_date.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(body.len() as u32).to_le_bytes());
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0; 8]);
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&offset.to_le_bytes());
        c.extend_from_slice(name_bytes);
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let start = self.out.len() as u32;
        let size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&start.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

use chrono::Timelike;

/// Text in Windows-1252, as DATEV and audit software expect it.
pub fn to_cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '€' => 0x80,
            '„' => 0x84,
            '“' => 0x93,
            '”' => 0x94,
            '–' => 0x96,
            '—' => 0x97,
            c if (c as u32) < 256 => c as u32 as u8,
            _ => b'?',
        })
        .collect()
}

fn amount(c: Cents) -> String {
    let s = format!("{:.2}", c.abs() as f64 / 100.0).replace('.', ",");
    if c < 0 { format!("-{s}") } else { s }
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\"").replace(['\r', '\n'], " "))
}

/// A file name part without characters that cause trouble.
fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

// ---- DATEV ------------------------------------------------------------------------------

/// DATEV tax key for accounts without automatic tax (input tax 19 % = 9, 7 % = 8;
/// output tax 19 % = 3, 7 % = 2). Automatic accounts (8400, 8300, 3400 …) need none.
fn tax_key(b: &Booking) -> &'static str {
    let acc = b.category.split(' ').next().unwrap_or("");
    let automatic = matches!(acc, "8400" | "8300" | "3400" | "3300");
    if automatic || b.vat == 0 || is_private(&b.category) {
        return "";
    }
    if b.reverse_charge {
        // § 13b services from EU businesses, 19 %.
        return "94";
    }
    match (b.kind, b.vat) {
        (BookingKind::Ausgabe, 19) => "9",
        (BookingKind::Ausgabe, 7) => "8",
        (BookingKind::Einnahme, 19) => "3",
        (BookingKind::Einnahme, 7) => "2",
        _ => "",
    }
}

/// The DATEV booking batch (EXTF format 700, category 21) of a period as CSV in Windows-1252.
pub fn datev_csv(data: &Data, from: NaiveDate, to: NaiveDate) -> Vec<u8> {
    let c = &data.company;
    let chart = c.chart;
    let now = chrono::Local::now();
    let skr = match chart {
        Chart::Skr03 => "03",
        Chart::Skr04 => "04",
    };
    let header = [
        "\"EXTF\"".to_string(),
        "700".into(),
        "21".into(),
        "\"Buchungsstapel\"".into(),
        "13".into(),
        now.format("%Y%m%d%H%M%S000").to_string(),
        String::new(),
        "\"OK\"".into(),
        "\"OpenKontor\"".into(),
        String::new(),
        c.datev_advisor.trim().to_string(),
        c.datev_client.trim().to_string(),
        format!("{}0101", from.year()),
        "4".into(),
        from.format("%Y%m%d").to_string(),
        to.format("%Y%m%d").to_string(),
        quoted(&format!("OpenKontor {}–{}", fmt_date(from), fmt_date(to))),
        String::new(),
        "1".into(),
        "0".into(),
        "0".into(),
        "\"EUR\"".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        format!("\"{skr}\""),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];
    let columns = [
        "Umsatz (ohne Soll/Haben-Kz)",
        "Soll/Haben-Kennzeichen",
        "WKZ Umsatz",
        "Kurs",
        "Basis-Umsatz",
        "WKZ Basis-Umsatz",
        "Konto",
        "Gegenkonto (ohne BU-Schlüssel)",
        "BU-Schlüssel",
        "Belegdatum",
        "Belegfeld 1",
        "Belegfeld 2",
        "Skonto",
        "Buchungstext",
    ];
    let mut out = String::new();
    out.push_str(&header.join(";"));
    out.push_str("\r\n");
    out.push_str(&columns.join(";"));
    out.push_str("\r\n");
    for b in data.bookings.iter().filter(|b| b.date >= from && b.date <= to) {
        let money = account_in(chart, if b.payment == CASH { "1000" } else { "1200" });
        let target = if b.category == ASSET_CATEGORY {
            data.assets
                .iter()
                .find(|a| a.booking_id == Some(b.id))
                .map(|a| account_in(chart, &a.account))
                .unwrap_or_else(|| account_in(chart, "0400"))
        } else {
            account_in(chart, &b.category)
        };
        // Money account on the debit side for income, the expense account for expenses.
        let (konto, gegen) = match b.kind {
            BookingKind::Einnahme => (money, target),
            BookingKind::Ausgabe => (target, money),
        };
        let receipt: String = b.receipt.chars().filter(|c| c.is_alphanumeric() || "$&%*+-/".contains(*c)).take(36).collect();
        let text: String = if b.partner.is_empty() { b.text.clone() } else { format!("{} {}", b.text, b.partner) };
        let row = [
            amount(b.gross),
            "\"S\"".into(),
            "\"EUR\"".into(),
            String::new(),
            String::new(),
            String::new(),
            konto,
            gegen,
            quoted(tax_key(b)),
            b.date.format("%d%m").to_string(),
            quoted(&receipt),
            String::new(),
            String::new(),
            quoted(&text.chars().take(60).collect::<String>()),
        ];
        out.push_str(&row.join(";"));
        out.push_str("\r\n");
    }
    to_cp1252(&out)
}

/// The DATEV package: booking batch plus the original receipt files, named by booking.
/// `files` gives the stored file of a receipt id.
pub fn datev_package(data: &Data, from: NaiveDate, to: NaiveDate, files: &mut dyn FnMut(u64) -> Option<Vec<u8>>) -> Vec<u8> {
    let mut zip = Zip::new();
    let name = format!("EXTF_Buchungsstapel_{}_{}.csv", from.format("%Y%m%d"), to.format("%Y%m%d"));
    zip.add(&name, &datev_csv(data, from, to));
    let mut list = String::from("Belegdatum;Belegnummer;Lieferant;Betrag;Datei\r\n");
    for r in data.receipts.iter().filter(|r| r.status == ReceiptStatus::Gebucht) {
        let in_range = data.bookings.iter().any(|b| b.receipt_id == Some(r.id) && b.date >= from && b.date <= to);
        if !in_range {
            continue;
        }
        if let Some(bytes) = files(r.id) {
            let file = format!("Belege/{}_{}_{}", r.date.map(|d| d.format("%Y%m%d").to_string()).unwrap_or_default(), safe(&r.number), safe(&r.file_name));
            zip.add(&file, &bytes);
            list.push_str(&format!(
                "{};{};{};{};{}\r\n",
                r.date.map(fmt_date).unwrap_or_default(),
                quoted(&r.number),
                quoted(&r.supplier),
                amount(r.gross),
                quoted(&file)
            ));
        }
    }
    zip.add("Belege/Belegliste.csv", &to_cp1252(&list));
    zip.add(
        "LIESMICH.txt",
        &to_cp1252(&format!(
            "DATEV-Export aus OpenKontor ({} bis {})\r\n\r\n\
             {name}: Buchungsstapel im DATEV-Format (EXTF 700). In DATEV Rechnungswesen über\r\n\
             „Bestand > Importieren > ASCII-Daten“ bzw. den DATEV-Import einlesen.\r\n\
             Belege/: die Originalbelege des Belegeingangs, Belegliste.csv ordnet sie den Buchungen zu\r\n\
             (Belegfeld 1 = Belegnummer).\r\n\r\n\
             Beraternummer {} · Mandantennummer {} · Kontenrahmen {}\r\n",
            fmt_date(from),
            fmt_date(to),
            data.company.datev_advisor,
            data.company.datev_client,
            match data.company.chart {
                Chart::Skr03 => "SKR 03",
                Chart::Skr04 => "SKR 04",
            }
        )),
    );
    zip.finish()
}

// ---- GoBD data export (Z3) -----------------------------------------------------------------

struct Table {
    file: &'static str,
    name: &'static str,
    description: &'static str,
    /// (column name, kind): 'a' text, 'n' amount with 2 decimals, 'i' whole number, 'd' date.
    columns: Vec<(&'static str, char)>,
    rows: Vec<Vec<String>>,
}

fn csv_cell(kind: char, v: &str) -> String {
    match kind {
        'a' => quoted(v),
        _ => v.to_string(),
    }
}

/// The data of a fiscal year for a tax audit: one CSV file per table plus `index.xml`
/// (description standard of the tax administration), packed as ZIP.
pub fn gobd_export(data: &Data, year: i32) -> Vec<u8> {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let to = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let small = data.company.small_business;
    let d = |v: NaiveDate| fmt_date(v);
    let id = |v: u64| v.to_string();
    let in_year = |v: NaiveDate| v >= from && v <= to;

    let mut tables = Vec::new();
    tables.push(Table {
        file: "buchungen.csv",
        name: "Buchungen",
        description: "Einnahmen und Ausgaben (Buchungsjournal)",
        columns: vec![
            ("ID", 'i'), ("Datum", 'd'), ("Art", 'a'), ("Konto", 'a'), ("Buchungstext", 'a'), ("Partner", 'a'),
            ("Belegnummer", 'a'), ("Brutto", 'n'), ("Netto", 'n'), ("Steuer", 'n'), ("Steuersatz", 'i'),
            ("Zahlungsweg", 'a'), ("Ausgangsbeleg_ID", 'i'), ("Eingangsbeleg_ID", 'i'),
        ],
        rows: data
            .bookings
            .iter()
            .filter(|b| in_year(b.date))
            .map(|b| {
                vec![
                    id(b.id), d(b.date), b.kind.name().into(), b.category.clone(), b.text.clone(), b.partner.clone(),
                    b.receipt.clone(), amount(b.gross), amount(b.net()), amount(b.tax()), b.vat.to_string(),
                    b.payment.clone(), b.doc_id.map(id).unwrap_or_default(), b.receipt_id.map(id).unwrap_or_default(),
                ]
            })
            .collect(),
    });
    let docs: Vec<&Document> = data.documents.iter().filter(|x| x.status != DocStatus::Entwurf && in_year(x.date)).collect();
    tables.push(Table {
        file: "ausgangsbelege.csv",
        name: "Ausgangsbelege",
        description: "Rechnungen, Gutschriften, Angebote, Lieferscheine",
        columns: vec![
            ("ID", 'i'), ("Art", 'a'), ("Nummer", 'a'), ("Datum", 'd'), ("Kunde_Nr", 'a'), ("Kunde", 'a'),
            ("Leistungszeitraum", 'a'), ("Netto", 'n'), ("Steuer", 'n'), ("Brutto", 'n'), ("Status", 'a'),
            ("Bezahlt_am", 'd'),
        ],
        rows: docs
            .iter()
            .map(|x| {
                let t = x.totals(small);
                vec![
                    id(x.id), x.kind.name().into(), x.number.clone(), d(x.date),
                    data.contact(x.contact_id).map(|c| c.number.clone()).unwrap_or_default(),
                    data.contact_name(x.contact_id), x.service_date.clone(), amount(t.net), amount(t.tax),
                    amount(t.gross), x.status.name().into(), x.paid_date.map(d).unwrap_or_default(),
                ]
            })
            .collect(),
    });
    tables.push(Table {
        file: "belegpositionen.csv",
        name: "Belegpositionen",
        description: "Positionen der Ausgangsbelege",
        columns: vec![
            ("Beleg_ID", 'i'), ("Pos", 'i'), ("Artikel_ID", 'i'), ("Bezeichnung", 'a'), ("Menge", 'n'),
            ("Einheit", 'a'), ("Einzelpreis", 'n'), ("Rabatt_Prozent", 'n'), ("Netto", 'n'), ("Steuersatz", 'i'),
        ],
        rows: docs
            .iter()
            .flat_map(|x| {
                x.lines.iter().enumerate().map(move |(i, l)| {
                    vec![
                        x.id.to_string(), (i + 1).to_string(), l.article_id.map(|v| v.to_string()).unwrap_or_default(),
                        l.text.clone(), format!("{:.2}", l.qty).replace('.', ","), l.unit.clone(), amount(l.price_net),
                        format!("{:.2}", l.discount_pct).replace('.', ","), amount(l.net()), l.vat.to_string(),
                    ]
                })
            })
            .collect(),
    });
    tables.push(Table {
        file: "eingangsbelege.csv",
        name: "Eingangsbelege",
        description: "Belegeingang (Lieferantenrechnungen, Quittungen) mit Prüfsumme der Originaldatei",
        columns: vec![
            ("ID", 'i'), ("Eingang", 'a'), ("Quelle", 'a'), ("Datei", 'a'), ("SHA256", 'a'), ("Status", 'a'),
            ("Lieferant", 'a'), ("Belegnummer", 'a'), ("Belegdatum", 'd'), ("Brutto", 'n'), ("Konto", 'a'),
        ],
        rows: data
            .receipts
            .iter()
            .filter(|r| r.date.is_none_or(in_year))
            .map(|r| {
                vec![
                    id(r.id), r.received.clone(), r.source.clone(), r.file_name.clone(), r.sha256.clone(),
                    r.status.name().into(), r.supplier.clone(), r.number.clone(), r.date.map(d).unwrap_or_default(),
                    amount(r.gross), r.category.clone(),
                ]
            })
            .collect(),
    });
    tables.push(Table {
        file: "kontakte.csv",
        name: "Kontakte",
        description: "Kunden und Lieferanten",
        columns: vec![
            ("ID", 'i'), ("Nummer", 'a'), ("Art", 'a'), ("Firma", 'a'), ("Person", 'a'), ("Strasse", 'a'),
            ("PLZ_Ort", 'a'), ("Land", 'a'), ("USt_IdNr", 'a'),
        ],
        rows: data
            .contacts
            .iter()
            .map(|c| {
                vec![
                    id(c.id), c.number.clone(), c.kind.name().into(), c.company.clone(), c.person.clone(),
                    c.street.clone(), c.zip_city.clone(), c.country.clone(), c.vat_id.clone(),
                ]
            })
            .collect(),
    });
    let mut cash_rows = Vec::new();
    for (b, balance) in data.cash_book() {
        if in_year(b.date) {
            cash_rows.push(vec![d(b.date), b.text.clone(), b.receipt.clone(), amount(b.signed()), amount(balance)]);
        }
    }
    tables.push(Table {
        file: "kassenbuch.csv",
        name: "Kassenbuch",
        description: "Bareinnahmen und -ausgaben mit laufendem Bestand",
        columns: vec![("Datum", 'd'), ("Text", 'a'), ("Beleg", 'a'), ("Betrag", 'n'), ("Bestand", 'n')],
        rows: cash_rows,
    });
    tables.push(Table {
        file: "bankumsaetze.csv",
        name: "Bankumsaetze",
        description: "Eingelesene Kontoauszüge",
        columns: vec![("ID", 'i'), ("Datum", 'd'), ("Betrag", 'n'), ("Name", 'a'), ("IBAN", 'a'), ("Verwendungszweck", 'a'), ("Status", 'a')],
        rows: data
            .bank
            .iter()
            .filter(|t| in_year(t.date))
            .map(|t| vec![id(t.id), d(t.date), amount(t.amount), t.name.clone(), t.iban.clone(), t.purpose.clone(), t.status.name().into()])
            .collect(),
    });
    tables.push(Table {
        file: "anlagen.csv",
        name: "Anlagenverzeichnis",
        description: "Anlagegüter mit Abschreibung des Jahres",
        columns: vec![
            ("Nummer", 'a'), ("Bezeichnung", 'a'), ("Konto", 'a'), ("Anschaffung", 'd'), ("Kosten", 'n'),
            ("Nutzungsdauer", 'i'), ("Wert_Jahresbeginn", 'n'), ("AfA", 'n'), ("Wert_Jahresende", 'n'),
        ],
        rows: data
            .assets
            .iter()
            .filter(|a| a.date <= to)
            .map(|a| {
                vec![
                    a.number.clone(), a.name.clone(), a.account.clone(), d(a.date), amount(a.cost), a.years.to_string(),
                    amount(crate::accounting::value_at_start(a, year)), amount(crate::accounting::afa_for_year(a, year)),
                    amount(crate::accounting::value_at_end(a, year)),
                ]
            })
            .collect(),
    });
    tables.push(Table {
        file: "protokoll.csv",
        name: "Aenderungsprotokoll",
        description: "Wer hat wann was geändert",
        columns: vec![("Zeitpunkt", 'a'), ("Benutzer", 'a'), ("Aktion", 'a')],
        rows: data.log.iter().map(|e| vec![e.time.clone(), e.user.clone(), e.action.clone()]).collect(),
    });

    let mut zip = Zip::new();
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"windows-1252\"?>\r\n");
    xml.push_str("<!DOCTYPE DataSet SYSTEM \"gdpdu-01-09-2004.dtd\">\r\n<DataSet>\r\n <Version>1.0</Version>\r\n");
    xml.push_str(&format!(
        " <DataSupplier>\r\n  <Name>{}</Name>\r\n  <Location>{}</Location>\r\n  <Comment>Datenüberlassung {year} aus OpenKontor</Comment>\r\n </DataSupplier>\r\n",
        xml_escape(&data.company.name),
        xml_escape(&format!("{} {}", data.company.street, data.company.zip_city))
    ));
    xml.push_str(" <Media>\r\n  <Name>OpenKontor Export</Name>\r\n");
    for t in &tables {
        let mut csv = String::new();
        csv.push_str(&t.columns.iter().map(|c| quoted(c.0)).collect::<Vec<_>>().join(";"));
        csv.push_str("\r\n");
        for row in &t.rows {
            let cells: Vec<String> = row.iter().zip(&t.columns).map(|(v, c)| csv_cell(c.1, v)).collect();
            csv.push_str(&cells.join(";"));
            csv.push_str("\r\n");
        }
        zip.add(t.file, &to_cp1252(&csv));
        xml.push_str(&format!(
            "  <Table>\r\n   <URL>{}</URL>\r\n   <Name>{}</Name>\r\n   <Description>{}</Description>\r\n   \
             <Validity><Range><From>{}</From><To>{}</To></Range><Format>DD.MM.YYYY</Format></Validity>\r\n   \
             <DecimalSymbol>,</DecimalSymbol>\r\n   <DigitGroupingSymbol>.</DigitGroupingSymbol>\r\n   <SkipNumBytes>0</SkipNumBytes>\r\n   \
             <Range><From>2</From></Range>\r\n   <VariableLength>\r\n    <ColumnDelimiter>;</ColumnDelimiter>\r\n    \
             <RecordDelimiter>&#13;&#10;</RecordDelimiter>\r\n    <TextEncapsulator>\"</TextEncapsulator>\r\n",
            t.file,
            t.name,
            xml_escape(t.description),
            fmt_date(from),
            fmt_date(to)
        ));
        for (k, (name, kind)) in t.columns.iter().enumerate() {
            let tag = if k == 0 && *kind == 'i' { "VariablePrimaryKey" } else { "VariableColumn" };
            let ty = match kind {
                'n' => "<Numeric><Accuracy>2</Accuracy></Numeric>",
                'i' => "<Numeric/>",
                'd' => "<Date><Format>DD.MM.YYYY</Format></Date>",
                _ => "<AlphaNumeric/>",
            };
            xml.push_str(&format!("    <{tag}><Name>{name}</Name>{ty}</{tag}>\r\n"));
        }
        xml.push_str("   </VariableLength>\r\n  </Table>\r\n");
    }
    xml.push_str(" </Media>\r\n</DataSet>\r\n");
    zip.add("index.xml", &to_cp1252(&xml));
    zip.finish()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ---- Procedure documentation --------------------------------------------------------------

/// A template of the procedure documentation required by the GoBD, filled with the
/// company's details and how OpenKontor works. To be checked and completed by the company.
pub fn procedure_documentation(data: &Data, server: bool) -> Vec<u8> {
    use crate::pdf::Block::{Heading, Paragraph, Rows};
    let c = &data.company;
    let mode = if server {
        "OpenKontor läuft als Server im Unternehmen; die Arbeitsplätze greifen über das Netzwerk mit Benutzername und Passwort (bzw. Microsoft 365) zu. Jeder Benutzer hat eine Rolle (Administrator, Buchhaltung, Vertrieb, Belegerfassung, Nur lesen)."
    } else {
        "OpenKontor läuft als Einzelplatz; die Daten liegen auf diesem Rechner."
    };
    let chart = match c.chart {
        Chart::Skr03 => "SKR 03",
        Chart::Skr04 => "SKR 04",
    };
    let method = match c.accounting {
        Accounting::Euer => "Einnahmen-Überschuss-Rechnung (§ 4 Abs. 3 EStG), Ist-Versteuerung",
        Accounting::Bilanz => "Doppelte Buchführung mit Bilanz (§ 4 Abs. 1, § 5 EStG)",
    };
    let blocks = vec![
        Paragraph("Diese Vorlage beschreibt, wie im Unternehmen Belege entstehen, erfasst, verarbeitet und aufbewahrt werden. Sie ist vom Unternehmen zu prüfen, zu ergänzen (Abschnitte mit „…“) und bei Änderungen fortzuschreiben. Frühere Fassungen sind aufzubewahren.".into()),
        Heading("1. Allgemeine Beschreibung".into()),
        Rows(vec![
            ("Unternehmen".into(), c.name.clone(), false),
            ("Inhaber / Geschäftsführung".into(), c.owner.clone(), false),
            ("Anschrift".into(), format!("{}, {}", c.street, c.zip_city), false),
            ("Steuernummer / USt-IdNr.".into(), format!("{} / {}", c.tax_number, c.vat_id), false),
            ("Gewinnermittlung".into(), method.into(), false),
            ("Kontenrahmen".into(), chart.into(), false),
            ("Kleinunternehmer (§ 19 UStG)".into(), if c.small_business { "ja".into() } else { "nein".into() }, false),
        ]),
        Paragraph("Tätigkeit des Unternehmens: …\nVerantwortlich für die Buchführung: …\nSteuerberatung: …".into()),
        Heading("2. Anwenderdokumentation".into()),
        Paragraph(format!("Eingesetzte Software: OpenKontor (freie Software, Lizenz AGPL-3.0), Version {}. {mode}", env!("CARGO_PKG_VERSION"))),
        Paragraph("Ausgangsrechnungen: Angebote, Auftragsbestätigungen, Lieferscheine, Rechnungen und Gutschriften werden in OpenKontor erstellt. Entwürfe erhalten beim Fertigstellen eine fortlaufende, lückenlose Nummer und können danach nicht mehr verändert werden; Korrekturen erfolgen ausschließlich über Gutschriften bzw. Stornorechnungen. Rechnungen werden als PDF, ZUGFeRD oder XRechnung erzeugt und per E-Mail oder Post versandt; der Versand wird am Beleg protokolliert.".into()),
        Paragraph("Eingangsrechnungen: Belege kommen über das Sammelpostfach (E-Mail), per Upload oder per Scan in den Belegeingang. Die Originaldatei wird unverändert gespeichert, ihr SHA-256-Fingerabdruck festgehalten. Die erkannten Angaben (Lieferant, Datum, Beträge) werden geprüft und als Ausgabe gebucht. Papierbelege werden … (ersetzendes Scannen: ja/nein, Verfahren …).".into()),
        Paragraph("Zahlungsverkehr: Kontoauszüge werden als CAMT- oder CSV-Datei eingelesen und den offenen Rechnungen bzw. Belegen zugeordnet. Barzahlungen werden im Kassenbuch erfasst; die Kasse wird täglich bzw. … abgeschlossen. Nach dem Abschluss sind Kassenbuchungen bis zu diesem Tag nicht mehr änderbar.".into()),
        Heading("3. Technische Systemdokumentation".into()),
        Paragraph(if server {
            "Datenhaltung auf dem Server (PostgreSQL-Datenbank oder JSON-Datei). Der Server legt täglich eine Sicherung als JSON-Datei an. Speicherort, Datensicherung außer Haus und Wiederherstellung: …".into()
        } else {
            "Datenhaltung in einer Datei im Benutzerordner. Datensicherung über Einstellungen → Daten → Sicherung speichern; Speicherort und Rhythmus: …".into()
        }),
        Paragraph("Jede Änderung wird mit Zeitpunkt und Benutzer im Änderungsprotokoll festgehalten. Fertige Belege, gebuchte Eingangsbelege und abgeschlossene Kassentage sind gegen Veränderung gesperrt.".into()),
        Heading("4. Betriebsdokumentation".into()),
        Paragraph("Zugriffsrechte und Benutzer: …\nVertretungsregelung: …\nUpdates der Software: werden von … eingespielt; die eingesetzte Version wird dokumentiert.".into()),
        Heading("5. Internes Kontrollsystem".into()),
        Paragraph("Kontrollen: Abgleich der Bankkonten mit den Buchungen (Bank → offene Umsätze), monatliche Prüfung der offenen Posten und der Umsatzsteuer-Voranmeldung, Vier-Augen-Prinzip bei …".into()),
        Heading("6. Aufbewahrung und Datenzugriff".into()),
        Paragraph("Bücher, Rechnungen und Buchungsbelege werden 8 bzw. 10 Jahre aufbewahrt (§ 147 AO, § 14b UStG). Für eine Betriebsprüfung stellt OpenKontor den Datenexport nach dem Beschreibungsstandard der Finanzverwaltung bereit (Auswertungen → Exporte → GoBD-Export).".into()),
    ];
    crate::pdf::report_pdf(data, "Verfahrensdokumentation (Vorlage)", &format!("Stand {}", fmt_date(today())), &blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_is_readable() {
        let mut z = Zip::new();
        z.add("a.txt", b"Hallo Hallo Hallo Hallo Hallo");
        z.add("ordner/b.txt", b"x");
        let bytes = z.finish();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        if let Ok(dir) = std::env::var("OPENKONTOR_EXPORT_OUT") {
            std::fs::write(format!("{dir}/test.zip"), &bytes).unwrap();
        }
    }

    #[test]
    fn datev_and_gobd() {
        let mut data = sample_data();
        data.company.datev_advisor = "1001".into();
        data.company.datev_client = "20002".into();
        let year = today().year();
        let (from, to) = (NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), NaiveDate::from_ymd_opt(year, 12, 31).unwrap());
        let csv = String::from_utf8_lossy(&datev_csv(&data, from, to)).into_owned();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with("\"EXTF\";700;21;\"Buchungsstapel\";13;"));
        assert_eq!(lines.len(), 2 + data.bookings.iter().filter(|b| b.date.year() == year).count());
        let gobd = gobd_export(&data, year);
        assert_eq!(&gobd[..2], b"PK");
        let doc = procedure_documentation(&data, true);
        assert!(doc.starts_with(b"%PDF"));
        if let Ok(dir) = std::env::var("OPENKONTOR_EXPORT_OUT") {
            std::fs::write(format!("{dir}/datev.csv"), csv).unwrap();
            std::fs::write(format!("{dir}/gobd.zip"), gobd).unwrap();
            std::fs::write(format!("{dir}/verfahrensdoku.pdf"), doc).unwrap();
        }
    }
}
