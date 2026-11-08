//! Creating e-invoices: the structured invoice data in the EN 16931 format, CII syntax.
//!
//! The same XML serves as an XRechnung (with the German XRechnung rules, for public
//! authorities and anyone who asks for it) and as the data inside a ZUGFeRD / Factur-X PDF
//! (profile EN 16931). Before writing, the mandatory details are checked and missing ones
//! are reported in plain words.

use crate::model::*;
use serde::{Deserialize, Serialize};

/// How a customer receives invoices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EInvoiceFormat {
    /// PDF with the invoice data embedded (readable by people and programs).
    #[default]
    Zugferd,
    /// Pure XML as required by public authorities (with a PDF copy for reading).
    XRechnung,
    /// Only a PDF (no e-invoice; allowed only in a few cases, e.g. private customers).
    NurPdf,
}

impl EInvoiceFormat {
    pub const ALL: [EInvoiceFormat; 3] = [EInvoiceFormat::Zugferd, EInvoiceFormat::XRechnung, EInvoiceFormat::NurPdf];
    pub fn name(self) -> &'static str {
        match self {
            EInvoiceFormat::Zugferd => "E-Rechnung ZUGFeRD (PDF mit Daten)",
            EInvoiceFormat::XRechnung => "E-Rechnung XRechnung (XML)",
            EInvoiceFormat::NurPdf => "Nur PDF (keine E-Rechnung)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// German CIUS with its extra rules (buyer reference, seller contact …).
    XRechnung,
    /// The European core standard, used inside ZUGFeRD PDFs.
    En16931,
}

const XRECHNUNG_ID: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
const EN16931_ID: &str = "urn:cen.eu:en16931:2017";
const PEPPOL_PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn amount(c: Cents) -> String {
    format!("{}{}.{:02}", if c < 0 { "-" } else { "" }, c.abs() / 100, c.abs() % 100)
}

/// Up to four decimals, without trailing zeros (quantities, unit prices).
fn decimal(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

fn date(d: chrono::NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// "10115 Berlin" → ("10115", "Berlin").
pub fn split_zip_city(s: &str) -> (String, String) {
    let s = s.trim();
    match s.split_once(char::is_whitespace) {
        Some((zip, city)) if zip.chars().any(|c| c.is_ascii_digit()) => (zip.to_string(), city.trim().to_string()),
        _ => (String::new(), s.to_string()),
    }
}

/// ISO country code from a country name as people write it.
pub fn country_code(name: &str) -> String {
    let n = name.trim().to_lowercase();
    let code = match n.as_str() {
        "" | "deutschland" | "germany" | "de" | "brd" => "DE",
        "österreich" | "austria" => "AT",
        "schweiz" | "switzerland" | "suisse" => "CH",
        "frankreich" | "france" => "FR",
        "niederlande" | "netherlands" | "holland" => "NL",
        "belgien" | "belgium" => "BE",
        "luxemburg" | "luxembourg" => "LU",
        "italien" | "italy" => "IT",
        "spanien" | "spain" => "ES",
        "polen" | "poland" => "PL",
        "dänemark" | "denmark" => "DK",
        "tschechien" | "czech republic" | "czechia" => "CZ",
        "vereinigtes königreich" | "united kingdom" | "großbritannien" | "uk" => "GB",
        "usa" | "vereinigte staaten" | "united states" => "US",
        other if other.len() == 2 => return other.to_uppercase(),
        _ => "DE",
    };
    code.into()
}

/// UN/ECE Recommendation 20 unit code for the unit as written on the invoice.
pub fn unit_code(unit: &str) -> &'static str {
    match unit.trim().to_lowercase().trim_end_matches('.') {
        "stunde" | "stunden" | "std" | "h" | "hour" | "hours" => "HUR",
        "minute" | "minuten" | "min" => "MIN",
        "tag" | "tage" | "day" | "days" => "DAY",
        "woche" | "wochen" | "week" => "WEE",
        "monat" | "monate" | "month" | "months" => "MON",
        "jahr" | "jahre" | "year" => "ANN",
        "pauschal" | "pausch" | "psch" | "pauschale" | "lump sum" => "LS",
        "kg" | "kilogramm" => "KGM",
        "g" | "gramm" => "GRM",
        "t" | "tonne" | "tonnen" => "TNE",
        "m" | "meter" => "MTR",
        "km" | "kilometer" => "KMT",
        "m²" | "m2" | "qm" | "quadratmeter" => "MTK",
        "m³" | "m3" | "kubikmeter" => "MTQ",
        "l" | "liter" => "LTR",
        "paket" | "pakete" | "pack" | "packung" => "XPK",
        "stück" | "stk" | "st" | "piece" | "pieces" | "pcs" => "H87",
        _ => "C62",
    }
}

/// The checked figures of a document, positive for credit notes too (the type code tells).
struct Figures {
    lines: Vec<(Line, Cents)>,
    by_rate: Vec<(u8, Cents, Cents)>,
    net: Cents,
    tax: Cents,
    gross: Cents,
}

fn figures(doc: &Document, small: bool) -> Figures {
    let t = doc.totals(small);
    Figures {
        lines: doc.lines.iter().map(|l| (l.clone(), l.net())).collect(),
        by_rate: t.by_rate.iter().map(|(r, (n, x))| (*r, n.abs(), x.abs())).collect(),
        net: t.net.abs(),
        tax: t.tax.abs(),
        gross: t.gross.abs(),
    }
}

/// What is missing for an e-invoice of this document, in plain words. Empty = all fine.
pub fn check(data: &Data, doc: &Document, profile: Profile) -> Vec<String> {
    let c = &data.company;
    let mut miss = Vec::new();
    let xr = profile == Profile::XRechnung;
    if !matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift) {
        miss.push("E-Rechnungen gibt es nur für Rechnungen und Gutschriften.".to_string());
        return miss;
    }
    if doc.number.trim().is_empty() {
        miss.push("Der Beleg ist noch nicht fertiggestellt (Rechnungsnummer fehlt).".into());
    }
    if c.name.trim().is_empty() {
        miss.push("Firmendaten: Firmenname fehlt.".into());
    }
    let (zip, city) = split_zip_city(&c.zip_city);
    if c.street.trim().is_empty() || city.is_empty() || (xr && zip.is_empty()) {
        miss.push("Firmendaten: Straße sowie PLZ und Ort vollständig angeben.".into());
    }
    if c.vat_id.trim().is_empty() && c.tax_number.trim().is_empty() {
        miss.push("Firmendaten: USt-IdNr. oder Steuernummer fehlt.".into());
    }
    if c.iban.trim().is_empty() {
        miss.push("Firmendaten: IBAN fehlt (für die Zahlungsangaben).".into());
    }
    if xr {
        if c.email.trim().is_empty() {
            miss.push("Firmendaten: E-Mail-Adresse fehlt.".into());
        }
        if c.phone.trim().is_empty() {
            miss.push("Firmendaten: Telefonnummer fehlt (XRechnung verlangt einen Ansprechpartner mit Telefon).".into());
        }
    }
    match data.contact(doc.contact_id) {
        None => miss.push("Kein Kunde ausgewählt.".into()),
        Some(k) => {
            if k.display_name().trim().is_empty() || k.display_name() == "(ohne Namen)" {
                miss.push("Kunde: Name fehlt.".into());
            }
            let (kzip, kcity) = split_zip_city(&k.zip_city);
            if kcity.is_empty() || (xr && kzip.is_empty()) {
                miss.push(format!("Kunde {}: PLZ und Ort fehlen.", k.display_name()));
            }
            if xr && k.email.trim().is_empty() {
                miss.push(format!("Kunde {}: E-Mail-Adresse fehlt (Pflicht in der XRechnung).", k.display_name()));
            }
            if xr && k.buyer_reference.trim().is_empty() && k.number.trim().is_empty() {
                miss.push(format!("Kunde {}: Leitweg-ID bzw. Käuferreferenz fehlt.", k.display_name()));
            }
        }
    }
    if doc.lines.iter().all(|l| l.text.trim().is_empty()) {
        miss.push("Der Beleg hat keine Positionen.".into());
    }
    if doc.lines.iter().any(|l| l.text.trim().is_empty()) {
        miss.push("Jede Position braucht eine Bezeichnung.".into());
    }
    miss
}

/// The e-invoice XML (CII) for a finished invoice or credit note.
pub fn build(data: &Data, doc: &Document, profile: Profile) -> Result<String, Vec<String>> {
    let problems = check(data, doc, profile);
    if !problems.is_empty() {
        return Err(problems);
    }
    let c = &data.company;
    let k = data.contact(doc.contact_id).expect("checked above");
    let small = c.small_business;
    let f = figures(doc, small);
    let credit = doc.kind == DocKind::Gutschrift;
    let mut x = String::with_capacity(8192);
    let mut push = |s: &str| {
        x.push_str(s);
        x.push('\n');
    };

    push(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    push(r#"<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:qdt="urn:un:unece:uncefact:data:standard:QualifiedDataType:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">"#);
    push("  <rsm:ExchangedDocumentContext>");
    push(&format!("    <ram:BusinessProcessSpecifiedDocumentContextParameter><ram:ID>{PEPPOL_PROCESS}</ram:ID></ram:BusinessProcessSpecifiedDocumentContextParameter>"));
    let guideline = if profile == Profile::XRechnung { XRECHNUNG_ID } else { EN16931_ID };
    push(&format!("    <ram:GuidelineSpecifiedDocumentContextParameter><ram:ID>{guideline}</ram:ID></ram:GuidelineSpecifiedDocumentContextParameter>"));
    push("  </rsm:ExchangedDocumentContext>");

    push("  <rsm:ExchangedDocument>");
    push(&format!("    <ram:ID>{}</ram:ID>", esc(&doc.number)));
    // 326 = partial (advance) invoice, 380 = invoice, 381 = credit note.
    let type_code = if credit {
        381
    } else if doc.part == InvoicePart::Abschlag {
        326
    } else {
        380
    };
    push(&format!("    <ram:TypeCode>{type_code}</ram:TypeCode>"));
    push(&format!("    <ram:IssueDateTime><udt:DateTimeString format=\"102\">{}</udt:DateTimeString></ram:IssueDateTime>", date(doc.date)));
    for note in [&doc.title, &doc.intro, &doc.service_date].iter().filter(|s| !s.trim().is_empty()) {
        push(&format!("    <ram:IncludedNote><ram:Content>{}</ram:Content></ram:IncludedNote>", esc(note.trim())));
    }
    if small {
        push("    <ram:IncludedNote><ram:Content>Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.</ram:Content></ram:IncludedNote>");
    } else if let Some(note) = doc.vat_case.invoice_note() {
        push(&format!("    <ram:IncludedNote><ram:Content>{}</ram:Content></ram:IncludedNote>", esc(note)));
    }
    let case = if small { VatCase::Inland } else { doc.vat_case };
    // "Not subject to VAT" invoices carry no VAT identifiers (BR-O-2).
    let show_vat_ids = case != VatCase::DrittlandLeistung;
    push("  </rsm:ExchangedDocument>");

    push("  <rsm:SupplyChainTradeTransaction>");
    for (i, (l, net)) in f.lines.iter().enumerate() {
        let (cat, rate) = vat_category(small, case, l.vat);
        let rate = rate_element(cat, &rate);
        // Prices are never negative; a deduction is a negative quantity instead.
        let (price, qty) = if l.price_net < 0 { (-l.price_net, -l.qty) } else { (l.price_net, l.qty) };
        push("    <ram:IncludedSupplyChainTradeLineItem>");
        push(&format!("      <ram:AssociatedDocumentLineDocument><ram:LineID>{}</ram:LineID></ram:AssociatedDocumentLineDocument>", i + 1));
        push(&format!("      <ram:SpecifiedTradeProduct><ram:Name>{}</ram:Name></ram:SpecifiedTradeProduct>", esc(l.text.trim())));
        push("      <ram:SpecifiedLineTradeAgreement>");
        let gross_price = price as f64 / 100.0;
        let net_price = gross_price * (1.0 - l.discount_pct / 100.0);
        if l.discount_pct > 0.0 {
            push(&format!(
                "        <ram:GrossPriceProductTradePrice><ram:ChargeAmount>{}</ram:ChargeAmount><ram:AppliedTradeAllowanceCharge><ram:ChargeIndicator><udt:Indicator>false</udt:Indicator></ram:ChargeIndicator><ram:ActualAmount>{}</ram:ActualAmount></ram:AppliedTradeAllowanceCharge></ram:GrossPriceProductTradePrice>",
                decimal(gross_price),
                decimal(gross_price - net_price)
            ));
        }
        push(&format!("        <ram:NetPriceProductTradePrice><ram:ChargeAmount>{}</ram:ChargeAmount></ram:NetPriceProductTradePrice>", decimal(net_price)));
        push("      </ram:SpecifiedLineTradeAgreement>");
        push(&format!(
            "      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode=\"{}\">{}</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>",
            unit_code(&l.unit),
            decimal(qty)
        ));
        push("      <ram:SpecifiedLineTradeSettlement>");
        push(&format!(
            "        <ram:ApplicableTradeTax><ram:TypeCode>VAT</ram:TypeCode><ram:CategoryCode>{cat}</ram:CategoryCode>{rate}</ram:ApplicableTradeTax>"
        ));
        push(&format!("        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>{}</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>", amount(*net)));
        push("      </ram:SpecifiedLineTradeSettlement>");
        push("    </ram:IncludedSupplyChainTradeLineItem>");
    }

    // Seller and buyer.
    push("    <ram:ApplicableHeaderTradeAgreement>");
    let buyer_ref = if k.buyer_reference.trim().is_empty() { k.number.trim() } else { k.buyer_reference.trim() };
    if !buyer_ref.is_empty() {
        push(&format!("      <ram:BuyerReference>{}</ram:BuyerReference>", esc(buyer_ref)));
    }
    push("      <ram:SellerTradeParty>");
    if !show_vat_ids {
        // Without a VAT id the seller needs another identifier (BR-CO-26).
        let id = if c.tax_number.trim().is_empty() { c.name.trim() } else { c.tax_number.trim() };
        push(&format!("        <ram:ID>{}</ram:ID>", esc(id)));
    }
    push(&format!("        <ram:Name>{}</ram:Name>", esc(&c.name)));
    let contact_name = if c.owner.trim().is_empty() { c.name.trim() } else { c.owner.trim() };
    let mut contact = format!("        <ram:DefinedTradeContact><ram:PersonName>{}</ram:PersonName>", esc(contact_name));
    if !c.phone.trim().is_empty() {
        contact.push_str(&format!("<ram:TelephoneUniversalCommunication><ram:CompleteNumber>{}</ram:CompleteNumber></ram:TelephoneUniversalCommunication>", esc(c.phone.trim())));
    }
    if !c.email.trim().is_empty() {
        contact.push_str(&format!("<ram:EmailURIUniversalCommunication><ram:URIID>{}</ram:URIID></ram:EmailURIUniversalCommunication>", esc(c.email.trim())));
    }
    contact.push_str("</ram:DefinedTradeContact>");
    push(&contact);
    push(&address(&c.street, &c.zip_city, "DE"));
    if !c.email.trim().is_empty() {
        push(&format!("        <ram:URIUniversalCommunication><ram:URIID schemeID=\"EM\">{}</ram:URIID></ram:URIUniversalCommunication>", esc(c.email.trim())));
    }
    if !c.tax_number.trim().is_empty() {
        push(&format!("        <ram:SpecifiedTaxRegistration><ram:ID schemeID=\"FC\">{}</ram:ID></ram:SpecifiedTaxRegistration>", esc(c.tax_number.trim())));
    }
    if !c.vat_id.trim().is_empty() && show_vat_ids {
        push(&format!("        <ram:SpecifiedTaxRegistration><ram:ID schemeID=\"VA\">{}</ram:ID></ram:SpecifiedTaxRegistration>", esc(&c.vat_id.replace(' ', ""))));
    }
    push("      </ram:SellerTradeParty>");
    push("      <ram:BuyerTradeParty>");
    if !k.number.trim().is_empty() {
        push(&format!("        <ram:ID>{}</ram:ID>", esc(k.number.trim())));
    }
    push(&format!("        <ram:Name>{}</ram:Name>", esc(&k.display_name())));
    if !k.person.trim().is_empty() && !k.company.trim().is_empty() {
        push(&format!("        <ram:DefinedTradeContact><ram:PersonName>{}</ram:PersonName></ram:DefinedTradeContact>", esc(k.person.trim())));
    }
    push(&address(&k.street, &k.zip_city, &country_code(&k.country)));
    if !k.email.trim().is_empty() {
        push(&format!("        <ram:URIUniversalCommunication><ram:URIID schemeID=\"EM\">{}</ram:URIID></ram:URIUniversalCommunication>", esc(k.email.trim())));
    }
    if !k.vat_id.trim().is_empty() && show_vat_ids {
        push(&format!("        <ram:SpecifiedTaxRegistration><ram:ID schemeID=\"VA\">{}</ram:ID></ram:SpecifiedTaxRegistration>", esc(&k.vat_id.replace(' ', ""))));
    }
    push("      </ram:BuyerTradeParty>");
    push("    </ram:ApplicableHeaderTradeAgreement>");

    // Delivery date: the stated service date if it is a date, else the invoice date.
    let delivered = parse_date(&doc.service_date).unwrap_or(doc.date);
    // Intra-community deliveries name the country the goods go to (BR-IC-12).
    let ship_to = if case == VatCase::IgLieferung {
        format!(
            "<ram:ShipToTradeParty>{}</ram:ShipToTradeParty>",
            address(&k.street, &k.zip_city, &country_code(&k.country)).trim()
        )
    } else {
        String::new()
    };
    push(&format!(
        "    <ram:ApplicableHeaderTradeDelivery>{ship_to}<ram:ActualDeliverySupplyChainEvent><ram:OccurrenceDateTime><udt:DateTimeString format=\"102\">{}</udt:DateTimeString></ram:OccurrenceDateTime></ram:ActualDeliverySupplyChainEvent></ram:ApplicableHeaderTradeDelivery>",
        date(delivered)
    ));

    push("    <ram:ApplicableHeaderTradeSettlement>");
    push(&format!("      <ram:PaymentReference>{}</ram:PaymentReference>", esc(&doc.number)));
    // Another currency also names euro as the tax currency (BR-53, BT-6).
    if doc.is_foreign() {
        push("      <ram:TaxCurrencyCode>EUR</ram:TaxCurrencyCode>");
    }
    push(&format!("      <ram:InvoiceCurrencyCode>{}</ram:InvoiceCurrencyCode>", esc(doc.currency())));
    let mut means = format!(
        "      <ram:SpecifiedTradeSettlementPaymentMeans><ram:TypeCode>58</ram:TypeCode><ram:PayeePartyCreditorFinancialAccount><ram:IBANID>{}</ram:IBANID><ram:AccountName>{}</ram:AccountName></ram:PayeePartyCreditorFinancialAccount>",
        esc(&c.iban.replace(' ', "")),
        esc(&c.name)
    );
    if !c.bic.trim().is_empty() {
        means.push_str(&format!("<ram:PayeeSpecifiedCreditorFinancialInstitution><ram:BICID>{}</ram:BICID></ram:PayeeSpecifiedCreditorFinancialInstitution>", esc(c.bic.trim())));
    }
    means.push_str("</ram:SpecifiedTradeSettlementPaymentMeans>");
    push(&means);
    for (rate, net, tax) in &f.by_rate {
        let (cat, rate_text) = vat_category(small, case, *rate);
        // The breakdown always names a rate (BR-DE-14), 0 when not subject to VAT.
        let rate_text = format!("<ram:RateApplicablePercent>{}</ram:RateApplicablePercent>", if rate_text.is_empty() { "0" } else { &rate_text });
        let reason = match cat {
            "E" => "<ram:ExemptionReason>Kein Ausweis von Umsatzsteuer, da Kleinunternehmer gemäß § 19 UStG</ram:ExemptionReason>",
            "K" => "<ram:ExemptionReason>Steuerfreie innergemeinschaftliche Lieferung (§ 4 Nr. 1b UStG)</ram:ExemptionReason>",
            "AE" => "<ram:ExemptionReason>Steuerschuldnerschaft des Leistungsempfängers</ram:ExemptionReason>",
            "G" => "<ram:ExemptionReason>Steuerfreie Ausfuhrlieferung (§ 4 Nr. 1a UStG)</ram:ExemptionReason>",
            "O" => "<ram:ExemptionReason>Im Inland nicht steuerbare Leistung</ram:ExemptionReason>",
            _ => "",
        };
        push(&format!(
            "      <ram:ApplicableTradeTax><ram:CalculatedAmount>{}</ram:CalculatedAmount><ram:TypeCode>VAT</ram:TypeCode>{reason}<ram:BasisAmount>{}</ram:BasisAmount><ram:CategoryCode>{cat}</ram:CategoryCode>{rate_text}</ram:ApplicableTradeTax>",
            amount(*tax),
            amount(*net)
        ));
    }
    // Payment terms; early-payment discounts in the notation XRechnung expects.
    let mut terms = String::from("      <ram:SpecifiedTradePaymentTerms>");
    let discounts: Vec<&DiscountStage> = doc.discounts.iter().filter(|s| s.percent > 0.0 && s.days < doc.due_days).collect();
    let description = if credit {
        "Der Betrag wird erstattet bzw. verrechnet.".to_string()
    } else if discounts.is_empty() {
        if doc.due_days == 0 {
            "Zahlbar sofort ohne Abzug.".to_string()
        } else {
            format!("Zahlbar bis {} ohne Abzug.", fmt_date(doc.due_date()))
        }
    } else {
        discounts.iter().map(|s| format!("#SKONTO#TAGE={}#PROZENT={:.2}#\n", s.days, s.percent)).collect()
    };
    terms.push_str(&format!("<ram:Description>{}</ram:Description>", esc(&description)));
    if !credit {
        terms.push_str(&format!("<ram:DueDateDateTime><udt:DateTimeString format=\"102\">{}</udt:DateTimeString></ram:DueDateDateTime>", date(doc.due_date())));
    }
    terms.push_str("</ram:SpecifiedTradePaymentTerms>");
    push(&terms);
    push("      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>");
    let line_total: Cents = f.lines.iter().map(|(_, n)| n).sum();
    push(&format!("        <ram:LineTotalAmount>{}</ram:LineTotalAmount>", amount(line_total)));
    push(&format!("        <ram:TaxBasisTotalAmount>{}</ram:TaxBasisTotalAmount>", amount(f.net)));
    push(&format!("        <ram:TaxTotalAmount currencyID=\"{}\">{}</ram:TaxTotalAmount>", esc(doc.currency()), amount(f.tax)));
    if doc.is_foreign() {
        push(&format!("        <ram:TaxTotalAmount currencyID=\"EUR\">{}</ram:TaxTotalAmount>", amount(doc.to_eur(f.tax as Cents).abs())));
    }
    push(&format!("        <ram:GrandTotalAmount>{}</ram:GrandTotalAmount>", amount(f.gross)));
    push(&format!("        <ram:DuePayableAmount>{}</ram:DuePayableAmount>", amount(f.gross)));
    push("      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>");
    if let Some(src) = doc.source_id.and_then(|id| data.documents.iter().find(|d| d.id == id && d.kind == DocKind::Rechnung)) {
        push(&format!(
            "      <ram:InvoiceReferencedDocument><ram:IssuerAssignedID>{}</ram:IssuerAssignedID><ram:FormattedIssueDateTime><qdt:DateTimeString format=\"102\">{}</qdt:DateTimeString></ram:FormattedIssueDateTime></ram:InvoiceReferencedDocument>",
            esc(&src.number),
            date(src.date)
        ));
    }
    push("    </ram:ApplicableHeaderTradeSettlement>");
    push("  </rsm:SupplyChainTradeTransaction>");
    push("</rsm:CrossIndustryInvoice>");
    Ok(x)
}

/// VAT category code and rate text: S standard, Z zero rate, E exempt (small business),
/// K intra-community supply, AE reverse charge, G export, O not subject to VAT.
fn vat_category(small: bool, case: VatCase, rate: u8) -> (&'static str, String) {
    match (small, case, rate) {
        (true, _, _) => ("E", "0".into()),
        (false, VatCase::IgLieferung, _) => ("K", "0".into()),
        (false, VatCase::ReverseCharge, _) => ("AE", "0".into()),
        (false, VatCase::Ausfuhr, _) => ("G", "0".into()),
        (false, VatCase::DrittlandLeistung, _) => ("O", String::new()),
        (false, VatCase::Inland, 0) => ("Z", "0".into()),
        (false, VatCase::Inland, r) => ("S", r.to_string()),
    }
}

/// The rate element; "not subject to VAT" has none (BR-O-5).
fn rate_element(cat: &str, rate: &str) -> String {
    if cat == "O" { String::new() } else { format!("<ram:RateApplicablePercent>{rate}</ram:RateApplicablePercent>") }
}

fn address(street: &str, zip_city: &str, country: &str) -> String {
    let (zip, city) = split_zip_city(zip_city);
    let mut a = String::from("        <ram:PostalTradeAddress>");
    if !zip.is_empty() {
        a.push_str(&format!("<ram:PostcodeCode>{}</ram:PostcodeCode>", esc(&zip)));
    }
    if !street.trim().is_empty() {
        a.push_str(&format!("<ram:LineOne>{}</ram:LineOne>", esc(street.trim())));
    }
    a.push_str(&format!("<ram:CityName>{}</ram:CityName><ram:CountryID>{country}</ram:CountryID></ram:PostalTradeAddress>", esc(&city)));
    a
}

/// File name of the XRechnung, e.g. "XRechnung RE-2026-0012.xml".
pub fn file_name(doc: &Document) -> String {
    let pdf = crate::pdf::file_name(doc);
    let number = pdf.split_once(' ').map(|(_, n)| n).unwrap_or(&pdf).trim_end_matches(".pdf");
    format!("XRechnung {number}.xml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoice(data: &Data) -> Document {
        data.documents.iter().find(|d| d.kind == DocKind::Rechnung && !d.number.is_empty()).unwrap().clone()
    }

    #[test]
    fn sample_invoice_becomes_xml_that_reads_back() {
        let data = sample_data();
        let doc = invoice(&data);
        let xml = build(&data, &doc, Profile::XRechnung).unwrap();
        let back = crate::einvoice::parse_xml(&xml).unwrap();
        let t = doc.totals(false);
        assert_eq!(back.number, doc.number);
        assert_eq!((back.net, back.tax, back.gross), (t.net, t.tax, t.gross));
        assert_eq!(back.seller, data.company.name);
        assert!(xml.contains("xrechnung_3.0"));
    }

    #[test]
    fn missing_details_are_named() {
        let mut data = sample_data();
        data.company.iban.clear();
        let doc = invoice(&data);
        let cid = doc.contact_id.unwrap();
        data.contacts.iter_mut().find(|c| c.id == cid).unwrap().email.clear();
        let problems = check(&data, &doc, Profile::XRechnung);
        assert!(problems.iter().any(|p| p.contains("IBAN")));
        assert!(problems.iter().any(|p| p.contains("E-Mail")));
        // The EN 16931 core needs fewer details than XRechnung.
        assert!(!check(&data, &doc, Profile::En16931).iter().any(|p| p.contains("E-Mail")));
    }

    /// Writes example e-invoices for checking with the official validators:
    /// OPENKONTOR_EINVOICE_OUT=/some/folder cargo test write_examples
    #[test]
    fn write_examples() {
        let Ok(dir) = std::env::var("OPENKONTOR_EINVOICE_OUT") else { return };
        let mut data = sample_data();
        for d in data.documents.iter_mut().filter(|d| d.kind == DocKind::Angebot) {
            d.status = DocStatus::Angenommen;
        }
        let mut doc = invoice(&data);
        // One line with a discount and one with 7 %, and early-payment discounts.
        doc.lines[0].discount_pct = 5.0;
        doc.lines.push(Line { text: "Fachbuch".into(), qty: 2.0, unit: "Stück".into(), price_net: 3490, vat: 7, ..Line::default() });
        doc.apply_term(&standard_terms()[4]);
        std::fs::write(format!("{dir}/xrechnung.xml"), build(&data, &doc, Profile::XRechnung).unwrap()).unwrap();
        std::fs::write(format!("{dir}/en16931.xml"), build(&data, &doc, Profile::En16931).unwrap()).unwrap();
        std::fs::write(format!("{dir}/zugferd.pdf"), crate::pdf::zugferd_pdf(&data, &doc).unwrap()).unwrap();
        let mut with_logo = data.clone();
        let rgb: Vec<u8> = (0..60 * 20).flat_map(|i| if (i % 60) < 30 { [31, 111, 139] } else { [47, 179, 156] }).collect();
        with_logo.company.logo = Some(crate::pdf::make_logo(60, 20, &rgb));
        std::fs::write(format!("{dir}/zugferd-logo.pdf"), crate::pdf::zugferd_pdf(&with_logo, &doc).unwrap()).unwrap();
        let mut credit = doc.clone();
        credit.kind = DocKind::Gutschrift;
        credit.number = "GS-2026-0001".into();
        credit.source_id = Some(doc.id);
        std::fs::write(format!("{dir}/gutschrift.xml"), build(&data, &credit, Profile::XRechnung).unwrap()).unwrap();
        let mut small = data.clone();
        small.company.small_business = true;
        std::fs::write(format!("{dir}/kleinunternehmer.xml"), build(&small, &doc, Profile::XRechnung).unwrap()).unwrap();
        // EU and export cases: a customer in Austria and one in Switzerland.
        let mut eu = data.clone();
        let cid = doc.contact_id.unwrap();
        let k = eu.contacts.iter_mut().find(|c| c.id == cid).unwrap();
        k.country = "Österreich".into();
        k.zip_city = "1010 Wien".into();
        k.vat_id = "ATU12345678".into();
        for (case, name) in [
            (VatCase::IgLieferung, "eu-lieferung"),
            (VatCase::ReverseCharge, "eu-reverse-charge"),
            (VatCase::Ausfuhr, "ausfuhr"),
            (VatCase::DrittlandLeistung, "drittland-leistung"),
        ] {
            let mut d = doc.clone();
            d.vat_case = case;
            std::fs::write(format!("{dir}/{name}.xml"), build(&eu, &d, Profile::XRechnung).unwrap()).unwrap();
        }
        // An invoice in US dollars with German VAT: the VAT is also given in euros.
        let mut usd = doc.clone();
        usd.currency = "USD".into();
        usd.exchange_rate = 1.0832;
        std::fs::write(format!("{dir}/usd.xml"), build(&data, &usd, Profile::XRechnung).unwrap()).unwrap();
        std::fs::write(format!("{dir}/usd.pdf"), crate::pdf::zugferd_pdf(&data, &usd).unwrap()).unwrap();
        // Advance invoice and final invoice with the deduction.
        let mut advance = doc.clone();
        advance.part = InvoicePart::Abschlag;
        advance.number = "RE-2026-0101".into();
        std::fs::write(format!("{dir}/abschlag.xml"), build(&data, &advance, Profile::XRechnung).unwrap()).unwrap();
        let mut last = doc.clone();
        last.part = InvoicePart::Schluss;
        last.number = "RE-2026-0102".into();
        last.lines.push(Line { text: "abzüglich Abschlagsrechnung RE-2026-0101".into(), qty: 1.0, unit: "Pauschal".into(), price_net: -10000, vat: 19, ..Line::default() });
        let xml = build(&data, &last, Profile::XRechnung).unwrap();
        assert!(xml.contains("<ram:BilledQuantity unitCode=\"LS\">-1</ram:BilledQuantity>") || xml.contains(">-1<"));
        std::fs::write(format!("{dir}/schluss.xml"), xml).unwrap();
    }

    #[test]
    fn units_and_countries() {
        assert_eq!(unit_code("Stunde"), "HUR");
        assert_eq!(unit_code("Stk."), "H87");
        assert_eq!(unit_code("Pauschal"), "LS");
        assert_eq!(unit_code("Fass"), "C62");
        assert_eq!(country_code("Österreich"), "AT");
        assert_eq!(split_zip_city("10115 Berlin"), ("10115".into(), "Berlin".into()));
        assert_eq!(decimal(90.25), "90.25");
        assert_eq!(decimal(40.0), "40");
    }
}
