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

/// Today's reference rate of the European Central Bank: units of `currency` for one euro.
pub fn ecb_rate(currency: &str) -> Result<f64, String> {
    let config = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(10))).build();
    let xml = ureq::Agent::new_with_config(config)
        .get("https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml")
        .call()
        .map_err(|e| format!("EZB nicht erreichbar ({e})."))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let needle = format!("currency='{}' rate='", currency.trim());
    xml.split(&needle)
        .nth(1)
        .and_then(|r| r.split('\'').next())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("Für {currency} veröffentlicht die EZB keinen Kurs."))
}

/// Where new versions are published.
pub const RELEASES_API: &str = "https://api.github.com/repos/johmarjac/OpenKontor/releases/latest";

/// Whether version `a` ("0.2.0" or "v0.2.0") is newer than `b`.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u32> { v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

/// Looks on GitHub for a newer release (in the background). Sends the version and the
/// address of its download page, or nothing if this is the newest (or GitHub is unreachable).
pub fn check_for_update() -> std::sync::mpsc::Receiver<Option<(String, String)>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        #[derive(serde::Deserialize)]
        struct Release {
            tag_name: String,
            html_url: String,
        }
        let config = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(8))).build();
        let found = ureq::Agent::new_with_config(config)
            .get(RELEASES_API)
            .header("User-Agent", "OpenKontor")
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_json::<Release>().ok())
            .filter(|r| is_newer(&r.tag_name, env!("CARGO_PKG_VERSION")))
            .map(|r| (r.tag_name.trim_start_matches('v').to_string(), r.html_url));
        let _ = tx.send(found);
    });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_order() {
        assert!(super::is_newer("v0.2.0", "0.1.9"));
        assert!(super::is_newer("1.0.0", "0.9.12"));
        assert!(!super::is_newer("v0.1.0", "0.1.0"));
    }
}
