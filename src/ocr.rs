//! Text recognition for incoming receipts and picking out the important figures.
//!
//! PDFs with a text layer are read directly (`pdftotext`, else built in). Scans and photos
//! go through Tesseract (`tesseract`, with `pdftoppm` for scanned PDFs). These are free
//! programs installed next to the server, e.g. `apt install tesseract-ocr tesseract-ocr-deu poppler-utils`.

use crate::model::Cents;
use chrono::NaiveDate;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What could be read from the text of a receipt. Everything is a suggestion to check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extracted {
    pub supplier: String,
    pub number: String,
    pub date: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    pub gross: Cents,
    pub net: Cents,
    pub vat_rate: Option<u8>,
    pub iban: String,
    pub vat_id: String,
}

fn has_program(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}

/// Which helper programs are installed: (pdftotext, pdftoppm, tesseract).
pub fn tools() -> (bool, bool, bool) {
    (has_program("pdftotext"), has_program("pdftoppm"), has_program("tesseract"))
}

/// A private scratch folder that is removed afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> std::io::Result<Scratch> {
        let mut buf = [0u8; 8];
        getrandom::fill(&mut buf).map_err(|e| std::io::Error::other(e.to_string()))?;
        let name: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("openkontor-ocr-{name}"));
        std::fs::create_dir_all(&dir)?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(cmd: &mut Command) -> Option<String> {
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn tesseract(image: &Path) -> Option<String> {
    run(Command::new("tesseract").arg(image).arg("stdout").args(["-l", "deu+eng", "--psm", "4"]))
        .or_else(|| run(Command::new("tesseract").arg(image).arg("stdout")))
}

/// Built-in reading of the text layer of a PDF.
fn pdf_text_builtin(bytes: &[u8]) -> String {
    let Ok(pdf) = lopdf::Document::load_mem(bytes) else { return String::new() };
    let pages: Vec<u32> = pdf.get_pages().keys().copied().take(5).collect();
    // Keep what could be read even if one font cannot be decoded.
    pdf.extract_text_chunks(&pages).into_iter().filter_map(Result::ok).collect::<Vec<_>>().join("")
}

/// The first page of a PDF as a small PNG picture, if Poppler is installed.
pub fn pdf_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"%PDF") || !has_program("pdftoppm") {
        return None;
    }
    let scratch = Scratch::new().ok()?;
    let file = scratch.0.join("beleg.pdf");
    std::fs::write(&file, bytes).ok()?;
    let prefix = scratch.0.join("vorschau");
    run(Command::new("pdftoppm").args(["-r", "40", "-f", "1", "-l", "1", "-singlefile", "-png"]).arg(&file).arg(&prefix))?;
    std::fs::read(scratch.0.join("vorschau.png")).ok()
}

/// The text of a receipt and how it was obtained ("Textebene" or "Texterkennung").
pub fn read_text(name: &str, bytes: &[u8]) -> Result<(String, &'static str), String> {
    let lower = name.to_lowercase();
    let scratch = Scratch::new().map_err(|e| e.to_string())?;
    if bytes.starts_with(b"%PDF") {
        let file = scratch.0.join("beleg.pdf");
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        let layer = run(Command::new("pdftotext").args(["-layout", "-l", "5"]).arg(&file).arg("-"))
            .unwrap_or_else(|| pdf_text_builtin(bytes));
        if layer.chars().filter(|c| c.is_alphanumeric()).count() > 40 {
            return Ok((layer, "Textebene"));
        }
        // A scan: turn the pages into pictures and recognise them.
        if !has_program("pdftoppm") || !has_program("tesseract") {
            return Err("Der Beleg ist ein Scan. Für die Texterkennung auf dem Server bitte „tesseract-ocr“, „tesseract-ocr-deu“ und „poppler-utils“ installieren.".into());
        }
        let prefix = scratch.0.join("seite");
        run(Command::new("pdftoppm").args(["-r", "300", "-l", "3", "-png"]).arg(&file).arg(&prefix))
            .ok_or("Der PDF-Scan konnte nicht in Bilder umgewandelt werden.")?;
        let mut pages: Vec<PathBuf> = std::fs::read_dir(&scratch.0)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "png"))
            .collect();
        pages.sort();
        let text: Vec<String> = pages.iter().filter_map(|p| tesseract(p)).collect();
        return Ok((text.join("\n"), "Texterkennung"));
    }
    let is_image = [".png", ".jpg", ".jpeg", ".tif", ".tiff", ".bmp", ".webp"].iter().any(|x| lower.ends_with(x))
        || bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(b"\xFF\xD8");
    if is_image {
        if !has_program("tesseract") {
            return Err("Für die Texterkennung von Fotos bitte auf dem Server „tesseract-ocr“ und „tesseract-ocr-deu“ installieren.".into());
        }
        let ext = Path::new(&lower).extension().and_then(|x| x.to_str()).unwrap_or("png");
        let file = scratch.0.join(format!("beleg.{ext}"));
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        return tesseract(&file).map(|t| (t, "Texterkennung")).ok_or("Die Texterkennung ist fehlgeschlagen.".into());
    }
    if lower.ends_with(".txt") {
        return Ok((String::from_utf8_lossy(bytes).into_owned(), "Textebene"));
    }
    Err("Dieses Dateiformat wird nicht erkannt. Möglich sind PDF, XML (E-Rechnung), JPG und PNG.".into())
}

// ---- Picking out the figures ---------------------------------------------------------

/// Reads an amount like "1.234,56", "1234,56", "1,234.56" or "1234.56".
fn parse_amount_token(tok: &str) -> Option<Cents> {
    let t: String = tok
        .trim_matches(|c: char| !c.is_ascii_digit() && c != ',' && c != '.' && c != '-')
        .to_string();
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, t),
    };
    let last_sep = t.rfind([',', '.'])?;
    let decimals = &t[last_sep + 1..];
    if decimals.len() != 2 || !decimals.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole: String = t[..last_sep].chars().filter(|c| c.is_ascii_digit()).collect();
    if whole.is_empty() || whole.len() > 9 {
        return None;
    }
    let v = whole.parse::<i64>().ok()? * 100 + decimals.parse::<i64>().ok()?;
    Some(if neg { -v } else { v })
}

fn amounts_in(line: &str) -> Vec<Cents> {
    // Join "1 234,56" written with a space as thousands separator.
    line.split(|c: char| c.is_whitespace() || c == '€' || c == ':' || c == '*')
        .filter_map(parse_amount_token)
        .collect()
}

fn dates_in(line: &str) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    for tok in line.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':') {
        let tok = tok.trim_matches(|c: char| !c.is_ascii_digit());
        for fmt in ["%d.%m.%Y", "%d.%m.%y", "%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"] {
            if let Ok(d) = NaiveDate::parse_from_str(tok, fmt)
                && (2000..=2100).contains(&chrono::Datelike::year(&d))
            {
                out.push(d);
                break;
            }
        }
    }
    out
}

/// "DE" followed by 20 digits (spaces allowed).
fn find_iban(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len().saturating_sub(2) {
        if chars[i] == 'D' && chars[i + 1] == 'E' && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let digits: String = chars[i + 2..]
                .iter()
                .take(30)
                .filter(|c| !c.is_whitespace())
                .take_while(|c| c.is_ascii_digit())
                .take(20)
                .collect();
            if digits.len() == 20 {
                return format!("DE{digits}");
            }
        }
    }
    String::new()
}

/// A German VAT ID: "DE" and 9 digits.
fn find_vat_id(text: &str) -> String {
    for (i, _) in text.match_indices("DE") {
        let rest: String = text[i + 2..].chars().filter(|c| !c.is_whitespace()).take(10).collect();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() == 9 {
            return format!("DE{digits}");
        }
    }
    String::new()
}

fn value_after_keyword(line: &str, keyword_end: usize) -> String {
    line[keyword_end..]
        .trim_start_matches(|c: char| c == ':' || c == '.' || c == '#' || c.is_whitespace())
        .split_whitespace()
        .find(|w| w.chars().any(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .trim_end_matches([',', ';', '.'])
        .to_string()
}

/// Picks invoice number, dates, amounts, VAT rate, IBAN and supplier out of receipt text.
pub fn extract(text: &str) -> Extracted {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let low: Vec<String> = lines.iter().map(|l| l.to_lowercase()).collect();
    let mut x = Extracted { iban: find_iban(text), vat_id: find_vat_id(text), ..Extracted::default() };

    // Invoice number.
    const NUMBER_KEYS: [&str; 10] = [
        "rechnungsnummer", "rechnungs-nr", "rechnung-nr", "rechnung nr", "rechnungsnr", "re-nr", "beleg-nr", "belegnummer",
        "invoice number", "invoice no",
    ];
    'number: for (i, (l, orig)) in low.iter().zip(&lines).enumerate() {
        for key in NUMBER_KEYS {
            if let Some(pos) = l.find(key) {
                let mut v = value_after_keyword(orig, pos + key.len());
                // Label and value are often separate pieces of text: look at the next line.
                if v.is_empty()
                    && let Some(next) = lines.get(i + 1)
                    && dates_in(next).is_empty()
                {
                    v = value_after_keyword(next, 0);
                }
                if !v.is_empty() {
                    x.number = v;
                    break 'number;
                }
            }
        }
    }

    // Dates: invoice date by keyword, else the first date; due date by keyword.
    for (i, (l, orig)) in low.iter().zip(&lines).enumerate() {
        let mut ds = dates_in(orig);
        if ds.is_empty() && l.len() < 30 {
            ds = lines.get(i + 1).map(|n| dates_in(n)).unwrap_or_default();
        }
        if ds.is_empty() {
            continue;
        }
        if x.due.is_none() && (l.contains("fällig") || l.contains("zahlbar bis") || l.contains("due")) {
            x.due = ds.last().copied();
        } else if x.date.is_none() && (l.contains("rechnungsdatum") || l.contains("datum") || l.contains("date")) {
            x.date = Some(ds[0]);
        }
    }
    if x.date.is_none() {
        x.date = lines.iter().flat_map(|l| dates_in(l)).next();
    }

    // Amounts: the total by priority of the words in front of it.
    const GROSS_KEYS: [&str; 10] = [
        "gesamtbetrag", "rechnungsbetrag", "endbetrag", "zahlbetrag", "zu zahlen", "gesamtsumme",
        "summe brutto", "brutto", "total", "summe",
    ];
    // Amounts on the line itself, else on the next line.
    let amounts_near = |i: usize| -> Vec<Cents> {
        let own = amounts_in(lines[i]);
        if own.is_empty() { lines.get(i + 1).map(|n| amounts_in(n)).unwrap_or_default() } else { own }
    };
    'gross: for key in GROSS_KEYS {
        for (i, l) in low.iter().enumerate() {
            if l.contains(key) && !l.contains("netto") && !l.contains("subtotal") && !l.contains("zwischensumme") {
                if let Some(v) = amounts_near(i).into_iter().filter(|v| *v > 0).max() {
                    x.gross = v;
                    break 'gross;
                }
            }
        }
    }
    if x.gross == 0 {
        x.gross = lines.iter().flat_map(|l| amounts_in(l)).filter(|v| *v > 0).max().unwrap_or(0);
    }
    for key in ["summe netto", "nettobetrag", "netto", "zwischensumme", "subtotal"] {
        if let Some(v) = low
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains(key))
            .filter_map(|(i, _)| amounts_near(i).into_iter().filter(|v| *v > 0 && *v <= x.gross).max())
            .next()
        {
            x.net = v;
            break;
        }
    }

    // VAT rate: written next to "MwSt"/"USt", else worked out from net and gross.
    for l in &low {
        if l.contains("mwst") || l.contains("ust") || l.contains("umsatzsteuer") || l.contains("mehrwertsteuer") || l.contains("vat") {
            if l.contains("19 %") || l.contains("19%") || l.contains("19,00 %") || l.contains("19,0 %") {
                x.vat_rate = Some(19);
                break;
            }
            if l.contains("7 %") || l.contains("7%") || l.contains("7,00 %") {
                x.vat_rate = Some(7);
                break;
            }
        }
    }
    if x.vat_rate.is_none() && x.net > 0 && x.gross > x.net {
        let rate = ((x.gross as f64 / x.net as f64) - 1.0) * 100.0;
        x.vat_rate = [19u8, 7].into_iter().find(|r| (rate - *r as f64).abs() < 0.6);
    }
    if x.net == 0 && x.gross > 0 {
        let rate = x.vat_rate.unwrap_or(19) as f64;
        x.net = (x.gross as f64 * 100.0 / (100.0 + rate)).round() as Cents;
    }

    // Supplier: the first line that looks like a name (letterheads start with it).
    const NOT_A_NAME: [&str; 6] = ["rechnung", "invoice", "seite", "page", "datum", "kopie"];
    x.supplier = lines
        .iter()
        .take(8)
        .find(|l| {
            let lw = l.to_lowercase();
            l.chars().filter(|c| c.is_alphabetic()).count() >= 3 && !NOT_A_NAME.iter().any(|w| lw.starts_with(w)) && dates_in(l).is_empty()
        })
        .map(|l| l.split("  ").next().unwrap_or(l).trim().to_string())
        .unwrap_or_default();
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECEIPT: &str = "Bürowelt Versand GmbH        Industriestraße 40 · 70173 Stuttgart
USt-IdNr. DE 987654321

Rechnung
Rechnungsnummer: BW-2026-01234
Rechnungsdatum: 03.11.2026
Kunden-Nr. 55012

Pos  Artikel                      Menge   Preis
1    Druckerpapier A4, 5 x 500 Bl.    2    24,90 €
2    Toner schwarz                     1    61,40 €

Summe netto                                   72,52 €
zzgl. 19 % MwSt.                              13,78 €
Gesamtbetrag                                  86,30 €

Zahlbar bis 17.11.2026 ohne Abzug.
IBAN DE44 5001 0517 5407 3249 31  BIC INGDDEFFXXX";

    #[test]
    fn reads_a_typical_invoice() {
        let x = extract(RECEIPT);
        assert_eq!(x.supplier, "Bürowelt Versand GmbH");
        assert_eq!(x.number, "BW-2026-01234");
        assert_eq!(x.date, NaiveDate::from_ymd_opt(2026, 11, 3));
        assert_eq!(x.due, NaiveDate::from_ymd_opt(2026, 11, 17));
        assert_eq!(x.gross, 8630);
        assert_eq!(x.net, 7252);
        assert_eq!(x.vat_rate, Some(19));
        assert_eq!(x.iban, "DE44500105175407324931");
        assert_eq!(x.vat_id, "DE987654321");
    }

    #[test]
    fn amounts_in_both_notations() {
        assert_eq!(parse_amount_token("1.234,56"), Some(123456));
        assert_eq!(parse_amount_token("1,234.56"), Some(123456));
        assert_eq!(parse_amount_token("59,90€"), Some(5990));
        assert_eq!(parse_amount_token("2026"), None);
        assert_eq!(parse_amount_token("03.11.2026"), None);
    }

    #[test]
    fn our_own_invoice_pdf_can_be_read_back() {
        let data = crate::model::sample_data();
        let doc = data.documents.iter().find(|d| d.kind == crate::model::DocKind::Rechnung).unwrap();
        let pdf = crate::pdf::document_pdf(&data, doc);
        let text = pdf_text_builtin(&pdf);
        let x = extract(&text);
        assert_eq!(x.number, doc.number, "{text}");
        assert_eq!(x.gross, doc.totals(false).gross, "{text}");
    }
}
