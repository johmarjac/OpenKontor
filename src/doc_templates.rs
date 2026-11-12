// Document templates: how printed documents look and the texts they start with.
// Included into model.rs.

/// What the top of the first page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeaderStyle {
    #[default]
    NameUndLogo,
    NurLogo,
    NurName,
    /// Nothing, e.g. for printed letter paper or a background image.
    Keiner,
}

impl HeaderStyle {
    pub const ALL: [HeaderStyle; 4] = [HeaderStyle::NameUndLogo, HeaderStyle::NurLogo, HeaderStyle::NurName, HeaderStyle::Keiner];

    pub fn name(self) -> &'static str {
        match self {
            HeaderStyle::NameUndLogo => "Firmenname und Logo",
            HeaderStyle::NurLogo => "nur Logo",
            HeaderStyle::NurName => "nur Firmenname",
            HeaderStyle::Keiner => "leer (Briefpapier)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogoPos {
    Links,
    Mitte,
    #[default]
    Rechts,
}

impl LogoPos {
    pub const ALL: [LogoPos; 3] = [LogoPos::Links, LogoPos::Mitte, LogoPos::Rechts];

    pub fn name(self) -> &'static str {
        match self {
            LogoPos::Links => "links",
            LogoPos::Mitte => "Mitte",
            LogoPos::Rechts => "rechts",
        }
    }
}

/// Where the address sits, after DIN 5008.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressPos {
    /// Form B: address field 45 mm from the top (the usual one).
    #[default]
    FormB,
    /// Form A: address field 27 mm from the top, small letterhead.
    FormA,
}

impl AddressPos {
    pub const ALL: [AddressPos; 2] = [AddressPos::FormB, AddressPos::FormA];

    pub fn name(self) -> &'static str {
        match self {
            AddressPos::FormB => "DIN 5008 Form B (hoher Briefkopf)",
            AddressPos::FormA => "DIN 5008 Form A (flacher Briefkopf)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FooterStyle {
    /// Company, contact, tax and bank details in four columns.
    #[default]
    Automatisch,
    /// Four columns of the company's own text.
    Eigener,
    Keiner,
}

impl FooterStyle {
    pub const ALL: [FooterStyle; 3] = [FooterStyle::Automatisch, FooterStyle::Eigener, FooterStyle::Keiner];

    pub fn name(self) -> &'static str {
        match self {
            FooterStyle::Automatisch => "automatisch aus den Firmendaten",
            FooterStyle::Eigener => "eigener Text",
            FooterStyle::Keiner => "keine Fußzeile",
        }
    }
}

/// How printed documents look. One for all documents, and optionally one per kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PdfLayout {
    pub header: HeaderStyle,
    pub logo_pos: LogoPos,
    /// Height of the logo in millimetres.
    pub logo_height: f32,
    /// A coloured line under the letterhead.
    pub header_rule: bool,
    /// A line under the company name, e.g. a slogan.
    pub tagline: String,
    /// Colour of company name, line and headings (RGB).
    pub accent: [u8; 3],
    /// Size of the running text in points.
    pub font_size: f32,
    pub address: AddressPos,
    /// The small sender line above the address; empty = from the company details.
    pub sender_line: String,
    /// Page margins in millimetres.
    pub margin_left: f32,
    pub margin_right: f32,
    /// Marks at the left edge for folding and punching.
    pub fold_marks: bool,
    pub page_numbers: bool,
    pub show_position: bool,
    pub show_vat_column: bool,
    pub show_customer_number: bool,
    pub footer: FooterStyle,
    /// Up to four columns for the own footer; placeholders like {firma} are filled in.
    pub footer_columns: Vec<String>,
    /// Letter paper as a picture behind the page.
    pub background: Option<Logo>,
    pub background_first_page_only: bool,
    /// A signature or stamp under the closing text.
    pub signature: Option<Logo>,
}

impl Default for PdfLayout {
    fn default() -> Self {
        Self {
            header: HeaderStyle::NameUndLogo,
            logo_pos: LogoPos::Rechts,
            logo_height: 18.0,
            header_rule: true,
            tagline: String::new(),
            accent: [31, 112, 140],
            font_size: 10.0,
            address: AddressPos::FormB,
            sender_line: String::new(),
            margin_left: 25.0,
            margin_right: 20.0,
            fold_marks: false,
            page_numbers: true,
            show_position: true,
            show_vat_column: true,
            show_customer_number: true,
            footer: FooterStyle::Automatisch,
            footer_columns: vec![String::new(); 4],
            background: None,
            background_first_page_only: false,
            signature: None,
        }
    }
}

/// The kinds that can have their own layout and texts: the document kinds and the reminders.
/// The key is what is stored; the label is shown.
pub const TEMPLATE_KINDS: [(&str, &str); 7] = [
    ("Rechnung", "Rechnungen"),
    ("Gutschrift", "Gutschriften"),
    ("Angebot", "Angebote"),
    ("Auftragsbestätigung", "Auftragsbestätigungen"),
    ("Lieferschein", "Lieferscheine"),
    ("Bestellung", "Bestellungen"),
    ("Mahnung", "Zahlungserinnerungen & Mahnungen"),
];

/// Text kinds: the document kinds and each reminder level.
pub const TEXT_KINDS: [(&str, &str); 10] = [
    ("Rechnung", "Rechnung"),
    ("Gutschrift", "Gutschrift"),
    ("Angebot", "Angebot"),
    ("Auftragsbestätigung", "Auftragsbestätigung"),
    ("Lieferschein", "Lieferschein"),
    ("Bestellung", "Bestellung"),
    ("Zahlungserinnerung", "Zahlungserinnerung"),
    ("1. Mahnung", "1. Mahnung"),
    ("2. Mahnung", "2. Mahnung"),
    ("Letzte Mahnung", "Letzte Mahnung"),
];

/// A company's own title, opening and closing text for one kind in one language.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocText {
    pub kind: String,
    pub lang: Lang,
    pub title: String,
    pub intro: String,
    pub outro: String,
}

/// The built-in title, opening and closing text of a kind.
pub fn default_doc_text(kind: &str, lang: Lang) -> (String, String, String) {
    let l = |de: &'static str| crate::i18n::t_in(lang, de).to_string();
    let (title, intro, outro) = match kind {
        "Angebot" => ("Angebot", "Gerne unterbreiten wir Ihnen folgendes Angebot:", "Wir freuen uns auf Ihren Auftrag."),
        "Gutschrift" => ("Gutschrift", "Wir schreiben Ihnen gut:", ""),
        "Auftragsbestätigung" => (
            "Auftragsbestätigung",
            "Vielen Dank für Ihren Auftrag. Wir bestätigen Ihnen folgende Leistungen:",
            "Wir freuen uns auf die weitere Zusammenarbeit.",
        ),
        "Lieferschein" => ("Lieferschein", "Wir liefern Ihnen:", "Ware vollständig und in einwandfreiem Zustand erhalten:"),
        "Bestellung" => ("Bestellung", "Hiermit bestellen wir:", "Bitte bestätigen Sie uns den Liefertermin."),
        "Zahlungserinnerung" => (
            "Zahlungserinnerung",
            "sicher ist es Ihrer Aufmerksamkeit entgangen, dass die folgende Rechnung noch offen ist. Bitte überweisen Sie den Betrag bis zum unten genannten Datum.",
            "Sollten Sie die Zahlung inzwischen veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos.",
        ),
        "1. Mahnung" => (
            "1. Mahnung",
            "trotz unserer Zahlungserinnerung konnten wir zu der folgenden Rechnung noch keinen Zahlungseingang feststellen. Bitte überweisen Sie den offenen Betrag bis zum unten genannten Datum.",
            "Sollten Sie die Zahlung inzwischen veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos.",
        ),
        "2. Mahnung" => (
            "2. Mahnung",
            "leider ist die folgende Rechnung trotz Mahnung weiterhin offen. Wir bitten Sie dringend, den offenen Betrag bis zum unten genannten Datum zu überweisen.",
            "Sollten Sie die Zahlung inzwischen veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos.",
        ),
        "Letzte Mahnung" => (
            "Letzte Mahnung",
            "trotz mehrfacher Erinnerung ist die folgende Rechnung noch immer nicht bezahlt. Geht der offene Betrag nicht bis zum unten genannten Datum ein, werden wir die Forderung ohne weitere Ankündigung gerichtlich geltend machen oder an ein Inkassounternehmen übergeben.",
            "Sollten Sie die Zahlung inzwischen veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos.",
        ),
        _ => (
            "Rechnung",
            "Vielen Dank für Ihren Auftrag. Wir berechnen Ihnen folgende Leistungen:",
            "Vielen Dank für die gute Zusammenarbeit.",
        ),
    };
    // Letters (the reminders) start with the greeting.
    let letter = matches!(kind, "Zahlungserinnerung" | "1. Mahnung" | "2. Mahnung" | "Letzte Mahnung");
    let intro = if letter { format!("{{anrede}}\n\n{}", l(intro)) } else { l(intro) };
    (l(title), intro, if outro.is_empty() { String::new() } else { l(outro) })
}

/// The placeholders documents, e-mails and footers may use, with an explanation.
pub const PLACEHOLDERS: [(&str, &str); 23] = [
    ("{anrede}", "Anrede mit Name des Ansprechpartners"),
    ("{kunde}", "Name des Kunden"),
    ("{ansprechpartner}", "Ansprechpartner beim Kunden"),
    ("{kundennummer}", "Kundennummer"),
    ("{nummer}", "Belegnummer"),
    ("{art}", "Belegart, z. B. Rechnung"),
    ("{datum}", "Belegdatum"),
    ("{leistungsdatum}", "Leistungsdatum bzw. -zeitraum"),
    ("{betrag}", "Betrag brutto"),
    ("{netto}", "Betrag netto"),
    ("{offen}", "Noch offener Betrag"),
    ("{faellig}", "Fälligkeitsdatum"),
    ("{zahlung}", "Zahlungsbedingungen mit Skonto-Fristen"),
    ("{bezug}", "Vorbeleg, z. B. Angebot AN-2026-0003"),
    ("{firma}", "Ihr Firmenname"),
    ("{absender}", "Inhaber/in bzw. Absendername"),
    ("{iban}", "Ihre IBAN"),
    ("{telefon}", "Ihre Telefonnummer"),
    ("{email}", "Ihre E-Mail-Adresse"),
    ("{signatur}", "Ihre E-Mail-Signatur"),
    ("{gebuehr}", "Mahngebühr dieser Mahnung"),
    ("{gesamt}", "Offener Betrag mit allen Mahngebühren"),
    ("{neue_frist}", "Neue Zahlungsfrist der Mahnung"),
];

impl Company {
    /// The layout for a kind: its own if it has one, else the one for all documents.
    pub fn layout(&self, kind: &str) -> PdfLayout {
        self.pdf_layouts.get(kind).or_else(|| self.pdf_layouts.get("")).cloned().unwrap_or_default()
    }

    /// The company's own texts for a kind, if it set any.
    pub fn own_doc_text(&self, kind: &str, lang: Lang) -> Option<(String, String, String)> {
        self.doc_texts
            .iter()
            .find(|t| t.kind == kind && t.lang == lang)
            .map(|t| (t.title.clone(), t.intro.clone(), t.outro.clone()))
    }

    /// Title, opening and closing text for a new document of a kind.
    pub fn doc_text(&self, kind: &str, lang: Lang) -> (String, String, String) {
        self.own_doc_text(kind, lang).unwrap_or_else(|| default_doc_text(kind, lang))
    }

    /// Company placeholders only (for footers and file names).
    pub fn fill_company(&self, text: &str) -> String {
        if !text.contains('{') {
            return text.to_string();
        }
        let sender = if self.owner.trim().is_empty() { self.name.clone() } else { self.owner.clone() };
        [
            ("{firma}", self.name.clone()),
            ("{absender}", sender),
            ("{inhaber}", self.owner.clone()),
            ("{strasse}", self.street.clone()),
            ("{ort}", self.zip_city.clone()),
            ("{telefon}", self.phone.clone()),
            ("{email}", self.email.clone()),
            ("{steuernummer}", self.tax_number.clone()),
            ("{ustid}", self.vat_id.clone()),
            ("{bank}", self.bank.clone()),
            ("{iban}", self.iban.clone()),
            ("{bic}", self.bic.clone()),
            ("{signatur}", self.email_signature.clone()),
        ]
        .iter()
        .fold(text.to_string(), |acc, (key, value)| acc.replace(key, value))
    }

    /// The PDF file name for a document, from the company's pattern.
    pub fn pdf_file_name(&self, doc: &Document, customer: &str) -> String {
        let number = if doc.number.is_empty() { "Entwurf".to_string() } else { doc.number.clone() };
        let pattern = if self.file_name_pattern.trim().is_empty() { "{art} {nummer}" } else { self.file_name_pattern.as_str() };
        let name = [
            ("{art}", doc.kind.label().to_string()),
            ("{nummer}", number),
            ("{kunde}", customer.to_string()),
            ("{datum}", doc.date.format("%Y-%m-%d").to_string()),
            ("{jahr}", doc.date.format("%Y").to_string()),
            ("{firma}", self.name.clone()),
        ]
        .iter()
        .fold(pattern.to_string(), |acc, (key, value)| acc.replace(key, value));
        let clean: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || " -_.,()&".contains(c) { c } else { '_' })
            .collect();
        let clean = clean.trim().trim_matches('.').to_string();
        format!("{}.pdf", if clean.is_empty() { "Beleg".to_string() } else { clean })
    }
}

impl Data {
    /// Fills the placeholders of a text for a document. `fee` and `deadline` matter for reminders only.
    pub fn fill_text(&self, text: &str, doc: &Document, fee: Cents, deadline: NaiveDate) -> String {
        if !text.contains('{') {
            return text.to_string();
        }
        let contact = self.contact(doc.contact_id);
        let lang = contact.map(|k| k.language).unwrap_or_default();
        let small = self.company.small_business;
        let totals = doc.totals(small);
        let fees: Cents = doc.reminders.iter().map(|r| r.fee).sum::<Cents>() + fee;
        let person = contact.map(|k| k.person.trim().to_string()).unwrap_or_default();
        let greeting = match (lang, person.as_str()) {
            (Lang::De, "") => "Sehr geehrte Damen und Herren,".to_string(),
            (Lang::De, name) => format!("Guten Tag {name},"),
            (Lang::En, "") => "Dear Sir or Madam,".to_string(),
            (Lang::En, name) => format!("Dear {name},"),
        };
        let source = doc
            .source_id
            .and_then(|id| self.documents.iter().find(|d| d.id == id))
            .map(|s| format!("{} {}", crate::i18n::t_in(lang, s.kind.name()), s.number))
            .unwrap_or_default();
        let money = |v: Cents| fmt_money_cur(lang, v, doc.currency());
        let filled = [
            ("{anrede}", greeting),
            ("{kunde}", self.contact_name(doc.contact_id)),
            ("{ansprechpartner}", person),
            ("{kundennummer}", contact.map(|k| k.number.clone()).unwrap_or_default()),
            ("{nummer}", doc.number.clone()),
            ("{art}", crate::i18n::t_in(lang, doc.kind.name()).to_string()),
            ("{datum}", fmt_date_in(lang, doc.date)),
            ("{leistungsdatum}", if doc.service_date.trim().is_empty() { fmt_date_in(lang, doc.date) } else { doc.service_date.clone() }),
            ("{betrag}", money(totals.gross)),
            ("{netto}", money(totals.net)),
            ("{offen}", money(totals.gross - doc.paid_total())),
            ("{faellig}", fmt_date_in(lang, doc.due_date())),
            ("{zahlung}", payment_lines(doc, small, lang).join("\n")),
            ("{bezug}", source),
            ("{gebuehr}", money(fee)),
            ("{gesamt}", money(totals.gross + fees)),
            ("{neue_frist}", fmt_date_in(lang, deadline)),
        ]
        .iter()
        .fold(text.to_string(), |acc, (key, value)| acc.replace(key, value));
        self.company.fill_company(&filled)
    }
}
