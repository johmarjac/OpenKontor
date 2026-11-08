//! Taking over data from another program (e.g. Lexware): contacts, articles and open
//! invoices from CSV files. Columns are recognised by their headings; files in UTF-8 or
//! Windows-1252, separated by semicolon, comma or tab.

use crate::model::*;

/// What a file is read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Contacts,
    Articles,
    OpenItems,
}

/// The result of reading a file: what was understood, and what not.
#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub contacts: Vec<Contact>,
    pub articles: Vec<Article>,
    pub items: Vec<Document>,
    /// Which column was used for which field ("Firma ← Firmenname").
    pub mapping: Vec<(String, String)>,
    /// Lines that could not be used, with the reason.
    pub problems: Vec<String>,
}

impl Parsed {
    pub fn count(&self) -> usize {
        self.contacts.len() + self.articles.len() + self.items.len()
    }
}

fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80 => '€',
                0x84 => '„',
                0x93 => '“',
                0x96 => '–',
                _ => b as char,
            })
            .collect(),
    }
}

fn split(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Rows of a CSV file as maps from normalised heading to value.
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn read(bytes: &[u8]) -> Result<Table, String> {
        let text = decode(bytes);
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let first = lines.first().ok_or("Die Datei ist leer.")?;
        let sep = [';', '\t', ','].into_iter().max_by_key(|s| first.matches(*s).count()).unwrap_or(';');
        let headers = split(first, sep);
        if headers.len() < 2 {
            return Err("In der ersten Zeile werden Spaltenüberschriften erwartet (getrennt durch Semikolon, Komma oder Tabulator).".into());
        }
        let rows = lines[1..].iter().map(|l| split(l, sep)).collect();
        Ok(Table { headers, rows })
    }

    /// The column whose heading matches one of the names (normalised), best first.
    fn col(&self, names: &[&str], field: &str, mapping: &mut Vec<(String, String)>) -> Option<usize> {
        let normed: Vec<String> = self.headers.iter().map(|h| norm(h)).collect();
        for n in names {
            let n = norm(n);
            if let Some(i) = normed.iter().position(|h| *h == n) {
                mapping.push((field.to_string(), self.headers[i].clone()));
                return Some(i);
            }
        }
        for n in names {
            let n = norm(n);
            if let Some(i) = normed.iter().position(|h| h.contains(&n)) {
                mapping.push((field.to_string(), self.headers[i].clone()));
                return Some(i);
            }
        }
        None
    }
}

fn get(row: &[String], col: Option<usize>) -> String {
    col.and_then(|i| row.get(i)).cloned().unwrap_or_default()
}

fn money(s: &str) -> Option<Cents> {
    let t: String = s.chars().filter(|c| !c.is_whitespace() && *c != '€').collect();
    if t.is_empty() {
        return None;
    }
    let normalized = match (t.rfind(','), t.rfind('.')) {
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        (Some(_), None) => t.replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        _ => t,
    };
    normalized.parse::<f64>().ok().map(|v| (v * 100.0).round() as Cents)
}

fn date(s: &str) -> Option<chrono::NaiveDate> {
    for f in ["%d.%m.%Y", "%Y-%m-%d", "%d.%m.%y", "%d/%m/%Y"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s.trim(), f) {
            return Some(d);
        }
    }
    None
}

fn rate(s: &str) -> Option<u8> {
    let v: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    v.replace(',', ".").parse::<f64>().ok().map(|r| r.round() as u8).filter(|r| VAT_RATES.contains(r))
}

/// The net price whose VAT rounds to exactly the gross amount.
pub fn net_for_gross(gross: Cents, vat: u8) -> Cents {
    let guess = (gross as f64 * 100.0 / (100.0 + vat as f64)).round() as Cents;
    (guess - 2..=guess + 2)
        .find(|n| n + (*n as f64 * vat as f64 / 100.0).round() as Cents == gross)
        .unwrap_or(guess)
}

pub fn parse(kind: ImportKind, bytes: &[u8], data: &Data) -> Result<Parsed, String> {
    let table = Table::read(bytes)?;
    let mut p = Parsed::default();
    let m = &mut p.mapping;
    match kind {
        ImportKind::Contacts => {
            let number = table.col(&["Kundennummer", "Kunden-Nr", "Lieferantennummer", "Kontonummer", "Nummer", "Nr"], "Nummer", m);
            let company = table.col(&["Firma", "Firmenname", "Unternehmen", "Name 1", "Name1", "Company"], "Firma", m);
            let first = table.col(&["Vorname", "First name"], "Vorname", m);
            let last = table.col(&["Nachname", "Name", "Last name", "Ansprechpartner"], "Nachname / Name", m);
            let street = table.col(&["Straße", "Strasse", "Adresse", "Street"], "Straße", m);
            let zip = table.col(&["PLZ", "Postleitzahl", "Zip"], "PLZ", m);
            let city = table.col(&["Ort", "Stadt", "City"], "Ort", m);
            let country = table.col(&["Land", "Country"], "Land", m);
            let email = table.col(&["E-Mail", "Email", "Mail"], "E-Mail", m);
            let phone = table.col(&["Telefon", "Tel", "Phone"], "Telefon", m);
            let vat_id = table.col(&["USt-IdNr", "UStIdNr", "USt-ID", "VAT"], "USt-IdNr.", m);
            let iban = table.col(&["IBAN"], "IBAN", m);
            let kind_col = table.col(&["Kontaktart", "Typ", "Art", "Rolle"], "Art", m);
            if company.is_none() && last.is_none() {
                return Err("Keine Spalte für Firma oder Name gefunden.".into());
            }
            for (i, row) in table.rows.iter().enumerate() {
                let person = [get(row, first), get(row, last)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
                let company_name = get(row, company);
                if company_name.is_empty() && person.is_empty() {
                    p.problems.push(format!("Zeile {}: ohne Namen", i + 2));
                    continue;
                }
                let k = norm(&get(row, kind_col));
                let zip_city = [get(row, zip), get(row, city)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
                let country_text = get(row, country);
                p.contacts.push(Contact {
                    number: get(row, number),
                    kind: if k.contains("lieferant") || k.contains("supplier") || k.contains("kreditor") { ContactKind::Lieferant } else { ContactKind::Kunde },
                    company: company_name,
                    person,
                    street: get(row, street),
                    zip_city,
                    country: if country_text.is_empty() || country_text == "DE" { "Deutschland".into() } else { country_text },
                    email: get(row, email),
                    phone: get(row, phone),
                    vat_id: get(row, vat_id),
                    iban: get(row, iban).replace(' ', ""),
                    ..Contact::default()
                });
            }
        }
        ImportKind::Articles => {
            let number = table.col(&["Artikelnummer", "Artikel-Nr", "Nummer", "Nr"], "Nummer", m);
            let name = table.col(&["Bezeichnung", "Artikelbezeichnung", "Name", "Titel"], "Bezeichnung", m);
            let desc = table.col(&["Beschreibung", "Langtext", "Description"], "Beschreibung", m);
            let unit = table.col(&["Einheit", "Mengeneinheit", "Unit"], "Einheit", m);
            let net = table.col(&["Preis netto", "Nettopreis", "VK netto", "Verkaufspreis netto", "Verkaufspreis", "Preis"], "Preis netto", m);
            let gross = table.col(&["Preis brutto", "Bruttopreis", "VK brutto"], "Preis brutto", m);
            let vat = table.col(&["Steuersatz", "MwSt", "USt", "Umsatzsteuer"], "Steuersatz", m);
            let purchase = table.col(&["Einkaufspreis", "EK netto", "EK"], "Einkaufspreis", m);
            let kind_col = table.col(&["Art", "Typ", "Artikelart"], "Art", m);
            if name.is_none() {
                return Err("Keine Spalte „Bezeichnung“ gefunden.".into());
            }
            for (i, row) in table.rows.iter().enumerate() {
                let n = get(row, name);
                if n.is_empty() {
                    p.problems.push(format!("Zeile {}: ohne Bezeichnung", i + 2));
                    continue;
                }
                let v = rate(&get(row, vat)).unwrap_or(19);
                let price = money(&get(row, net)).or_else(|| money(&get(row, gross)).map(|g| net_for_gross(g, v))).unwrap_or(0);
                let k = norm(&get(row, kind_col));
                p.articles.push(Article {
                    number: get(row, number),
                    name: n,
                    description: get(row, desc),
                    unit: Some(get(row, unit)).filter(|u| !u.is_empty()).unwrap_or_else(|| "Stück".into()),
                    price_net: price,
                    vat: v,
                    service: k.contains("dienst") || k.contains("leistung") || k.contains("service"),
                    purchase_price: money(&get(row, purchase)).unwrap_or(0),
                    ..Article::default()
                });
            }
        }
        ImportKind::OpenItems => {
            let number = table.col(&["Rechnungsnummer", "Rechnungs-Nr", "Belegnummer", "Beleg-Nr", "Nummer", "Nr"], "Rechnungsnummer", m);
            let when = table.col(&["Rechnungsdatum", "Belegdatum", "Datum"], "Datum", m);
            let due = table.col(&["Fälligkeit", "Fällig am", "Fälligkeitsdatum", "Faellig"], "Fällig", m);
            let customer_no = table.col(&["Kundennummer", "Kunden-Nr", "Debitor", "Konto"], "Kundennummer", m);
            let customer = table.col(&["Kunde", "Firma", "Name", "Kundenname"], "Kunde", m);
            let open = table.col(&["Offener Betrag", "Offen", "Restbetrag", "Saldo"], "Offener Betrag", m);
            let total = table.col(&["Bruttobetrag", "Betrag brutto", "Brutto", "Rechnungsbetrag", "Betrag"], "Betrag brutto", m);
            let vat = table.col(&["Steuersatz", "MwSt", "USt"], "Steuersatz", m);
            let currency = table.col(&["Währung", "Waehrung", "Currency"], "Währung", m);
            if number.is_none() || (open.is_none() && total.is_none()) {
                return Err("Es braucht mindestens die Spalten Rechnungsnummer und Betrag.".into());
            }
            for (i, row) in table.rows.iter().enumerate() {
                let line = i + 2;
                let no = get(row, number);
                let Some(amount) = money(&get(row, open)).or_else(|| money(&get(row, total))).filter(|a| *a != 0) else {
                    p.problems.push(format!("Zeile {line}: kein Betrag"));
                    continue;
                };
                let (cno, cname) = (get(row, customer_no), get(row, customer));
                let contact = data
                    .contacts
                    .iter()
                    .find(|c| !cno.is_empty() && c.number == cno)
                    .or_else(|| data.contacts.iter().find(|c| !cname.is_empty() && c.display_name().eq_ignore_ascii_case(&cname)));
                let Some(contact) = contact else {
                    p.problems.push(format!("Zeile {line}: Kunde „{}“ nicht gefunden – bitte zuerst die Kontakte übernehmen", if cno.is_empty() { &cname } else { &cno }));
                    continue;
                };
                let d = date(&get(row, when)).unwrap_or_else(today);
                let due_days = date(&get(row, due)).map(|f| (f - d).num_days().max(0) as u32).unwrap_or(14);
                let v = if contact.vat_case == VatCase::Inland && !data.company.small_business { rate(&get(row, vat)).unwrap_or(19) } else { 0 };
                let (kind, gross) = if amount < 0 { (DocKind::Gutschrift, -amount) } else { (DocKind::Rechnung, amount) };
                let cur = get(row, currency).to_uppercase();
                p.items.push(Document {
                    kind,
                    number: no.clone(),
                    contact_id: Some(contact.id),
                    date: d,
                    due_days,
                    title: format!("{} {no}", kind.name()),
                    intro: "Offener Posten, aus dem bisherigen Programm übernommen.".into(),
                    lines: vec![Line {
                        text: format!("Offener Betrag aus {} {no}", kind.name()),
                        qty: 1.0,
                        unit: "Pauschal".into(),
                        price_net: net_for_gross(gross, v),
                        vat: v,
                        ..Line::default()
                    }],
                    status: DocStatus::Offen,
                    vat_case: contact.vat_case,
                    currency: if cur.is_empty() || cur == "EUR" { String::new() } else { cur },
                    term_name: String::new(),
                    ..Document::default()
                });
            }
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexware_style_files() {
        let data = sample_data();
        let contacts = "Kundennummer;Firma;Vorname;Nachname;Straße;PLZ;Ort;E-Mail;USt-IdNr.\n\
                        20001;Neu GmbH;Eva;Fink;Weg 1;80331;München;eva@neu.de;DE123456789\n\
                        20002;;Max;Muster;Gasse 2;50667;Köln;;\n";
        let p = parse(ImportKind::Contacts, contacts.as_bytes(), &data).unwrap();
        assert_eq!(p.contacts.len(), 2);
        assert_eq!(p.contacts[0].zip_city, "80331 München");
        assert_eq!(p.contacts[1].person, "Max Muster");
        let articles = "Artikelnummer;Bezeichnung;Einheit;VK brutto;MwSt\nA-9;Beratung;Stunde;119,00;19 %\n";
        let p = parse(ImportKind::Articles, articles.as_bytes(), &data).unwrap();
        assert_eq!(p.articles[0].price_net, 10_000);
        let customer = &data.contacts[0];
        let items = format!("Rechnungsnummer;Datum;Fällig am;Kundennummer;Offener Betrag\nALT-77;01.10.2026;15.10.2026;{};1.234,57\n", customer.number);
        let p = parse(ImportKind::OpenItems, items.as_bytes(), &data).unwrap();
        let d = &p.items[0];
        assert_eq!(d.totals(false).gross, 123_457);
        assert_eq!(d.due_days, 14);
        let mut data = data;
        data.apply(Op::ImportOpenItems(p.items.clone()), "t").unwrap();
        assert!(data.apply(Op::ImportOpenItems(p.items), "t").unwrap().is_none());
        assert_eq!(data.documents.iter().filter(|x| x.number == "ALT-77").count(), 1);
    }
}
