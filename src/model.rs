//! Data model, calculations and storage for OpenKontor.

use crate::i18n::Lang;
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Money is stored as whole cents to avoid rounding errors.
pub type Cents = i64;

pub const VAT_RATES: [u8; 3] = [19, 7, 0];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Company {
    pub name: String,
    pub owner: String,
    pub street: String,
    pub zip_city: String,
    pub email: String,
    pub phone: String,
    pub tax_number: String,
    pub vat_id: String,
    pub bank: String,
    pub iban: String,
    pub bic: String,
    /// Kleinunternehmerregelung nach § 19 UStG: no VAT on invoices.
    pub small_business: bool,
    pub payment_days: u32,
    /// Payment conditions to choose from (e.g. "10 Tage 2 % Skonto, 30 Tage netto").
    pub payment_terms: Vec<PaymentTerm>,
    /// Condition for new invoices when the customer has none of their own.
    pub default_term_id: u64,
    /// The company's own e-mail texts (else the built-in ones are used).
    pub email_templates: Vec<MailTemplate>,
    /// Reminder fee per level: reminder, 1st, 2nd and final notice.
    pub reminder_fees: Vec<Cents>,
    /// Days until the new deadline set in a reminder.
    pub reminder_days: u32,
    /// How invoices are sent unless a customer has its own setting.
    pub einvoice_default: crate::xrechnung::EInvoiceFormat,
    pub invoice_prefix: String,
    pub offer_prefix: String,
    pub credit_prefix: String,
    pub order_prefix: String,
    pub delivery_prefix: String,
    pub purchase_prefix: String,
    pub next_invoice_no: u32,
    pub next_offer_no: u32,
    pub next_credit_no: u32,
    pub next_order_no: u32,
    pub next_delivery_no: u32,
    pub next_purchase_no: u32,
    pub next_contact_no: u32,
    pub next_article_no: u32,
    /// Company logo printed on the letterhead.
    pub logo: Option<Logo>,
    /// DATEV consultant and client number for the export to the tax advisor.
    pub datev_advisor: String,
    pub datev_client: String,
    /// Chart of accounts for exports and the balance sheet.
    pub chart: Chart,
    /// Cash-basis accounts (EÜR) or full double-entry with balance sheet.
    pub accounting: Accounting,
    /// Cash in hand before the first entry of the cash book.
    pub cash_opening: Cents,
    /// Balance of the business account on a day, as start of the cash-flow forecast.
    pub bank_balance: Cents,
    pub bank_balance_date: Option<NaiveDate>,
}

/// A logo as plain RGB pixels (compressed, base64), ready to be placed in PDFs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Logo {
    pub width: u32,
    pub height: u32,
    /// zlib-compressed RGB bytes, base64-encoded.
    pub rgb: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chart {
    #[default]
    Skr03,
    Skr04,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accounting {
    /// Einnahmen-Überschuss-Rechnung: income and expenses count when paid.
    #[default]
    Euer,
    /// Double-entry bookkeeping with balance sheet (e.g. GmbH).
    Bilanz,
}

impl Default for Company {
    fn default() -> Self {
        Self {
            name: "Meine Firma".into(),
            owner: String::new(),
            street: String::new(),
            zip_city: String::new(),
            email: String::new(),
            phone: String::new(),
            tax_number: String::new(),
            vat_id: String::new(),
            bank: String::new(),
            iban: String::new(),
            bic: String::new(),
            small_business: false,
            payment_days: 14,
            payment_terms: standard_terms(),
            default_term_id: 1,
            email_templates: Vec::new(),
            reminder_fees: vec![0, 500, 1000, 1500],
            reminder_days: 7,
            einvoice_default: crate::xrechnung::EInvoiceFormat::Zugferd,
            invoice_prefix: "RE-".into(),
            offer_prefix: "AN-".into(),
            credit_prefix: "GS-".into(),
            order_prefix: "AB-".into(),
            delivery_prefix: "LS-".into(),
            purchase_prefix: "BE-".into(),
            next_invoice_no: 1,
            next_offer_no: 1,
            next_credit_no: 1,
            next_order_no: 1,
            next_delivery_no: 1,
            next_purchase_no: 1,
            next_contact_no: 10001,
            next_article_no: 1,
            logo: None,
            datev_advisor: String::new(),
            datev_client: String::new(),
            chart: Chart::Skr03,
            accounting: Accounting::Euer,
            cash_opening: 0,
            bank_balance: 0,
            bank_balance_date: None,
        }
    }
}

/// What an e-mail is about. Reminders count from 0 (friendly reminder) to 3 (final notice).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailKind {
    Angebot,
    Rechnung,
    Gutschrift,
    Mahnung(u8),
    Auftragsbestaetigung,
    Lieferschein,
    Bestellung,
}

impl MailKind {
    pub fn for_doc(kind: DocKind) -> MailKind {
        match kind {
            DocKind::Angebot => MailKind::Angebot,
            DocKind::Rechnung => MailKind::Rechnung,
            DocKind::Gutschrift => MailKind::Gutschrift,
            DocKind::Auftragsbestaetigung => MailKind::Auftragsbestaetigung,
            DocKind::Lieferschein => MailKind::Lieferschein,
            DocKind::Bestellung => MailKind::Bestellung,
        }
    }

    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            MailKind::Angebot => "Angebot",
            MailKind::Rechnung => "Rechnung",
            MailKind::Gutschrift => "Gutschrift",
            MailKind::Mahnung(level) => reminder_title(level),
            MailKind::Auftragsbestaetigung => "Auftragsbestätigung",
            MailKind::Lieferschein => "Lieferschein",
            MailKind::Bestellung => "Bestellung",
        }
    }

    pub const ALL: [MailKind; 10] = [
        MailKind::Angebot,
        MailKind::Auftragsbestaetigung,
        MailKind::Lieferschein,
        MailKind::Rechnung,
        MailKind::Gutschrift,
        MailKind::Bestellung,
        MailKind::Mahnung(0),
        MailKind::Mahnung(1),
        MailKind::Mahnung(2),
        MailKind::Mahnung(3),
    ];
}

/// German title of a reminder level.
pub fn reminder_title(level: u8) -> &'static str {
    match level {
        0 => "Zahlungserinnerung",
        1 => "1. Mahnung",
        2 => "2. Mahnung",
        _ => "Letzte Mahnung",
    }
}

/// A company's own text for one kind of e-mail in one language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MailTemplate {
    pub kind: MailKind,
    pub lang: Lang,
    pub subject: String,
    pub body: String,
}

/// An e-mail that was sent about a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailRecord {
    pub time: String,
    pub user: String,
    pub to: String,
    pub subject: String,
    pub kind: String,
}

/// A payment reminder for an invoice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReminderRecord {
    pub level: u8,
    pub date: NaiveDate,
    pub fee: Cents,
    /// New payment deadline set by this reminder.
    pub due: NaiveDate,
}

/// One early-payment discount: pay within `days` and deduct `percent`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscountStage {
    pub days: u32,
    pub percent: f64,
}

/// A payment condition: optional discount stages plus the net payment period.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentTerm {
    pub id: u64,
    pub name: String,
    pub discounts: Vec<DiscountStage>,
    pub net_days: u32,
}

impl Default for PaymentTerm {
    fn default() -> Self {
        Self { id: 0, name: String::new(), discounts: Vec::new(), net_days: 14 }
    }
}

impl PaymentTerm {
    /// A readable summary, e.g. "10 Tage 2 % Skonto · 30 Tage netto".
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self
            .discounts
            .iter()
            .map(|s| crate::tf!("{} Tage {} % Skonto", s.days, fmt_qty(s.percent)))
            .collect();
        parts.push(if self.net_days == 0 {
            crate::i18n::t("sofort ohne Abzug").to_string()
        } else {
            crate::tf!("{} Tage netto", self.net_days)
        });
        parts.join(" · ")
    }
}

pub fn standard_terms() -> Vec<PaymentTerm> {
    let stage = |days, percent| DiscountStage { days, percent };
    vec![
        PaymentTerm { id: 1, name: "14 Tage netto".into(), discounts: vec![], net_days: 14 },
        PaymentTerm { id: 2, name: "Sofort ohne Abzug".into(), discounts: vec![], net_days: 0 },
        PaymentTerm { id: 3, name: "30 Tage netto".into(), discounts: vec![], net_days: 30 },
        PaymentTerm { id: 4, name: "10 Tage 2 % Skonto, 30 Tage netto".into(), discounts: vec![stage(10, 2.0)], net_days: 30 },
        PaymentTerm {
            id: 5,
            name: "7 Tage 3 %, 14 Tage 2 % Skonto, 30 Tage netto".into(),
            discounts: vec![stage(7, 3.0), stage(14, 2.0)],
            net_days: 30,
        },
    ]
}

impl Company {
    pub fn term(&self, id: Option<u64>) -> Option<&PaymentTerm> {
        id.and_then(|id| self.payment_terms.iter().find(|t| t.id == id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactKind {
    Kunde,
    Lieferant,
}

impl ContactKind {
    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            ContactKind::Kunde => "Kunde",
            ContactKind::Lieferant => "Lieferant",
        }
    }

    /// Name in the interface language.
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    pub id: u64,
    pub number: String,
    pub kind: ContactKind,
    pub company: String,
    pub person: String,
    pub street: String,
    pub zip_city: String,
    pub country: String,
    pub email: String,
    pub phone: String,
    pub vat_id: String,
    pub note: String,
    /// Language of the offers and invoices this contact receives.
    pub language: Lang,
    /// Payment condition for this customer; `None` uses the company's default.
    pub payment_term_id: Option<u64>,
    /// Leitweg-ID (public authorities) or other reference the customer wants on e-invoices.
    pub buyer_reference: String,
    /// How this customer receives invoices; `None` uses the company's default.
    pub einvoice: Option<crate::xrechnung::EInvoiceFormat>,
    /// VAT treatment of documents for this contact (domestic, EU, outside the EU).
    pub vat_case: VatCase,
    /// Bank account, to recognise payments in bank statements.
    pub iban: String,
    /// Currency of this contact's documents (ISO code); empty = euro.
    pub currency: String,
}

/// How VAT applies to a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VatCase {
    #[default]
    Inland,
    /// Tax-free delivery of goods to a business in another EU country (§ 4 Nr. 1b UStG).
    IgLieferung,
    /// Service to a business in another EU country; the customer pays the VAT (reverse charge).
    ReverseCharge,
    /// Export of goods outside the EU (§ 4 Nr. 1a UStG).
    Ausfuhr,
    /// Service to a customer outside the EU: not taxable in Germany.
    DrittlandLeistung,
}

impl VatCase {
    pub const ALL: [VatCase; 5] =
        [VatCase::Inland, VatCase::IgLieferung, VatCase::ReverseCharge, VatCase::Ausfuhr, VatCase::DrittlandLeistung];

    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            VatCase::Inland => "Inland (mit Umsatzsteuer)",
            VatCase::IgLieferung => "EU: innergemeinschaftliche Lieferung",
            VatCase::ReverseCharge => "EU: Leistung, Steuerschuld beim Kunden",
            VatCase::Ausfuhr => "Ausfuhr (außerhalb der EU)",
            VatCase::DrittlandLeistung => "Leistung außerhalb der EU",
        }
    }

    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
    }

    /// The note printed on the invoice, in the customer's language.
    pub fn invoice_note(self) -> Option<&'static str> {
        match self {
            VatCase::Inland => None,
            VatCase::IgLieferung => Some("Steuerfreie innergemeinschaftliche Lieferung (§ 4 Nr. 1b UStG)."),
            VatCase::ReverseCharge => Some("Steuerschuldnerschaft des Leistungsempfängers (Reverse Charge)."),
            VatCase::Ausfuhr => Some("Steuerfreie Ausfuhrlieferung (§ 4 Nr. 1a UStG)."),
            VatCase::DrittlandLeistung => Some("Im Inland nicht steuerbare Leistung."),
        }
    }

    /// Income account for payments of such documents.
    pub fn income_category(self) -> Option<&'static str> {
        match self {
            VatCase::Inland => None,
            VatCase::IgLieferung => Some(INCOME_CATEGORIES[6]),
            VatCase::ReverseCharge => Some(INCOME_CATEGORIES[7]),
            VatCase::Ausfuhr => Some(INCOME_CATEGORIES[3]),
            VatCase::DrittlandLeistung => Some(INCOME_CATEGORIES[8]),
        }
    }
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            id: 0,
            number: String::new(),
            kind: ContactKind::Kunde,
            company: String::new(),
            person: String::new(),
            street: String::new(),
            zip_city: String::new(),
            country: "Deutschland".into(),
            email: String::new(),
            phone: String::new(),
            vat_id: String::new(),
            note: String::new(),
            language: Lang::De,
            payment_term_id: None,
            buyer_reference: String::new(),
            einvoice: None,
            vat_case: VatCase::Inland,
            iban: String::new(),
            currency: String::new(),
        }
    }
}

impl Contact {
    pub fn display_name(&self) -> String {
        if !self.company.trim().is_empty() {
            self.company.clone()
        } else if !self.person.trim().is_empty() {
            self.person.clone()
        } else {
            "(ohne Namen)".into()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Article {
    pub id: u64,
    pub number: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub price_net: Cents,
    pub vat: u8,
    pub service: bool,
    /// Keep track of the stock of this article.
    pub track_stock: bool,
    /// Warn when the stock falls below this.
    pub min_stock: f64,
    /// Net purchase price, for orders and the value of the stock.
    pub purchase_price: Cents,
}

impl Default for Article {
    fn default() -> Self {
        Self {
            id: 0,
            number: String::new(),
            name: String::new(),
            description: String::new(),
            unit: "Stück".into(),
            price_net: 0,
            vat: 19,
            service: false,
            track_stock: false,
            min_stock: 0.0,
            purchase_price: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    Angebot,
    Rechnung,
    Gutschrift,
    Auftragsbestaetigung,
    Lieferschein,
    /// An order to a supplier.
    Bestellung,
}

impl DocKind {
    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            DocKind::Angebot => "Angebot",
            DocKind::Rechnung => "Rechnung",
            DocKind::Gutschrift => "Gutschrift",
            DocKind::Auftragsbestaetigung => "Auftragsbestätigung",
            DocKind::Lieferschein => "Lieferschein",
            DocKind::Bestellung => "Bestellung",
        }
    }

    /// Name in the interface language.
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
    }

    /// Invoices and credit notes are booked and can be paid.
    pub fn is_billing(self) -> bool {
        matches!(self, DocKind::Rechnung | DocKind::Gutschrift)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocStatus {
    Entwurf,
    Offen,
    Bezahlt,
    Angenommen,
    Abgelehnt,
    Storniert,
    /// Delivered (delivery note) or received (order to a supplier).
    Erledigt,
}

/// Whether an invoice is a normal one, an advance (Abschlagsrechnung) or the final one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvoicePart {
    #[default]
    Normal,
    Abschlag,
    Schluss,
}

/// A payment received for an invoice (or paid out for a credit note).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Payment {
    pub date: NaiveDate,
    /// Amount paid, with the sign of the document's total.
    pub amount: Cents,
    /// Early-payment discount accepted with this payment (closes the rest).
    pub discount: Cents,
    /// "Bank", "Kasse" …
    pub method: String,
    /// The bank statement line this payment came from.
    pub bank_tx: Option<u64>,
    /// For documents in another currency: the euros actually received.
    pub eur: Option<Cents>,
}

impl DocStatus {
    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            DocStatus::Entwurf => "Entwurf",
            DocStatus::Offen => "Offen",
            DocStatus::Bezahlt => "Bezahlt",
            DocStatus::Angenommen => "Angenommen",
            DocStatus::Abgelehnt => "Abgelehnt",
            DocStatus::Storniert => "Storniert",
            DocStatus::Erledigt => "Erledigt",
        }
    }

    /// Name in the interface language.
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub article_id: Option<u64>,
    pub text: String,
    pub qty: f64,
    pub unit: String,
    pub price_net: Cents,
    pub vat: u8,
    pub discount_pct: f64,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            article_id: None,
            text: String::new(),
            qty: 1.0,
            unit: "Stück".into(),
            price_net: 0,
            vat: 19,
            discount_pct: 0.0,
        }
    }
}

impl Line {
    pub fn net(&self) -> Cents {
        let raw = self.qty * self.price_net as f64 * (1.0 - self.discount_pct / 100.0);
        raw.round() as Cents
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Document {
    pub id: u64,
    pub kind: DocKind,
    pub number: String,
    pub contact_id: Option<u64>,
    pub date: NaiveDate,
    pub service_date: String,
    pub due_days: u32,
    pub title: String,
    pub intro: String,
    pub outro: String,
    pub lines: Vec<Line>,
    pub status: DocStatus,
    pub paid_date: Option<NaiveDate>,
    pub source_id: Option<u64>,
    /// Name of the chosen payment condition (for the document's records).
    pub term_name: String,
    /// Early-payment discounts; `due_days` is the net period. Copied into the
    /// document so a finished invoice keeps its conditions.
    pub discounts: Vec<DiscountStage>,
    /// Discount actually deducted when the invoice was paid.
    pub paid_discount_pct: f64,
    /// E-mails sent about this document.
    pub emails: Vec<EmailRecord>,
    /// Payment reminders for this invoice.
    pub reminders: Vec<ReminderRecord>,
    /// Advance or final invoice of a project.
    pub part: InvoicePart,
    /// VAT treatment (domestic, EU, export); taken from the contact.
    pub vat_case: VatCase,
    /// Payments received so far (several for part payments).
    pub payments: Vec<Payment>,
    /// Currency of the document (ISO code, e.g. "USD"); empty = euro.
    pub currency: String,
    /// Units of the currency for one euro (as the ECB publishes it), e.g. 1.0832 for USD.
    pub exchange_rate: f64,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            id: 0,
            kind: DocKind::Rechnung,
            number: String::new(),
            contact_id: None,
            date: today(),
            service_date: String::new(),
            due_days: 14,
            title: String::new(),
            intro: String::new(),
            outro: String::new(),
            lines: vec![Line::default()],
            status: DocStatus::Entwurf,
            paid_date: None,
            source_id: None,
            term_name: String::new(),
            discounts: Vec::new(),
            paid_discount_pct: 0.0,
            emails: Vec::new(),
            reminders: Vec::new(),
            part: InvoicePart::Normal,
            vat_case: VatCase::Inland,
            payments: Vec::new(),
            currency: String::new(),
            exchange_rate: 0.0,
        }
    }
}

/// Net, tax and gross sums of a document, with a breakdown per VAT rate.
#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub net: Cents,
    pub tax: Cents,
    pub gross: Cents,
    pub by_rate: BTreeMap<u8, (Cents, Cents)>,
}

impl Document {
    pub fn due_date(&self) -> NaiveDate {
        self.date + Duration::days(self.due_days as i64)
    }

    pub fn totals(&self, small_business: bool) -> Totals {
        let mut t = Totals::default();
        for line in &self.lines {
            let net = line.net();
            let rate = if small_business || self.vat_case != VatCase::Inland { 0 } else { line.vat };
            t.by_rate.entry(rate).or_default().0 += net;
        }
        for (rate, (net, tax)) in t.by_rate.iter_mut() {
            *tax = (*net as f64 * *rate as f64 / 100.0).round() as Cents;
            t.net += *net;
            t.tax += *tax;
        }
        t.gross = t.net + t.tax;
        let sign = if self.kind == DocKind::Gutschrift { -1 } else { 1 };
        if sign < 0 {
            t.net = -t.net;
            t.tax = -t.tax;
            t.gross = -t.gross;
            for v in t.by_rate.values_mut() {
                v.0 = -v.0;
                v.1 = -v.1;
            }
        }
        t
    }

    /// Takes over a payment condition.
    pub fn apply_term(&mut self, term: &PaymentTerm) {
        self.term_name = term.name.clone();
        self.discounts = term.discounts.clone();
        self.discounts.sort_by_key(|s| s.days);
        self.due_days = term.net_days;
    }

    /// Payment deadlines in order: (last day, discount %, amount to pay).
    pub fn payment_schedule(&self, small_business: bool) -> Vec<(NaiveDate, f64, Cents)> {
        let gross = self.totals(small_business).gross;
        let mut out: Vec<(NaiveDate, f64, Cents)> = self
            .discounts
            .iter()
            .filter(|s| s.percent > 0.0 && s.days < self.due_days)
            .map(|s| {
                let deduct = (gross as f64 * s.percent / 100.0).round() as Cents;
                (self.date + Duration::days(s.days as i64), s.percent, gross - deduct)
            })
            .collect();
        out.sort_by_key(|x| x.0);
        out.push((self.due_date(), 0.0, gross));
        out
    }

    /// The best discount still allowed when paying on `day`.
    pub fn discount_on(&self, day: NaiveDate) -> f64 {
        self.discounts
            .iter()
            .filter(|s| s.days < self.due_days && day <= self.date + Duration::days(s.days as i64))
            .map(|s| s.percent)
            .fold(0.0, f64::max)
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.kind == DocKind::Rechnung && self.status == DocStatus::Offen && self.due_date() < today
    }

    /// The currency code; euro when none is set.
    pub fn currency(&self) -> &str {
        if self.currency.trim().is_empty() { "EUR" } else { self.currency.trim() }
    }

    pub fn is_foreign(&self) -> bool {
        self.currency() != "EUR"
    }

    /// An amount of this document in euros (at the document's exchange rate).
    pub fn to_eur(&self, c: Cents) -> Cents {
        if !self.is_foreign() || self.exchange_rate <= 0.0 { c } else { (c as f64 / self.exchange_rate).round() as Cents }
    }

    /// Paid so far, including accepted discounts (same sign as the total).
    pub fn paid_total(&self) -> Cents {
        self.payments.iter().map(|p| p.amount + p.discount).sum()
    }

    /// What is still to be paid (same sign as the total).
    pub fn open_amount(&self, small_business: bool) -> Cents {
        match self.status {
            DocStatus::Bezahlt | DocStatus::Storniert | DocStatus::Entwurf => 0,
            _ if !self.kind.is_billing() => 0,
            _ => self.totals(small_business).gross - self.paid_total(),
        }
    }

    /// The payments with their dates; older documents paid in one go have none recorded.
    pub fn effective_payments(&self, small_business: bool) -> Vec<Payment> {
        if !self.payments.is_empty() || self.status != DocStatus::Bezahlt {
            return self.payments.clone();
        }
        let gross = self.totals(small_business).gross;
        let amount = (gross as f64 * (1.0 - self.paid_discount_pct / 100.0)).round() as Cents;
        vec![Payment {
            date: self.paid_date.unwrap_or(self.date),
            amount,
            discount: gross - amount,
            method: "Bank".into(),
            bank_tx: None,
            eur: None,
        }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookingKind {
    Einnahme,
    Ausgabe,
}

impl BookingKind {
    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            BookingKind::Einnahme => "Einnahme",
            BookingKind::Ausgabe => "Ausgabe",
        }
    }

    /// Name in the interface language.
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Booking {
    pub id: u64,
    pub date: NaiveDate,
    pub kind: BookingKind,
    pub category: String,
    pub text: String,
    pub partner: String,
    pub receipt: String,
    /// Gross amount (incl. VAT), always positive; the kind gives the direction.
    pub gross: Cents,
    pub vat: u8,
    pub doc_id: Option<u64>,
    pub payment: String,
    /// The incoming receipt this expense was booked from.
    pub receipt_id: Option<u64>,
    /// The bank statement line this entry was created from.
    pub bank_tx: Option<u64>,
    /// Service from a foreign business where we owe the VAT (§ 13b UStG): the amount
    /// is net; the VAT is owed and deducted at the same time.
    pub reverse_charge: bool,
}

impl Default for Booking {
    fn default() -> Self {
        Self {
            id: 0,
            date: today(),
            kind: BookingKind::Ausgabe,
            category: EXPENSE_CATEGORIES[9].into(),
            text: String::new(),
            partner: String::new(),
            receipt: String::new(),
            gross: 0,
            vat: 19,
            doc_id: None,
            payment: "Bank".into(),
            receipt_id: None,
            bank_tx: None,
            reverse_charge: false,
        }
    }
}

impl Booking {
    pub fn net(&self) -> Cents {
        if self.reverse_charge {
            return self.gross;
        }
        (self.gross as f64 * 100.0 / (100.0 + self.vat as f64)).round() as Cents
    }
    /// VAT owed and deducted for a reverse-charge expense.
    pub fn reverse_charge_tax(&self) -> Cents {
        if self.reverse_charge { (self.gross as f64 * self.vat as f64 / 100.0).round() as Cents } else { 0 }
    }
    /// Whether the entry is managed elsewhere (invoice, inbox, bank) and cannot be edited here.
    pub fn is_linked(&self) -> bool {
        self.doc_id.is_some() || self.receipt_id.is_some() || self.bank_tx.is_some()
    }
    pub fn tax(&self) -> Cents {
        self.gross - self.net()
    }
    /// Signed gross: positive for income, negative for expenses.
    pub fn signed(&self) -> Cents {
        match self.kind {
            BookingKind::Einnahme => self.gross,
            BookingKind::Ausgabe => -self.gross,
        }
    }
}

pub const INCOME_CATEGORIES: [&str; 9] = [
    "8400 Erlöse 19 % USt",
    "8300 Erlöse 7 % USt",
    "8195 Erlöse Kleinunternehmer",
    "8120 Steuerfreie Umsätze",
    "2700 Sonstige Erträge",
    "1890 Privateinlage",
    "8125 Steuerfreie innergemeinschaftliche Lieferungen",
    "8336 Erlöse EU-Leistungen (Steuerschuld beim Kunden)",
    "8338 Erlöse Leistungen außerhalb der EU",
];

/// Purchases of fixed assets: not an expense at once, but written off over the years.
pub const ASSET_CATEGORY: &str = "0400 Anlagegut (wird abgeschrieben)";

pub const EXPENSE_CATEGORIES: [&str; 18] = [
    "3400 Wareneingang",
    "4120 Löhne und Gehälter",
    "4210 Miete",
    "4240 Strom, Gas, Wasser",
    "4360 Versicherungen",
    "4530 Kfz-Kosten",
    "4600 Werbekosten",
    "4650 Bewirtung",
    "4670 Reisekosten",
    "4930 Bürobedarf",
    "4920 Telefon und Internet",
    "4940 Zeitschriften, Bücher",
    "4950 Rechts- und Beratungskosten",
    "4970 Kosten des Geldverkehrs",
    "4855 Geringwertige Wirtschaftsgüter",
    "1800 Privatentnahme",
    "1780 Umsatzsteuer-Vorauszahlungen",
    ASSET_CATEGORY,
];

pub fn is_private(category: &str) -> bool {
    category.starts_with("1800") || category.starts_with("1890")
}

pub fn income_category_for_rate(rate: u8, small_business: bool) -> &'static str {
    if small_business {
        return INCOME_CATEGORIES[2];
    }
    match rate {
        19 => INCOME_CATEGORIES[0],
        7 => INCOME_CATEGORIES[1],
        _ => INCOME_CATEGORIES[3],
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiptStatus {
    #[default]
    Neu,
    Gebucht,
    Verworfen,
}

impl ReceiptStatus {
    pub fn name(self) -> &'static str {
        match self {
            ReceiptStatus::Neu => "Neu",
            ReceiptStatus::Gebucht => "Gebucht",
            ReceiptStatus::Verworfen => "Verworfen",
        }
    }
}

/// The tax of one VAT rate on an incoming invoice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReceiptVat {
    pub rate: u8,
    pub net: Cents,
    pub tax: Cents,
}

/// An incoming receipt: the stored original file plus what was recognised in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Receipt {
    pub id: u64,
    /// When it came in, e.g. "07.11.2026 14:03".
    pub received: String,
    /// "Hochgeladen von …" or "E-Mail von …".
    pub source: String,
    pub file_name: String,
    pub size: u64,
    /// Fingerprint of the original file; proves it was not changed.
    pub sha256: String,
    pub status: ReceiptStatus,
    /// "XRechnung (UBL)", "ZUGFeRD / Factur-X", "Textebene", "Texterkennung" or empty.
    pub recognized_by: String,
    /// Why nothing could be recognised, if so.
    pub hint: String,
    pub supplier: String,
    pub contact_id: Option<u64>,
    pub number: String,
    pub date: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    pub gross: Cents,
    /// VAT rate when the receipt has only one.
    pub vat_rate: u8,
    /// Several VAT rates (from e-invoices).
    pub vat: Vec<ReceiptVat>,
    pub iban: String,
    pub vat_id: String,
    pub category: String,
    pub payment: String,
    pub note: String,
    /// The recognised text (shortened), for searching and checking.
    pub text: String,
    pub booking_ids: Vec<u64>,
}

/// One line of the change log: who changed what, and when.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LogEntry {
    pub time: String,
    pub user: String,
    pub action: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub company: Company,
    pub contacts: Vec<Contact>,
    pub articles: Vec<Article>,
    pub documents: Vec<Document>,
    pub bookings: Vec<Booking>,
    pub log: Vec<LogEntry>,
    /// Incoming receipts (supplier invoices, scans, photos) waiting to be booked or booked.
    pub receipts: Vec<Receipt>,
    /// Imported bank statement lines.
    pub bank: Vec<BankTx>,
    /// Fixed assets that are written off over several years.
    pub assets: Vec<Asset>,
    /// Invoices that are written again every few months.
    pub recurring: Vec<Recurring>,
    /// Goods in and out of stock.
    pub stock_moves: Vec<StockMove>,
    /// Manual entries of the double-entry bookkeeping (Umbuchungen, opening balances).
    pub journal: Vec<JournalEntry>,
    /// Days whose cash book is closed (no more changes up to and including the date).
    pub cash_closings: Vec<CashClosing>,
    pub projects: Vec<Project>,
    pub times: Vec<TimeEntry>,
    pub next_id: u64,
}

/// A project or job; hours are recorded on it and billed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: u64,
    pub number: String,
    pub name: String,
    pub contact_id: Option<u64>,
    /// Net price per hour.
    pub rate: Cents,
    /// Planned hours (0 = no budget).
    pub budget_hours: f64,
    pub active: bool,
    pub note: String,
}

impl Default for Project {
    fn default() -> Self {
        Self { id: 0, number: String::new(), name: String::new(), contact_id: None, rate: 9500, budget_hours: 0.0, active: true, note: String::new() }
    }
}

/// Time worked on a project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeEntry {
    pub id: u64,
    pub project_id: u64,
    pub date: NaiveDate,
    pub user: String,
    pub hours: f64,
    pub text: String,
    /// Billed with this invoice.
    pub invoice_id: Option<u64>,
}

impl Default for TimeEntry {
    fn default() -> Self {
        Self { id: 0, project_id: 0, date: today(), user: String::new(), hours: 1.0, text: String::new(), invoice_id: None }
    }
}

/// One line of a bank statement.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BankTx {
    pub id: u64,
    /// Own account (IBAN) the line belongs to.
    pub account: String,
    pub date: NaiveDate,
    /// Positive for money received, negative for money paid.
    pub amount: Cents,
    /// The other party.
    pub name: String,
    pub iban: String,
    pub purpose: String,
    /// Fingerprint used to skip lines imported before.
    pub reference: String,
    pub status: BankStatus,
    pub link: Option<BankLink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BankStatus {
    #[default]
    Offen,
    Zugeordnet,
    Ignoriert,
}

impl BankStatus {
    pub fn name(self) -> &'static str {
        match self {
            BankStatus::Offen => "Offen",
            BankStatus::Zugeordnet => "Zugeordnet",
            BankStatus::Ignoriert => "Ignoriert",
        }
    }
}

/// What a bank line was matched with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BankLink {
    Document(u64),
    Receipt(u64),
    Booking(u64),
}

/// How a bank line is to be assigned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BankAssign {
    /// Payment of an invoice or credit note; `discount` accepts an early-payment discount.
    Document { id: u64, discount: bool },
    /// Payment of an incoming receipt (books it if not done yet).
    Receipt(u64),
    /// A new income or expense entry.
    Booking { category: String, vat: u8, text: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfaMethod {
    /// Straight-line over the useful life, by months in the first year.
    #[default]
    Linear,
    /// Low-value asset (up to 800 € net): fully written off in the year of purchase.
    Gwg,
    /// Pool for assets of 250–1000 € net: one fifth per year (§ 6 Abs. 2a EStG).
    Sammelposten,
}

/// A fixed asset (machine, car, computer …) in the asset register.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    pub id: u64,
    pub number: String,
    pub name: String,
    /// Asset account, e.g. "0420 Büroeinrichtung".
    pub account: String,
    pub date: NaiveDate,
    /// Net purchase cost.
    pub cost: Cents,
    /// Useful life in years (official AfA table).
    pub years: u32,
    pub method: AfaMethod,
    /// Sold or scrapped on this date; the rest value is written off then.
    pub disposed: Option<NaiveDate>,
    pub note: String,
    /// The purchase entry, if it was booked in OpenKontor.
    pub booking_id: Option<u64>,
}

impl Default for Asset {
    fn default() -> Self {
        Self {
            id: 0,
            number: String::new(),
            name: String::new(),
            account: ASSET_ACCOUNTS[0].into(),
            date: today(),
            cost: 0,
            years: 3,
            method: AfaMethod::Linear,
            disposed: None,
            note: String::new(),
            booking_id: None,
        }
    }
}

/// Asset accounts (SKR 03) to choose from.
pub const ASSET_ACCOUNTS: [&str; 7] = [
    "0420 Büroeinrichtung",
    "0490 Sonstige Betriebs- und Geschäftsausstattung",
    "0400 Betriebsausstattung",
    "0320 Pkw",
    "0210 Maschinen",
    "0027 EDV-Software",
    "0480 Geringwertige Wirtschaftsgüter",
];

/// An invoice written again and again, e.g. a maintenance contract.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Recurring {
    pub id: u64,
    pub name: String,
    /// The invoice to copy (contact, lines, texts, payment terms).
    pub template: Document,
    pub every_months: u32,
    /// Date of the next invoice.
    pub next_date: NaiveDate,
    pub until: Option<NaiveDate>,
    pub active: bool,
    /// Finish the invoice right away (gets its number) instead of leaving a draft.
    pub finalize: bool,
    /// Invoices created so far.
    pub created: Vec<u64>,
}

impl Default for Recurring {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            template: Document::default(),
            every_months: 1,
            next_date: today(),
            until: None,
            active: true,
            finalize: false,
            created: Vec::new(),
        }
    }
}

/// Goods in (positive) or out (negative) of stock.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StockMove {
    pub id: u64,
    pub date: NaiveDate,
    pub article_id: u64,
    pub qty: f64,
    pub doc_id: Option<u64>,
    pub note: String,
}

/// A manual entry of the double-entry bookkeeping: debit account to credit account.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JournalEntry {
    pub id: u64,
    pub date: NaiveDate,
    /// Account numbers in the company's chart of accounts.
    pub debit: String,
    pub credit: String,
    pub amount: Cents,
    pub text: String,
    pub receipt: String,
}

impl Default for JournalEntry {
    fn default() -> Self {
        Self {
            id: 0,
            date: today(),
            debit: String::new(),
            credit: String::new(),
            amount: 0,
            text: String::new(),
            receipt: String::new(),
        }
    }
}

/// The closing of the cash book for a day.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CashClosing {
    pub date: NaiveDate,
    /// Balance by the cash book.
    pub balance: Cents,
    /// Cash actually counted, if done.
    pub counted: Option<Cents>,
    pub user: String,
    pub time: String,
}

pub const CASH: &str = "Kasse";

/// Every change to the data is one of these. The same operations run locally
/// (single workplace) or are sent to the server, which applies them for everyone.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Op {
    SaveContact(Contact),
    DeleteContact(u64),
    SaveArticle(Article),
    DeleteArticle(u64),
    /// Only drafts can be changed; finished documents stay as they were sent.
    SaveDocument(Document),
    DeleteDocument(u64),
    /// Gives the draft its running number and makes it final.
    FinalizeDocument(u64),
    SetDocStatus(u64, DocStatus),
    MarkPaid(u64, NaiveDate),
    /// Paid on a date with an early-payment discount (percent) deducted.
    MarkPaidWithDiscount(u64, NaiveDate, f64),
    /// Notes that an e-mail about a document was sent.
    RecordEmail(u64, EmailRecord),
    /// Notes a payment reminder for an invoice (sent by e-mail or by post).
    RecordReminder(u64, ReminderRecord),
    MarkUnpaid(u64),
    /// Makes a draft invoice from an offer.
    ConvertOffer(u64),
    /// Makes a draft credit note that cancels an invoice.
    CreateCredit(u64),
    DuplicateDocument(u64),
    SaveBooking(Booking),
    DeleteBooking(u64),
    SaveCompany(Company),
    ReplaceAll(Box<Data>),
    /// A new incoming receipt (its file is stored separately and never changed).
    AddReceipt(Box<Receipt>),
    /// Corrects the recognised details of a receipt that is not booked yet.
    SaveReceipt(Box<Receipt>),
    /// Books a receipt as expense(s), one entry per VAT rate.
    BookReceipt(u64),
    /// Takes back the expense entries of a receipt.
    UnbookReceipt(u64),
    /// Marks a receipt as not relevant (or brings it back); the file is kept.
    DiscardReceipt(u64),
    /// Removes a receipt that was added by mistake. Only possible while it is not booked.
    DeleteReceipt(u64),
    /// Records a (part) payment of an invoice or credit note.
    AddPayment(u64, Payment),
    /// Makes a follow-up draft: offer → order confirmation → delivery note → invoice.
    ConvertDocument(u64, DocKind),
    /// Makes an advance invoice (Abschlagsrechnung) for a percentage of an offer or order.
    CreateAdvanceInvoice(u64, f64),
    /// Makes the final invoice of an offer or order, minus its advance invoices.
    CreateFinalInvoice(u64),
    /// Marks an order to a supplier as received and puts the goods into stock.
    ReceivePurchase(u64),
    /// Corrects the stock of an article (e.g. after counting): article, change, note.
    StockCorrection(u64, f64, String),
    SaveRecurring(Box<Recurring>),
    DeleteRecurring(u64),
    /// Writes all recurring invoices due up to this day.
    CreateDueRecurring(NaiveDate),
    /// Closes the cash book up to and including a day; optionally with the counted cash.
    CloseCash(NaiveDate, Option<Cents>),
    SaveAsset(Asset),
    DeleteAsset(u64),
    /// Adds bank statement lines; lines imported before are skipped.
    ImportBank(Vec<BankTx>),
    AssignBank(u64, BankAssign),
    /// Takes back the assignment of a bank line (and what it booked).
    UnassignBank(u64),
    /// Marks a bank line as not relevant (or brings it back).
    IgnoreBank(u64),
    SaveJournal(JournalEntry),
    DeleteJournal(u64),
    /// Takes over contacts from another program; existing numbers are skipped.
    ImportContacts(Vec<Contact>),
    ImportArticles(Vec<Article>),
    /// Open invoices from another program, with their numbers and amounts.
    ImportOpenItems(Vec<Document>),
    SaveProject(Project),
    DeleteProject(u64),
    SaveTime(TimeEntry),
    DeleteTime(u64),
    /// Makes a draft invoice from time entries of a project.
    BillTimes(u64, Vec<u64>),
}

impl Op {
    /// Changes only an administrator may make.
    pub fn admin_only(&self) -> bool {
        matches!(self, Op::ReplaceAll(_))
    }

    /// The kind of work a change belongs to, for finer user rights.
    pub fn area(&self) -> OpArea {
        match self {
            Op::SaveContact(_)
            | Op::DeleteContact(_)
            | Op::SaveArticle(_)
            | Op::DeleteArticle(_)
            | Op::SaveDocument(_)
            | Op::DeleteDocument(_)
            | Op::FinalizeDocument(_)
            | Op::SetDocStatus(..)
            | Op::RecordEmail(..)
            | Op::RecordReminder(..)
            | Op::ConvertOffer(_)
            | Op::CreateCredit(_)
            | Op::DuplicateDocument(_)
            | Op::ConvertDocument(..)
            | Op::CreateAdvanceInvoice(..)
            | Op::CreateFinalInvoice(_)
            | Op::ReceivePurchase(_)
            | Op::StockCorrection(..)
            | Op::SaveRecurring(_)
            | Op::DeleteRecurring(_)
            | Op::CreateDueRecurring(_)
            | Op::SaveProject(_)
            | Op::DeleteProject(_)
            | Op::SaveTime(_)
            | Op::DeleteTime(_)
            | Op::BillTimes(..) => OpArea::Sales,
            Op::ImportContacts(_) | Op::ImportArticles(_) | Op::ImportOpenItems(_) => OpArea::Settings,
            Op::AddReceipt(_) | Op::SaveReceipt(_) | Op::DiscardReceipt(_) | Op::DeleteReceipt(_) => OpArea::Receipts,
            Op::SaveCompany(_) | Op::ReplaceAll(_) => OpArea::Settings,
            _ => OpArea::Accounting,
        }
    }
}

/// Areas of work, for user rights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpArea {
    /// Contacts, articles, offers, invoices, stock.
    Sales,
    /// Collecting incoming receipts.
    Receipts,
    /// Payments, bookings, bank, cash book, assets.
    Accounting,
    /// Company settings and restoring backups.
    Settings,
}

impl Data {
    /// Applies a change and records it in the log. Returns the id of a created or saved item.
    pub fn apply(&mut self, op: Op, user: &str) -> Result<Option<u64>, String> {
        let (id, action) = self.apply_inner(op)?;
        self.log.push(LogEntry {
            time: chrono::Local::now().format("%d.%m.%Y %H:%M:%S").to_string(),
            user: user.to_string(),
            action,
        });
        if self.log.len() > 5000 {
            let drop = self.log.len() - 5000;
            self.log.drain(0..drop);
        }
        Ok(id)
    }

    fn doc_index(&self, id: u64) -> Result<usize, String> {
        self.documents
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| "Beleg nicht gefunden.".to_string())
    }

    fn apply_inner(&mut self, op: Op) -> Result<(Option<u64>, String), String> {
        match op {
            Op::SaveContact(mut c) => {
                if c.id == 0 {
                    c.id = self.new_id();
                    if c.number.trim().is_empty() {
                        c.number = self.next_contact_number();
                    }
                    let msg = format!("Kontakt {} angelegt", c.display_name());
                    let id = c.id;
                    self.contacts.push(c);
                    Ok((Some(id), msg))
                } else {
                    let slot = self
                        .contacts
                        .iter_mut()
                        .find(|x| x.id == c.id)
                        .ok_or("Kontakt nicht gefunden.")?;
                    let msg = format!("Kontakt {} geändert", c.display_name());
                    let id = c.id;
                    *slot = c;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteContact(id) => {
                if self.documents.iter().any(|d| d.contact_id == Some(id))
                    || self.recurring.iter().any(|r| r.template.contact_id == Some(id))
                {
                    return Err("Der Kontakt wird in Belegen verwendet und kann nicht gelöscht werden.".into());
                }
                let name = self.contact_name(Some(id));
                self.contacts.retain(|c| c.id != id);
                Ok((None, format!("Kontakt {name} gelöscht")))
            }
            Op::SaveArticle(mut a) => {
                if a.id == 0 {
                    a.id = self.new_id();
                    if a.number.trim().is_empty() {
                        a.number = self.next_article_number();
                    }
                    let msg = format!("Artikel {} angelegt", a.name);
                    let id = a.id;
                    self.articles.push(a);
                    Ok((Some(id), msg))
                } else {
                    let slot = self
                        .articles
                        .iter_mut()
                        .find(|x| x.id == a.id)
                        .ok_or("Artikel nicht gefunden.")?;
                    let msg = format!("Artikel {} geändert", a.name);
                    let id = a.id;
                    *slot = a;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteArticle(id) => {
                let name = self
                    .articles
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                self.articles.retain(|a| a.id != id);
                Ok((None, format!("Artikel {name} gelöscht")))
            }
            Op::SaveDocument(mut d) => {
                if d.id == 0 {
                    d.id = self.new_id();
                    d.status = DocStatus::Entwurf;
                    d.number.clear();
                    let msg = format!("{}-Entwurf angelegt", d.kind.name());
                    let id = d.id;
                    self.documents.push(d);
                    Ok((Some(id), msg))
                } else {
                    let i = self.doc_index(d.id)?;
                    if self.documents[i].status != DocStatus::Entwurf {
                        return Err("Fertige Belege können nicht mehr geändert werden. Erstellen Sie eine Gutschrift oder eine Kopie.".into());
                    }
                    d.status = DocStatus::Entwurf;
                    d.number.clear();
                    let msg = format!("{}-Entwurf geändert", d.kind.name());
                    let id = d.id;
                    self.documents[i] = d;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteDocument(id) => {
                let i = self.doc_index(id)?;
                if self.documents[i].status != DocStatus::Entwurf {
                    return Err("Nur Entwürfe können gelöscht werden. Fertige Belege bleiben zur Aufbewahrung erhalten.".into());
                }
                let kind = self.documents[i].kind;
                self.documents.remove(i);
                // Hours billed with this draft can be billed again.
                for t in self.times.iter_mut().filter(|t| t.invoice_id == Some(id)) {
                    t.invoice_id = None;
                }
                Ok((None, format!("{}-Entwurf gelöscht", kind.name())))
            }
            Op::FinalizeDocument(id) => {
                let i = self.doc_index(id)?;
                let d = &self.documents[i];
                if d.status != DocStatus::Entwurf {
                    return Err("Der Beleg ist bereits fertig.".into());
                }
                if d.contact_id.is_none() {
                    return Err("Bitte zuerst einen Kontakt auswählen.".into());
                }
                if d.lines.iter().all(|l| l.text.trim().is_empty() && l.price_net == 0) {
                    return Err("Der Beleg hat keine Positionen.".into());
                }
                if d.kind.is_billing()
                    && matches!(d.vat_case, VatCase::IgLieferung | VatCase::ReverseCharge)
                    && (self.contact(d.contact_id).is_none_or(|c| c.vat_id.trim().is_empty())
                        || self.company.vat_id.trim().is_empty())
                {
                    return Err("Für EU-Geschäfte ohne Umsatzsteuer braucht die Rechnung die USt-IdNr. des Kunden und Ihre eigene.".into());
                }
                let (kind, year) = (d.kind, d.date.year());
                let number = self.next_doc_number(kind, year);
                let d = &mut self.documents[i];
                d.number = number.clone();
                d.status = DocStatus::Offen;
                if self.books_stock_out(&self.documents[i]) {
                    self.move_stock(i, -1.0);
                }
                Ok((Some(id), format!("{} {number} fertiggestellt", kind.name())))
            }
            Op::SetDocStatus(id, status) => {
                let i = self.doc_index(id)?;
                if self.documents[i].status == DocStatus::Entwurf {
                    return Err("Bitte den Beleg zuerst fertigstellen.".into());
                }
                if status == DocStatus::Storniert {
                    if !self.documents[i].payments.is_empty() {
                        return Err("Zu diesem Beleg gibt es Zahlungen. Bitte zuerst die Zahlungen zurücknehmen.".into());
                    }
                    self.unbook_payment(id);
                    // Goods of a cancelled document go back into stock.
                    self.stock_moves.retain(|m| m.doc_id != Some(id));
                }
                let d = &mut self.documents[i];
                d.status = status;
                Ok((Some(id), format!("{} {} → {}", d.kind.name(), d.number, status.name())))
            }
            Op::MarkPaid(id, date) => self.mark_paid(id, date, 0.0),
            Op::MarkPaidWithDiscount(id, date, pct) => {
                if !(0.0..=100.0).contains(&pct) {
                    return Err("Ungültiger Skonto-Satz.".into());
                }
                self.mark_paid(id, date, pct)
            }
            Op::RecordEmail(id, record) => {
                let i = self.doc_index(id)?;
                let d = &mut self.documents[i];
                let msg = format!("{} {} per E-Mail an {} gesendet", record.kind, d.number, record.to);
                d.emails.push(record);
                Ok((Some(id), msg))
            }
            Op::RecordReminder(id, reminder) => {
                let i = self.doc_index(id)?;
                let d = &mut self.documents[i];
                if d.kind != DocKind::Rechnung || d.status != DocStatus::Offen {
                    return Err("Mahnungen gibt es nur für offene Rechnungen.".into());
                }
                let msg = format!("{} zu Rechnung {} erstellt", reminder_title(reminder.level), d.number);
                d.reminders.push(reminder);
                Ok((Some(id), msg))
            }
            Op::MarkUnpaid(id) => {
                let i = self.doc_index(id)?;
                let small = self.company.small_business;
                for p in self.documents[i].effective_payments(small) {
                    self.check_cash(&p.method, p.date)?;
                }
                let d = &mut self.documents[i];
                d.status = DocStatus::Offen;
                d.paid_date = None;
                d.paid_discount_pct = 0.0;
                d.payments.clear();
                let msg = format!("Zahlung zu {} {} zurückgenommen", d.kind.name(), d.number);
                self.unbook_payment(id);
                for tx in self.bank.iter_mut().filter(|t| t.link == Some(BankLink::Document(id))) {
                    tx.status = BankStatus::Offen;
                    tx.link = None;
                }
                Ok((Some(id), msg))
            }
            Op::AddPayment(id, p) => self.add_payment(id, p),
            Op::ConvertDocument(id, target) => self.convert_document(id, target),
            Op::CreateAdvanceInvoice(id, pct) => self.create_advance_invoice(id, pct),
            Op::CreateFinalInvoice(id) => self.create_final_invoice(id),
            Op::ReceivePurchase(id) => {
                let i = self.doc_index(id)?;
                let d = &self.documents[i];
                if d.kind != DocKind::Bestellung || d.status != DocStatus::Offen {
                    return Err("Nur offene Bestellungen können als erhalten gebucht werden.".into());
                }
                let number = d.number.clone();
                self.move_stock(i, 1.0);
                self.documents[i].status = DocStatus::Erledigt;
                Ok((Some(id), format!("Bestellung {number} erhalten, Ware eingelagert")))
            }
            Op::StockCorrection(article_id, qty, note) => {
                let a = self.articles.iter().find(|a| a.id == article_id).ok_or("Artikel nicht gefunden.")?;
                if !a.track_stock {
                    return Err("Für diesen Artikel wird kein Lagerbestand geführt.".into());
                }
                if qty == 0.0 {
                    return Err("Bitte eine Menge angeben.".into());
                }
                let msg = format!("Lagerbestand {} um {} korrigiert ({note})", a.name, fmt_qty(qty));
                let mid = self.new_id();
                self.stock_moves.push(StockMove { id: mid, date: today(), article_id, qty, doc_id: None, note });
                Ok((Some(mid), msg))
            }
            Op::SaveRecurring(r) => {
                let mut r = *r;
                if r.template.contact_id.is_none() {
                    return Err("Bitte zuerst einen Kontakt auswählen.".into());
                }
                if r.every_months == 0 {
                    return Err("Bitte ein Intervall von mindestens einem Monat wählen.".into());
                }
                r.template.kind = DocKind::Rechnung;
                r.template.status = DocStatus::Entwurf;
                r.template.number.clear();
                r.template.payments.clear();
                if r.name.trim().is_empty() {
                    r.name = format!("{} – {}", self.contact_name(r.template.contact_id), r.template.title);
                }
                let msg = format!("Wiederkehrende Rechnung „{}“ gespeichert", r.name);
                if r.id == 0 {
                    r.id = self.new_id();
                    let id = r.id;
                    self.recurring.push(r);
                    Ok((Some(id), msg))
                } else {
                    let slot = self.recurring.iter_mut().find(|x| x.id == r.id).ok_or("Nicht gefunden.")?;
                    let id = r.id;
                    r.created = std::mem::take(&mut slot.created);
                    *slot = r;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteRecurring(id) => {
                let name = self.recurring.iter().find(|r| r.id == id).map(|r| r.name.clone()).ok_or("Nicht gefunden.")?;
                self.recurring.retain(|r| r.id != id);
                Ok((None, format!("Wiederkehrende Rechnung „{name}“ beendet")))
            }
            Op::CreateDueRecurring(day) => self.create_due_recurring(day),
            Op::CloseCash(day, counted) => {
                if day > today() {
                    return Err("Die Kasse kann nur bis heute abgeschlossen werden.".into());
                }
                if self.cash_closings.last().is_some_and(|c| c.date >= day) {
                    return Err("Dieser Tag ist bereits abgeschlossen.".into());
                }
                if let Some((d, bal)) = self.cash_book().into_iter().filter(|(b, _)| b.date <= day).map(|(b, bal)| (b.date, bal)).find(|(_, bal)| *bal < 0) {
                    return Err(format!(
                        "Der Kassenbestand wäre am {} negativ ({}). Bitte die Kassenbuchungen prüfen.",
                        fmt_date(d),
                        fmt_money(bal)
                    ));
                }
                let balance = self.cash_balance(day);
                let mut msg = format!("Kasse bis {} abgeschlossen, Bestand {}", fmt_date(day), fmt_money(balance));
                if let Some(c) = counted
                    && c != balance
                {
                    msg.push_str(&format!(" (gezählt {}, Differenz {})", fmt_money(c), fmt_money(c - balance)));
                }
                self.cash_closings.push(CashClosing {
                    date: day,
                    balance,
                    counted,
                    user: String::new(),
                    time: chrono::Local::now().format("%d.%m.%Y %H:%M").to_string(),
                });
                Ok((None, msg))
            }
            Op::SaveAsset(mut a) => {
                if a.name.trim().is_empty() {
                    return Err("Bitte eine Bezeichnung eingeben.".into());
                }
                if a.cost <= 0 {
                    return Err("Bitte die Anschaffungskosten (netto) eintragen.".into());
                }
                if a.method == AfaMethod::Linear && a.years == 0 {
                    return Err("Bitte die Nutzungsdauer in Jahren angeben.".into());
                }
                if a.disposed.is_some_and(|d| d < a.date) {
                    return Err("Das Abgangsdatum liegt vor der Anschaffung.".into());
                }
                let msg = format!("Anlagegut „{}“ gespeichert", a.name);
                if a.id == 0 {
                    a.id = self.new_id();
                    if a.number.trim().is_empty() {
                        a.number = format!("AV-{:04}", self.assets.len() + 1);
                    }
                    let id = a.id;
                    self.assets.push(a);
                    Ok((Some(id), msg))
                } else {
                    let slot = self.assets.iter_mut().find(|x| x.id == a.id).ok_or("Anlagegut nicht gefunden.")?;
                    let id = a.id;
                    *slot = a;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteAsset(id) => {
                let name = self.assets.iter().find(|a| a.id == id).map(|a| a.name.clone()).ok_or("Anlagegut nicht gefunden.")?;
                self.assets.retain(|a| a.id != id);
                Ok((None, format!("Anlagegut „{name}“ gelöscht")))
            }
            Op::ImportBank(lines) => {
                let (mut added, mut known) = (0, 0);
                for mut tx in lines {
                    if self.bank.iter().any(|t| t.reference == tx.reference) {
                        known += 1;
                        continue;
                    }
                    tx.id = self.new_id();
                    tx.status = BankStatus::Offen;
                    tx.link = None;
                    self.bank.push(tx);
                    added += 1;
                }
                self.bank.sort_by_key(|t| (t.date, t.id));
                if added == 0 && known > 0 {
                    return Err("Alle Umsätze dieser Datei sind schon eingelesen.".into());
                }
                Ok((None, format!("{added} Bankumsätze eingelesen ({known} schon vorhanden)")))
            }
            Op::AssignBank(tx_id, how) => self.assign_bank(tx_id, how),
            Op::UnassignBank(tx_id) => {
                let ti = self.bank.iter().position(|t| t.id == tx_id).ok_or("Bankumsatz nicht gefunden.")?;
                let Some(link) = self.bank[ti].link else {
                    return Err("Der Umsatz ist nicht zugeordnet.".into());
                };
                match link {
                    BankLink::Document(id) => {
                        let i = self.doc_index(id)?;
                        let d = &mut self.documents[i];
                        d.payments.retain(|p| p.bank_tx != Some(tx_id));
                        if d.status == DocStatus::Bezahlt {
                            d.status = DocStatus::Offen;
                            d.paid_date = None;
                            d.paid_discount_pct = 0.0;
                        }
                        self.book_payment(id, today());
                    }
                    BankLink::Receipt(rid) => {
                        let booked_here = self.bookings.iter().any(|b| b.receipt_id == Some(rid) && b.bank_tx == Some(tx_id));
                        if booked_here {
                            self.apply_inner(Op::UnbookReceipt(rid))?;
                        }
                    }
                    BankLink::Booking(bid) => self.bookings.retain(|b| b.id != bid),
                }
                let tx = &mut self.bank[ti];
                tx.status = BankStatus::Offen;
                tx.link = None;
                Ok((Some(tx_id), format!("Zuordnung des Bankumsatzes „{}“ zurückgenommen", tx.name)))
            }
            Op::IgnoreBank(tx_id) => {
                let tx = self.bank.iter_mut().find(|t| t.id == tx_id).ok_or("Bankumsatz nicht gefunden.")?;
                tx.status = match tx.status {
                    BankStatus::Offen => BankStatus::Ignoriert,
                    BankStatus::Ignoriert => BankStatus::Offen,
                    BankStatus::Zugeordnet => return Err("Der Umsatz ist zugeordnet.".into()),
                };
                Ok((Some(tx_id), format!("Bankumsatz „{}“: {}", tx.name, tx.status.name())))
            }
            Op::SaveJournal(mut e) => {
                if e.debit.trim().is_empty() || e.credit.trim().is_empty() {
                    return Err("Bitte Soll- und Habenkonto angeben.".into());
                }
                if e.debit.trim() == e.credit.trim() {
                    return Err("Soll- und Habenkonto müssen verschieden sein.".into());
                }
                if e.amount <= 0 {
                    return Err("Bitte einen Betrag größer als 0 eingeben.".into());
                }
                let msg = format!("Umbuchung {} an {} über {} gespeichert", e.debit, e.credit, fmt_money(e.amount));
                if e.id == 0 {
                    e.id = self.new_id();
                    let id = e.id;
                    self.journal.push(e);
                    self.journal.sort_by_key(|e| e.date);
                    Ok((Some(id), msg))
                } else {
                    let slot = self.journal.iter_mut().find(|x| x.id == e.id).ok_or("Umbuchung nicht gefunden.")?;
                    let id = e.id;
                    *slot = e;
                    self.journal.sort_by_key(|e| e.date);
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteJournal(id) => {
                let e = self.journal.iter().find(|e| e.id == id).ok_or("Umbuchung nicht gefunden.")?;
                let msg = format!("Umbuchung {} an {} über {} gelöscht", e.debit, e.credit, fmt_money(e.amount));
                self.journal.retain(|e| e.id != id);
                Ok((None, msg))
            }
            Op::ImportContacts(list) => {
                let (mut added, mut skipped) = (0, 0);
                for mut c in list {
                    let dup = self.contacts.iter().any(|x| {
                        (!c.number.is_empty() && x.number == c.number)
                            || (x.display_name().eq_ignore_ascii_case(&c.display_name()) && x.zip_city == c.zip_city)
                    });
                    if dup || (c.company.trim().is_empty() && c.person.trim().is_empty()) {
                        skipped += 1;
                        continue;
                    }
                    c.id = self.new_id();
                    if c.number.trim().is_empty() {
                        c.number = self.next_contact_number();
                    }
                    self.contacts.push(c);
                    added += 1;
                }
                Ok((None, format!("{added} Kontakte übernommen ({skipped} übersprungen)")))
            }
            Op::ImportArticles(list) => {
                let (mut added, mut skipped) = (0, 0);
                for mut a in list {
                    let dup = self.articles.iter().any(|x| (!a.number.is_empty() && x.number == a.number) || x.name.eq_ignore_ascii_case(&a.name));
                    if dup || a.name.trim().is_empty() {
                        skipped += 1;
                        continue;
                    }
                    a.id = self.new_id();
                    if a.number.trim().is_empty() {
                        a.number = self.next_article_number();
                    }
                    self.articles.push(a);
                    added += 1;
                }
                Ok((None, format!("{added} Artikel übernommen ({skipped} übersprungen)")))
            }
            Op::ImportOpenItems(list) => {
                let (mut added, mut skipped) = (0, 0);
                for mut d in list {
                    if d.number.trim().is_empty() || self.documents.iter().any(|x| x.number == d.number) || d.contact_id.is_none() {
                        skipped += 1;
                        continue;
                    }
                    d.id = self.new_id();
                    d.status = DocStatus::Offen;
                    d.payments.clear();
                    self.documents.push(d);
                    added += 1;
                }
                Ok((None, format!("{added} offene Posten übernommen ({skipped} übersprungen)")))
            }
            Op::SaveProject(mut p) => {
                if p.name.trim().is_empty() {
                    return Err("Bitte einen Projektnamen eingeben.".into());
                }
                let msg = format!("Projekt „{}“ gespeichert", p.name);
                if p.id == 0 {
                    p.id = self.new_id();
                    if p.number.trim().is_empty() {
                        p.number = format!("P-{:04}", self.projects.len() + 1);
                    }
                    let id = p.id;
                    self.projects.push(p);
                    Ok((Some(id), msg))
                } else {
                    let slot = self.projects.iter_mut().find(|x| x.id == p.id).ok_or("Projekt nicht gefunden.")?;
                    let id = p.id;
                    *slot = p;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteProject(id) => {
                if self.times.iter().any(|t| t.project_id == id) {
                    return Err("Zu diesem Projekt gibt es Zeiten. Beenden Sie es stattdessen („nicht aktiv“).".into());
                }
                let name = self.projects.iter().find(|p| p.id == id).map(|p| p.name.clone()).ok_or("Projekt nicht gefunden.")?;
                self.projects.retain(|p| p.id != id);
                Ok((None, format!("Projekt „{name}“ gelöscht")))
            }
            Op::SaveTime(mut e) => {
                if !self.projects.iter().any(|p| p.id == e.project_id) {
                    return Err("Bitte ein Projekt wählen.".into());
                }
                if !(e.hours > 0.0 && e.hours <= 24.0) {
                    return Err("Bitte die Stunden angeben (höchstens 24 am Tag).".into());
                }
                if let Some(old) = self.times.iter().find(|x| x.id == e.id && e.id != 0)
                    && old.invoice_id.is_some()
                {
                    return Err("Diese Zeit ist schon abgerechnet.".into());
                }
                let msg = format!("{} Stunden „{}“ erfasst", fmt_qty(e.hours), e.text);
                if e.id == 0 {
                    e.id = self.new_id();
                    e.invoice_id = None;
                    let id = e.id;
                    self.times.push(e);
                    self.times.sort_by_key(|t| t.date);
                    Ok((Some(id), msg))
                } else {
                    let slot = self.times.iter_mut().find(|x| x.id == e.id).ok_or("Zeit nicht gefunden.")?;
                    let id = e.id;
                    *slot = e;
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteTime(id) => {
                let e = self.times.iter().find(|t| t.id == id).ok_or("Zeit nicht gefunden.")?;
                if e.invoice_id.is_some() {
                    return Err("Diese Zeit ist schon abgerechnet.".into());
                }
                let msg = format!("Zeit „{}“ ({} Std.) gelöscht", e.text, fmt_qty(e.hours));
                self.times.retain(|t| t.id != id);
                Ok((None, msg))
            }
            Op::BillTimes(project_id, ids) => {
                let p = self.projects.iter().find(|p| p.id == project_id).cloned().ok_or("Projekt nicht gefunden.")?;
                let Some(cid) = p.contact_id else {
                    return Err("Bitte dem Projekt zuerst einen Kunden zuordnen.".into());
                };
                let entries: Vec<TimeEntry> =
                    self.times.iter().filter(|t| t.project_id == project_id && ids.contains(&t.id) && t.invoice_id.is_none()).cloned().collect();
                if entries.is_empty() {
                    return Err("Keine offenen Zeiten ausgewählt.".into());
                }
                let lang = self.contact(Some(cid)).map(|c| c.language).unwrap_or_default();
                let lines = entries
                    .iter()
                    .map(|e| Line {
                        article_id: None,
                        text: if e.text.trim().is_empty() { format!("{} {}", fmt_date_in(lang, e.date), p.name) } else { format!("{}: {}", fmt_date_in(lang, e.date), e.text) },
                        qty: e.hours,
                        unit: crate::i18n::t_in(lang, "Std.").into(),
                        price_net: p.rate,
                        vat: 19,
                        discount_pct: 0.0,
                    })
                    .collect();
                let first = entries.iter().map(|e| e.date).min().unwrap_or_else(today);
                let last = entries.iter().map(|e| e.date).max().unwrap_or_else(today);
                let id = self.new_id();
                let mut doc = Document {
                    id,
                    kind: DocKind::Rechnung,
                    contact_id: Some(cid),
                    title: format!("{} {}", crate::i18n::t_in(lang, "Rechnung"), p.name),
                    intro: crate::i18n::t_in(lang, "Für unsere Leistungen berechnen wir Ihnen:").into(),
                    outro: crate::i18n::t_in(lang, "Vielen Dank für die gute Zusammenarbeit.").into(),
                    service_date: if first == last { fmt_date_in(lang, first) } else { format!("{} – {}", fmt_date_in(lang, first), fmt_date_in(lang, last)) },
                    lines,
                    ..Document::default()
                };
                if let Some(c) = self.contact(Some(cid)) {
                    doc.vat_case = c.vat_case;
                    doc.currency = c.currency.clone();
                }
                doc.apply_term(&self.term_for(Some(cid)));
                self.documents.push(doc);
                for t in self.times.iter_mut().filter(|t| entries.iter().any(|e| e.id == t.id)) {
                    t.invoice_id = Some(id);
                }
                Ok((Some(id), format!("{} Zeiten aus Projekt „{}“ in einen Rechnungsentwurf übernommen", entries.len(), p.name)))
            }
            Op::ConvertOffer(id) => {
                let i = self.doc_index(id)?;
                let src = self.documents[i].clone();
                if src.kind != DocKind::Angebot {
                    return Err("Nur Angebote können umgewandelt werden.".into());
                }
                if src.status == DocStatus::Offen {
                    self.documents[i].status = DocStatus::Angenommen;
                }
                let new_id = self.new_id();
                let mut invoice = Document {
                    id: new_id,
                    kind: DocKind::Rechnung,
                    number: String::new(),
                    date: today(),
                    title: "Rechnung".into(),
                    intro: format!("Gemäß unserem Angebot {} berechnen wir Ihnen:", src.number),
                    outro: "Vielen Dank für die gute Zusammenarbeit.".into(),
                    status: DocStatus::Entwurf,
                    paid_date: None,
                    paid_discount_pct: 0.0,
                    source_id: Some(id),
                    emails: Vec::new(),
                    reminders: Vec::new(),
                    payments: Vec::new(),
                    part: InvoicePart::Normal,
                    ..src.clone()
                };
                invoice.apply_term(&self.term_for(src.contact_id));
                self.documents.push(invoice);
                Ok((Some(new_id), format!("Angebot {} in Rechnung umgewandelt", src.number)))
            }
            Op::CreateCredit(id) => {
                let i = self.doc_index(id)?;
                let src = self.documents[i].clone();
                if src.kind != DocKind::Rechnung || src.status == DocStatus::Entwurf {
                    return Err("Gutschriften entstehen aus fertigen Rechnungen.".into());
                }
                let new_id = self.new_id();
                self.documents.push(Document {
                    id: new_id,
                    kind: DocKind::Gutschrift,
                    number: String::new(),
                    date: today(),
                    title: format!("Gutschrift zu Rechnung {}", src.number),
                    intro: format!("Hiermit schreiben wir Ihnen zur Rechnung {} gut:", src.number),
                    outro: "Der Betrag wird Ihnen erstattet bzw. verrechnet.".into(),
                    status: DocStatus::Entwurf,
                    paid_date: None,
                    source_id: Some(id),
                    emails: Vec::new(),
                    reminders: Vec::new(),
                    payments: Vec::new(),
                    part: InvoicePart::Normal,
                    ..src.clone()
                });
                Ok((Some(new_id), format!("Gutschrift zu {} angelegt", src.number)))
            }
            Op::DuplicateDocument(id) => {
                let i = self.doc_index(id)?;
                let src = self.documents[i].clone();
                let new_id = self.new_id();
                self.documents.push(Document {
                    id: new_id,
                    number: String::new(),
                    date: today(),
                    status: DocStatus::Entwurf,
                    paid_date: None,
                    source_id: None,
                    emails: Vec::new(),
                    reminders: Vec::new(),
                    payments: Vec::new(),
                    part: InvoicePart::Normal,
                    ..src.clone()
                });
                Ok((Some(new_id), format!("{} {} kopiert", src.kind.name(), src.number)))
            }
            Op::SaveBooking(mut b) => {
                if b.doc_id.is_some() {
                    return Err("Diese Buchung gehört zu einem Beleg. Ändern Sie die Zahlung dort.".into());
                }
                if b.receipt_id.is_some() {
                    return Err("Diese Buchung stammt aus dem Belegeingang. Nehmen Sie die Buchung dort zurück.".into());
                }
                if b.bank_tx.is_some() {
                    return Err("Diese Buchung stammt aus einem Bankumsatz. Nehmen Sie die Zuordnung dort zurück.".into());
                }
                self.check_cash(&b.payment, b.date)?;
                if let Some(old) = self.bookings.iter().find(|x| x.id == b.id && b.id != 0) {
                    self.check_cash(&old.payment, old.date)?;
                }
                let msg = format!("{} „{}“ über {} gespeichert", b.kind.name(), b.text, fmt_money(b.gross));
                if b.id == 0 {
                    b.id = self.new_id();
                    let id = b.id;
                    self.bookings.push(b);
                    self.bookings.sort_by_key(|b| b.date);
                    Ok((Some(id), msg))
                } else {
                    let slot = self
                        .bookings
                        .iter_mut()
                        .find(|x| x.id == b.id)
                        .ok_or("Buchung nicht gefunden.")?;
                    if slot.doc_id.is_some() {
                        return Err("Diese Buchung gehört zu einem Beleg.".into());
                    }
                    let id = b.id;
                    *slot = b;
                    self.bookings.sort_by_key(|b| b.date);
                    Ok((Some(id), msg))
                }
            }
            Op::DeleteBooking(id) => {
                let b = self
                    .bookings
                    .iter()
                    .find(|b| b.id == id)
                    .ok_or("Buchung nicht gefunden.")?;
                if b.doc_id.is_some() {
                    return Err("Diese Buchung gehört zu einem Beleg. Nehmen Sie die Zahlung dort zurück.".into());
                }
                if b.receipt_id.is_some() {
                    return Err("Diese Buchung stammt aus dem Belegeingang. Nehmen Sie die Buchung dort zurück.".into());
                }
                if b.bank_tx.is_some() {
                    return Err("Diese Buchung stammt aus einem Bankumsatz. Nehmen Sie die Zuordnung dort zurück.".into());
                }
                self.check_cash(&b.payment, b.date)?;
                if self.assets.iter().any(|a| a.booking_id == Some(id)) {
                    return Err("Zu dieser Buchung gehört ein Anlagegut. Bitte zuerst das Anlagegut löschen.".into());
                }
                let msg = format!("Buchung „{}“ gelöscht", b.text);
                self.bookings.retain(|b| b.id != id);
                Ok((None, msg))
            }
            Op::SaveCompany(c) => {
                self.company = c;
                Ok((None, "Firmendaten geändert".into()))
            }
            Op::AddReceipt(mut r) => {
                r.id = self.new_id();
                r.status = ReceiptStatus::Neu;
                r.booking_ids.clear();
                let id = r.id;
                let msg = format!("Beleg „{}“ eingegangen ({})", r.file_name, r.source);
                self.receipts.push(*r);
                Ok((Some(id), msg))
            }
            Op::SaveReceipt(r) => {
                let slot = self.receipts.iter_mut().find(|x| x.id == r.id).ok_or("Beleg nicht gefunden.")?;
                if slot.status == ReceiptStatus::Gebucht {
                    return Err("Der Beleg ist schon gebucht. Bitte zuerst die Buchung zurücknehmen.".into());
                }
                // The file and how it came in stay as they were.
                let kept = Receipt {
                    id: slot.id,
                    received: slot.received.clone(),
                    source: slot.source.clone(),
                    file_name: slot.file_name.clone(),
                    size: slot.size,
                    sha256: slot.sha256.clone(),
                    text: slot.text.clone(),
                    status: slot.status,
                    booking_ids: slot.booking_ids.clone(),
                    ..*r
                };
                let msg = format!("Beleg „{}“ geändert", kept.file_name);
                *slot = kept;
                Ok((Some(r.id), msg))
            }
            Op::BookReceipt(id) => {
                let i = self.receipts.iter().position(|x| x.id == id).ok_or("Beleg nicht gefunden.")?;
                let r = self.receipts[i].clone();
                if r.status != ReceiptStatus::Neu {
                    return Err("Nur neue Belege können gebucht werden.".into());
                }
                if r.gross <= 0 {
                    return Err("Bitte den Betrag des Belegs eintragen.".into());
                }
                if r.category.trim().is_empty() {
                    return Err("Bitte eine Kategorie wählen.".into());
                }
                let date = r.date.unwrap_or_else(today);
                self.check_cash(&r.payment, date)?;
                let text = if r.number.is_empty() {
                    r.supplier.clone()
                } else {
                    format!("{} {}", r.supplier, r.number).trim().to_string()
                };
                let parts: Vec<(u8, Cents)> = if r.vat.len() > 1 {
                    r.vat.iter().map(|p| (p.rate, p.net + p.tax)).collect()
                } else {
                    vec![(r.vat_rate, r.gross)]
                };
                let mut ids = Vec::new();
                for (rate, gross) in parts {
                    let bid = self.new_id();
                    self.bookings.push(Booking {
                        id: bid,
                        date,
                        kind: BookingKind::Ausgabe,
                        category: r.category.clone(),
                        text: text.clone(),
                        partner: r.supplier.clone(),
                        receipt: r.number.clone(),
                        gross,
                        vat: rate,
                        doc_id: None,
                        payment: if r.payment.is_empty() { "Bank".into() } else { r.payment.clone() },
                        receipt_id: Some(id),
                        bank_tx: None,
                        reverse_charge: false,
                    });
                    ids.push(bid);
                }
                self.bookings.sort_by_key(|b| b.date);
                let slot = &mut self.receipts[i];
                slot.status = ReceiptStatus::Gebucht;
                slot.booking_ids = ids;
                Ok((Some(id), format!("Beleg „{}“ als Ausgabe über {} gebucht", r.file_name, fmt_money(r.gross))))
            }
            Op::UnbookReceipt(id) => {
                let slot = self.receipts.iter_mut().find(|x| x.id == id).ok_or("Beleg nicht gefunden.")?;
                if slot.status != ReceiptStatus::Gebucht {
                    return Err("Der Beleg ist nicht gebucht.".into());
                }
                let ids = slot.booking_ids.clone();
                let file_name = slot.file_name.clone();
                for b in self.bookings.iter().filter(|b| ids.contains(&b.id)) {
                    self.check_cash(&b.payment, b.date)?;
                }
                let slot = self.receipts.iter_mut().find(|x| x.id == id).ok_or("Beleg nicht gefunden.")?;
                slot.status = ReceiptStatus::Neu;
                slot.booking_ids.clear();
                let msg = format!("Buchung zu Beleg „{file_name}“ zurückgenommen");
                self.bookings.retain(|b| !ids.contains(&b.id));
                for tx in self.bank.iter_mut().filter(|t| t.link == Some(BankLink::Receipt(id))) {
                    tx.status = BankStatus::Offen;
                    tx.link = None;
                }
                Ok((Some(id), msg))
            }
            Op::DeleteReceipt(id) => {
                let i = self.receipts.iter().position(|x| x.id == id).ok_or("Beleg nicht gefunden.")?;
                if self.receipts[i].status == ReceiptStatus::Gebucht {
                    return Err("Gebuchte Belege können nicht gelöscht werden. Bitte zuerst die Buchung zurücknehmen.".into());
                }
                let r = self.receipts.remove(i);
                // The log keeps what was removed, so a deletion stays traceable.
                Ok((None, format!("Beleg „{}“ gelöscht (nie gebucht, SHA-256 {})", r.file_name, &r.sha256[..r.sha256.len().min(16)])))
            }
            Op::DiscardReceipt(id) => {
                let slot = self.receipts.iter_mut().find(|x| x.id == id).ok_or("Beleg nicht gefunden.")?;
                let msg = match slot.status {
                    ReceiptStatus::Neu => {
                        slot.status = ReceiptStatus::Verworfen;
                        format!("Beleg „{}“ als nicht relevant markiert", slot.file_name)
                    }
                    ReceiptStatus::Verworfen => {
                        slot.status = ReceiptStatus::Neu;
                        format!("Beleg „{}“ wiederhergestellt", slot.file_name)
                    }
                    ReceiptStatus::Gebucht => return Err("Gebuchte Belege können nicht verworfen werden.".into()),
                };
                Ok((Some(id), msg))
            }
            Op::ReplaceAll(d) => {
                let log = std::mem::take(&mut self.log);
                *self = *d;
                self.log.splice(0..0, log);
                Ok((None, "Datenbestand aus Sicherung eingespielt".into()))
            }
        }
    }

    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn contact(&self, id: Option<u64>) -> Option<&Contact> {
        id.and_then(|id| self.contacts.iter().find(|c| c.id == id))
    }

    pub fn contact_name(&self, id: Option<u64>) -> String {
        self.contact(id)
            .map(|c| c.display_name())
            .unwrap_or_else(|| "–".into())
    }

    /// The payment condition for a customer: their own, else the company's default.
    pub fn term_for(&self, contact_id: Option<u64>) -> PaymentTerm {
        let own = self.contact(contact_id).and_then(|c| self.company.term(c.payment_term_id));
        own.or_else(|| self.company.term(Some(self.company.default_term_id)))
            .or(self.company.payment_terms.first())
            .cloned()
            .unwrap_or(PaymentTerm {
                id: 0,
                name: String::new(),
                discounts: Vec::new(),
                net_days: self.company.payment_days,
            })
    }

    /// The e-invoice format for a customer: their own, else the company's default.
    pub fn einvoice_format(&self, contact_id: Option<u64>) -> crate::xrechnung::EInvoiceFormat {
        self.contact(contact_id).and_then(|c| c.einvoice).unwrap_or(self.company.einvoice_default)
    }

    pub fn next_contact_number(&mut self) -> String {
        let n = self.company.next_contact_no;
        self.company.next_contact_no += 1;
        n.to_string()
    }

    pub fn next_article_number(&mut self) -> String {
        let n = self.company.next_article_no;
        self.company.next_article_no += 1;
        format!("A-{n:04}")
    }

    /// Hands out the next running number for a document kind, e.g. "RE-2026-0007".
    pub fn next_doc_number(&mut self, kind: DocKind, year: i32) -> String {
        let c = &mut self.company;
        let (prefix, counter) = match kind {
            DocKind::Rechnung => (c.invoice_prefix.clone(), &mut c.next_invoice_no),
            DocKind::Angebot => (c.offer_prefix.clone(), &mut c.next_offer_no),
            DocKind::Gutschrift => (c.credit_prefix.clone(), &mut c.next_credit_no),
            DocKind::Auftragsbestaetigung => (c.order_prefix.clone(), &mut c.next_order_no),
            DocKind::Lieferschein => (c.delivery_prefix.clone(), &mut c.next_delivery_no),
            DocKind::Bestellung => (c.purchase_prefix.clone(), &mut c.next_purchase_no),
        };
        let n = *counter;
        *counter += 1;
        format!("{prefix}{year}-{n:04}")
    }

    /// Pays the rest of an invoice, optionally minus an early-payment discount (percent of the total).
    fn mark_paid(&mut self, id: u64, date: NaiveDate, pct: f64) -> Result<(Option<u64>, String), String> {
        let small = self.company.small_business;
        let i = self.doc_index(id)?;
        let d = &self.documents[i];
        if !d.kind.is_billing() || d.status == DocStatus::Entwurf {
            return Err("Nur fertige Rechnungen und Gutschriften können bezahlt werden.".into());
        }
        if d.status == DocStatus::Bezahlt {
            return Err("Der Beleg ist bereits bezahlt.".into());
        }
        let gross = d.totals(small).gross;
        let open = d.open_amount(small);
        let discount = ((gross as f64 * pct / 100.0).round() as Cents).clamp(-open.abs(), open.abs());
        self.add_payment(id, Payment { date, amount: open - discount, discount, method: "Bank".into(), bank_tx: None, eur: None })
    }

    /// Records a payment; the document counts as paid once nothing is left open.
    fn add_payment(&mut self, id: u64, p: Payment) -> Result<(Option<u64>, String), String> {
        let small = self.company.small_business;
        let i = self.doc_index(id)?;
        let d = &self.documents[i];
        if !d.kind.is_billing() || d.status == DocStatus::Entwurf {
            return Err("Nur fertige Rechnungen und Gutschriften können bezahlt werden.".into());
        }
        if d.status != DocStatus::Offen {
            return Err("Zahlungen gibt es nur für offene Belege.".into());
        }
        if p.amount == 0 && p.discount == 0 {
            return Err("Bitte einen Betrag eingeben.".into());
        }
        let gross = d.totals(small).gross;
        if p.amount.signum() * gross.signum() < 0 {
            return Err("Der Betrag passt nicht zur Richtung des Belegs.".into());
        }
        self.check_cash(&p.method, p.date)?;
        let d = &mut self.documents[i];
        d.payments.push(p.clone());
        let open = gross - d.paid_total();
        let paid_off = if gross >= 0 { open <= 0 } else { open >= 0 };
        let msg = if paid_off {
            d.status = DocStatus::Bezahlt;
            d.paid_date = Some(p.date);
            let discount: Cents = d.payments.iter().map(|x| x.discount).sum();
            d.paid_discount_pct =
                if gross != 0 { (discount as f64 * 10000.0 / gross as f64).round() / 100.0 } else { 0.0 };
            if d.paid_discount_pct > 0.0 {
                format!("{} {} als bezahlt gebucht (abzüglich {} % Skonto)", d.kind.name(), d.number, fmt_qty(d.paid_discount_pct))
            } else {
                format!("{} {} als bezahlt gebucht", d.kind.name(), d.number)
            }
        } else {
            format!(
                "Teilzahlung über {} zu {} {} gebucht, offen {}",
                fmt_money(p.amount.abs()),
                d.kind.name(),
                d.number,
                fmt_money(open.abs())
            )
        };
        self.book_payment(id, p.date);
        Ok((Some(id), msg))
    }

    /// The cash book: entries paid in cash with the balance after each.
    pub fn cash_book(&self) -> Vec<(&Booking, Cents)> {
        let mut list: Vec<&Booking> = self.bookings.iter().filter(|b| b.payment == CASH).collect();
        list.sort_by_key(|b| (b.date, b.id));
        let mut balance = self.company.cash_opening;
        list.into_iter()
            .map(|b| {
                balance += b.signed();
                (b, balance)
            })
            .collect()
    }

    /// Cash in hand at the end of a day.
    pub fn cash_balance(&self, day: NaiveDate) -> Cents {
        self.company.cash_opening
            + self.bookings.iter().filter(|b| b.payment == CASH && b.date <= day).map(|b| b.signed()).sum::<Cents>()
    }

    /// Cash entries up to the last closing can no longer be changed (GoBD).
    fn check_cash(&self, payment: &str, date: NaiveDate) -> Result<(), String> {
        match self.cash_closings.last() {
            Some(c) if payment == CASH && date <= c.date => Err(format!(
                "Die Kasse ist bis {} abgeschlossen. Kassenbuchungen bis zu diesem Tag können nicht mehr geändert werden.",
                fmt_date(c.date)
            )),
            _ => Ok(()),
        }
    }

    /// Current stock of an article.
    pub fn stock_of(&self, article_id: u64) -> f64 {
        self.stock_moves.iter().filter(|m| m.article_id == article_id).map(|m| m.qty).sum()
    }

    /// Whether finishing this document takes goods out of stock. Invoices do so unless
    /// the goods left with a delivery note of the same order.
    fn books_stock_out(&self, d: &Document) -> bool {
        match d.kind {
            DocKind::Lieferschein => true,
            DocKind::Rechnung if d.part != InvoicePart::Abschlag => {
                let src = d.source_id;
                !(src.is_some()
                    && self.documents.iter().any(|x| {
                        x.kind == DocKind::Lieferschein
                            && !matches!(x.status, DocStatus::Entwurf | DocStatus::Storniert)
                            && (Some(x.id) == src || x.source_id == src)
                    }))
            }
            _ => false,
        }
    }

    /// Moves the tracked articles of a document into (+1) or out of (-1) stock.
    fn move_stock(&mut self, doc_index: usize, sign: f64) {
        let d = self.documents[doc_index].clone();
        for l in &d.lines {
            let Some(aid) = l.article_id else { continue };
            if !self.articles.iter().any(|a| a.id == aid && a.track_stock) || l.qty == 0.0 {
                continue;
            }
            let id = self.new_id();
            self.stock_moves.push(StockMove {
                id,
                date: d.date,
                article_id: aid,
                qty: sign * l.qty,
                doc_id: Some(d.id),
                note: format!("{} {}", d.kind.name(), d.number),
            });
        }
    }

    /// A follow-up document: offer → order confirmation → delivery note → invoice.
    fn convert_document(&mut self, id: u64, target: DocKind) -> Result<(Option<u64>, String), String> {
        let i = self.doc_index(id)?;
        let src = self.documents[i].clone();
        let allowed = matches!(
            (src.kind, target),
            (DocKind::Angebot, DocKind::Auftragsbestaetigung | DocKind::Lieferschein | DocKind::Rechnung)
                | (DocKind::Auftragsbestaetigung, DocKind::Lieferschein | DocKind::Rechnung)
                | (DocKind::Lieferschein, DocKind::Rechnung)
        );
        if !allowed || matches!(src.status, DocStatus::Entwurf | DocStatus::Storniert | DocStatus::Abgelehnt) {
            return Err(format!("Aus diesem Beleg kann keine {} entstehen.", target.name()));
        }
        if src.kind == DocKind::Angebot && src.status == DocStatus::Offen {
            self.documents[i].status = DocStatus::Angenommen;
        }
        let lang = self.contact(src.contact_id).map(|c| c.language).unwrap_or_default();
        let l = |de| crate::i18n::t_in(lang, de);
        let from = format!("{} {}", l(src.kind.name()), src.number);
        let (title, intro, outro) = match target {
            DocKind::Auftragsbestaetigung => (
                l("Auftragsbestätigung").to_string(),
                crate::i18n::fill(l("Vielen Dank für Ihren Auftrag. Gemäß {} bestätigen wir:"), &[&from]),
                l("Wir freuen uns auf die weitere Zusammenarbeit.").to_string(),
            ),
            DocKind::Lieferschein => (
                l("Lieferschein").to_string(),
                crate::i18n::fill(l("Wir liefern Ihnen gemäß {}:"), &[&from]),
                l("Ware vollständig und in einwandfreiem Zustand erhalten:").to_string(),
            ),
            _ => (
                l("Rechnung").to_string(),
                crate::i18n::fill(l("Gemäß {} berechnen wir Ihnen:"), &[&from]),
                l("Vielen Dank für die gute Zusammenarbeit.").to_string(),
            ),
        };
        let new_id = self.new_id();
        let mut doc = Document {
            id: new_id,
            kind: target,
            number: String::new(),
            date: today(),
            title,
            intro,
            outro,
            status: DocStatus::Entwurf,
            paid_date: None,
            paid_discount_pct: 0.0,
            source_id: Some(id),
            emails: Vec::new(),
            reminders: Vec::new(),
            payments: Vec::new(),
            part: InvoicePart::Normal,
            ..src.clone()
        };
        match target {
            DocKind::Rechnung => doc.apply_term(&self.term_for(src.contact_id)),
            _ => {
                doc.due_days = 0;
                doc.discounts.clear();
                doc.term_name.clear();
            }
        }
        self.documents.push(doc);
        Ok((Some(new_id), format!("{} {} in {} umgewandelt", src.kind.name(), src.number, target.name())))
    }

    /// Net sums of a document's lines per VAT rate (as on the lines).
    fn net_by_rate(d: &Document) -> BTreeMap<u8, Cents> {
        let mut m = BTreeMap::new();
        for l in &d.lines {
            *m.entry(l.vat).or_insert(0) += l.net();
        }
        m.retain(|_, v| *v != 0);
        m
    }

    fn create_advance_invoice(&mut self, id: u64, pct: f64) -> Result<(Option<u64>, String), String> {
        let i = self.doc_index(id)?;
        let src = self.documents[i].clone();
        if !matches!(src.kind, DocKind::Angebot | DocKind::Auftragsbestaetigung)
            || matches!(src.status, DocStatus::Entwurf | DocStatus::Storniert | DocStatus::Abgelehnt)
        {
            return Err("Abschlagsrechnungen entstehen aus fertigen Angeboten oder Auftragsbestätigungen.".into());
        }
        if !(pct > 0.0 && pct <= 100.0) {
            return Err("Bitte einen Anteil zwischen 1 und 100 % angeben.".into());
        }
        if src.kind == DocKind::Angebot && src.status == DocStatus::Offen {
            self.documents[i].status = DocStatus::Angenommen;
        }
        let lang = self.contact(src.contact_id).map(|c| c.language).unwrap_or_default();
        let l = |de| crate::i18n::t_in(lang, de);
        let from = format!("{} {}", l(src.kind.name()), src.number);
        let rates = Self::net_by_rate(&src);
        let several = rates.len() > 1;
        let lines = rates
            .iter()
            .map(|(rate, net)| {
                let mut text = crate::i18n::fill(l("Abschlag {} % gemäß {}"), &[&fmt_qty_in(lang, pct), &from]);
                if several {
                    text.push_str(&format!(" ({rate} %)"));
                }
                Line {
                    article_id: None,
                    text,
                    qty: 1.0,
                    unit: l("Pauschal").into(),
                    price_net: (*net as f64 * pct / 100.0).round() as Cents,
                    vat: *rate,
                    discount_pct: 0.0,
                }
            })
            .collect();
        let new_id = self.new_id();
        let mut doc = Document {
            id: new_id,
            kind: DocKind::Rechnung,
            date: today(),
            title: l("Abschlagsrechnung").into(),
            intro: crate::i18n::fill(l("Gemäß {} berechnen wir Ihnen eine Abschlagszahlung:"), &[&from]),
            outro: l("Der Abschlag wird mit der Schlussrechnung verrechnet.").into(),
            lines,
            source_id: Some(id),
            part: InvoicePart::Abschlag,
            vat_case: src.vat_case,
            contact_id: src.contact_id,
            service_date: src.service_date.clone(),
            ..Document::default()
        };
        doc.apply_term(&self.term_for(src.contact_id));
        self.documents.push(doc);
        Ok((Some(new_id), format!("Abschlagsrechnung ({} %) zu {} {} angelegt", fmt_qty(pct), src.kind.name(), src.number)))
    }

    fn create_final_invoice(&mut self, id: u64) -> Result<(Option<u64>, String), String> {
        let i = self.doc_index(id)?;
        let src = self.documents[i].clone();
        if !matches!(src.kind, DocKind::Angebot | DocKind::Auftragsbestaetigung) {
            return Err("Schlussrechnungen entstehen aus Angeboten oder Auftragsbestätigungen.".into());
        }
        let advances: Vec<Document> = self
            .documents
            .iter()
            .filter(|d| {
                d.source_id == Some(id)
                    && d.part == InvoicePart::Abschlag
                    && !matches!(d.status, DocStatus::Entwurf | DocStatus::Storniert)
            })
            .cloned()
            .collect();
        if advances.is_empty() {
            return Err("Zu diesem Beleg gibt es keine fertigen Abschlagsrechnungen. Wandeln Sie ihn einfach in eine Rechnung um.".into());
        }
        let lang = self.contact(src.contact_id).map(|c| c.language).unwrap_or_default();
        let l = |de| crate::i18n::t_in(lang, de);
        let mut lines = src.lines.clone();
        for a in &advances {
            for (rate, net) in Self::net_by_rate(a) {
                lines.push(Line {
                    article_id: None,
                    text: crate::i18n::fill(l("abzüglich Abschlagsrechnung {} vom {}"), &[&a.number, &fmt_date_in(lang, a.date)]),
                    qty: 1.0,
                    unit: l("Pauschal").into(),
                    price_net: -net,
                    vat: rate,
                    discount_pct: 0.0,
                });
            }
        }
        let from = format!("{} {}", l(src.kind.name()), src.number);
        let new_id = self.new_id();
        let mut doc = Document {
            id: new_id,
            kind: DocKind::Rechnung,
            date: today(),
            title: l("Schlussrechnung").into(),
            intro: crate::i18n::fill(l("Gemäß {} berechnen wir Ihnen abschließend:"), &[&from]),
            outro: l("Vielen Dank für die gute Zusammenarbeit.").into(),
            lines,
            source_id: Some(id),
            part: InvoicePart::Schluss,
            vat_case: src.vat_case,
            contact_id: src.contact_id,
            service_date: src.service_date.clone(),
            ..Document::default()
        };
        doc.apply_term(&self.term_for(src.contact_id));
        self.documents.push(doc);
        Ok((Some(new_id), format!("Schlussrechnung zu {} {} angelegt ({} Abschläge abgezogen)", src.kind.name(), src.number, advances.len())))
    }

    /// Recurring invoices due up to `day` (and how many there are).
    pub fn due_recurring(&self, day: NaiveDate) -> usize {
        self.recurring
            .iter()
            .filter(|r| r.active && r.next_date <= day && r.until.is_none_or(|u| r.next_date <= u))
            .count()
    }

    fn create_due_recurring(&mut self, day: NaiveDate) -> Result<(Option<u64>, String), String> {
        let mut made = 0;
        for k in 0..self.recurring.len() {
            while made < 500 {
                let r = &self.recurring[k];
                if !r.active || r.next_date > day || r.until.is_some_and(|u| r.next_date > u) {
                    break;
                }
                let (date, months, finalize) = (r.next_date, r.every_months.max(1), r.finalize);
                let mut doc = r.template.clone();
                let lang = self.contact(doc.contact_id).map(|c| c.language).unwrap_or_default();
                doc.id = self.new_id();
                doc.kind = DocKind::Rechnung;
                doc.status = DocStatus::Entwurf;
                doc.number.clear();
                doc.date = date;
                doc.service_date = service_period(lang, date, months);
                doc.payments.clear();
                doc.emails.clear();
                doc.reminders.clear();
                doc.source_id = None;
                let id = doc.id;
                self.documents.push(doc);
                if finalize {
                    // A problem (e.g. missing contact) leaves a draft to fix by hand.
                    let _ = self.apply_inner(Op::FinalizeDocument(id));
                }
                let r = &mut self.recurring[k];
                r.created.push(id);
                r.next_date = date.checked_add_months(chrono::Months::new(months)).unwrap_or(date + Duration::days(30));
                made += 1;
            }
        }
        if made == 0 {
            return Err("Zurzeit ist keine wiederkehrende Rechnung fällig.".into());
        }
        Ok((None, format!("{made} wiederkehrende Rechnung(en) erstellt")))
    }

    fn assign_bank(&mut self, tx_id: u64, how: BankAssign) -> Result<(Option<u64>, String), String> {
        let small = self.company.small_business;
        let ti = self.bank.iter().position(|t| t.id == tx_id).ok_or("Bankumsatz nicht gefunden.")?;
        let tx = self.bank[ti].clone();
        if tx.status != BankStatus::Offen {
            return Err("Der Umsatz ist schon zugeordnet oder ignoriert.".into());
        }
        let (link, msg) = match how {
            BankAssign::Document { id, discount } => {
                let i = self.doc_index(id)?;
                let d = &self.documents[i];
                let open = d.open_amount(small);
                let gross = d.totals(small).gross;
                if open == 0 {
                    return Err("Der Beleg ist nicht offen.".into());
                }
                if open.signum() != tx.amount.signum() {
                    return Err("Der Betrag passt nicht zur Richtung des Belegs (Eingang/Ausgang).".into());
                }
                // Bank lines are in euros; a document in another currency is paid in full
                // unless clearly less arrived (exchange rates move a little).
                let (amount, eur) = if d.is_foreign() {
                    let converted = (tx.amount as f64 * d.exchange_rate.max(0.0001)).round() as Cents;
                    if converted.abs() as f64 >= open.abs() as f64 * 0.97 { (open, Some(tx.amount)) } else { (converted, Some(tx.amount)) }
                } else {
                    (tx.amount, None)
                };
                let rest = if discount && eur.is_none() { open - tx.amount } else { 0 };
                if discount && (rest.signum() != open.signum() || rest.abs() * 10 > gross.abs()) {
                    return Err("Der Abzug wäre größer als 10 %. Bitte als Teilzahlung zuordnen.".into());
                }
                let number = d.number.clone();
                self.add_payment(
                    id,
                    Payment { date: tx.date, amount, discount: rest, method: "Bank".into(), bank_tx: Some(tx_id), eur },
                )?;
                (BankLink::Document(id), format!("Bankumsatz von {} der Rechnung {number} zugeordnet", tx.name))
            }
            BankAssign::Receipt(rid) => {
                let r = self.receipts.iter().find(|r| r.id == rid).ok_or("Beleg nicht gefunden.")?.clone();
                if tx.amount >= 0 {
                    return Err("Eingangsbelege werden mit Abbuchungen bezahlt.".into());
                }
                match r.status {
                    ReceiptStatus::Neu => {
                        self.apply_inner(Op::BookReceipt(rid))?;
                        // Under the cash principle the expense counts on the day it was paid.
                        let ids = self.receipts.iter().find(|x| x.id == rid).map(|x| x.booking_ids.clone()).unwrap_or_default();
                        for b in self.bookings.iter_mut().filter(|b| ids.contains(&b.id)) {
                            b.date = tx.date;
                            b.payment = "Bank".into();
                            b.bank_tx = Some(tx_id);
                        }
                        self.bookings.sort_by_key(|b| b.date);
                    }
                    ReceiptStatus::Gebucht => {}
                    ReceiptStatus::Verworfen => return Err("Der Beleg ist als nicht relevant markiert.".into()),
                }
                (BankLink::Receipt(rid), format!("Bankumsatz an {} dem Beleg „{}“ zugeordnet", tx.name, r.file_name))
            }
            BankAssign::Booking { category, vat, text } => {
                let bid = self.new_id();
                let text = if text.trim().is_empty() { tx.purpose.chars().take(80).collect() } else { text };
                self.bookings.push(Booking {
                    id: bid,
                    date: tx.date,
                    kind: if tx.amount >= 0 { BookingKind::Einnahme } else { BookingKind::Ausgabe },
                    category: category.clone(),
                    text,
                    partner: tx.name.clone(),
                    receipt: String::new(),
                    gross: tx.amount.abs(),
                    vat,
                    doc_id: None,
                    payment: "Bank".into(),
                    receipt_id: None,
                    bank_tx: Some(tx_id),
                    reverse_charge: false,
                });
                self.bookings.sort_by_key(|b| b.date);
                (BankLink::Booking(bid), format!("Bankumsatz von {} als „{category}“ gebucht", tx.name))
            }
        };
        let t = &mut self.bank[ti];
        t.status = BankStatus::Zugeordnet;
        t.link = Some(link);
        Ok((Some(tx_id), msg))
    }

    /// Books the payments of an invoice (or credit note) as income, one entry per payment
    /// and VAT rate, split in the proportion of the rates. An early-payment discount simply
    /// lowers what is booked, so net and VAT shrink alike (§ 17 UStG).
    pub fn book_payment(&mut self, doc_id: u64, _paid: NaiveDate) {
        let Some(doc) = self.documents.iter().find(|d| d.id == doc_id).cloned() else {
            return;
        };
        let small = self.company.small_business;
        let partner = self.contact_name(doc.contact_id);
        let totals = doc.totals(small);
        self.bookings.retain(|b| b.doc_id != Some(doc_id));
        let rates: Vec<(u8, Cents)> =
            totals.by_rate.iter().map(|(r, (n, t))| (*r, n + t)).filter(|(_, g)| *g != 0).collect();
        if totals.gross == 0 || rates.is_empty() {
            return;
        }
        let payments = doc.effective_payments(small);
        let several = payments.len() > 1;
        for p in payments {
            let mut text = format!("{} {}", doc.kind.name(), doc.number);
            if p.discount != 0 {
                text.push_str(" (abzgl. Skonto)");
            } else if several {
                text.push_str(" (Teilzahlung)");
            }
            // Entries are always in euros: what arrived, or the amount at the document's rate.
            let paid = p.eur.unwrap_or_else(|| doc.to_eur(p.amount));
            let mut rest = paid;
            for (k, (rate, gross)) in rates.iter().enumerate() {
                let part = if k + 1 == rates.len() {
                    rest
                } else {
                    (paid as f64 * *gross as f64 / totals.gross as f64).round() as Cents
                };
                rest -= part;
                if part == 0 {
                    continue;
                }
                let id = self.new_id();
                self.bookings.push(Booking {
                    id,
                    date: p.date,
                    kind: if part >= 0 { BookingKind::Einnahme } else { BookingKind::Ausgabe },
                    category: doc
                        .vat_case
                        .income_category()
                        .filter(|_| !small)
                        .unwrap_or_else(|| income_category_for_rate(*rate, small))
                        .into(),
                    text: text.clone(),
                    partner: partner.clone(),
                    receipt: doc.number.clone(),
                    gross: part.abs(),
                    vat: *rate,
                    doc_id: Some(doc_id),
                    payment: if p.method.is_empty() { "Bank".into() } else { p.method.clone() },
                    receipt_id: None,
                    bank_tx: p.bank_tx,
                    reverse_charge: false,
                });
            }
        }
        self.bookings.sort_by_key(|b| b.date);
    }

    pub fn unbook_payment(&mut self, doc_id: u64) {
        self.bookings.retain(|b| b.doc_id != Some(doc_id));
    }
}

/// Figures for an Einnahmen-Überschuss-Rechnung or a VAT return over a date range.
#[derive(Clone, Debug, Default)]
pub struct PeriodReport {
    pub income_by_cat: BTreeMap<String, Cents>,
    pub expense_by_cat: BTreeMap<String, Cents>,
    pub income_net: Cents,
    pub expense_net: Cents,
    pub vat_collected: Cents,
    pub vat_paid: Cents,
    pub base_19: Cents,
    pub tax_19: Cents,
    pub base_7: Cents,
    pub tax_7: Cents,
    pub base_0: Cents,
    pub private_in: Cents,
    pub private_out: Cents,
    /// Tax-free EU deliveries (Kennzahl 41).
    pub eu_deliveries: Cents,
    /// EU services where the customer owes the VAT (Kennzahl 21).
    pub eu_services: Cents,
    /// Exports and other tax-free sales with input tax deduction (Kennzahl 43).
    pub exports: Cents,
    /// Services outside the EU, not taxable here (Kennzahl 45).
    pub non_taxable: Cents,
    /// Services received from foreign businesses, VAT owed by us (§ 13b; Kennzahl 46/47).
    pub rc_base: Cents,
    pub rc_tax: Cents,
    /// Write-offs of fixed assets (only in reports over whole years).
    pub afa: Cents,
}

impl PeriodReport {
    /// Profit as in the EÜR: VAT received counts as income, input tax and VAT paid to the
    /// tax office (account 1780) as expenses.
    pub fn profit(&self) -> Cents {
        self.income_net - self.expense_net + self.vat_due()
    }
    pub fn vat_due(&self) -> Cents {
        self.vat_collected - self.vat_paid
    }
}

pub fn period_report(data: &Data, from: NaiveDate, to: NaiveDate) -> PeriodReport {
    let mut r = PeriodReport::default();
    for b in data
        .bookings
        .iter()
        .filter(|b| b.date >= from && b.date <= to)
    {
        if is_private(&b.category) {
            match b.kind {
                BookingKind::Einnahme => r.private_in += b.gross,
                BookingKind::Ausgabe => r.private_out += b.gross,
            }
            continue;
        }
        let (net, tax) = (b.net(), b.tax());
        match b.kind {
            BookingKind::Einnahme => {
                *r.income_by_cat.entry(b.category.clone()).or_default() += net;
                r.income_net += net;
                r.vat_collected += tax;
                let account = b.category.split(' ').next().unwrap_or("");
                match (account, b.vat) {
                    ("8125", _) => r.eu_deliveries += net,
                    ("8336", _) => r.eu_services += net,
                    ("8120", _) => r.exports += net,
                    ("8338", _) => r.non_taxable += net,
                    (_, 19) => {
                        r.base_19 += net;
                        r.tax_19 += tax;
                    }
                    (_, 7) => {
                        r.base_7 += net;
                        r.tax_7 += tax;
                    }
                    _ => r.base_0 += net,
                }
            }
            BookingKind::Ausgabe => {
                r.vat_paid += tax;
                if b.reverse_charge {
                    let rc = b.reverse_charge_tax();
                    r.rc_base += net;
                    r.rc_tax += rc;
                    r.vat_collected += rc;
                    r.vat_paid += rc;
                }
                // Fixed assets are not an expense at once; they are written off over the years.
                if b.category == ASSET_CATEGORY {
                    continue;
                }
                *r.expense_by_cat.entry(b.category.clone()).or_default() += net;
                r.expense_net += net;
            }
        }
    }
    // Write-offs belong to the year; they are added when the report covers whole years.
    if from.month() == 1 && from.day() == 1 && to.month() == 12 && to.day() == 31 {
        for year in from.year()..=to.year() {
            for (cat, amount) in crate::accounting::afa_by_category(data, year) {
                *r.expense_by_cat.entry(cat).or_default() += amount;
                r.expense_net += amount;
                r.afa += amount;
            }
        }
    }
    r
}

/// Net income and expenses for each month of a year.
pub fn monthly(data: &Data, year: i32) -> [(Cents, Cents); 12] {
    let mut m = [(0, 0); 12];
    for b in data
        .bookings
        .iter()
        .filter(|b| b.date.year() == year && !is_private(&b.category) && b.category != ASSET_CATEGORY)
    {
        let i = b.date.month0() as usize;
        match b.kind {
            BookingKind::Einnahme => m[i].0 += b.net(),
            BookingKind::Ausgabe => m[i].1 += b.net(),
        }
    }
    m
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

const MONTH_NAMES: [&str; 12] = [
    "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember",
];

/// "November 2026" or "November 2026 – Januar 2027" for a period of several months.
pub fn service_period(lang: Lang, start: NaiveDate, months: u32) -> String {
    let name = |d: NaiveDate| format!("{} {}", crate::i18n::t_in(lang, MONTH_NAMES[d.month0() as usize]), d.year());
    if months <= 1 {
        return name(start);
    }
    let end = start.checked_add_months(chrono::Months::new(months - 1)).unwrap_or(start);
    format!("{} – {}", name(start), name(end))
}

// ---------------------------------------------------------------------------
// Formatting and parsing in German style.

pub fn fmt_money(c: Cents) -> String {
    format!("{} €", fmt_amount(c))
}

/// Money as written in a language: "1.234,56 €" or "€1,234.56".
pub fn fmt_money_in(lang: Lang, c: Cents) -> String {
    match lang {
        Lang::De => fmt_money(c),
        Lang::En => {
            let s = fmt_amount(c.abs()).replace('.', "#").replace(',', ".").replace('#', ",");
            format!("{}€{s}", if c < 0 { "-" } else { "" })
        }
    }
}

/// Money in a currency: euros as usual, others with their code ("1.234,56 USD").
pub fn fmt_money_cur(lang: Lang, c: Cents, currency: &str) -> String {
    let code = currency.trim();
    if code.is_empty() || code == "EUR" {
        return fmt_money_in(lang, c);
    }
    let n = match lang {
        Lang::De => fmt_amount(c),
        Lang::En => fmt_amount(c).replace('.', "#").replace(',', ".").replace('#', ","),
    };
    format!("{n} {code}")
}

/// Currencies to choose from for documents.
pub const CURRENCIES: [&str; 16] =
    ["EUR", "USD", "GBP", "CHF", "PLN", "CZK", "DKK", "SEK", "NOK", "HUF", "RON", "TRY", "JPY", "CNY", "CAD", "AUD"];

/// A date as written in a language: "07.11.2026" or "7 Nov 2026".
pub fn fmt_date_in(lang: Lang, d: NaiveDate) -> String {
    match lang {
        Lang::De => fmt_date(d),
        Lang::En => d.format("%-d %b %Y").to_string(),
    }
}

pub fn fmt_qty_in(lang: Lang, q: f64) -> String {
    match lang {
        Lang::De => fmt_qty(q),
        Lang::En => fmt_qty(q).replace(',', "."),
    }
}

/// The payment lines printed under an invoice, in the customer's language, e.g.
/// "Bei Zahlung bis 17.11.2026 abzüglich 2 % Skonto: 4.895,71 €".
pub fn payment_lines(doc: &Document, small_business: bool, lang: Lang) -> Vec<String> {
    use crate::i18n::{fill, t_in};
    if doc.kind != DocKind::Rechnung {
        return Vec::new();
    }
    let schedule = doc.payment_schedule(small_business);
    if schedule.len() == 1 {
        let (date, _, _) = schedule[0];
        return vec![if doc.due_days == 0 {
            t_in(lang, "Zahlbar sofort ohne Abzug.").to_string()
        } else {
            fill(t_in(lang, "Zahlbar bis {} ohne Abzug."), &[&fmt_date_in(lang, date)])
        }];
    }
    schedule
        .iter()
        .map(|(date, pct, amount)| {
            if *pct > 0.0 {
                fill(
                    t_in(lang, "Bei Zahlung bis {} abzüglich {} % Skonto: {}"),
                    &[&fmt_date_in(lang, *date), &fmt_qty_in(lang, *pct), &fmt_money_cur(lang, *amount, doc.currency())],
                )
            } else {
                fill(
                    t_in(lang, "Bei Zahlung bis {} ohne Abzug: {}"),
                    &[&fmt_date_in(lang, *date), &fmt_money_cur(lang, *amount, doc.currency())],
                )
            }
        })
        .collect()
}

pub fn fmt_amount(c: Cents) -> String {
    let neg = c < 0;
    let c = c.unsigned_abs();
    let euros = (c / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in euros.chars().enumerate() {
        if i > 0 && (euros.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!("{}{},{:02}", if neg { "-" } else { "" }, grouped, c % 100)
}

pub fn parse_amount(s: &str) -> Option<Cents> {
    let s: String = s
        .trim()
        .trim_end_matches('€')
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if s.is_empty() {
        return Some(0);
    }
    // German style: "." groups thousands, "," is the decimal mark.
    let normalized = if s.contains(',') {
        s.replace('.', "").replace(',', ".")
    } else {
        s
    };
    let v: f64 = normalized.parse().ok()?;
    Some((v * 100.0).round() as Cents)
}

pub fn fmt_qty(q: f64) -> String {
    if (q - q.round()).abs() < 1e-9 {
        format!("{}", q.round() as i64)
    } else {
        format!("{q:.2}").replace('.', ",")
    }
}

pub fn parse_qty(s: &str) -> Option<f64> {
    s.trim().replace('.', "").replace(',', ".").parse().ok()
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%d.%m.%Y").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%d.%m.%Y")
        .or_else(|_| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d"))
        .ok()
}

pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
];

// ---------------------------------------------------------------------------
// Storage

pub fn data_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("OpenKontor").join("daten.json")
}

pub fn load() -> Option<Data> {
    let text = std::fs::read_to_string(data_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(data: &Data) -> Result<(), String> {
    let path = data_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Sample data so a new user sees how everything fits together.

pub fn sample_data() -> Data {
    let t = today();
    let mut d = Data {
        company: Company {
            name: "Muster Design & Technik".into(),
            owner: "Alex Beispiel".into(),
            street: "Hauptstraße 12".into(),
            zip_city: "10115 Berlin".into(),
            email: "info@muster-design.example".into(),
            phone: "030 1234567".into(),
            tax_number: "12/345/67890".into(),
            vat_id: "DE123456789".into(),
            bank: "Musterbank".into(),
            iban: "DE02 1203 0000 0000 2020 51".into(),
            bic: "BYLADEM1001".into(),
            ..Company::default()
        },
        ..Data::default()
    };

    let customers = [
        ("Bäckerei Sonnenschein GmbH", "Maria Korn", "Brotweg 3", "80331 München"),
        ("Grünwerk Gartenbau", "Jonas Blatt", "Am Feld 8", "50667 Köln"),
        ("Nordlicht Immobilien KG", "Sara Hafen", "Kai 21", "20095 Hamburg"),
        ("Praxis Dr. Weiß", "Dr. Lena Weiß", "Lindenallee 5", "04109 Leipzig"),
        ("Café Morgenrot", "Tom Bohne", "Marktplatz 1", "60311 Frankfurt"),
    ];
    for (company, person, street, city) in customers {
        let id = d.new_id();
        let number = d.next_contact_number();
        d.contacts.push(Contact {
            id,
            number,
            kind: ContactKind::Kunde,
            company: company.into(),
            person: person.into(),
            street: street.into(),
            zip_city: city.into(),
            email: format!(
                "kontakt@{}.example",
                company.split_whitespace().next().unwrap_or("firma").to_lowercase()
            ),
            ..Contact::default()
        });
    }
    let id = d.new_id();
    let number = d.next_contact_number();
    d.contacts.push(Contact {
        id,
        number,
        kind: ContactKind::Lieferant,
        company: "Bürowelt Versand".into(),
        street: "Industriestraße 40".into(),
        zip_city: "70173 Stuttgart".into(),
        ..Contact::default()
    });

    let articles = [
        ("Webdesign Stundensatz", "Stunde", 9500, 19, true),
        ("Logo-Entwicklung", "Pauschal", 85000, 19, true),
        ("Wartungsvertrag Website", "Monat", 4900, 19, true),
        ("Fotoshooting halbtags", "Pauschal", 39000, 19, true),
        ("Visitenkarten 500 Stk.", "Paket", 6900, 19, false),
        ("Fachbuch Gestaltung", "Stück", 3490, 7, false),
    ];
    for (name, unit, price, vat, service) in articles {
        let id = d.new_id();
        let number = d.next_article_number();
        d.articles.push(Article {
            id,
            number,
            name: name.into(),
            unit: unit.into(),
            price_net: price,
            vat,
            service,
            ..Article::default()
        });
    }

    // Invoices spread over the last months: (days ago, customer, [(article, qty)], paid?)
    let plan: [(i64, usize, &[(usize, f64)], bool); 12] = [
        (300, 0, &[(1, 1.0), (4, 2.0)], true),
        (270, 1, &[(0, 12.0)], true),
        (240, 2, &[(0, 20.0), (2, 3.0)], true),
        (205, 3, &[(3, 1.0)], true),
        (180, 4, &[(1, 1.0), (5, 3.0)], true),
        (150, 0, &[(0, 8.0)], true),
        (120, 2, &[(2, 6.0), (0, 10.0)], true),
        (90, 1, &[(3, 2.0)], true),
        (60, 3, &[(0, 16.0)], true),
        (35, 4, &[(0, 6.5), (4, 1.0)], false),
        (20, 2, &[(1, 1.0)], false),
        (6, 0, &[(0, 4.0), (2, 1.0)], false),
    ];
    for (ago, cust, items, paid) in plan {
        let date = t - Duration::days(ago);
        let lines = items
            .iter()
            .map(|&(a, qty)| {
                let art = &d.articles[a];
                Line {
                    article_id: Some(art.id),
                    text: art.name.clone(),
                    qty,
                    unit: art.unit.clone(),
                    price_net: art.price_net,
                    vat: art.vat,
                    discount_pct: 0.0,
                }
            })
            .collect();
        let id = d.new_id();
        let number = d.next_doc_number(DocKind::Rechnung, date.year());
        d.documents.push(Document {
            id,
            kind: DocKind::Rechnung,
            number,
            contact_id: Some(d.contacts[cust].id),
            date,
            due_days: 14,
            title: "Rechnung".into(),
            intro: "Vielen Dank für Ihren Auftrag. Wir berechnen Ihnen folgende Leistungen:".into(),
            outro: "Vielen Dank für die gute Zusammenarbeit.".into(),
            term_name: "14 Tage netto".into(),
            lines,
            status: if paid { DocStatus::Bezahlt } else { DocStatus::Offen },
            paid_date: paid.then(|| date + Duration::days(9)),
            ..Document::default()
        });
        if paid {
            d.book_payment(id, date + Duration::days(9));
        }
    }

    // One open offer and one draft.
    let id = d.new_id();
    let number = d.next_doc_number(DocKind::Angebot, t.year());
    let a0 = d.articles[0].clone();
    let a2 = d.articles[2].clone();
    d.documents.push(Document {
        id,
        kind: DocKind::Angebot,
        number,
        contact_id: Some(d.contacts[1].id),
        date: t - Duration::days(3),
        due_days: 30,
        title: "Angebot Relaunch Website".into(),
        intro: "Gerne unterbreiten wir Ihnen folgendes Angebot:".into(),
        outro: "Das Angebot ist 30 Tage gültig.".into(),
        lines: vec![
            Line {
                article_id: Some(a0.id),
                text: a0.name.clone(),
                qty: 40.0,
                unit: a0.unit.clone(),
                price_net: a0.price_net,
                vat: 19,
                discount_pct: 5.0,
            },
            Line {
                article_id: Some(a2.id),
                text: a2.name.clone(),
                qty: 12.0,
                unit: a2.unit.clone(),
                price_net: a2.price_net,
                vat: 19,
                discount_pct: 0.0,
            },
        ],
        status: DocStatus::Offen,
        ..Document::default()
    });

    // Expenses: monthly rent, phone, plus some one-offs.
    for m in 0..10 {
        let date = t - Duration::days(300 - m * 30);
        for (cat, text, partner, gross, vat) in [
            (2usize, "Büromiete", "Hausverwaltung Stein", 65000, 0u8),
            (10, "Mobilfunk & Internet", "Netzfunk AG", 5990, 19),
        ] {
            let id = d.new_id();
            d.bookings.push(Booking {
                id,
                date,
                kind: BookingKind::Ausgabe,
                category: EXPENSE_CATEGORIES[cat].into(),
                text: text.into(),
                partner: partner.into(),
                gross,
                vat,
                ..Booking::default()
            });
        }
    }
    for (ago, cat, text, partner, gross, vat) in [
        (280i64, 14usize, "Monitor 27 Zoll", "Technikmarkt", 34900, 19u8),
        (230, 9, "Druckerpapier und Toner", "Bürowelt Versand", 8930, 19),
        (190, 6, "Online-Anzeigen", "Werbenetz", 25000, 19),
        (140, 8, "Bahnticket Kundentermin", "Bahn", 11980, 7),
        (100, 12, "Steuerberatung Jahresabschluss", "Kanzlei Rat", 47600, 19),
        (70, 7, "Geschäftsessen", "Restaurant Linde", 8650, 19),
        (40, 11, "Fachliteratur", "Buchhandlung Seite", 4280, 7),
        (12, 9, "Notizbücher, Stifte", "Bürowelt Versand", 3570, 19),
    ] {
        let id = d.new_id();
        d.bookings.push(Booking {
            id,
            date: t - Duration::days(ago),
            kind: BookingKind::Ausgabe,
            category: EXPENSE_CATEGORIES[cat].into(),
            text: text.into(),
            partner: partner.into(),
            gross,
            vat,
            ..Booking::default()
        });
    }
    d.bookings.sort_by_key(|b| b.date);

    // Two customers get early-payment discount conditions; the newest open invoice uses them.
    for (contact, term) in [(0usize, 4u64), (2, 5)] {
        d.contacts[contact].payment_term_id = Some(term);
    }
    let term = d.company.term(Some(4)).cloned().unwrap();
    if let Some(last) = d
        .documents
        .iter_mut()
        .filter(|x| x.kind == DocKind::Rechnung && x.status == DocStatus::Offen)
        .max_by_key(|x| x.date)
    {
        last.apply_term(&term);
    }

    // Goods in stock: business cards and books are bought in and sold.
    for (i, purchase, min, start) in [(4usize, 3200, 5.0, 14.0), (5, 2100, 2.0, 6.0)] {
        d.articles[i].track_stock = true;
        d.articles[i].purchase_price = purchase;
        d.articles[i].min_stock = min;
        let (id, article_id) = (d.new_id(), d.articles[i].id);
        d.stock_moves.push(StockMove { id, date: t - Duration::days(320), article_id, qty: start, doc_id: None, note: "Anfangsbestand".into() });
    }
    let sold: Vec<(u64, NaiveDate, u64, f64, String)> = d
        .documents
        .iter()
        .filter(|x| x.kind == DocKind::Rechnung)
        .flat_map(|x| {
            x.lines.iter().filter_map(move |l| l.article_id.map(|a| (x.id, x.date, a, l.qty, format!("Rechnung {}", x.number))))
        })
        .collect();
    for (doc_id, date, article_id, qty, note) in sold {
        if d.articles.iter().any(|a| a.id == article_id && a.track_stock) {
            let id = d.new_id();
            d.stock_moves.push(StockMove { id, date, article_id, qty: -qty, doc_id: Some(doc_id), note });
        }
    }

    // Small purchases paid in cash, and a laptop as fixed asset.
    d.company.cash_opening = 30000;
    for b in d.bookings.iter_mut().filter(|b| b.text == "Geschäftsessen" || b.text == "Notizbücher, Stifte") {
        b.payment = CASH.into();
    }
    let laptop = d.new_id();
    d.bookings.push(Booking {
        id: laptop,
        date: t - Duration::days(200),
        kind: BookingKind::Ausgabe,
        category: ASSET_CATEGORY.into(),
        text: "Laptop 14 Zoll".into(),
        partner: "Technikmarkt".into(),
        gross: 178_381,
        vat: 19,
        ..Booking::default()
    });
    d.bookings.sort_by_key(|b| b.date);
    let id = d.new_id();
    d.assets.push(Asset {
        id,
        number: "AV-0001".into(),
        name: "Laptop 14 Zoll".into(),
        account: ASSET_ACCOUNTS[1].into(),
        date: t - Duration::days(200),
        cost: 149_900,
        years: 3,
        booking_id: Some(laptop),
        ..Asset::default()
    });

    // A maintenance contract invoiced every month.
    let customer = d.contacts[2].id;
    let wartung = d.articles[2].clone();
    let mut template = Document {
        contact_id: Some(customer),
        title: "Rechnung Wartungsvertrag".into(),
        intro: "Für die Wartung Ihrer Website berechnen wir Ihnen:".into(),
        outro: "Vielen Dank für die gute Zusammenarbeit.".into(),
        lines: vec![Line {
            article_id: Some(wartung.id),
            text: wartung.name.clone(),
            qty: 1.0,
            unit: wartung.unit.clone(),
            price_net: wartung.price_net,
            vat: 19,
            discount_pct: 0.0,
        }],
        ..Document::default()
    };
    template.apply_term(&d.term_for(Some(customer)));
    let id = d.new_id();
    d.recurring.push(Recurring {
        id,
        name: "Wartungsvertrag Website".into(),
        template,
        every_months: 1,
        next_date: t + Duration::days(9),
        ..Recurring::default()
    });

    // Bank statement lines: a customer payment, the phone bill and something unclear.
    let small = d.company.small_business;
    let open = d
        .documents
        .iter()
        .filter(|x| x.kind == DocKind::Rechnung && x.status == DocStatus::Offen)
        .min_by_key(|x| x.date)
        .map(|x| (x.number.clone(), x.open_amount(small), d.contact_name(x.contact_id)));
    let mut lines = Vec::new();
    if let Some((number, amount, name)) = open {
        lines.push(BankTx { date: t - Duration::days(2), amount, name, purpose: format!("Rechnung {number} vielen Dank"), reference: "demo-1".into(), ..BankTx::default() });
    }
    lines.push(BankTx { date: t - Duration::days(4), amount: -5990, name: "Netzfunk AG".into(), purpose: "Kundennr. 88123 Mobilfunk".into(), reference: "demo-2".into(), ..BankTx::default() });
    lines.push(BankTx { date: t - Duration::days(1), amount: -1250, name: "Parkhaus City".into(), purpose: "Parkticket".into(), reference: "demo-3".into(), ..BankTx::default() });
    for mut tx in lines {
        tx.id = d.new_id();
        tx.account = d.company.iban.replace(' ', "");
        d.bank.push(tx);
    }
    d.bank.sort_by_key(|t| (t.date, t.id));

    // A project with recorded hours, partly not yet invoiced.
    let pid = d.new_id();
    d.projects.push(Project {
        id: pid,
        number: "P-0001".into(),
        name: "Relaunch Website".into(),
        contact_id: Some(d.contacts[1].id),
        rate: 9500,
        budget_hours: 40.0,
        ..Project::default()
    });
    for (ago, hours, text, user) in [
        (12i64, 3.0, "Workshop Anforderungen", "Alex"),
        (9, 2.5, "Entwurf Startseite", "Alex"),
        (6, 4.0, "Umsetzung Vorlagen", "Sam"),
        (3, 1.5, "Abstimmung mit dem Kunden", "Alex"),
        (1, 3.5, "Inhalte übernehmen", "Sam"),
    ] {
        let id = d.new_id();
        d.times.push(TimeEntry { id, project_id: pid, date: t - Duration::days(ago), user: user.into(), hours, text: text.into(), invoice_id: None });
    }
    d.company.bank_balance = 1_245_000;
    d.company.bank_balance_date = Some(t);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_round_trip() {
        assert_eq!(fmt_amount(123456), "1.234,56");
        assert_eq!(fmt_amount(-5), "-0,05");
        assert_eq!(parse_amount("1.234,56"), Some(123456));
        assert_eq!(parse_amount("12.5"), Some(1250));
        assert_eq!(parse_amount("95 €"), Some(9500));
        assert_eq!(fmt_money_in(Lang::En, 123456), "€1,234.56");
        assert_eq!(fmt_money_in(Lang::En, -500), "-€5.00");
    }

    #[test]
    fn invoice_totals() {
        let doc = Document {
            lines: vec![
                Line { qty: 2.0, price_net: 10000, vat: 19, ..Line::default() },
                Line { qty: 1.0, price_net: 1000, vat: 7, ..Line::default() },
            ],
            ..Document::default()
        };
        let t = doc.totals(false);
        assert_eq!(t.net, 21000);
        assert_eq!(t.tax, 3800 + 70);
        assert_eq!(t.gross, 24870);
        assert_eq!(doc.totals(true).tax, 0);
    }

    #[test]
    fn early_payment_discounts() {
        let mut doc = Document {
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            lines: vec![Line { qty: 1.0, price_net: 100000, vat: 19, ..Line::default() }],
            ..Document::default()
        };
        doc.apply_term(&standard_terms()[4]); // 7 Tage 3 %, 14 Tage 2 %, 30 Tage netto
        let s = doc.payment_schedule(false);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], (NaiveDate::from_ymd_opt(2026, 11, 8).unwrap(), 3.0, 115430));
        assert_eq!(s[2].2, 119000);
        assert_eq!(doc.discount_on(NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()), 3.0);
        assert_eq!(doc.discount_on(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()), 2.0);
        assert_eq!(doc.discount_on(NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()), 0.0);
        let lines = payment_lines(&doc, false, Lang::De);
        assert_eq!(lines[0], "Bei Zahlung bis 08.11.2026 abzüglich 3 % Skonto: 1.154,30 €");

        // Paying with discount books less income and less VAT.
        let mut data = Data::default();
        doc.id = data.new_id();
        doc.status = DocStatus::Offen;
        data.documents.push(doc.clone());
        data.apply(Op::MarkPaidWithDiscount(doc.id, doc.date, 3.0), "test").unwrap();
        let b = &data.bookings[0];
        assert_eq!(b.gross, 115430);
        assert_eq!(b.net(), 97000);
    }
}
