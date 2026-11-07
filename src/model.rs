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
    pub next_invoice_no: u32,
    pub next_offer_no: u32,
    pub next_credit_no: u32,
    pub next_contact_no: u32,
    pub next_article_no: u32,
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
            next_invoice_no: 1,
            next_offer_no: 1,
            next_credit_no: 1,
            next_contact_no: 10001,
            next_article_no: 1,
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
}

impl MailKind {
    pub fn for_doc(kind: DocKind) -> MailKind {
        match kind {
            DocKind::Angebot => MailKind::Angebot,
            DocKind::Rechnung => MailKind::Rechnung,
            DocKind::Gutschrift => MailKind::Gutschrift,
        }
    }

    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            MailKind::Angebot => "Angebot",
            MailKind::Rechnung => "Rechnung",
            MailKind::Gutschrift => "Gutschrift",
            MailKind::Mahnung(level) => reminder_title(level),
        }
    }

    pub const ALL: [MailKind; 7] = [
        MailKind::Angebot,
        MailKind::Rechnung,
        MailKind::Gutschrift,
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    Angebot,
    Rechnung,
    Gutschrift,
}

impl DocKind {
    /// German name; also used in the change log.
    pub fn name(self) -> &'static str {
        match self {
            DocKind::Angebot => "Angebot",
            DocKind::Rechnung => "Rechnung",
            DocKind::Gutschrift => "Gutschrift",
        }
    }

    /// Name in the interface language.
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.name())
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
            let rate = if small_business { 0 } else { line.vat };
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
        }
    }
}

impl Booking {
    pub fn net(&self) -> Cents {
        (self.gross as f64 * 100.0 / (100.0 + self.vat as f64)).round() as Cents
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

pub const INCOME_CATEGORIES: [&str; 6] = [
    "8400 Erlöse 19 % USt",
    "8300 Erlöse 7 % USt",
    "8195 Erlöse Kleinunternehmer",
    "8120 Steuerfreie Umsätze",
    "2700 Sonstige Erträge",
    "1890 Privateinlage",
];

pub const EXPENSE_CATEGORIES: [&str; 16] = [
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
    pub next_id: u64,
}

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
}

impl Op {
    /// Changes only an administrator may make.
    pub fn admin_only(&self) -> bool {
        matches!(self, Op::ReplaceAll(_))
    }
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
                if self.documents.iter().any(|d| d.contact_id == Some(id)) {
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
                let (kind, year) = (d.kind, d.date.year());
                let number = self.next_doc_number(kind, year);
                let d = &mut self.documents[i];
                d.number = number.clone();
                d.status = DocStatus::Offen;
                Ok((Some(id), format!("{} {number} fertiggestellt", kind.name())))
            }
            Op::SetDocStatus(id, status) => {
                let i = self.doc_index(id)?;
                if self.documents[i].status == DocStatus::Entwurf {
                    return Err("Bitte den Beleg zuerst fertigstellen.".into());
                }
                if status == DocStatus::Storniert {
                    self.unbook_payment(id);
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
                let d = &mut self.documents[i];
                d.status = DocStatus::Offen;
                d.paid_date = None;
                d.paid_discount_pct = 0.0;
                let msg = format!("Zahlung zu {} {} zurückgenommen", d.kind.name(), d.number);
                self.unbook_payment(id);
                Ok((Some(id), msg))
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
                slot.status = ReceiptStatus::Neu;
                let ids = std::mem::take(&mut slot.booking_ids);
                let msg = format!("Buchung zu Beleg „{}“ zurückgenommen", slot.file_name);
                self.bookings.retain(|b| !ids.contains(&b.id));
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
        };
        let n = *counter;
        *counter += 1;
        format!("{prefix}{year}-{n:04}")
    }

    fn mark_paid(&mut self, id: u64, date: NaiveDate, pct: f64) -> Result<(Option<u64>, String), String> {
        let i = self.doc_index(id)?;
        let d = &mut self.documents[i];
        if d.kind == DocKind::Angebot || d.status == DocStatus::Entwurf {
            return Err("Nur fertige Rechnungen und Gutschriften können bezahlt werden.".into());
        }
        d.status = DocStatus::Bezahlt;
        d.paid_date = Some(date);
        d.paid_discount_pct = pct;
        let msg = if pct > 0.0 {
            format!("{} {} als bezahlt gebucht (abzüglich {} % Skonto)", d.kind.name(), d.number, fmt_qty(pct))
        } else {
            format!("{} {} als bezahlt gebucht", d.kind.name(), d.number)
        };
        self.book_payment(id, date);
        Ok((Some(id), msg))
    }

    /// Books a paid invoice (or credit note) as income, one booking per VAT rate.
    /// An early-payment discount reduces net and VAT alike (§ 17 UStG).
    pub fn book_payment(&mut self, doc_id: u64, paid: NaiveDate) {
        let Some(doc) = self.documents.iter().find(|d| d.id == doc_id).cloned() else {
            return;
        };
        let small = self.company.small_business;
        let partner = self.contact_name(doc.contact_id);
        let totals = doc.totals(small);
        self.bookings.retain(|b| b.doc_id != Some(doc_id));
        let factor = 1.0 - doc.paid_discount_pct / 100.0;
        let text = if doc.paid_discount_pct > 0.0 {
            format!("{} {} (abzgl. {} % Skonto)", doc.kind.name(), doc.number, fmt_qty(doc.paid_discount_pct))
        } else {
            format!("{} {}", doc.kind.name(), doc.number)
        };
        for (rate, (net, tax)) in totals.by_rate {
            let gross = ((net + tax) as f64 * factor).round() as Cents;
            if gross == 0 {
                continue;
            }
            let id = self.new_id();
            self.bookings.push(Booking {
                id,
                date: paid,
                kind: if gross >= 0 {
                    BookingKind::Einnahme
                } else {
                    BookingKind::Ausgabe
                },
                category: income_category_for_rate(rate, small).into(),
                text: text.clone(),
                partner: partner.clone(),
                receipt: doc.number.clone(),
                gross: gross.abs(),
                vat: rate,
                doc_id: Some(doc_id),
                payment: "Bank".into(),
                receipt_id: None,
            });
        }
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
}

impl PeriodReport {
    pub fn profit(&self) -> Cents {
        self.income_net - self.expense_net
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
                match b.vat {
                    19 => {
                        r.base_19 += net;
                        r.tax_19 += tax;
                    }
                    7 => {
                        r.base_7 += net;
                        r.tax_7 += tax;
                    }
                    _ => r.base_0 += net,
                }
            }
            BookingKind::Ausgabe => {
                *r.expense_by_cat.entry(b.category.clone()).or_default() += net;
                r.expense_net += net;
                r.vat_paid += tax;
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
        .filter(|b| b.date.year() == year && !is_private(&b.category))
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
                    &[&fmt_date_in(lang, *date), &fmt_qty_in(lang, *pct), &fmt_money_in(lang, *amount)],
                )
            } else {
                fill(
                    t_in(lang, "Bei Zahlung bis {} ohne Abzug: {}"),
                    &[&fmt_date_in(lang, *date), &fmt_money_in(lang, *amount)],
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
