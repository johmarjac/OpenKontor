//! Spreadsheet export and opening files with the system's programs.

use crate::model::*;

fn csv_field(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// All bookings in a date range as a semicolon CSV (opens directly in spreadsheet programs).
pub fn bookings_csv(data: &Data, from: chrono::NaiveDate, to: chrono::NaiveDate) -> String {
    let mut out = String::from(
        "Datum;Art;Konto/Kategorie;Buchungstext;Partner;Beleg;Brutto;USt-Satz;Netto;Steuer;Zahlungsweg\n",
    );
    for b in data.bookings.iter().filter(|b| b.date >= from && b.date <= to) {
        out.push_str(
            &[
                fmt_date(b.date),
                b.kind.label().to_string(),
                b.category.clone(),
                b.text.clone(),
                b.partner.clone(),
                b.receipt.clone(),
                fmt_amount(b.gross),
                format!("{} %", b.vat),
                fmt_amount(b.net()),
                fmt_amount(b.tax()),
                b.payment.clone(),
            ]
            .iter()
            .map(|s| csv_field(s))
            .collect::<Vec<_>>()
            .join(";"),
        );
        out.push('\n');
    }
    out
}

/// Open a web address in the default browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Open a file with the program the system uses for it (browser, spreadsheet...).
pub fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
