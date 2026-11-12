//! Checks whether an own domain is ready for sending e-mail through the chosen mail server:
//! reads MX, SPF and DMARC from the public DNS and says what to add.
//!
//! Receiving and sending are separate: MX records (e.g. Cloudflare Email Routing, which
//! forwards incoming mail) only decide where mail *to* the domain goes. Sending needs an
//! outgoing server that may send for the domain, which the SPF record allows.

use serde::Deserialize;

/// One finding of the check.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub ok: bool,
    pub text: String,
    /// A DNS record to add or change: (type, name, value).
    pub record: Option<(String, String, String)>,
}

/// The SPF include of well-known outgoing servers.
fn spf_include(host: &str) -> Option<&'static str> {
    let h = host.trim().to_lowercase();
    let known: [(&str, &str); 7] = [
        ("smtp.gmail.com", "_spf.google.com"),
        ("smtp.office365.com", "spf.protection.outlook.com"),
        ("smtp-relay.brevo.com", "spf.brevo.com"),
        ("in-v3.mailjet.com", "spf.mailjet.com"),
        ("mail.smtp2go.com", "spf.smtp2go.com"),
        ("smtp.ionos.de", "_spf-eu.ionos.com"),
        ("amazonaws.com", "amazonses.com"),
    ];
    known.iter().find(|(k, _)| h == *k || (k.starts_with("amazonaws") && h.ends_with(k))).map(|(_, v)| *v)
}

/// The domain of an e-mail address.
pub fn domain_of(address: &str) -> Option<String> {
    let d = address.trim().rsplit_once('@')?.1.trim().trim_end_matches('>').to_lowercase();
    (d.contains('.') && !d.contains(' ')).then_some(d)
}

/// Mail providers whose addresses are not an own domain (nothing to set up).
fn is_freemail(domain: &str) -> bool {
    ["gmail.com", "googlemail.com", "gmx.de", "gmx.net", "web.de", "t-online.de", "outlook.com", "outlook.de", "hotmail.com", "yahoo.com", "yahoo.de", "icloud.com"].contains(&domain)
}

/// An SPF record with the include added before the closing "all".
fn merged_spf(existing: Option<&str>, include: &str) -> String {
    match existing {
        None => format!("v=spf1 include:{include} ~all"),
        Some(spf) => {
            let mut parts: Vec<&str> = spf.split_whitespace().collect();
            let item = format!("include:{include}");
            let at = parts.iter().position(|p| p.trim_start_matches(['~', '-', '?', '+']) == "all").unwrap_or(parts.len());
            parts.insert(at, &item);
            let mut out = parts.join(" ");
            if at == parts.len() - 1 {
                out.push_str(" ~all");
            }
            out
        }
    }
}

/// Works out the findings from the records found (MX hosts, TXT of the domain, TXT of _dmarc).
pub fn analyse(domain: &str, host: &str, mx: &[String], txt: &[String], dmarc: &[String]) -> Vec<Finding> {
    let mut out = Vec::new();
    let say = |ok: bool, text: String| Finding { ok, text, record: None };
    if is_freemail(domain) {
        out.push(say(true, format!("{domain} gehört dem Mail-Anbieter – hier ist nichts einzurichten.")));
        return out;
    }
    // Receiving
    if mx.iter().any(|m| m.contains("mx.cloudflare.net")) {
        out.push(say(
            true,
            "Eingehende E-Mails leitet Cloudflare Email Routing weiter. Das bleibt so – der Versand läuft getrennt über den Postausgangsserver, die MX-Einträge werden nicht geändert.".into(),
        ));
        out.push(say(
            true,
            "Cloudflare Email Routing kann selbst keine E-Mails verschicken. Tragen Sie als Postausgangsserver einen Versanddienst ein (z. B. Brevo, Mailjet, SMTP2GO, Amazon SES) oder den Ihres E-Mail-Postfachs.".into(),
        ));
    } else if mx.is_empty() {
        out.push(say(false, "Die Domain hat keine MX-Einträge – Antworten an diese Adresse kommen nicht an. Tragen Sie unter „Antwort an“ eine erreichbare Adresse ein.".into()));
    } else {
        out.push(say(true, format!("Eingehende E-Mails gehen an {}.", mx.join(", "))));
    }
    // SPF: who may send for the domain
    let spfs: Vec<&String> = txt.iter().filter(|t| t.trim_start().to_lowercase().starts_with("v=spf1")).collect();
    if spfs.len() > 1 {
        out.push(say(false, "Es gibt mehrere SPF-Einträge (TXT mit „v=spf1“). Erlaubt ist nur einer – fassen Sie sie zu einem zusammen, sonst gelten alle als ungültig.".into()));
    }
    let spf = spfs.first().map(|s| s.as_str());
    match spf_include(host) {
        Some(include) if spf.is_some_and(|s| s.contains(&format!("include:{include}"))) => {
            out.push(say(true, format!("Der SPF-Eintrag erlaubt {host}, für {domain} zu senden.")));
        }
        Some(include) => out.push(Finding {
            ok: false,
            text: format!(
                "Der SPF-Eintrag erlaubt {host} noch nicht. Ändern Sie den TXT-Eintrag bei Cloudflare (DNS) so – die Weiterleitung bleibt dabei erhalten:"
            ),
            record: Some(("TXT".into(), domain.to_string(), merged_spf(spf, include))),
        }),
        None if host.trim().is_empty() => out.push(say(false, "Tragen Sie zuerst einen Postausgangsserver ein.".into())),
        None => match spf {
            Some(s) => out.push(say(true, format!("SPF-Eintrag: {s} – prüfen Sie, ob Ihr Versanddienst darin genannt ist (er sagt Ihnen, welches „include:“ nötig ist)."))),
            None => out.push(say(false, "Es gibt keinen SPF-Eintrag. Ihr Versanddienst nennt Ihnen den Eintrag (TXT mit „v=spf1 …“); ohne landen E-Mails oft im Spam.".into())),
        },
    }
    // DKIM can't be checked without the provider's selector.
    out.push(say(
        true,
        "DKIM: Lassen Sie beim Versanddienst Ihre Domain bestätigen; er zeigt die DKIM-Einträge (meist TXT oder CNAME), die Sie bei Cloudflare unter DNS eintragen.".into(),
    ));
    // DMARC
    if dmarc.iter().any(|d| d.trim_start().to_lowercase().starts_with("v=dmarc1")) {
        out.push(say(true, "Ein DMARC-Eintrag ist vorhanden.".into()));
    } else {
        out.push(Finding {
            ok: false,
            text: "Es gibt keinen DMARC-Eintrag. Gmail und Yahoo erwarten ihn; zum Anfang genügt:".into(),
            record: Some(("TXT".into(), format!("_dmarc.{domain}"), "v=DMARC1; p=none".into())),
        });
    }
    out
}

#[derive(Deserialize)]
struct DohAnswer {
    #[serde(default, rename = "Answer")]
    answer: Vec<DohRecord>,
}

#[derive(Deserialize)]
struct DohRecord {
    #[serde(rename = "type")]
    kind: u16,
    data: String,
}

/// Looks up records over DNS-over-HTTPS (works the same on every system).
fn lookup(agent: &ureq::Agent, name: &str, kind: &str) -> Result<Vec<String>, String> {
    let url = format!("https://cloudflare-dns.com/dns-query?name={name}&type={kind}");
    let answer: DohAnswer = agent
        .get(&url)
        .header("Accept", "application/dns-json")
        .call()
        .map_err(|e| format!("Der DNS-Dienst ist nicht erreichbar ({e})."))?
        .body_mut()
        .read_json()
        .map_err(|e| e.to_string())?;
    let want = if kind == "MX" { 15 } else { 16 };
    Ok(answer
        .answer
        .into_iter()
        .filter(|r| r.kind == want)
        .map(|r| {
            if kind == "MX" {
                r.data.split_whitespace().last().unwrap_or("").trim_end_matches('.').to_string()
            } else {
                // TXT data comes quoted, long ones in several pieces.
                r.data.split('"').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, p)| p).collect::<String>()
            }
        })
        .collect())
}

/// Checks the domain of the sender address against the outgoing server.
pub fn check(from_address: &str, host: &str) -> Result<Vec<Finding>, String> {
    let domain = domain_of(from_address).ok_or("Tragen Sie zuerst eine Absenderadresse ein.")?;
    let config = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(10))).build();
    let agent = ureq::Agent::new_with_config(config);
    let mx = lookup(&agent, &domain, "MX")?;
    let txt = lookup(&agent, &domain, "TXT")?;
    let dmarc = lookup(&agent, &format!("_dmarc.{domain}"), "TXT")?;
    Ok(analyse(&domain, host, &mx, &txt, &dmarc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_forwarding_and_spf() {
        let mx = vec!["route1.mx.cloudflare.net".to_string()];
        let txt = vec!["v=spf1 include:_spf.mx.cloudflare.net ~all".to_string()];
        let f = analyse("meine-firma.de", "smtp-relay.brevo.com", &mx, &txt, &[]);
        assert!(f[0].ok && f[0].text.contains("Cloudflare"));
        let spf = f.iter().find_map(|x| x.record.clone().filter(|r| r.1 == "meine-firma.de")).unwrap();
        assert_eq!(spf.2, "v=spf1 include:_spf.mx.cloudflare.net include:spf.brevo.com ~all");
        assert!(f.iter().any(|x| x.record.as_ref().is_some_and(|r| r.1 == "_dmarc.meine-firma.de")));
        // Already set up: nothing to change.
        let txt = vec!["v=spf1 include:_spf.mx.cloudflare.net include:spf.brevo.com ~all".to_string()];
        let f = analyse("meine-firma.de", "smtp-relay.brevo.com", &mx, &txt, &["v=DMARC1; p=none".into()]);
        assert!(f.iter().all(|x| x.ok));
        assert_eq!(merged_spf(None, "_spf.google.com"), "v=spf1 include:_spf.google.com ~all");
        assert_eq!(merged_spf(Some("v=spf1 mx"), "a.b"), "v=spf1 mx include:a.b ~all");
        assert_eq!(domain_of("Rechnung <rechnung@Meine-Firma.de>").as_deref(), Some("meine-firma.de"));
        assert!(analyse("gmx.de", "mail.gmx.net", &[], &[], &[])[0].ok);
    }
}
