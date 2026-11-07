//! The incoming-receipts inbox: turning an uploaded or e-mailed file into a receipt with
//! suggested booking details. E-invoices are read exactly; everything else via its text.

use crate::model::*;
use sha2::{Digest, Sha256};

/// Largest file accepted (25 MB).
pub const MAX_FILE: usize = 25 * 1024 * 1024;
/// Recognised text kept with the receipt (for searching and checking).
const MAX_TEXT: usize = 8000;

/// File types the inbox takes.
pub fn is_supported(name: &str) -> bool {
    let n = name.to_lowercase();
    [".pdf", ".xml", ".jpg", ".jpeg", ".png", ".tif", ".tiff", ".txt"].iter().any(|x| n.ends_with(x))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()
}

/// Finds the supplier among the contacts and the earlier bookings, and the category last used for it.
fn match_supplier(data: &Data, r: &mut Receipt, text_lower: &str) {
    let vat = normalize(&r.vat_id);
    let by_vat = (!vat.is_empty())
        .then(|| data.contacts.iter().find(|c| !c.vat_id.is_empty() && normalize(&c.vat_id) == vat))
        .flatten();
    let by_name = || {
        let mut found: Vec<&Contact> = data
            .contacts
            .iter()
            .filter(|c| {
                let n = c.display_name().to_lowercase();
                n.chars().count() >= 4 && (text_lower.contains(&n) || r.supplier.to_lowercase() == n)
            })
            .collect();
        found.sort_by_key(|c| (c.kind != ContactKind::Lieferant, std::cmp::Reverse(c.display_name().len())));
        found.first().copied()
    };
    if let Some(c) = by_vat.or_else(by_name) {
        r.contact_id = Some(c.id);
        r.supplier = c.display_name();
    } else if let Some(partner) = data
        .bookings
        .iter()
        .rev()
        .filter(|b| b.kind == BookingKind::Ausgabe)
        .map(|b| b.partner.trim())
        .find(|p| p.chars().count() >= 4 && text_lower.contains(&p.to_lowercase()))
    {
        r.supplier = partner.to_string();
    }
    if r.category.is_empty() {
        let supplier = r.supplier.to_lowercase();
        r.category = data
            .bookings
            .iter()
            .rev()
            .find(|b| b.kind == BookingKind::Ausgabe && !supplier.is_empty() && b.partner.to_lowercase() == supplier)
            .map(|b| b.category.clone())
            .unwrap_or_default();
    }
}

fn shorten(text: &str) -> String {
    let t = text.trim();
    if t.len() <= MAX_TEXT {
        return t.to_string();
    }
    let mut end = MAX_TEXT;
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_string()
}

/// Recognises a receipt's details from its file. Never fails: problems end up in `hint`.
pub fn recognize(data: &Data, r: &mut Receipt, bytes: &[u8]) {
    r.hint.clear();
    if let Some(inv) = crate::einvoice::from_file(&r.file_name, bytes) {
        r.recognized_by = inv.format.clone();
        r.supplier = inv.seller.clone();
        r.number = inv.number.clone();
        r.date = inv.date;
        r.due = inv.due;
        r.gross = if inv.gross > 0 { inv.gross } else { inv.payable };
        r.iban = inv.iban.clone();
        r.vat_id = inv.seller_vat_id.clone();
        r.vat = inv
            .vat
            .iter()
            .map(|p| ReceiptVat { rate: p.rate.round().clamp(0.0, 100.0) as u8, net: p.net, tax: p.tax })
            .collect();
        r.vat_rate = r.vat.first().map(|p| p.rate).unwrap_or(19);
        if r.vat.len() < 2 {
            r.vat.clear();
        }
        if !inv.currency.is_empty() && inv.currency != "EUR" {
            r.hint = format!("Achtung: Die Rechnung ist in {} ausgestellt.", inv.currency);
        }
        r.text = shorten(&format!(
            "{}\n{} {}\n{}",
            inv.format, inv.seller, inv.number, r.file_name
        ));
        let lower = r.text.to_lowercase();
        match_supplier(data, r, &lower);
        return;
    }
    match crate::ocr::read_text(&r.file_name, bytes) {
        Ok((text, method)) => {
            let x = crate::ocr::extract(&text);
            r.recognized_by = method.to_string();
            r.supplier = x.supplier;
            r.number = x.number;
            r.date = x.date;
            r.due = x.due;
            r.gross = x.gross;
            r.vat_rate = x.vat_rate.unwrap_or(19);
            r.vat.clear();
            r.iban = x.iban;
            r.vat_id = x.vat_id;
            r.text = shorten(&text);
            if r.gross == 0 {
                r.hint = "Kein Betrag gefunden – bitte eintragen.".into();
            }
            let lower = text.to_lowercase();
            match_supplier(data, r, &lower);
        }
        Err(e) => {
            r.recognized_by.clear();
            r.hint = e;
        }
    }
}

/// A new receipt for a file, with everything that could be recognised.
pub fn new_receipt(data: &Data, file_name: &str, bytes: &[u8], source: &str) -> Receipt {
    let mut r = Receipt {
        received: chrono::Local::now().format("%d.%m.%Y %H:%M").to_string(),
        source: source.to_string(),
        file_name: file_name.trim().to_string(),
        size: bytes.len() as u64,
        sha256: sha256_hex(bytes),
        payment: "Bank".into(),
        ..Receipt::default()
    };
    recognize(data, &mut r, bytes);
    r
}

/// Refuses files the inbox cannot take, and files that are already in it.
pub fn check_new_file(data: &Data, file_name: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("Die Datei ist leer.".into());
    }
    if bytes.len() > MAX_FILE {
        return Err("Die Datei ist größer als 25 MB.".into());
    }
    if !is_supported(file_name) {
        return Err(format!("„{file_name}“: Möglich sind PDF, XML (E-Rechnung), JPG und PNG."));
    }
    let hash = sha256_hex(bytes);
    if let Some(dup) = data.receipts.iter().find(|r| r.sha256 == hash) {
        return Err(format!("„{file_name}“ ist schon im Belegeingang (eingegangen am {}).", dup.received));
    }
    Ok(())
}

/// Example supplier invoices for the demo: an XRechnung and a PDF invoice.
pub fn demo_files() -> Vec<(String, Vec<u8>)> {
    let t = today();
    let xml = crate::einvoice::SAMPLE_UBL
        .replace("2026-10-30", &(t - chrono::Duration::days(4)).format("%Y-%m-%d").to_string())
        .replace("2026-11-13", &(t + chrono::Duration::days(10)).format("%Y-%m-%d").to_string());
    let mut supplier = Data {
        company: Company {
            name: "Bürowelt Versand GmbH".into(),
            owner: "Jana Papier".into(),
            street: "Industriestraße 40".into(),
            zip_city: "70173 Stuttgart".into(),
            vat_id: "DE987654321".into(),
            bank: "Volksbank Stuttgart".into(),
            iban: "DE44 5001 0517 5407 3249 31".into(),
            ..Company::default()
        },
        ..Data::default()
    };
    supplier.contacts.push(Contact {
        id: 1,
        number: "K-55012".into(),
        company: "Muster Design & Technik".into(),
        street: "Hauptstraße 12".into(),
        zip_city: "10115 Berlin".into(),
        ..Contact::default()
    });
    let line = |text: &str, qty: f64, price: Cents| Line { text: text.into(), qty, unit: "Stück".into(), price_net: price, vat: 19, discount_pct: 0.0, article_id: None };
    let doc = Document {
        number: "BW-2026-01234".into(),
        contact_id: Some(1),
        date: t - chrono::Duration::days(2),
        due_days: 14,
        title: "Rechnung".into(),
        intro: "Vielen Dank für Ihre Bestellung.".into(),
        lines: vec![line("Druckerpapier A4, 5 x 500 Blatt", 2.0, 2490), line("Toner schwarz", 1.0, 6140)],
        status: DocStatus::Offen,
        ..Document::default()
    };
    vec![
        ("XRechnung Netzfunk R-1001.xml".into(), xml.into_bytes()),
        ("Rechnung Bürowelt BW-2026-01234.pdf".into(), crate::pdf::document_pdf(&supplier, &doc)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplier_and_category_come_from_earlier_bookings() {
        let data = sample_data();
        let text = "Bürowelt Versand\nRechnungsnummer: 778\nRechnungsdatum 01.11.2026\nGesamtbetrag 23,80 €\n19 % MwSt";
        let r = new_receipt(&data, "rechnung.txt", text.as_bytes(), "test");
        assert_eq!(r.recognized_by, "Textebene");
        assert_eq!(r.supplier, "Bürowelt Versand");
        assert!(r.contact_id.is_some());
        assert_eq!(r.category, "4930 Bürobedarf");
        assert_eq!(r.gross, 2380);
        assert_eq!(r.number, "778");
    }

    #[test]
    fn duplicates_are_refused() {
        let mut data = sample_data();
        let r = new_receipt(&data, "a.txt", b"Gesamtbetrag 10,00", "test");
        data.apply(Op::AddReceipt(Box::new(r)), "t").unwrap();
        assert!(check_new_file(&data, "b.txt", b"Gesamtbetrag 10,00").is_err());
        assert!(check_new_file(&data, "c.exe", b"x").is_err());
    }

    #[test]
    fn booking_a_receipt_with_two_rates() {
        let mut data = sample_data();
        let mut r = new_receipt(&data, "a.txt", b"x", "test");
        r.supplier = "Großhandel".into();
        r.gross = 17250;
        r.category = "3400 Wareneingang".into();
        r.vat = vec![ReceiptVat { rate: 19, net: 10000, tax: 1900 }, ReceiptVat { rate: 7, net: 5000, tax: 350 }];
        let id = data.apply(Op::AddReceipt(Box::new(r)), "t").unwrap().unwrap();
        let before = data.bookings.len();
        data.apply(Op::BookReceipt(id), "t").unwrap();
        assert_eq!(data.bookings.len(), before + 2);
        let total: Cents = data.bookings.iter().filter(|b| b.receipt_id == Some(id)).map(|b| b.gross).sum();
        assert_eq!(total, 17250);
        // Booked entries can only be taken back through the receipt.
        let bid = data.bookings.iter().find(|b| b.receipt_id == Some(id)).unwrap().id;
        assert!(data.apply(Op::DeleteBooking(bid), "t").is_err());
        // While booked it cannot be deleted; after taking the booking back it can.
        assert!(data.apply(Op::DeleteReceipt(id), "t").is_err());
        data.apply(Op::UnbookReceipt(id), "t").unwrap();
        assert_eq!(data.bookings.len(), before);
        data.apply(Op::DeleteReceipt(id), "t").unwrap();
        assert!(data.receipts.iter().all(|r| r.id != id));
        assert!(data.log.last().unwrap().action.contains("gelöscht"));
    }
}
