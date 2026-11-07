//! Bank statements: reading CAMT.052/053 (XML from online banking) and CSV exports,
//! and suggesting which open invoice or receipt a payment belongs to.

use crate::model::*;
use chrono::NaiveDate;
use sha2::{Digest, Sha256};

/// Reads a bank statement file. Returns the lines or a readable error.
pub fn parse(name: &str, bytes: &[u8]) -> Result<Vec<BankTx>, String> {
    let text = decode(bytes);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let lines = if trimmed.starts_with('<') || name.to_lowercase().ends_with(".xml") {
        parse_camt(trimmed)?
    } else {
        parse_csv(trimmed)?
    };
    if lines.is_empty() {
        return Err("In der Datei wurden keine Umsätze gefunden.".into());
    }
    Ok(lines)
}

/// Text in UTF-8, or Windows-1252 as many German banks still write it.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

fn cp1252(b: u8) -> char {
    match b {
        0x80 => '€',
        0x8a => 'Š',
        0x9a => 'š',
        0x96 => '–',
        0x84 => '„',
        0x93 => '“',
        _ => b as char,
    }
}

fn fingerprint(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

// ---- CAMT ---------------------------------------------------------------------------

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn path<'a>(n: roxmltree::Node<'a, 'a>, names: &[&str]) -> Option<roxmltree::Node<'a, 'a>> {
    let mut cur = n;
    for name in names {
        cur = child(cur, name)?;
    }
    Some(cur)
}

fn text_at(n: roxmltree::Node, names: &[&str]) -> String {
    path(n, names).and_then(|x| x.text()).unwrap_or("").trim().to_string()
}

fn camt_date(n: roxmltree::Node) -> Option<NaiveDate> {
    let s = path(n, &["Dt"]).or_else(|| path(n, &["DtTm"]))?.text()?.trim().to_string();
    NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()
}

fn parse_camt(xml: &str) -> Result<Vec<BankTx>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("Die XML-Datei ist nicht lesbar: {e}"))?;
    let mut out = Vec::new();
    for stmt in doc.descendants().filter(|n| n.is_element() && matches!(n.tag_name().name(), "Stmt" | "Rpt" | "Ntfctn")) {
        let account = text_at(stmt, &["Acct", "Id", "IBAN"]);
        for (k, e) in stmt.children().filter(|c| c.is_element() && c.tag_name().name() == "Ntry").enumerate() {
            // Only booked entries; pending ones may still change.
            let status = text_at(e, &["Sts"]);
            let status = if status.is_empty() { text_at(e, &["Sts", "Cd"]) } else { status };
            if !status.is_empty() && status != "BOOK" {
                continue;
            }
            let amount_text = text_at(e, &["Amt"]);
            let Some(mut amount) = amount_text.parse::<f64>().ok().map(|v| (v * 100.0).round() as Cents) else {
                continue;
            };
            if text_at(e, &["CdtDbtInd"]) == "DBIT" {
                amount = -amount;
            }
            let date = path(e, &["BookgDt"]).and_then(camt_date).or_else(|| path(e, &["ValDt"]).and_then(camt_date));
            let Some(date) = date else { continue };
            let tx = path(e, &["NtryDtls", "TxDtls"]);
            let incoming = amount >= 0;
            let (party, acct) = if incoming { ("Dbtr", "DbtrAcct") } else { ("Cdtr", "CdtrAcct") };
            let mut name = String::new();
            let mut iban = String::new();
            let mut purpose = String::new();
            let mut reference = text_at(e, &["AcctSvcrRef"]);
            if let Some(tx) = tx {
                let parties = path(tx, &["RltdPties"]);
                if let Some(p) = parties {
                    name = text_at(p, &[party, "Nm"]);
                    if name.is_empty() {
                        name = text_at(p, &[party, "Pty", "Nm"]);
                    }
                    iban = text_at(p, &[acct, "Id", "IBAN"]);
                }
                purpose = tx
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "Ustrd")
                    .filter_map(|n| n.text())
                    .collect::<Vec<_>>()
                    .join(" ");
                if purpose.is_empty() {
                    purpose = text_at(tx, &["RmtInf", "Strd", "CdtrRefInf", "Ref"]);
                }
                if reference.is_empty() {
                    reference = text_at(tx, &["Refs", "AcctSvcrRef"]);
                }
            }
            if purpose.is_empty() {
                purpose = text_at(e, &["AddtlNtryInf"]);
            }
            let reference = if reference.is_empty() {
                fingerprint(&[&account, &date.to_string(), &amount.to_string(), &name, &purpose, &k.to_string()])
            } else {
                fingerprint(&[&account, &reference])
            };
            out.push(BankTx {
                account: account.clone(),
                date,
                amount,
                name: name.trim().to_string(),
                iban: iban.replace(' ', ""),
                purpose: purpose.split_whitespace().collect::<Vec<_>>().join(" "),
                reference,
                ..BankTx::default()
            });
        }
    }
    Ok(out)
}

// ---- CSV --------------------------------------------------------------------------------

fn split_csv(line: &str, sep: char) -> Vec<String> {
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

fn parse_date_any(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for f in ["%d.%m.%Y", "%Y-%m-%d", "%d.%m.%y", "%d/%m/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    None
}

/// Amounts like "-1.234,56", "1234.56", "1.234,56 S" or "-12,00 €".
fn parse_money_any(s: &str) -> Option<Cents> {
    let mut t: String = s.chars().filter(|c| !c.is_whitespace() && *c != '€').collect();
    let mut sign = 1;
    if let Some(rest) = t.strip_suffix('S') {
        sign = -1;
        t = rest.to_string();
    } else if let Some(rest) = t.strip_suffix('H') {
        t = rest.to_string();
    }
    if t.is_empty() {
        return None;
    }
    let normalized = match (t.rfind(','), t.rfind('.')) {
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        (Some(_), None) => t.replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        _ => t,
    };
    let v: f64 = normalized.parse().ok()?;
    Some(sign * (v * 100.0).round() as Cents)
}

fn find_col(header: &[String], keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Some(i) = header.iter().position(|h| h.to_lowercase().replace("ü", "ue").replace("ä", "ae").replace("ö", "oe").contains(key)) {
            return Some(i);
        }
    }
    None
}

fn parse_csv(text: &str) -> Result<Vec<BankTx>, String> {
    let lines: Vec<&str> = text.lines().collect();
    // Banks put account details above the table; the header is the first line naming a date and an amount.
    let sep = if lines.iter().take(30).any(|l| l.matches(';').count() >= 3) { ';' } else { ',' };
    let header_at = lines
        .iter()
        .position(|l| {
            let h = l.to_lowercase();
            (h.contains("buchung") || h.contains("datum") || h.contains("date")) && (h.contains("betrag") || h.contains("umsatz") || h.contains("amount") || h.contains("soll"))
        })
        .ok_or("Die Spalten der CSV-Datei wurden nicht erkannt (Datum, Betrag). Bitte CAMT oder eine CSV-Datei mit Kopfzeile verwenden.")?;
    let header = split_csv(lines[header_at], sep);
    let date_col = find_col(&header, &["buchungstag", "buchungsdatum", "datum", "date"]).ok_or("Spalte „Datum“ nicht gefunden.")?;
    let amount_col = find_col(&header, &["betrag", "umsatz", "amount"]);
    let debit_col = find_col(&header, &["soll"]);
    let credit_col = find_col(&header, &["haben"]);
    if amount_col.is_none() && debit_col.is_none() {
        return Err("Spalte „Betrag“ nicht gefunden.".into());
    }
    let name_col = find_col(&header, &["beguenstigter", "zahlungspflichtiger", "zahlungsbeteiligter", "auftraggeber", "empfaenger", "name", "payee"]);
    let iban_col = find_col(&header, &["iban", "kontonummer"]);
    let purpose_col = find_col(&header, &["verwendungszweck", "zweck", "purpose", "beschreibung", "description"]);
    let account = String::new();
    let mut out = Vec::new();
    for (k, line) in lines[header_at + 1..].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let cells = split_csv(line, sep);
        let get = |i: Option<usize>| i.and_then(|i| cells.get(i)).cloned().unwrap_or_default();
        let Some(date) = parse_date_any(&get(Some(date_col))) else { continue };
        let amount = match amount_col {
            Some(i) => parse_money_any(&get(Some(i))),
            None => {
                let d = parse_money_any(&get(debit_col)).unwrap_or(0);
                let c = parse_money_any(&get(credit_col)).unwrap_or(0);
                Some(c - d.abs())
            }
        };
        let Some(amount) = amount.filter(|a| *a != 0) else { continue };
        let (name, iban, purpose) = (get(name_col), get(iban_col), get(purpose_col));
        out.push(BankTx {
            account: account.clone(),
            date,
            amount,
            reference: fingerprint(&[&date.to_string(), &amount.to_string(), &name, &purpose, &iban, &k.to_string()]),
            name,
            iban: iban.replace(' ', ""),
            purpose: purpose.split_whitespace().collect::<Vec<_>>().join(" "),
            ..BankTx::default()
        });
    }
    Ok(out)
}

// ---- Matching ------------------------------------------------------------------------------

/// A suggestion for a bank line, best first.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub assign: BankAssign,
    pub label: String,
    /// 0–100: how sure the match is.
    pub score: u8,
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

fn name_matches(a: &str, b: &str) -> bool {
    let words = |s: &str| {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 4)
            .map(|w| w.to_lowercase())
            .filter(|w| !matches!(w.as_str(), "gmbh" | "mbh" | "kg" | "ohg" | "und" | "bank"))
            .collect::<Vec<_>>()
    };
    let wb = words(b);
    words(a).iter().any(|w| wb.contains(w))
}

pub fn suggestions(data: &Data, tx: &BankTx) -> Vec<Suggestion> {
    let small = data.company.small_business;
    let purpose = compact(&tx.purpose);
    let mut out = Vec::new();
    for d in data.documents.iter().filter(|d| d.kind.is_billing() && d.status == DocStatus::Offen) {
        let open = d.open_amount(small);
        if open == 0 || open.signum() != tx.amount.signum() {
            continue;
        }
        let contact = data.contact(d.contact_id);
        let by_number = !d.number.is_empty() && purpose.contains(&compact(&d.number));
        let by_name = contact.is_some_and(|c| {
            name_matches(&c.display_name(), &tx.name) || (!c.iban.is_empty() && c.iban.replace(' ', "") == tx.iban)
        });
        let gross = d.totals(small).gross;
        let with_discount = d
            .discounts
            .iter()
            .any(|s| (open - (gross as f64 * s.percent / 100.0).round() as Cents) == tx.amount);
        let exact = open == tx.amount;
        let score = match (by_number, exact || with_discount, by_name) {
            (true, true, _) => 100,
            (true, false, _) => 80,
            (false, true, true) => 90,
            (false, true, false) => 60,
            (false, false, true) => 40,
            _ => continue,
        };
        let label = if with_discount && !exact {
            crate::tf!("{} {} – {} (mit Skonto)", d.kind.label(), d.number, data.contact_name(d.contact_id))
        } else if (tx.amount.abs()) < open.abs() {
            crate::tf!("{} {} – {} (Teilzahlung)", d.kind.label(), d.number, data.contact_name(d.contact_id))
        } else {
            format!("{} {} – {}", d.kind.label(), d.number, data.contact_name(d.contact_id))
        };
        out.push(Suggestion { assign: BankAssign::Document { id: d.id, discount: with_discount && !exact }, label, score });
    }
    if tx.amount < 0 {
        for r in data.receipts.iter().filter(|r| r.status != ReceiptStatus::Verworfen && r.gross > 0) {
            if data.bank.iter().any(|t| t.link == Some(BankLink::Receipt(r.id))) {
                continue;
            }
            let exact = r.gross == -tx.amount;
            let by_number = !r.number.is_empty() && purpose.contains(&compact(&r.number));
            let by_name = name_matches(&r.supplier, &tx.name) || (!r.iban.is_empty() && r.iban.replace(' ', "") == tx.iban);
            let score = match (exact, by_number, by_name) {
                (true, true, _) => 100,
                (true, false, true) => 90,
                (true, false, false) => 55,
                (false, true, _) => 50,
                _ => continue,
            };
            let label = crate::tf!("Eingangsbeleg {} {} ({})", r.supplier, r.number, fmt_money(r.gross));
            out.push(Suggestion { assign: BankAssign::Receipt(r.id), label, score });
        }
    }
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(5);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.02">
 <BkToCstmrStmt><Stmt><Acct><Id><IBAN>DE02120300000000202051</IBAN></Id></Acct>
  <Ntry><Amt Ccy="EUR">1190.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts>BOOK</Sts>
   <BookgDt><Dt>2026-11-03</Dt></BookgDt><AcctSvcrRef>A1</AcctSvcrRef>
   <NtryDtls><TxDtls><RltdPties><Dbtr><Nm>Kunde GmbH</Nm></Dbtr><DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct></RltdPties>
    <RmtInf><Ustrd>RE-2026-0007 Danke</Ustrd></RmtInf></TxDtls></NtryDtls></Ntry>
  <Ntry><Amt Ccy="EUR">45.50</Amt><CdtDbtInd>DBIT</CdtDbtInd><Sts>BOOK</Sts>
   <BookgDt><Dt>2026-11-04</Dt></BookgDt>
   <NtryDtls><TxDtls><RltdPties><Cdtr><Nm>Telefon AG</Nm></Cdtr></RltdPties><RmtInf><Ustrd>Rechnung 4711</Ustrd></RmtInf></TxDtls></NtryDtls></Ntry>
 </Stmt></BkToCstmrStmt></Document>"#;

    #[test]
    fn reads_camt() {
        let l = parse("auszug.xml", CAMT.as_bytes()).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].amount, 119_000);
        assert_eq!(l[0].name, "Kunde GmbH");
        assert_eq!(l[0].purpose, "RE-2026-0007 Danke");
        assert_eq!(l[1].amount, -4_550);
        assert_eq!(l[1].name, "Telefon AG");
        assert_ne!(l[0].reference, l[1].reference);
    }

    #[test]
    fn reads_csv() {
        let csv = "Kontoinhaber;Muster\n\n\"Buchungstag\";\"Valutadatum\";\"Beguenstigter/Zahlungspflichtiger\";\"Verwendungszweck\";\"IBAN\";\"Betrag\"\n\
                   \"03.11.2026\";\"03.11.2026\";\"Kunde GmbH\";\"RE-2026-0007\";\"DE89 3704\";\"1.190,00\"\n\
                   \"04.11.2026\";\"04.11.2026\";\"Telefon AG\";\"Rechnung 4711\";\"\";\"-45,50\"\n";
        let l = parse("umsatz.csv", csv.as_bytes()).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].amount, 119_000);
        assert_eq!(l[1].amount, -4_550);
        assert_eq!(l[0].iban, "DE893704");
        assert_eq!(parse_money_any("1,234.56"), Some(123_456));
        assert_eq!(parse_money_any("12,00 S"), Some(-1_200));
    }

    #[test]
    fn suggests_and_books_invoice() {
        let mut data = sample_data();
        let small = data.company.small_business;
        let inv = data
            .documents
            .iter()
            .find(|d| d.kind == DocKind::Rechnung && d.status == DocStatus::Offen)
            .cloned()
            .expect("offene Rechnung");
        let open = inv.open_amount(small);
        let tx = BankTx {
            date: today(),
            amount: open,
            name: data.contact_name(inv.contact_id),
            purpose: format!("Zahlung {}", inv.number),
            reference: "x1".into(),
            ..BankTx::default()
        };
        data.apply(Op::ImportBank(vec![tx.clone()]), "t").unwrap();
        let tx = data.bank.last().unwrap().clone();
        let s = suggestions(&data, &tx);
        assert_eq!(s[0].score, 100);
        data.apply(Op::AssignBank(tx.id, s[0].assign.clone()), "t").unwrap();
        let d = data.documents.iter().find(|d| d.id == inv.id).unwrap();
        assert_eq!(d.status, DocStatus::Bezahlt);
        let booked: Cents = data.bookings.iter().filter(|b| b.doc_id == Some(inv.id)).map(|b| b.gross).sum();
        assert_eq!(booked, open);
        // Taking it back reopens the invoice.
        data.apply(Op::UnassignBank(tx.id), "t").unwrap();
        let d = data.documents.iter().find(|d| d.id == inv.id).unwrap();
        assert_eq!(d.status, DocStatus::Offen);
        assert!(data.bookings.iter().all(|b| b.doc_id != Some(inv.id)));
        // A second import of the same line is skipped.
        assert!(data.apply(Op::ImportBank(vec![BankTx { reference: "x1".into(), ..tx }]), "t").is_err());
    }

    #[test]
    fn part_payments() {
        let mut data = sample_data();
        let small = data.company.small_business;
        let inv = data.documents.iter().find(|d| d.kind == DocKind::Rechnung && d.status == DocStatus::Offen).cloned().unwrap();
        let open = inv.open_amount(small);
        let first = open / 3;
        let pay = |amount| Payment { date: today(), amount, discount: 0, method: "Bank".into(), bank_tx: None };
        data.apply(Op::AddPayment(inv.id, pay(first)), "t").unwrap();
        let d = data.documents.iter().find(|d| d.id == inv.id).unwrap();
        assert_eq!(d.status, DocStatus::Offen);
        assert_eq!(d.open_amount(small), open - first);
        data.apply(Op::AddPayment(inv.id, pay(open - first)), "t").unwrap();
        let d = data.documents.iter().find(|d| d.id == inv.id).unwrap();
        assert_eq!(d.status, DocStatus::Bezahlt);
        let booked: Cents = data.bookings.iter().filter(|b| b.doc_id == Some(inv.id)).map(|b| b.gross).sum();
        assert_eq!(booked, open);
    }
}
