//! Deeper bookkeeping: write-offs of fixed assets, the chart of accounts (SKR 03/04),
//! a journal in double entry derived from the bookings, trial balance, profit and loss
//! statement, balance sheet, the EÜR form groups and the EU sales list (ZM).
//!
//! Internally every category carries its SKR 03 account number; SKR 04 numbers are
//! produced by `account_in` when the company uses that chart.

use crate::model::*;
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;

// ---- Write-offs (AfA) -----------------------------------------------------------

/// Write-off of an asset in a calendar year (incl. the rest value when it leaves).
pub fn afa_for_year(a: &Asset, year: i32) -> Cents {
    if year < a.date.year() {
        return 0;
    }
    value_at_start(a, year) - value_at_start(a, year + 1)
}

/// Months of use before the start of `year`.
fn months_between(bought: NaiveDate, year: i32) -> i64 {
    if year <= bought.year() {
        return 0;
    }
    (13 - bought.month() as i64) + 12 * (year - bought.year() - 1) as i64
}

/// Book value at the start of a year.
pub fn value_at_start(a: &Asset, year: i32) -> Cents {
    let mut v = a.cost;
    for y in a.date.year()..year {
        v -= afa_for_year_raw(a, y, v);
    }
    v.max(0)
}

fn afa_for_year_raw(a: &Asset, year: i32, before: Cents) -> Cents {
    if before <= 0 || a.disposed.is_some_and(|d| d.year() < year) {
        return 0;
    }
    if a.disposed.is_some_and(|d| d.year() == year) && a.method != AfaMethod::Sammelposten {
        return before;
    }
    let start = a.date.year();
    let regular = match a.method {
        AfaMethod::Gwg => a.cost,
        AfaMethod::Sammelposten => {
            if year < start + 5 {
                a.cost / 5 + if year == start + 4 { a.cost % 5 } else { 0 }
            } else {
                0
            }
        }
        AfaMethod::Linear => {
            let total_months = a.years.max(1) as i64 * 12;
            let used = months_between(a.date, year);
            let this_year = if year == start { 13 - a.date.month() as i64 } else { 12 };
            let this_year = this_year.min(total_months - used).max(0);
            (a.cost as i128 * this_year as i128 / total_months as i128) as Cents
        }
    };
    // The last year takes the rounding rest.
    if a.method == AfaMethod::Linear {
        let total_months = a.years.max(1) as i64 * 12;
        let used = months_between(a.date, year) + if year == start { 13 - a.date.month() as i64 } else { 12 };
        if used >= total_months {
            return before;
        }
    }
    regular.min(before)
}

/// Book value at the end of a year.
pub fn value_at_end(a: &Asset, year: i32) -> Cents {
    if a.date.year() > year {
        return 0;
    }
    value_at_start(a, year + 1)
}

/// Expense account (SKR 03) for an asset's write-off.
pub fn afa_category(a: &Asset, year: i32) -> &'static str {
    if a.disposed.is_some_and(|d| d.year() == year) && a.method != AfaMethod::Sammelposten {
        return "2310 Anlagenabgänge (Restbuchwert)";
    }
    match a.method {
        AfaMethod::Linear => "4830 Abschreibungen auf Sachanlagen",
        AfaMethod::Gwg => "4855 Geringwertige Wirtschaftsgüter",
        AfaMethod::Sammelposten => "4862 Abschreibung Sammelposten",
    }
}

/// Write-offs of a year summed per expense account.
pub fn afa_by_category(data: &Data, year: i32) -> Vec<(String, Cents)> {
    let mut m: BTreeMap<String, Cents> = BTreeMap::new();
    for a in &data.assets {
        let v = value_at_start(a, year) - value_at_start(a, year + 1);
        if v > 0 {
            *m.entry(afa_category(a, year).to_string()).or_default() += v;
        }
    }
    m.into_iter().collect()
}

// ---- Chart of accounts -------------------------------------------------------------

/// SKR 03 → SKR 04 for the accounts OpenKontor uses.
const SKR04: [(&str, &str); 54] = [
    ("2660", "4840"), ("2150", "6880"),
    ("8400", "4400"), ("8300", "4300"), ("8195", "4185"), ("8120", "4120"), ("8125", "4125"),
    ("8336", "4336"), ("8338", "4338"), ("2700", "4830"), ("8736", "4736"), ("1890", "2180"),
    ("1800", "2100"), ("3400", "5400"), ("4120", "6020"), ("4210", "6310"), ("4240", "6325"),
    ("4360", "6400"), ("4530", "6530"), ("4600", "6600"), ("4650", "6640"), ("4670", "6670"),
    ("4930", "6815"), ("4920", "6805"), ("4940", "6820"), ("4950", "6825"), ("4970", "6855"),
    ("4855", "6260"), ("4862", "6264"), ("4830", "6220"), ("2310", "6895"), ("1780", "3820"),
    ("1200", "1800"), ("1000", "1600"), ("1400", "1200"), ("1600", "3300"), ("1576", "1406"),
    ("1571", "1401"), ("1577", "1407"), ("1776", "3806"), ("1771", "3801"), ("1787", "3837"),
    ("0420", "0650"), ("0490", "0690"), ("0400", "0640"), ("0320", "0520"), ("0210", "0440"),
    ("0027", "0135"), ("0480", "0670"), ("0800", "2900"), ("0880", "2000"), ("9000", "9000"),
    ("1360", "1460"), ("0485", "0675"),
];

/// The account number of a category ("8400 Erlöse …" → "8400" or "4400").
pub fn account_in(chart: Chart, category_or_account: &str) -> String {
    let n = category_or_account.split(' ').next().unwrap_or("").to_string();
    match chart {
        Chart::Skr03 => n,
        Chart::Skr04 => SKR04.iter().find(|(a, _)| *a == n).map(|(_, b)| b.to_string()).unwrap_or(n),
    }
}

/// Back to SKR 03 for an account number entered in the company's chart.
pub fn to_skr03(chart: Chart, account: &str) -> String {
    let n = account.trim().to_string();
    match chart {
        Chart::Skr03 => n,
        Chart::Skr04 => SKR04.iter().find(|(_, b)| *b == n).map(|(a, _)| a.to_string()).unwrap_or(n),
    }
}

/// Names of the accounts used (SKR 03 numbers).
pub fn account_name(skr03: &str) -> String {
    let fixed = match skr03 {
        "1000" => "Kasse",
        "1200" => "Bank",
        "1360" => "Geldtransit",
        "1400" => "Forderungen aus Lieferungen und Leistungen",
        "1600" => "Verbindlichkeiten aus Lieferungen und Leistungen",
        "1571" => "Abziehbare Vorsteuer 7 %",
        "1576" => "Abziehbare Vorsteuer 19 %",
        "1577" => "Vorsteuer § 13b UStG",
        "1771" => "Umsatzsteuer 7 %",
        "1776" => "Umsatzsteuer 19 %",
        "1787" => "Umsatzsteuer § 13b UStG",
        "8736" => "Gewährte Skonti",
        "2660" => "Erträge aus Kursdifferenzen",
        "2150" => "Aufwendungen aus Kursdifferenzen",
        "0800" => "Gezeichnetes Kapital",
        "0880" => "Eigenkapital",
        "9000" => "Saldenvorträge",
        "2310" => "Anlagenabgänge (Restbuchwert)",
        "4830" => "Abschreibungen auf Sachanlagen",
        "4862" => "Abschreibung Sammelposten",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    INCOME_CATEGORIES
        .iter()
        .chain(EXPENSE_CATEGORIES.iter())
        .chain(ASSET_ACCOUNTS.iter())
        .find(|c| c.split(' ').next() == Some(skr03))
        .map(|c| c.split_once(' ').map(|x| x.1).unwrap_or("").to_string())
        .unwrap_or_default()
}

/// What an account is, by its SKR 03 number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountClass {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

pub fn class_of(skr03: &str) -> AccountClass {
    let n: u32 = skr03.parse().unwrap_or(0);
    match n {
        0..=799 => AccountClass::Asset,
        800..=999 => AccountClass::Equity,
        1000..=1599 => AccountClass::Asset,
        1600..=1799 => AccountClass::Liability,
        1800..=1999 => AccountClass::Equity,
        2000..=2499 => AccountClass::Expense,
        2500..=2999 => AccountClass::Revenue,
        3000..=6999 => AccountClass::Expense,
        7000..=7999 => AccountClass::Asset,
        8000..=8999 => AccountClass::Revenue,
        _ => AccountClass::Equity,
    }
}

// ---- Journal (double entry) -------------------------------------------------------------

/// One line of the journal: debit account to credit account (SKR 03 numbers).
#[derive(Clone, Debug, PartialEq)]
pub struct Posting {
    pub date: NaiveDate,
    pub debit: String,
    pub credit: String,
    pub amount: Cents,
    pub text: String,
    pub receipt: String,
}

fn money_account(payment: &str) -> &'static str {
    if payment == CASH { "1000" } else { "1200" }
}

fn input_tax_account(rate: u8) -> &'static str {
    if rate == 7 { "1571" } else { "1576" }
}

fn output_tax_account(rate: u8) -> &'static str {
    if rate == 7 { "1771" } else { "1776" }
}

/// The journal of a period in double entry, derived from bookings, invoices, assets and
/// the manual entries. With `Accounting::Bilanz` invoices count when issued (receivables).
pub fn journal(data: &Data, from: NaiveDate, to: NaiveDate) -> Vec<Posting> {
    let chart = data.company.chart;
    let bilanz = data.company.accounting == Accounting::Bilanz;
    let small = data.company.small_business;
    let mut out = Vec::new();
    let mut push = |date, debit: &str, credit: &str, amount: Cents, text: &str, receipt: &str| {
        if amount == 0 {
            return;
        }
        // Negative amounts swap the sides, so every line is positive.
        let (debit, credit, amount) =
            if amount < 0 { (credit, debit, -amount) } else { (debit, credit, amount) };
        out.push(Posting {
            date,
            debit: debit.to_string(),
            credit: credit.to_string(),
            amount,
            text: text.to_string(),
            receipt: receipt.to_string(),
        });
    };
    let in_range = |d: NaiveDate| d >= from && d <= to;

    // Invoices as receivables (balance sheet accounting only).
    if bilanz {
        for d in data.documents.iter().filter(|d| {
            d.kind.is_billing() && !matches!(d.status, DocStatus::Entwurf | DocStatus::Storniert) && in_range(d.date)
        }) {
            let t = d.totals(small);
            let text = format!("{} {} {}", d.kind.name(), d.number, data.contact_name(d.contact_id));
            for (rate, (net, tax)) in &t.by_rate {
                let revenue = d
                    .vat_case
                    .income_category()
                    .filter(|_| !small)
                    .unwrap_or_else(|| income_category_for_rate(*rate, small));
                push(d.date, "1400", &account_in(Chart::Skr03, revenue), d.to_eur(*net), &text, &d.number);
                push(d.date, "1400", output_tax_account(*rate), d.to_eur(*tax), &text, &d.number);
            }
        }
    }

    for b in data.bookings.iter().filter(|b| in_range(b.date)) {
        let money = money_account(&b.payment);
        let cat = account_in(Chart::Skr03, &b.category);
        let (net, tax) = (b.net(), b.tax());
        let text = if b.partner.is_empty() { b.text.clone() } else { format!("{} – {}", b.text, b.partner) };
        match b.kind {
            BookingKind::Einnahme => {
                if bilanz && b.doc_id.is_some() {
                    push(b.date, money, "1400", b.gross, &text, &b.receipt);
                } else if is_private(&b.category) {
                    push(b.date, money, &cat, b.gross, &text, &b.receipt);
                } else {
                    push(b.date, money, &cat, net, &text, &b.receipt);
                    push(b.date, money, output_tax_account(b.vat), tax, &text, &b.receipt);
                }
            }
            BookingKind::Ausgabe => {
                if bilanz && b.doc_id.is_some() {
                    // Refund of a credit note.
                    push(b.date, "1400", money, b.gross, &text, &b.receipt);
                    continue;
                }
                let target = if b.category == ASSET_CATEGORY {
                    data.assets
                        .iter()
                        .find(|a| a.booking_id == Some(b.id))
                        .map(|a| account_in(Chart::Skr03, &a.account))
                        .unwrap_or_else(|| "0400".into())
                } else {
                    cat
                };
                if is_private(&b.category) {
                    push(b.date, &target, money, b.gross, &text, &b.receipt);
                    continue;
                }
                push(b.date, &target, money, net, &text, &b.receipt);
                push(b.date, input_tax_account(b.vat), money, tax, &text, &b.receipt);
                if b.reverse_charge {
                    push(b.date, "1577", "1787", b.reverse_charge_tax(), &text, &b.receipt);
                }
            }
        }
    }

    // Early-payment discounts reduce the receivable (balance sheet accounting only).
    if bilanz {
        for d in data.documents.iter().filter(|d| d.kind.is_billing()) {
            let t = d.totals(small);
            if t.gross == 0 {
                continue;
            }
            for p in d.effective_payments(small).iter().filter(|p| p.discount != 0 && in_range(p.date)) {
                let text = format!("Skonto {} {}", d.kind.name(), d.number);
                for (rate, (net, tax)) in &t.by_rate {
                    let share = |v: Cents| d.to_eur((v as f64 * p.discount as f64 / t.gross as f64).round() as Cents);
                    push(p.date, "8736", "1400", share(*net), &text, &d.number);
                    push(p.date, output_tax_account(*rate), "1400", share(*tax), &text, &d.number);
                }
            }
        }
    }

    // Documents in another currency: what arrived in euros differs from the receivable.
    if bilanz {
        for d in data.documents.iter().filter(|d| d.kind.is_billing() && d.is_foreign() && d.status == DocStatus::Bezahlt) {
            let pays = d.effective_payments(small);
            let Some(last) = pays.iter().map(|p| p.date).max().filter(|day| in_range(*day)) else { continue };
            let received: Cents = pays.iter().map(|p| p.eur.unwrap_or_else(|| d.to_eur(p.amount)) + d.to_eur(p.discount)).sum();
            let diff = received - d.to_eur(d.totals(small).gross);
            let text = format!("Kursdifferenz {} {}", d.kind.name(), d.number);
            if diff > 0 {
                push(last, "1400", "2660", diff, &text, &d.number);
            } else if diff < 0 {
                push(last, "2150", "1400", -diff, &text, &d.number);
            }
        }
    }

    // Write-offs at the end of each year in the range.
    for year in from.year()..=to.year() {
        let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
        if !in_range(end) {
            continue;
        }
        for a in &data.assets {
            let v = value_at_start(a, year) - value_at_start(a, year + 1);
            let text = format!("AfA {} {}", a.number, a.name);
            push(end, &account_in(Chart::Skr03, afa_category(a, year)), &account_in(Chart::Skr03, &a.account), v, &text, &a.number);
        }
    }

    // Manual entries, entered in the company's chart.
    for e in data.journal.iter().filter(|e| in_range(e.date)) {
        push(e.date, &to_skr03(chart, &e.debit), &to_skr03(chart, &e.credit), e.amount, &e.text, &e.receipt);
    }
    out.sort_by_key(|p| p.date);
    out
}

/// Debit and credit totals per account (SKR 03 numbers).
pub fn trial_balance(postings: &[Posting]) -> BTreeMap<String, (Cents, Cents)> {
    let mut m: BTreeMap<String, (Cents, Cents)> = BTreeMap::new();
    for p in postings {
        m.entry(p.debit.clone()).or_default().0 += p.amount;
        m.entry(p.credit.clone()).or_default().1 += p.amount;
    }
    m
}

/// Profit and loss statement and balance sheet of a year.
#[derive(Clone, Debug, Default)]
pub struct Statements {
    /// Revenue accounts with their (credit) balance.
    pub revenue: Vec<(String, Cents)>,
    /// Expense accounts with their (debit) balance.
    pub expenses: Vec<(String, Cents)>,
    pub profit: Cents,
    pub assets: Vec<(String, Cents)>,
    pub liabilities: Vec<(String, Cents)>,
    pub equity: Vec<(String, Cents)>,
    pub total_assets: Cents,
    pub total_liabilities: Cents,
}

/// Year-end statements. The balance sheet uses all entries up to the year's end, so
/// opening balances (account 9000) and earlier years carry over.
pub fn statements(data: &Data, year: i32) -> Statements {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let to = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let tb_year = trial_balance(&journal(data, from, to));
    let all_from = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
    let tb_all = trial_balance(&journal(data, all_from, to));
    let mut s = Statements::default();
    for (acc, (d, c)) in &tb_year {
        match class_of(acc) {
            AccountClass::Revenue if c - d != 0 => s.revenue.push((acc.clone(), c - d)),
            AccountClass::Expense if d - c != 0 => s.expenses.push((acc.clone(), d - c)),
            _ => {}
        }
    }
    s.profit = s.revenue.iter().map(|x| x.1).sum::<Cents>() - s.expenses.iter().map(|x| x.1).sum::<Cents>();
    // Results of earlier years become equity (profit carried forward).
    let mut earlier = 0;
    for (acc, (d, c)) in &tb_all {
        let year_part = tb_year.get(acc).copied().unwrap_or((0, 0));
        match class_of(acc) {
            AccountClass::Asset if d - c != 0 => s.assets.push((acc.clone(), d - c)),
            AccountClass::Liability if c - d != 0 => s.liabilities.push((acc.clone(), c - d)),
            AccountClass::Equity if c - d != 0 => s.equity.push((acc.clone(), c - d)),
            AccountClass::Revenue => earlier += (c - d) - (year_part.1 - year_part.0),
            AccountClass::Expense => earlier -= (d - c) - (year_part.0 - year_part.1),
            _ => {}
        }
    }
    if earlier != 0 {
        s.equity.push(("Gewinnvortrag".into(), earlier));
    }
    s.equity.push(("Jahresergebnis".into(), s.profit));
    s.total_assets = s.assets.iter().map(|x| x.1).sum();
    s.total_liabilities = s.liabilities.iter().map(|x| x.1).sum::<Cents>() + s.equity.iter().map(|x| x.1).sum::<Cents>();
    s
}

// ---- Anlage EÜR ------------------------------------------------------------------------

/// The EÜR figures grouped like the official form "Anlage EÜR".
#[derive(Clone, Debug, Default)]
pub struct EuerForm {
    pub income: Vec<(&'static str, Cents)>,
    pub expenses: Vec<(&'static str, Cents)>,
    pub total_income: Cents,
    pub total_expenses: Cents,
    pub profit: Cents,
    /// Entertainment expenses: only 70 % may be deducted.
    pub entertainment_not_deductible: Cents,
}

pub fn euer_form(data: &Data, year: i32) -> EuerForm {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let to = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let r = period_report(data, from, to);
    let acc = |cat: &str| cat.split(' ').next().unwrap_or("").to_string();
    let mut inc: BTreeMap<&'static str, Cents> = BTreeMap::new();
    for (cat, v) in &r.income_by_cat {
        let group = match acc(cat).as_str() {
            "8195" => "Betriebseinnahmen als Kleinunternehmer",
            "8120" | "8125" | "8336" | "8338" => "Umsatzsteuerfreie und nicht steuerbare Betriebseinnahmen",
            _ => "Umsatzsteuerpflichtige Betriebseinnahmen",
        };
        *inc.entry(group).or_default() += v;
    }
    let collected = r.vat_collected - r.rc_tax;
    if collected != 0 {
        *inc.entry("Vereinnahmte Umsatzsteuer").or_default() += collected;
    }
    let mut exp: BTreeMap<&'static str, Cents> = BTreeMap::new();
    let mut entertainment = 0;
    for (cat, v) in &r.expense_by_cat {
        let group = match acc(cat).as_str() {
            "3400" => "Waren, Roh- und Hilfsstoffe",
            "4120" => "Löhne und Gehälter",
            "4830" | "2310" => "Absetzung für Abnutzung (AfA)",
            "4855" | "4862" => "Geringwertige Wirtschaftsgüter und Sammelposten",
            "4210" | "4240" => "Raumkosten",
            "4530" => "Kraftfahrzeugkosten",
            "4600" => "Werbekosten",
            "4650" => {
                entertainment += (*v as f64 * 0.3).round() as Cents;
                "Bewirtungsaufwendungen"
            }
            "4670" => "Reisekosten",
            "4920" => "Telekommunikation",
            "4940" => "Fortbildung und Fachliteratur",
            "4950" => "Rechts- und Steuerberatung, Buchführung",
            "4360" => "Versicherungen und Beiträge",
            "1780" => "An das Finanzamt gezahlte Umsatzsteuer",
            _ => "Übrige Betriebsausgaben",
        };
        *exp.entry(group).or_default() += v;
    }
    let paid = r.vat_paid - r.rc_tax;
    if paid != 0 {
        *exp.entry("Gezahlte Vorsteuerbeträge").or_default() += paid;
    }
    let mut f = EuerForm {
        income: inc.into_iter().collect(),
        expenses: exp.into_iter().collect(),
        entertainment_not_deductible: entertainment,
        ..Default::default()
    };
    f.total_income = f.income.iter().map(|x| x.1).sum();
    f.total_expenses = f.expenses.iter().map(|x| x.1).sum();
    f.profit = f.total_income - f.total_expenses + f.entertainment_not_deductible;
    f
}

// ---- Cash-flow forecast (Liquiditätsvorschau) --------------------------------------------

/// One week of the forecast.
#[derive(Clone, Debug, Default)]
pub struct ForecastWeek {
    pub start: NaiveDate,
    pub inflow: Cents,
    pub outflow: Cents,
    /// Money at the end of the week.
    pub balance: Cents,
    /// What makes up the week: (text, amount in euros, positive = in).
    pub items: Vec<(String, Cents)>,
}

/// Expected money over the next weeks: open invoices by due date, open supplier receipts,
/// recurring invoices and the average running expenses of the last three months.
pub fn forecast(data: &Data, start_balance: Cents, from: NaiveDate, weeks: usize) -> Vec<ForecastWeek> {
    let small = data.company.small_business;
    let monday = from - chrono::Duration::days(from.weekday().num_days_from_monday() as i64);
    let mut out: Vec<ForecastWeek> =
        (0..weeks).map(|i| ForecastWeek { start: monday + chrono::Duration::weeks(i as i64), ..Default::default() }).collect();
    let end = monday + chrono::Duration::weeks(weeks as i64);
    let mut add = |day: NaiveDate, text: String, amount: Cents| {
        if amount == 0 || day >= end {
            return;
        }
        // Everything overdue is expected in the first week.
        let i = if day < monday { 0 } else { ((day - monday).num_days() / 7) as usize };
        let w = &mut out[i.min(weeks - 1)];
        if amount > 0 { w.inflow += amount } else { w.outflow -= amount }
        w.items.push((text, amount));
    };
    for d in data.documents.iter().filter(|d| d.kind.is_billing() && d.status == DocStatus::Offen) {
        let open = d.to_eur(d.open_amount(small));
        add(d.due_date(), format!("{} {} – {}", d.kind.name(), d.number, data.contact_name(d.contact_id)), open);
    }
    for r in data.receipts.iter().filter(|r| r.status == ReceiptStatus::Neu && r.gross > 0) {
        let paid = data.bank.iter().any(|t| t.link == Some(BankLink::Receipt(r.id)));
        if !paid {
            let day = r.due.or(r.date.map(|d| d + chrono::Duration::days(14))).unwrap_or(from);
            add(day, format!("Eingangsbeleg {} {}", r.supplier, r.number), -r.gross);
        }
    }
    for rec in data.recurring.iter().filter(|r| r.active) {
        let mut day = rec.next_date;
        let gross = rec.template.to_eur(rec.template.totals(small).gross);
        while day < end && rec.until.is_none_or(|u| day <= u) {
            add(day + chrono::Duration::days(rec.template.due_days as i64), format!("Wiederkehrend: {}", rec.name), gross);
            day = day.checked_add_months(chrono::Months::new(rec.every_months.max(1))).unwrap_or(end);
        }
    }
    // Running costs: average of the expenses (without fixed assets) of the last 90 days.
    let since = from - chrono::Duration::days(90);
    let spent: Cents = data
        .bookings
        .iter()
        .filter(|b| b.kind == BookingKind::Ausgabe && b.date >= since && b.date < from && b.category != ASSET_CATEGORY && b.receipt_id.is_none())
        .map(|b| b.gross)
        .sum();
    let per_week = spent * 7 / 90;
    let mut balance = start_balance;
    for w in out.iter_mut() {
        if per_week > 0 {
            w.outflow += per_week;
            w.items.push(("Laufende Ausgaben (Durchschnitt)".into(), -per_week));
        }
        balance += w.inflow - w.outflow;
        w.balance = balance;
    }
    out
}

// ---- EU sales list (Zusammenfassende Meldung) ---------------------------------------------

/// One line of the EU sales list.
#[derive(Clone, Debug, PartialEq)]
pub struct ZmLine {
    pub country: String,
    /// VAT id without the country prefix.
    pub vat_id: String,
    pub amount: Cents,
    /// "L" deliveries of goods, "S" services.
    pub kind: char,
}

pub fn zm_report(data: &Data, from: NaiveDate, to: NaiveDate) -> Vec<ZmLine> {
    let small = data.company.small_business;
    let mut m: BTreeMap<(String, char), Cents> = BTreeMap::new();
    for d in data.documents.iter().filter(|d| {
        d.kind.is_billing()
            && !matches!(d.status, DocStatus::Entwurf | DocStatus::Storniert)
            && d.date >= from
            && d.date <= to
    }) {
        let kind = match d.vat_case {
            VatCase::IgLieferung => 'L',
            VatCase::ReverseCharge => 'S',
            _ => continue,
        };
        let vat_id = data.contact(d.contact_id).map(|c| c.vat_id.replace(' ', "").to_uppercase()).unwrap_or_default();
        if vat_id.len() < 3 {
            continue;
        }
        *m.entry((vat_id, kind)).or_default() += d.to_eur(d.totals(small).net);
    }
    m.into_iter()
        .map(|((id, kind), amount)| ZmLine { country: id[..2].to_string(), vat_id: id[2..].to_string(), amount, kind })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(cost: Cents, years: u32, y: i32, m: u32, method: AfaMethod) -> Asset {
        Asset {
            cost,
            years,
            date: NaiveDate::from_ymd_opt(y, m, 15).unwrap(),
            method,
            ..Asset::default()
        }
    }

    #[test]
    fn linear_writeoff_by_months() {
        // 3,600 € over 3 years, bought in October: 3 months in the first year.
        let a = asset(360_000, 3, 2026, 10, AfaMethod::Linear);
        assert_eq!(afa_for_year(&a, 2026), 30_000);
        assert_eq!(afa_for_year(&a, 2027), 120_000);
        assert_eq!(afa_for_year(&a, 2028), 120_000);
        assert_eq!(afa_for_year(&a, 2029), 90_000);
        assert_eq!(afa_for_year(&a, 2030), 0);
        assert_eq!(value_at_end(&a, 2029), 0);
    }

    #[test]
    fn pool_and_low_value() {
        let p = asset(100_000, 0, 2026, 6, AfaMethod::Sammelposten);
        assert_eq!((2026..2031).map(|y| afa_for_year(&p, y)).sum::<Cents>(), 100_000);
        let g = asset(50_000, 0, 2026, 6, AfaMethod::Gwg);
        assert_eq!(afa_for_year(&g, 2026), 50_000);
        assert_eq!(afa_for_year(&g, 2027), 0);
    }

    #[test]
    fn journal_balances() {
        let mut data = sample_data();
        data.company.accounting = Accounting::Bilanz;
        let year = today().year();
        let s = statements(&data, year);
        assert_eq!(s.total_assets, s.total_liabilities, "Bilanz geht nicht auf: {s:?}");
        let tb = trial_balance(&journal(&data, NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), today()));
        let (d, c): (Cents, Cents) = tb.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1));
        assert_eq!(d, c);
    }

    #[test]
    fn foreign_currency_and_projects() {
        let mut data = sample_data();
        data.company.accounting = Accounting::Bilanz;
        let cid = data.contacts[0].id;
        // An invoice over 1,000.00 USD at 1.25 USD per euro (= 800 €), no VAT (export).
        let doc = Document {
            contact_id: Some(cid),
            currency: "USD".into(),
            exchange_rate: 1.25,
            vat_case: VatCase::DrittlandLeistung,
            lines: vec![Line { text: "Consulting".into(), qty: 1.0, price_net: 100_000, vat: 19, ..Line::default() }],
            ..Document::default()
        };
        let id = data.apply(Op::SaveDocument(doc), "t").unwrap().unwrap();
        data.apply(Op::FinalizeDocument(id), "t").unwrap();
        // 790 € arrive at the bank: paid in full, 10 € exchange loss.
        data.apply(Op::ImportBank(vec![BankTx { date: today(), amount: 79_000, name: "US Client".into(), reference: "fx".into(), ..BankTx::default() }]), "t").unwrap();
        let tx = data.bank.iter().find(|t| t.reference == "fx").unwrap().id;
        data.apply(Op::AssignBank(tx, BankAssign::Document { id, discount: false }), "t").unwrap();
        let d = data.documents.iter().find(|d| d.id == id).unwrap();
        assert_eq!(d.status, DocStatus::Bezahlt);
        let booked: Cents = data.bookings.iter().filter(|b| b.doc_id == Some(id)).map(|b| b.gross).sum();
        assert_eq!(booked, 79_000);
        let year = today().year();
        let j = journal(&data, NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), NaiveDate::from_ymd_opt(year, 12, 31).unwrap());
        assert!(j.iter().any(|p| p.debit == "2150" && p.amount == 1_000), "Kursverlust fehlt");
        let s = statements(&data, year);
        assert_eq!(s.total_assets, s.total_liabilities);
        assert_eq!(fmt_money_cur(crate::i18n::Lang::De, 123_456, "USD"), "1.234,56 USD");
        // Hours on a project become a draft invoice; deleting the draft frees them again.
        let pid = data.apply(Op::SaveProject(Project { name: "Website".into(), contact_id: Some(cid), rate: 8_000, ..Project::default() }), "t").unwrap().unwrap();
        let t1 = data.apply(Op::SaveTime(TimeEntry { project_id: pid, hours: 2.5, text: "Entwurf".into(), ..TimeEntry::default() }), "t").unwrap().unwrap();
        let inv = data.apply(Op::BillTimes(pid, vec![t1]), "t").unwrap().unwrap();
        let d = data.documents.iter().find(|d| d.id == inv).unwrap();
        assert_eq!(d.totals(false).net, 20_000);
        assert!(data.apply(Op::DeleteTime(t1), "t").is_err());
        data.apply(Op::DeleteDocument(inv), "t").unwrap();
        assert!(data.times[0].invoice_id.is_none());
        // The forecast sees the open invoices.
        let f = forecast(&data, 100_000, today(), 8);
        assert_eq!(f.len(), 8);
        assert!(f.iter().map(|w| w.inflow).sum::<Cents>() > 0);
    }

    #[test]
    fn skr04_mapping() {
        assert_eq!(account_in(Chart::Skr04, "8400 Erlöse 19 % USt"), "4400");
        assert_eq!(account_in(Chart::Skr03, "8400 Erlöse 19 % USt"), "8400");
        assert_eq!(account_in(Chart::Skr04, "4930 Bürobedarf"), "6815");
    }
}
