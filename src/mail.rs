//! Outgoing e-mail: quotes, invoices, credit notes and payment reminders with the PDF attached.
//!
//! With a server, the server sends (the SMTP password never leaves it); on a single
//! workplace the program sends itself. Every e-mail is recorded on the document.

use crate::i18n::Lang;
use crate::model::*;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    /// Port 587, encryption is switched on after connecting (most providers).
    #[default]
    StartTls,
    /// Port 465, encrypted from the start.
    Tls,
    /// No encryption – only for a mail server in the own network.
    None,
}

impl Security {
    pub const ALL: [Security; 3] = [Security::StartTls, Security::Tls, Security::None];
    pub fn name(self) -> &'static str {
        match self {
            Security::StartTls => "STARTTLS (Port 587)",
            Security::Tls => "SSL/TLS (Port 465)",
            Security::None => "Unverschlüsselt (nur im eigenen Netz)",
        }
    }
}

/// How to reach the outgoing mail server. Kept away from the shared data because of the password.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MailSettings {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    /// Empty when sent to a workplace; saving an empty password keeps the old one.
    pub password: String,
    pub from_name: String,
    pub from_address: String,
    pub reply_to: String,
    /// Copy of every e-mail to this address (e.g. for the archive).
    pub bcc: String,
    /// Microsoft 365: send as an app (SMTP.SendAsApp) instead of with the password.
    pub microsoft: crate::oauth_mail::OAuthApp,
}

impl MailSettings {
    pub fn is_configured(&self) -> bool {
        !self.host.trim().is_empty() && !self.from_address.trim().is_empty()
    }
}

/// A request to send one e-mail about a document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendRequest {
    pub doc_id: u64,
    pub kind: MailKind,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    /// Reminder fee (only for reminders).
    pub fee: Cents,
    /// Days until the new payment deadline (only for reminders).
    pub deadline_days: u32,
}

#[derive(Serialize, Deserialize)]
pub struct TestRequest {
    pub to: String,
}

// ---- Templates --------------------------------------------------------------------

/// The built-in subject and text; companies can override them per kind and language.
pub fn default_template(kind: MailKind, lang: Lang) -> (String, String) {
    let (subject, body) = match (lang, kind) {
        (Lang::De, MailKind::Angebot) => (
            "Angebot {nummer} – {firma}",
            "{anrede}\n\nanbei erhalten Sie unser Angebot {nummer} vom {datum} über {betrag}.\n\nBei Fragen sind wir gerne für Sie da. Wir freuen uns auf Ihren Auftrag.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Rechnung) => (
            "Rechnung {nummer} – {firma}",
            "{anrede}\n\nvielen Dank für Ihren Auftrag. Anbei erhalten Sie unsere Rechnung {nummer} vom {datum} über {betrag}.\n\n{zahlung}\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Gutschrift) => (
            "Gutschrift {nummer} – {firma}",
            "{anrede}\n\nanbei erhalten Sie unsere Gutschrift {nummer} vom {datum} über {betrag}.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Mahnung(0)) => (
            "Zahlungserinnerung zu Rechnung {nummer}",
            "{anrede}\n\nsicher ist es Ihrer Aufmerksamkeit entgangen: Unsere Rechnung {nummer} vom {datum} über {betrag} war am {faellig} fällig. Ein Zahlungseingang ist bei uns bisher nicht verzeichnet.\n\nBitte überweisen Sie den Betrag bis zum {neue_frist}. Sollten Sie die Zahlung bereits veranlasst haben, betrachten Sie diese Nachricht bitte als gegenstandslos.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Mahnung(1)) => (
            "1. Mahnung zu Rechnung {nummer}",
            "{anrede}\n\nzu unserer Rechnung {nummer} vom {datum} konnten wir trotz Zahlungserinnerung noch keinen Zahlungseingang feststellen. Der Rechnungsbetrag war am {faellig} fällig.\n\nBitte überweisen Sie den offenen Betrag von {gesamt} bis zum {neue_frist}. Die Mahnung liegt als PDF bei.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Mahnung(2)) => (
            "2. Mahnung zu Rechnung {nummer}",
            "{anrede}\n\nleider ist der Betrag unserer Rechnung {nummer} vom {datum} trotz Mahnung weiterhin offen.\n\nWir bitten Sie dringend, den offenen Betrag von {gesamt} bis spätestens {neue_frist} zu überweisen. Die Mahnung liegt als PDF bei.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Mahnung(_)) => (
            "Letzte Mahnung zu Rechnung {nummer}",
            "{anrede}\n\ntrotz mehrfacher Erinnerung ist unsere Rechnung {nummer} vom {datum} noch immer nicht bezahlt.\n\nSollte der offene Betrag von {gesamt} nicht bis zum {neue_frist} bei uns eingehen, werden wir die Forderung ohne weitere Ankündigung gerichtlich geltend machen oder an ein Inkassounternehmen übergeben.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Auftragsbestaetigung) => (
            "Auftragsbestätigung {nummer} – {firma}",
            "{anrede}\n\nvielen Dank für Ihren Auftrag. Anbei erhalten Sie unsere Auftragsbestätigung {nummer} vom {datum} über {betrag}.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Lieferschein) => (
            "Lieferschein {nummer} – {firma}",
            "{anrede}\n\nanbei erhalten Sie den Lieferschein {nummer} vom {datum} zu Ihrer Lieferung.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::De, MailKind::Bestellung) => (
            "Bestellung {nummer} – {firma}",
            "{anrede}\n\nhiermit bestellen wir gemäß beiliegender Bestellung {nummer} vom {datum}. Bitte bestätigen Sie uns den Liefertermin.\n\nMit freundlichen Grüßen\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Auftragsbestaetigung) => (
            "Order confirmation {nummer} – {firma}",
            "{anrede}\n\nthank you for your order. Please find attached our order confirmation {nummer} dated {datum} for {betrag}.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Lieferschein) => (
            "Delivery note {nummer} – {firma}",
            "{anrede}\n\nplease find attached delivery note {nummer} dated {datum} for your delivery.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Bestellung) => (
            "Purchase order {nummer} – {firma}",
            "{anrede}\n\nwe hereby place the attached purchase order {nummer} dated {datum}. Please confirm the delivery date.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Angebot) => (
            "Quote {nummer} – {firma}",
            "{anrede}\n\nplease find attached our quote {nummer} dated {datum} for {betrag}.\n\nIf you have any questions, we will be happy to help. We look forward to your order.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Rechnung) => (
            "Invoice {nummer} – {firma}",
            "{anrede}\n\nthank you for your order. Please find attached our invoice {nummer} dated {datum} for {betrag}.\n\n{zahlung}\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Gutschrift) => (
            "Credit note {nummer} – {firma}",
            "{anrede}\n\nplease find attached our credit note {nummer} dated {datum} for {betrag}.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Mahnung(0)) => (
            "Payment reminder for invoice {nummer}",
            "{anrede}\n\nthis may have escaped your attention: our invoice {nummer} dated {datum} for {betrag} was due on {faellig}, and we have not yet received payment.\n\nPlease transfer the amount by {neue_frist}. If you have already paid, please disregard this message.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Mahnung(1)) => (
            "First reminder for invoice {nummer}",
            "{anrede}\n\ndespite our payment reminder, we have not yet received payment for invoice {nummer} dated {datum}, which was due on {faellig}.\n\nPlease transfer the outstanding amount of {gesamt} by {neue_frist}. The reminder is attached as a PDF.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Mahnung(2)) => (
            "Second reminder for invoice {nummer}",
            "{anrede}\n\nunfortunately invoice {nummer} dated {datum} is still unpaid despite our reminder.\n\nWe urgently ask you to transfer the outstanding amount of {gesamt} by {neue_frist} at the latest. The reminder is attached as a PDF.\n\nKind regards\n{absender}\n{firma}",
        ),
        (Lang::En, MailKind::Mahnung(_)) => (
            "Final reminder for invoice {nummer}",
            "{anrede}\n\ndespite several reminders, invoice {nummer} dated {datum} remains unpaid.\n\nIf the outstanding amount of {gesamt} is not received by {neue_frist}, we will take legal action or hand the claim over to a collection agency without further notice.\n\nKind regards\n{absender}\n{firma}",
        ),
    };
    (subject.to_string(), body.to_string())
}

/// The company's own template if there is one, else the built-in one.
pub fn template(company: &Company, kind: MailKind, lang: Lang) -> (String, String) {
    company
        .email_templates
        .iter()
        .find(|t| t.kind == kind && t.lang == lang)
        .map(|t| (t.subject.clone(), t.body.clone()))
        .unwrap_or_else(|| default_template(kind, lang))
}

/// The placeholders a template may use, with an explanation.
pub const PLACEHOLDERS: [(&str, &str); 12] = [
    ("{anrede}", "Anrede mit Name des Ansprechpartners"),
    ("{nummer}", "Belegnummer"),
    ("{datum}", "Belegdatum"),
    ("{betrag}", "Betrag brutto"),
    ("{faellig}", "Fälligkeitsdatum"),
    ("{zahlung}", "Zahlungsbedingungen mit Skonto-Fristen"),
    ("{firma}", "Ihr Firmenname"),
    ("{absender}", "Inhaber/in bzw. Absendername"),
    ("{kunde}", "Name des Kunden"),
    ("{gebuehr}", "Mahngebühr dieser Mahnung"),
    ("{gesamt}", "Offener Betrag mit allen Mahngebühren"),
    ("{neue_frist}", "Neue Zahlungsfrist der Mahnung"),
];

/// Fills a template for a document. `fee` and `deadline` matter for reminders only.
pub fn render(text: &str, data: &Data, doc: &Document, fee: Cents, deadline: chrono::NaiveDate) -> String {
    let contact = data.contact(doc.contact_id);
    let lang = contact.map(|k| k.language).unwrap_or_default();
    let small = data.company.small_business;
    let gross = doc.totals(small).gross;
    let fees: Cents = doc.reminders.iter().map(|r| r.fee).sum::<Cents>() + fee;
    let greeting = match (lang, contact.map(|k| k.person.trim()).unwrap_or("")) {
        (Lang::De, "") => "Sehr geehrte Damen und Herren,".to_string(),
        (Lang::De, name) => format!("Guten Tag {name},"),
        (Lang::En, "") => "Dear Sir or Madam,".to_string(),
        (Lang::En, name) => format!("Dear {name},"),
    };
    let c = &data.company;
    let sender = if c.owner.trim().is_empty() { c.name.clone() } else { c.owner.clone() };
    [
        ("{anrede}", greeting),
        ("{nummer}", doc.number.clone()),
        ("{datum}", fmt_date_in(lang, doc.date)),
        ("{betrag}", fmt_money_in(lang, gross)),
        ("{faellig}", fmt_date_in(lang, doc.due_date())),
        ("{zahlung}", payment_lines(doc, small, lang).join("\n")),
        ("{firma}", c.name.clone()),
        ("{absender}", sender),
        ("{kunde}", data.contact_name(doc.contact_id)),
        ("{gebuehr}", fmt_money_in(lang, fee)),
        ("{gesamt}", fmt_money_in(lang, gross + fees)),
        ("{neue_frist}", fmt_date_in(lang, deadline)),
    ]
    .iter()
    .fold(text.to_string(), |acc, (key, value)| acc.replace(key, value))
}

/// The reminder level that comes next for an invoice (0 = friendly reminder … 3 = final).
pub fn next_reminder_level(doc: &Document) -> u8 {
    doc.reminders.iter().map(|r| r.level + 1).max().unwrap_or(0).min(3)
}

// ---- Sending ------------------------------------------------------------------------

fn mailbox(address: &str, name: Option<&str>) -> Result<Mailbox, String> {
    let address = address.trim();
    let parsed = address
        .parse()
        .map_err(|_| format!("„{address}“ ist keine gültige E-Mail-Adresse."))?;
    Ok(Mailbox::new(name.filter(|n| !n.trim().is_empty()).map(|n| n.trim().to_string()), parsed))
}

/// Several addresses separated by comma or semicolon.
fn mailboxes(list: &str) -> Result<Vec<Mailbox>, String> {
    list.split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| mailbox(s, None))
        .collect()
}

/// Sends one e-mail with optional PDF attachments.
pub fn send(
    settings: &MailSettings,
    to: &str,
    cc: &str,
    subject: &str,
    body: &str,
    attachments: Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    if !settings.is_configured() {
        return Err("Der E-Mail-Versand ist noch nicht eingerichtet (Einstellungen → E-Mail).".into());
    }
    let recipients = mailboxes(to)?;
    if recipients.is_empty() {
        return Err("Bitte mindestens einen Empfänger angeben.".into());
    }
    let mut builder = Message::builder()
        .from(mailbox(&settings.from_address, Some(&settings.from_name))?)
        .subject(subject);
    for m in recipients {
        builder = builder.to(m);
    }
    for m in mailboxes(cc)? {
        builder = builder.cc(m);
    }
    for m in mailboxes(&settings.bcc)? {
        builder = builder.bcc(m);
    }
    if !settings.reply_to.trim().is_empty() {
        builder = builder.reply_to(mailbox(&settings.reply_to, None)?);
    }
    let mut parts = MultiPart::mixed().singlepart(SinglePart::plain(body.to_string()));
    for (name, bytes) in attachments {
        let kind = if name.to_lowercase().ends_with(".xml") { "application/xml" } else { "application/pdf" };
        parts = parts.singlepart(Attachment::new(name).body(bytes, ContentType::parse(kind).unwrap()));
    }
    let message = builder.multipart(parts).map_err(|e| format!("E-Mail konnte nicht erstellt werden: {e}"))?;

    let host = settings.host.trim();
    let transport = match settings.security {
        Security::StartTls => SmtpTransport::starttls_relay(host),
        Security::Tls => SmtpTransport::relay(host),
        Security::None => Ok(SmtpTransport::builder_dangerous(host)),
    }
    .map_err(|e| format!("Mailserver: {e}"))?;
    let default_port = match settings.security {
        Security::StartTls => 587,
        Security::Tls => 465,
        Security::None => 25,
    };
    let mut transport = transport
        .port(if settings.port == 0 { default_port } else { settings.port })
        .timeout(Some(Duration::from_secs(30)));
    if settings.microsoft.is_set() {
        let token = crate::oauth_mail::token(&settings.microsoft)?;
        transport = transport
            .credentials(Credentials::new(settings.username.trim().to_string(), token))
            .authentication(vec![lettre::transport::smtp::authentication::Mechanism::Xoauth2]);
    } else if !settings.username.trim().is_empty() {
        transport = transport.credentials(Credentials::new(settings.username.trim().to_string(), settings.password.clone()));
    }
    transport
        .build()
        .send(&message)
        .map(|_| ())
        .map_err(|e| format!("Versand fehlgeschlagen: {e}"))
}

/// The files that go out for a document, in the customer's e-invoice format:
/// ZUGFeRD PDF, XRechnung XML plus a PDF copy, or just the PDF.
pub fn document_files(data: &Data, doc: &Document) -> Result<Vec<(String, Vec<u8>)>, Vec<String>> {
    use crate::xrechnung::{EInvoiceFormat, Profile};
    let pdf = (crate::pdf::file_name(doc), crate::pdf::document_pdf(data, doc));
    if !matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift) {
        return Ok(vec![pdf]);
    }
    Ok(match data.einvoice_format(doc.contact_id) {
        EInvoiceFormat::Zugferd => vec![(crate::pdf::file_name(doc), crate::pdf::zugferd_pdf(data, doc)?)],
        EInvoiceFormat::XRechnung => {
            let xml = crate::xrechnung::build(data, doc, Profile::XRechnung)?;
            vec![(crate::xrechnung::file_name(doc), xml.into_bytes()), pdf]
        }
        EInvoiceFormat::NurPdf => vec![pdf],
    })
}

/// The names of the files that go out with an e-mail (without building them).
pub fn attachment_names(data: &Data, doc: &Document, kind: MailKind) -> Vec<String> {
    let mut names = Vec::new();
    if let MailKind::Mahnung(level) = kind {
        let r = ReminderRecord { level, date: today(), fee: 0, due: today() };
        names.push(crate::pdf::reminder_file_name(doc, &r));
    }
    let einvoice = matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift);
    match data.einvoice_format(doc.contact_id) {
        crate::xrechnung::EInvoiceFormat::XRechnung if einvoice => {
            names.push(crate::xrechnung::file_name(doc));
            names.push(crate::pdf::file_name(doc));
        }
        crate::xrechnung::EInvoiceFormat::Zugferd if einvoice => {
            names.push(format!("{} (ZUGFeRD)", crate::pdf::file_name(doc)));
        }
        _ => names.push(crate::pdf::file_name(doc)),
    }
    names
}

/// Sends a document (or a reminder about it) and records it on the document.
pub fn send_document(data: &mut Data, settings: &MailSettings, req: &SendRequest, user: &str) -> Result<(), String> {
    let doc = data
        .documents
        .iter()
        .find(|d| d.id == req.doc_id)
        .cloned()
        .ok_or("Beleg nicht gefunden.")?;
    if doc.status == DocStatus::Entwurf {
        return Err("Bitte den Beleg zuerst fertigstellen.".into());
    }
    let mut attachments = Vec::new();
    let mut reminder = None;
    if let MailKind::Mahnung(level) = req.kind {
        if doc.kind != DocKind::Rechnung || doc.status != DocStatus::Offen {
            return Err("Mahnungen gibt es nur für offene Rechnungen.".into());
        }
        let today = today();
        let r = ReminderRecord {
            level: level.min(3),
            date: today,
            fee: req.fee.max(0),
            due: today + chrono::Duration::days(req.deadline_days as i64),
        };
        attachments.push((crate::pdf::reminder_file_name(&doc, &r), crate::pdf::reminder_pdf(data, &doc, &r)));
        reminder = Some(r);
    }
    attachments.extend(document_files(data, &doc).map_err(|p| {
        format!("Die E-Rechnung kann noch nicht erstellt werden: {}", p.join(" "))
    })?);

    send(settings, &req.to, &req.cc, &req.subject, &req.body, attachments)?;

    if let Some(r) = reminder {
        data.apply(Op::RecordReminder(doc.id, r), user)?;
    }
    let record = EmailRecord {
        time: chrono::Local::now().format("%d.%m.%Y %H:%M").to_string(),
        user: user.to_string(),
        to: req.to.trim().to_string(),
        subject: req.subject.clone(),
        kind: req.kind.name().to_string(),
    };
    data.apply(Op::RecordEmail(doc.id, record), user)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_filled() {
        let data = sample_data();
        let doc = data.documents.iter().find(|d| d.kind == DocKind::Rechnung).unwrap();
        let (subject, body) = template(&data.company, MailKind::Rechnung, Lang::De);
        let s = render(&subject, &data, doc, 0, today());
        assert!(s.starts_with("Rechnung RE-"));
        let b = render(&body, &data, doc, 0, today());
        assert!(b.starts_with("Guten Tag Maria Korn,"));
        assert!(!b.contains('{'));
    }

    #[test]
    fn reminder_levels_count_up() {
        let mut doc = Document::default();
        assert_eq!(next_reminder_level(&doc), 0);
        doc.reminders.push(ReminderRecord { level: 0, date: today(), fee: 0, due: today() });
        assert_eq!(next_reminder_level(&doc), 1);
        doc.reminders.push(ReminderRecord { level: 3, date: today(), fee: 0, due: today() });
        assert_eq!(next_reminder_level(&doc), 3);
    }

    #[test]
    fn unconfigured_server_says_so() {
        let err = send(&MailSettings::default(), "a@b.de", "", "x", "y", vec![]).unwrap_err();
        assert!(err.contains("nicht eingerichtet"));
    }
}
