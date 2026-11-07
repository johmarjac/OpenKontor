//! Reading German/EU e-invoices: XRechnung (UBL or CII syntax) and ZUGFeRD / Factur-X
//! (a PDF that carries the CII XML as an embedded file). The figures come from the
//! structured data, so nothing has to be guessed.

use crate::model::Cents;
use chrono::NaiveDate;

/// The tax of one VAT rate on an invoice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VatPart {
    pub rate: f64,
    pub net: Cents,
    pub tax: Cents,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EInvoice {
    /// "XRechnung (UBL)", "XRechnung (CII)" or "ZUGFeRD / Factur-X".
    pub format: String,
    pub number: String,
    pub date: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    pub seller: String,
    pub seller_vat_id: String,
    pub iban: String,
    pub net: Cents,
    pub tax: Cents,
    pub gross: Cents,
    /// What is left to pay (after prepayments).
    pub payable: Cents,
    pub vat: Vec<VatPart>,
    pub currency: String,
}

fn amount(s: &str) -> Cents {
    s.trim().parse::<f64>().map(|v| (v * 100.0).round() as Cents).unwrap_or(0)
}

/// Dates come as "20261107" (CII format 102) or "2026-11-07" (UBL).
fn date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y%m%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .ok()
}

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// Follows a path of element names below a node.
fn path<'a, 'i>(n: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(n, |node, name| child(node, name))
}

fn text(n: Option<Node<'_, '_>>) -> String {
    n.and_then(|n| n.text()).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// All elements with this name anywhere below a node.
fn all<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.descendants().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// Reads an e-invoice from XML text, if it is one.
pub fn parse_xml(xml: &str) -> Option<EInvoice> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "CrossIndustryInvoice" => Some(parse_cii(root)),
        "Invoice" | "CreditNote" => Some(parse_ubl(root)),
        _ => None,
    }
}

fn parse_cii(root: Node<'_, '_>) -> EInvoice {
    let doc = child(root, "ExchangedDocument");
    let tx = child(root, "SupplyChainTradeTransaction");
    let agreement = tx.and_then(|t| child(t, "ApplicableHeaderTradeAgreement"));
    let settlement = tx.and_then(|t| child(t, "ApplicableHeaderTradeSettlement"));
    let seller = agreement.and_then(|a| child(a, "SellerTradeParty"));
    let totals = settlement.and_then(|s| child(s, "SpecifiedTradeSettlementHeaderMonetarySummation"));
    let mut inv = EInvoice {
        format: "XRechnung (CII)".into(),
        number: text(doc.and_then(|d| child(d, "ID"))),
        date: doc
            .and_then(|d| path(d, &["IssueDateTime", "DateTimeString"]))
            .and_then(|n| n.text())
            .and_then(date),
        seller: text(seller.and_then(|s| child(s, "Name"))),
        currency: text(settlement.and_then(|s| child(s, "InvoiceCurrencyCode"))),
        ..EInvoice::default()
    };
    if let Some(seller) = seller {
        inv.seller_vat_id = seller
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "SpecifiedTaxRegistration")
            .filter_map(|r| child(r, "ID"))
            .find(|id| id.attribute("schemeID") == Some("VA"))
            .and_then(|id| id.text())
            .unwrap_or_default()
            .trim()
            .to_string();
    }
    if let Some(s) = settlement {
        inv.iban = all(s, "IBANID").next().and_then(|n| n.text()).unwrap_or_default().replace(' ', "");
        inv.due = all(s, "DueDateDateTime")
            .next()
            .and_then(|n| child(n, "DateTimeString"))
            .and_then(|n| n.text())
            .and_then(date);
        for t in s.children().filter(|c| c.is_element() && c.tag_name().name() == "ApplicableTradeTax") {
            inv.vat.push(VatPart {
                rate: text(child(t, "RateApplicablePercent")).parse().unwrap_or(0.0),
                net: amount(&text(child(t, "BasisAmount"))),
                tax: amount(&text(child(t, "CalculatedAmount"))),
            });
        }
    }
    if let Some(t) = totals {
        inv.net = amount(&text(child(t, "TaxBasisTotalAmount")));
        // The tax total can appear twice (invoice and accounting currency); take the first.
        inv.tax = amount(&text(child(t, "TaxTotalAmount")));
        inv.gross = amount(&text(child(t, "GrandTotalAmount")));
        inv.payable = amount(&text(child(t, "DuePayableAmount")));
    }
    inv
}

fn parse_ubl(root: Node<'_, '_>) -> EInvoice {
    let supplier = path(root, &["AccountingSupplierParty", "Party"]);
    let totals = child(root, "LegalMonetaryTotal");
    let seller = supplier
        .and_then(|p| path(p, &["PartyLegalEntity", "RegistrationName"]))
        .or_else(|| supplier.and_then(|p| path(p, &["PartyName", "Name"])));
    let mut inv = EInvoice {
        format: "XRechnung (UBL)".into(),
        number: text(child(root, "ID")),
        date: child(root, "IssueDate").and_then(|n| n.text()).and_then(date),
        due: child(root, "DueDate")
            .or_else(|| path(root, &["PaymentMeans", "PaymentDueDate"]))
            .and_then(|n| n.text())
            .and_then(date),
        seller: text(seller),
        seller_vat_id: text(supplier.and_then(|p| path(p, &["PartyTaxScheme", "CompanyID"]))),
        iban: text(path(root, &["PaymentMeans", "PayeeFinancialAccount", "ID"])).replace(' ', ""),
        currency: text(child(root, "DocumentCurrencyCode")),
        ..EInvoice::default()
    };
    if let Some(t) = totals {
        inv.net = amount(&text(child(t, "TaxExclusiveAmount")));
        inv.gross = amount(&text(child(t, "TaxInclusiveAmount")));
        inv.payable = amount(&text(child(t, "PayableAmount")));
    }
    if let Some(tax_total) = child(root, "TaxTotal") {
        inv.tax = amount(&text(child(tax_total, "TaxAmount")));
        for sub in tax_total.children().filter(|c| c.is_element() && c.tag_name().name() == "TaxSubtotal") {
            inv.vat.push(VatPart {
                rate: text(path(sub, &["TaxCategory", "Percent"])).parse().unwrap_or(0.0),
                net: amount(&text(child(sub, "TaxableAmount"))),
                tax: amount(&text(child(sub, "TaxAmount"))),
            });
        }
    }
    inv
}

/// Finds the invoice XML embedded in a ZUGFeRD / Factur-X PDF.
pub fn xml_from_pdf(bytes: &[u8]) -> Option<String> {
    let pdf = lopdf::Document::load_mem(bytes).ok()?;
    for object in pdf.objects.values() {
        let Ok(stream) = object.as_stream() else { continue };
        let is_embedded = stream
            .dict
            .get(b"Type")
            .ok()
            .and_then(|t| t.as_name().ok())
            .is_some_and(|n| n == b"EmbeddedFile");
        let content = match stream.decompressed_content() {
            Ok(c) => c,
            Err(_) if stream.dict.get(b"Filter").is_err() => stream.content.clone(),
            Err(_) => continue,
        };
        let head = String::from_utf8_lossy(&content[..content.len().min(600)]).to_string();
        let looks_like_invoice = head.contains("CrossIndustryInvoice") || head.contains(":Invoice") || head.contains("<Invoice");
        if (is_embedded || head.trim_start().starts_with("<?xml")) && looks_like_invoice {
            return String::from_utf8(content).ok();
        }
    }
    None
}

/// An e-invoice from a file of any supported kind (XML or PDF).
pub fn from_file(name: &str, bytes: &[u8]) -> Option<EInvoice> {
    let lower = name.to_lowercase();
    if lower.ends_with(".xml") || bytes.starts_with(b"<?xml") || bytes.starts_with(b"\xEF\xBB\xBF<?xml") {
        let text = String::from_utf8_lossy(bytes);
        return parse_xml(text.trim_start_matches('\u{feff}'));
    }
    if bytes.starts_with(b"%PDF") {
        let xml = xml_from_pdf(bytes)?;
        let mut inv = parse_xml(&xml)?;
        if inv.format.contains("CII") {
            inv.format = "ZUGFeRD / Factur-X".into();
        }
        return Some(inv);
    }
    None
}

/// A small XRechnung in UBL syntax (used for the demo and the tests).
pub const SAMPLE_UBL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ubl:Invoice xmlns:ubl="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"
  xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2"
  xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>R-1001</cbc:ID><cbc:IssueDate>2026-10-30</cbc:IssueDate><cbc:DueDate>2026-11-13</cbc:DueDate>
  <cbc:DocumentCurrencyCode>EUR</cbc:DocumentCurrencyCode>
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyTaxScheme><cbc:CompanyID>DE111222333</cbc:CompanyID></cac:PartyTaxScheme>
    <cac:PartyLegalEntity><cbc:RegistrationName>Netzfunk AG</cbc:RegistrationName></cac:PartyLegalEntity>
  </cac:Party></cac:AccountingSupplierParty>
  <cac:PaymentMeans><cbc:PaymentMeansCode>58</cbc:PaymentMeansCode>
    <cac:PayeeFinancialAccount><cbc:ID>DE02 1203 0000 0000 2020 51</cbc:ID></cac:PayeeFinancialAccount></cac:PaymentMeans>
  <cac:TaxTotal><cbc:TaxAmount currencyID="EUR">9.56</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount currencyID="EUR">50.34</cbc:TaxableAmount><cbc:TaxAmount currencyID="EUR">9.56</cbc:TaxAmount>
      <cac:TaxCategory><cbc:ID>S</cbc:ID><cbc:Percent>19</cbc:Percent></cac:TaxCategory></cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:LegalMonetaryTotal><cbc:TaxExclusiveAmount currencyID="EUR">50.34</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount currencyID="EUR">59.90</cbc:TaxInclusiveAmount><cbc:PayableAmount currencyID="EUR">59.90</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
</ubl:Invoice>"#;

#[cfg(test)]
mod tests {
    use super::*;

    const CII: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"
  xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100"
  xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">
  <rsm:ExchangedDocument><ram:ID>2026-4711</ram:ID><ram:TypeCode>380</ram:TypeCode>
    <ram:IssueDateTime><udt:DateTimeString format="102">20261105</udt:DateTimeString></ram:IssueDateTime>
  </rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
    <ram:ApplicableHeaderTradeAgreement>
      <ram:SellerTradeParty><ram:Name>Bürowelt Versand GmbH</ram:Name>
        <ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">DE987654321</ram:ID></ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>
      <ram:SpecifiedTradeSettlementPaymentMeans><ram:TypeCode>58</ram:TypeCode>
        <ram:PayeePartyCreditorFinancialAccount><ram:IBANID>DE44500105175407324931</ram:IBANID></ram:PayeePartyCreditorFinancialAccount>
      </ram:SpecifiedTradeSettlementPaymentMeans>
      <ram:ApplicableTradeTax><ram:CalculatedAmount>19.00</ram:CalculatedAmount><ram:TypeCode>VAT</ram:TypeCode>
        <ram:BasisAmount>100.00</ram:BasisAmount><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>19</ram:RateApplicablePercent></ram:ApplicableTradeTax>
      <ram:ApplicableTradeTax><ram:CalculatedAmount>3.50</ram:CalculatedAmount><ram:TypeCode>VAT</ram:TypeCode>
        <ram:BasisAmount>50.00</ram:BasisAmount><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>7</ram:RateApplicablePercent></ram:ApplicableTradeTax>
      <ram:SpecifiedTradePaymentTerms><ram:DueDateDateTime><udt:DateTimeString format="102">20261205</udt:DateTimeString></ram:DueDateDateTime></ram:SpecifiedTradePaymentTerms>
      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>
        <ram:LineTotalAmount>150.00</ram:LineTotalAmount><ram:TaxBasisTotalAmount>150.00</ram:TaxBasisTotalAmount>
        <ram:TaxTotalAmount currencyID="EUR">22.50</ram:TaxTotalAmount><ram:GrandTotalAmount>172.50</ram:GrandTotalAmount>
        <ram:DuePayableAmount>172.50</ram:DuePayableAmount>
      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>
    </ram:ApplicableHeaderTradeSettlement>
  </rsm:SupplyChainTradeTransaction>
</rsm:CrossIndustryInvoice>"#;

    const UBL: &str = SAMPLE_UBL;


    #[test]
    fn reads_cii() {
        let inv = parse_xml(CII).unwrap();
        assert_eq!(inv.number, "2026-4711");
        assert_eq!(inv.seller, "Bürowelt Versand GmbH");
        assert_eq!(inv.seller_vat_id, "DE987654321");
        assert_eq!(inv.date, NaiveDate::from_ymd_opt(2026, 11, 5));
        assert_eq!(inv.due, NaiveDate::from_ymd_opt(2026, 12, 5));
        assert_eq!((inv.net, inv.tax, inv.gross), (15000, 2250, 17250));
        assert_eq!(inv.vat.len(), 2);
        assert_eq!(inv.vat[1], VatPart { rate: 7.0, net: 5000, tax: 350 });
        assert_eq!(inv.iban, "DE44500105175407324931");
    }

    #[test]
    fn reads_ubl() {
        let inv = from_file("rechnung.xml", UBL.as_bytes()).unwrap();
        assert_eq!(inv.format, "XRechnung (UBL)");
        assert_eq!(inv.number, "R-1001");
        assert_eq!(inv.seller, "Netzfunk AG");
        assert_eq!((inv.net, inv.tax, inv.gross), (5034, 956, 5990));
        assert_eq!(inv.iban, "DE02120300000000202051");
        assert_eq!(inv.due, NaiveDate::from_ymd_opt(2026, 11, 13));
    }

    #[test]
    fn reads_zugferd_pdf() {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut pdf = Document::with_version("1.7");
        let mut file = Stream::new(dictionary! { "Type" => "EmbeddedFile", "Subtype" => "text/xml" }, CII.as_bytes().to_vec());
        file.compress().unwrap();
        let file_id = pdf.add_object(file);
        let spec = pdf.add_object(dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("factur-x.xml"),
            "EF" => dictionary! { "F" => file_id },
        });
        let pages = pdf.add_object(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 });
        let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "AF" => vec![spec.into()] });
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();

        let inv = from_file("rechnung.pdf", &bytes).unwrap();
        assert_eq!(inv.format, "ZUGFeRD / Factur-X");
        assert_eq!(inv.gross, 17250);
    }

    #[test]
    fn other_xml_is_not_an_invoice() {
        assert!(parse_xml("<?xml version=\"1.0\"?><note>hallo</note>").is_none());
    }
}
