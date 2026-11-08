//! Contacts, articles, income/expense bookings and the change log.

use super::*;

const CONTACT_COLS: [Col; 7] = [
    col("Nr.", 0.7),
    col("Name", 2.2),
    col("Ansprechpartner", 1.5),
    col("Ort", 1.5),
    col("Art", 0.9),
    rcol("Umsatz netto", 1.1),
    rcol("Offen", 1.0),
];

impl App {
    pub(super) fn contacts(&mut self, ui: &mut Ui) {
        toolbar(ui, "contacts", |ui, side| match side {
            Side::Left => {
            ui.add(
                TextEdit::singleline(&mut self.contact_search)
                    .hint_text(t("Suchen …"))
                    .desired_width(240.0),
            );
            ui.selectable_value(&mut self.contact_kind, None, t("Alle"));
            ui.selectable_value(&mut self.contact_kind, Some(ContactKind::Kunde), t("Kunden"));
            ui.selectable_value(&mut self.contact_kind, Some(ContactKind::Lieferant), t("Lieferanten"));
            }
            Side::Right => {
                if primary(ui, t("+ Neuer Kontakt")).clicked() {
                    self.contact_edit = Some(Contact {
                        kind: self.contact_kind.unwrap_or(ContactKind::Kunde),
                        ..Contact::default()
                    });
                }
            }
        });
        ui.add_space(8.0);
        let q = self.contact_search.to_lowercase();
        let small = self.data.company.small_business;
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &CONTACT_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let mut list: Vec<&Contact> = self
                    .data
                    .contacts
                    .iter()
                    .filter(|c| self.contact_kind.is_none_or(|k| c.kind == k))
                    .filter(|c| {
                        q.is_empty()
                            || format!("{} {} {} {} {}", c.number, c.company, c.person, c.zip_city, c.email)
                                .to_lowercase()
                                .contains(&q)
                    })
                    .collect();
                list.sort_by_key(|c| c.display_name().to_lowercase());
                if list.is_empty() {
                    empty_hint(ui, t("Keine Kontakte gefunden."));
                }
                for c in list {
                    let mut revenue = 0;
                    let mut due = 0;
                    for d in self
                        .data
                        .documents
                        .iter()
                        .filter(|d| d.contact_id == Some(c.id) && d.kind != DocKind::Angebot)
                    {
                        match d.status {
                            DocStatus::Bezahlt => revenue += d.to_eur(d.totals(small).net),
                            DocStatus::Offen => due += d.to_eur(d.open_amount(small)),
                            _ => {}
                        }
                    }
                    let cells = [
                        Cell::Muted(c.number.clone()),
                        Cell::B(c.display_name()),
                        Cell::T(c.person.clone()),
                        Cell::T(c.zip_city.clone()),
                        Cell::Chip(c.kind.label().into()),
                        Cell::Money(revenue),
                        if due > 0 {
                            Cell::MoneyColored(due, ORANGE)
                        } else {
                            Cell::Muted("–".into())
                        },
                    ];
                    if table_row(ui, &CONTACT_COLS, &cells, false).clicked() {
                        open = Some(c.clone());
                    }
                }
            });
        });
        if open.is_some() {
            self.contact_edit = open;
        }
    }

    pub(super) fn contact_dialog(&mut self, ctx: &egui::Context) {
        let mut c = self.contact_edit.take().unwrap();
        let mut keep = true;
        let mut save = false;
        let mut delete = false;
        let mut open_doc = None;
        let modal = egui::Modal::new(Id::unique("contact")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(
                RichText::new(if c.id == 0 { t("Neuer Kontakt") } else { t("Kontakt bearbeiten") })
                    .size(18.0)
                    .strong(),
            );
            ui.add_space(6.0);
            egui::Grid::new("contact_grid")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label(muted(t("Art")));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut c.kind, ContactKind::Kunde, t("Kunde"));
                        ui.radio_value(&mut c.kind, ContactKind::Lieferant, t("Lieferant"));
                    });
                    ui.end_row();
                    ui.label(muted(t("Kontakt-Nr.")));
                    ui.add(
                        TextEdit::singleline(&mut c.number)
                            .hint_text(t("wird vergeben"))
                            .desired_width(140.0),
                    );
                    ui.end_row();
                    text_row(ui, t("Firma"), &mut c.company, 380.0);
                    text_row(ui, t("Ansprechpartner"), &mut c.person, 380.0);
                    text_row(ui, t("Straße"), &mut c.street, 380.0);
                    text_row(ui, t("PLZ und Ort"), &mut c.zip_city, 380.0);
                    text_row(ui, t("Land"), &mut c.country, 380.0);
                    text_row(ui, t("E-Mail"), &mut c.email, 380.0);
                    text_row(ui, t("Telefon"), &mut c.phone, 380.0);
                    text_row(ui, t("USt-IdNr."), &mut c.vat_id, 380.0);
                    text_row(ui, t("IBAN"), &mut c.iban, 380.0);
                    ui.label(muted(t("Währung")));
                    let mut cur = if c.currency.is_empty() { "EUR".to_string() } else { c.currency.clone() };
                    egui::ComboBox::from_id_salt("contact_currency").width(80.0).selected_text(&cur).show_ui(ui, |ui| {
                        for code in CURRENCIES {
                            ui.selectable_value(&mut cur, code.to_string(), code);
                        }
                    });
                    c.currency = if cur == "EUR" { String::new() } else { cur };
                    ui.end_row();
                    ui.label(muted(t("Umsatzsteuer")));
                    egui::ComboBox::from_id_salt("contact_vat_case")
                        .width(300.0)
                        .selected_text(c.vat_case.label())
                        .show_ui(ui, |ui| {
                            for v in VatCase::ALL {
                                ui.selectable_value(&mut c.vat_case, v, v.label());
                            }
                        });
                    ui.end_row();
                    if matches!(c.vat_case, VatCase::IgLieferung | VatCase::ReverseCharge) && c.vat_id.trim().is_empty() {
                        ui.label("");
                        ui.label(RichText::new(t("Für EU-Geschäfte ohne Umsatzsteuer ist die USt-IdNr. des Kunden Pflicht.")).color(ORANGE));
                        ui.end_row();
                    }
                    ui.label(muted(t("Zahlungsbedingung")));
                    term_combo(
                        ui,
                        "contact_term",
                        &self.data.company.payment_terms,
                        &mut c.payment_term_id,
                        Some(t("Standard der Firma")),
                    );
                    ui.end_row();
                    ui.label(muted(t("Rechnungsversand")));
                    egui::ComboBox::from_id_salt("contact_einvoice")
                        .width(300.0)
                        .selected_text(match c.einvoice {
                            None => tf!("Standard der Firma ({})", t(self.data.company.einvoice_default.name())),
                            Some(f) => t(f.name()).to_string(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut c.einvoice, None, t("Standard der Firma"));
                            for f in crate::xrechnung::EInvoiceFormat::ALL {
                                ui.selectable_value(&mut c.einvoice, Some(f), t(f.name()));
                            }
                        });
                    ui.end_row();
                    ui.label(muted(t("Leitweg-ID / Käuferreferenz")));
                    ui.add(
                        TextEdit::singleline(&mut c.buyer_reference)
                            .hint_text(t("bei Behörden Pflicht, sonst z. B. Bestellnummer"))
                            .desired_width(300.0),
                    );
                    ui.end_row();
                    ui.label(muted(t("Sprache der Belege")));
                    egui::ComboBox::from_id_salt("contact_lang")
                        .selected_text(c.language.native_name())
                        .show_ui(ui, |ui| {
                            for l in crate::i18n::Lang::ALL {
                                ui.selectable_value(&mut c.language, l, l.native_name());
                            }
                        });
                    ui.end_row();
                    ui.label(muted(t("Notiz")));
                    ui.add(TextEdit::multiline(&mut c.note).desired_rows(2).desired_width(380.0));
                    ui.end_row();
                });
            if c.id != 0 {
                let docs: Vec<&Document> = self
                    .data
                    .documents
                    .iter()
                    .filter(|d| d.contact_id == Some(c.id))
                    .collect();
                if !docs.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t("Belege")).strong());
                    ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        for d in docs.iter().rev() {
                            ui.horizontal(|ui| {
                                let num = if d.number.is_empty() {
                                    t("Entwurf").into()
                                } else {
                                    d.number.clone()
                                };
                                if ui.link(format!("{} {num}", d.kind.label())).clicked() {
                                    open_doc = Some(d.id);
                                }
                                ui.label(muted(fmt_date(d.date)));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_money_cur(crate::i18n::lang(), d.totals(self.data.company.small_business).gross, d.currency()));
                                    chip(ui, status_label(d, self.today));
                                });
                            });
                        }
                    });
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
                if c.id != 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger(ui, t("Löschen")).clicked() {
                            delete = true;
                        }
                    });
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save {
            if c.company.trim().is_empty() && c.person.trim().is_empty() {
                self.error(t("Bitte eine Firma oder einen Namen eingeben."));
            } else if self.run(Op::SaveContact(c.clone())).is_ok() {
                self.info(t("Kontakt gespeichert."));
                keep = false;
            }
        }
        if delete {
            self.confirm = Some((
                tf!("Kontakt „{}“ löschen?", c.display_name()),
                Op::DeleteContact(c.id),
            ));
        }
        if let Some(id) = open_doc {
            keep = false;
            self.open_doc(Some(id));
        }
        if keep {
            self.contact_edit = Some(c);
        }
    }
}

// ---- Articles -------------------------------------------------------------

const ARTICLE_COLS: [Col; 7] = [
    col("Nr.", 0.8),
    col("Bezeichnung", 2.6),
    col("Art", 0.9),
    col("Einheit", 0.9),
    rcol("Netto", 1.0),
    rcol("USt.", 0.6),
    rcol("Brutto", 1.0),
];

fn gross_of(net: Cents, vat: u8) -> Cents {
    net + (net as f64 * vat as f64 / 100.0).round() as Cents
}

impl App {
    pub(super) fn articles(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.stock.tab, stock::StockTab::Artikel, t("Artikel & Leistungen"));
            ui.selectable_value(&mut self.stock.tab, stock::StockTab::Lager, t("Lagerbestand"));
        });
        ui.add_space(6.0);
        if self.stock.tab == stock::StockTab::Lager {
            self.stock_page(ui);
            return;
        }
        toolbar(ui, "articles", |ui, side| match side {
            Side::Left => {
            ui.add(
                TextEdit::singleline(&mut self.article_search)
                    .hint_text(t("Suchen …"))
                    .desired_width(240.0),
            );
            }
            Side::Right => {
                if primary(ui, t("+ Neuer Artikel")).clicked() {
                    self.article_edit = Some(Article::default());
                }
            }
        });
        ui.add_space(8.0);
        let q = self.article_search.to_lowercase();
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &ARTICLE_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let list: Vec<&Article> = self
                    .data
                    .articles
                    .iter()
                    .filter(|a| {
                        q.is_empty()
                            || format!("{} {} {}", a.number, a.name, a.description)
                                .to_lowercase()
                                .contains(&q)
                    })
                    .collect();
                if list.is_empty() {
                    empty_hint(ui, t("Keine Artikel gefunden."));
                }
                for a in list {
                    let cells = [
                        Cell::Muted(a.number.clone()),
                        Cell::B(a.name.clone()),
                        Cell::T(if a.service { t("Leistung") } else { t("Ware") }.into()),
                        Cell::T(a.unit.clone()),
                        Cell::Money(a.price_net),
                        Cell::T(format!("{} %", a.vat)),
                        Cell::Money(gross_of(a.price_net, a.vat)),
                    ];
                    if table_row(ui, &ARTICLE_COLS, &cells, false).clicked() {
                        open = Some(a.clone());
                    }
                }
            });
        });
        if open.is_some() {
            self.article_edit = open;
        }
    }

    pub(super) fn article_dialog(&mut self, ctx: &egui::Context) {
        let mut a = self.article_edit.take().unwrap();
        let mut keep = true;
        let mut save = false;
        let mut delete = false;
        let modal = egui::Modal::new(Id::unique("article")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(
                RichText::new(if a.id == 0 { t("Neuer Artikel") } else { t("Artikel bearbeiten") })
                    .size(18.0)
                    .strong(),
            );
            ui.add_space(6.0);
            egui::Grid::new("article_grid")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label(muted(t("Art")));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut a.service, true, t("Dienstleistung"));
                        ui.radio_value(&mut a.service, false, t("Ware"));
                    });
                    ui.end_row();
                    ui.label(muted(t("Artikel-Nr.")));
                    ui.add(
                        TextEdit::singleline(&mut a.number)
                            .hint_text(t("wird vergeben"))
                            .desired_width(140.0),
                    );
                    ui.end_row();
                    text_row(ui, t("Bezeichnung"), &mut a.name, 340.0);
                    ui.label(muted(t("Beschreibung")));
                    ui.add(TextEdit::multiline(&mut a.description).desired_rows(2).desired_width(340.0));
                    ui.end_row();
                    text_row(ui, t("Einheit"), &mut a.unit, 140.0);
                    ui.label(muted(t("Preis netto")));
                    ui.horizontal(|ui| {
                        money_edit(ui, Id::unique("art_price"), &mut a.price_net, 110.0);
                        ui.label("€");
                    });
                    ui.end_row();
                    ui.label(muted(t("Umsatzsteuer")));
                    vat_combo(ui, "art_vat", &mut a.vat);
                    ui.end_row();
                    ui.label(muted(t("Preis brutto")));
                    ui.label(fmt_money(gross_of(a.price_net, a.vat)));
                    ui.end_row();
                    if !a.service {
                        ui.label(muted(t("Einkaufspreis netto")));
                        ui.horizontal(|ui| {
                            money_edit(ui, Id::unique("art_purchase"), &mut a.purchase_price, 110.0);
                            ui.label("€");
                        });
                        ui.end_row();
                        ui.label(muted(t("Lager")));
                        ui.checkbox(&mut a.track_stock, t("Lagerbestand führen"));
                        ui.end_row();
                        if a.track_stock {
                            ui.label(muted(t("Mindestbestand")));
                            qty_edit(ui, Id::unique("art_min"), &mut a.min_stock, 80.0);
                            ui.end_row();
                            if a.id != 0 {
                                ui.label(muted(t("Bestand")));
                                ui.label(RichText::new(format!("{} {}", fmt_qty(self.data.stock_of(a.id)), a.unit)).strong());
                                ui.end_row();
                            }
                        }
                    }
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
                if a.id != 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger(ui, t("Löschen")).clicked() {
                            delete = true;
                        }
                    });
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save {
            if a.name.trim().is_empty() {
                self.error(t("Bitte eine Bezeichnung eingeben."));
            } else if self.run(Op::SaveArticle(a.clone())).is_ok() {
                self.info(t("Artikel gespeichert."));
                keep = false;
            }
        }
        if delete {
            self.confirm = Some((tf!("Artikel „{}“ löschen?", a.name), Op::DeleteArticle(a.id)));
        }
        if keep {
            self.article_edit = Some(a);
        }
    }
}

// ---- Income and expenses ----------------------------------------------------

const BOOKING_COLS: [Col; 7] = [
    col("Datum", 0.9),
    col("Art", 0.9),
    col("Kategorie / Konto", 2.0),
    col("Buchungstext", 2.2),
    col("Partner", 1.5),
    rcol("USt.", 0.5),
    rcol("Betrag brutto", 1.2),
];

impl App {
    pub(super) fn bookings(&mut self, ui: &mut Ui) {
        let years = self.years();
        toolbar(ui, "bookings", |ui, side| match side {
            Side::Left => {
            egui::ComboBox::from_id_salt("b_year")
                .selected_text(self.booking_year.to_string())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for y in &years {
                        ui.selectable_value(&mut self.booking_year, *y, y.to_string());
                    }
                });
            ui.selectable_value(&mut self.booking_kind, None, t("Alle"));
            ui.selectable_value(&mut self.booking_kind, Some(BookingKind::Einnahme), t("Einnahmen"));
            ui.selectable_value(&mut self.booking_kind, Some(BookingKind::Ausgabe), t("Ausgaben"));
            ui.add(
                TextEdit::singleline(&mut self.booking_search)
                    .hint_text(t("Text, Partner, Konto …"))
                    .desired_width(200.0),
            );
            }
            Side::Right => {
                if primary(ui, t("+ Ausgabe")).clicked() {
                    self.booking_edit = Some(Booking {
                        date: self.today,
                        ..Booking::default()
                    });
                }
                if ui.button(t("+ Einnahme")).clicked() {
                    self.booking_edit = Some(Booking {
                        date: self.today,
                        kind: BookingKind::Einnahme,
                        category: INCOME_CATEGORIES[0].into(),
                        ..Booking::default()
                    });
                }
                if ui.button(t("CSV exportieren")).clicked() {
                    let (from, to) = Period::Year.range(self.booking_year);
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("Buchungen-{}.csv", self.booking_year))
                        .add_filter(t("CSV"), &["csv"])
                        .save_file()
                    {
                        // A BOM lets spreadsheet programs detect UTF-8 (umlauts).
                        let text = format!("\u{feff}{}", export::bookings_csv(&self.data, from, to));
                        match std::fs::write(&path, text) {
                            Ok(()) => self.info(t("Buchungen exportiert.")),
                            Err(e) => self.error(e.to_string()),
                        }
                    }
                }
            }
        });
        ui.add_space(8.0);
        let q = self.booking_search.to_lowercase();
        let list: Vec<Booking> = self
            .data
            .bookings
            .iter()
            .rev()
            .filter(|b| b.date.year() == self.booking_year)
            .filter(|b| self.booking_kind.is_none_or(|k| b.kind == k))
            .filter(|b| {
                q.is_empty()
                    || format!("{} {} {} {}", b.text, b.partner, b.category, b.receipt)
                        .to_lowercase()
                        .contains(&q)
            })
            .cloned()
            .collect();
        let inc: Cents = list.iter().filter(|b| b.kind == BookingKind::Einnahme).map(|b| b.gross).sum();
        let exp: Cents = list.iter().filter(|b| b.kind == BookingKind::Ausgabe).map(|b| b.gross).sum();
        let w = (ui.available_width() - 28.0) / 3.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let year = self.booking_year.to_string();
            kpi(ui, w, t("Einnahmen (brutto)"), &fmt_money(inc), &year, INCOME);
            kpi(ui, w, t("Ausgaben (brutto)"), &fmt_money(exp), &year, EXPENSE);
            kpi(
                ui,
                w,
                t("Saldo"),
                &fmt_money(inc - exp),
                &tf!("{} Buchungen", list.len()),
                if inc >= exp { GREEN } else { RED },
            );
        });
        ui.add_space(12.0);
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &BOOKING_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if list.is_empty() {
                    empty_hint(ui, t("Keine Buchungen in diesem Zeitraum."));
                }
                for b in &list {
                    let cells = [
                        Cell::T(fmt_date(b.date)),
                        Cell::Chip(b.kind.label().into()),
                        Cell::Muted(tr(&b.category)),
                        Cell::T(b.text.clone()),
                        Cell::T(b.partner.clone()),
                        Cell::Muted(format!("{} %", b.vat)),
                        Cell::MoneyColored(
                            b.signed(),
                            if b.kind == BookingKind::Einnahme { GREEN } else { RED },
                        ),
                    ];
                    if table_row(ui, &BOOKING_COLS, &cells, false).clicked() {
                        open = Some(b.clone());
                    }
                }
            });
        });
        if open.is_some() {
            self.booking_edit = open;
        }
    }

    pub(super) fn booking_dialog(&mut self, ctx: &egui::Context) {
        let mut b = self.booking_edit.take().unwrap();
        let mut keep = true;
        let mut save = false;
        let mut delete = false;
        let mut open_doc = None;
        let linked = b.is_linked();
        let mut open_receipt = None;
        let modal = egui::Modal::new(Id::unique("booking")).show(ctx, |ui| {
            ui.set_width(540.0);
            let title = match (b.id == 0, b.kind) {
                (true, BookingKind::Ausgabe) => t("Ausgabe erfassen"),
                (true, BookingKind::Einnahme) => t("Einnahme erfassen"),
                (false, _) => t("Buchung"),
            };
            ui.label(RichText::new(title).size(18.0).strong());
            if b.bank_tx.is_some() && b.receipt_id.is_none() {
                ui.label(muted(t("Diese Buchung entstand aus einem Bankumsatz und wird unter „Bank“ verwaltet.")));
            } else if let Some(rid) = b.receipt_id {
                ui.label(muted(t("Diese Buchung stammt aus dem Belegeingang und wird dort verwaltet.")));
                if ui.link(t("Zum Originalbeleg")).clicked() {
                    open_receipt = Some(rid);
                }
            } else if linked {
                ui.label(muted(
                    t("Diese Buchung entstand aus einer bezahlten Rechnung und wird dort verwaltet."),
                ));
                if ui.link(t("Zum Beleg")).clicked() {
                    open_doc = b.doc_id;
                }
            }
            ui.add_space(6.0);
            ui.add_enabled_ui(!linked, |ui| {
                egui::Grid::new("booking_grid")
                    .num_columns(2)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(muted(t("Art")));
                        ui.horizontal(|ui| {
                            let before = b.kind;
                            ui.radio_value(&mut b.kind, BookingKind::Ausgabe, t("Ausgabe"));
                            ui.radio_value(&mut b.kind, BookingKind::Einnahme, t("Einnahme"));
                            if before != b.kind {
                                b.category = match b.kind {
                                    BookingKind::Einnahme => INCOME_CATEGORIES[0].into(),
                                    BookingKind::Ausgabe => EXPENSE_CATEGORIES[9].into(),
                                };
                            }
                        });
                        ui.end_row();
                        ui.label(muted(t("Datum")));
                        date_edit(ui, Id::unique("b_date"), &mut b.date);
                        ui.end_row();
                        ui.label(muted(t("Kategorie / Konto")));
                        let cats: &[&str] = match b.kind {
                            BookingKind::Einnahme => &INCOME_CATEGORIES,
                            BookingKind::Ausgabe => &EXPENSE_CATEGORIES,
                        };
                        egui::ComboBox::from_id_salt("b_cat")
                            .width(300.0)
                            .selected_text(tr(&b.category))
                            .show_ui(ui, |ui| {
                                for c in cats {
                                    ui.selectable_value(&mut b.category, c.to_string(), t(c));
                                }
                            });
                        ui.end_row();
                        text_row(ui, t("Buchungstext"), &mut b.text, 300.0);
                        text_row(ui, t("Partner"), &mut b.partner, 300.0);
                        text_row(ui, t("Belegnummer"), &mut b.receipt, 160.0);
                        ui.label(muted(t("Betrag brutto")));
                        ui.horizontal(|ui| {
                            money_edit(ui, Id::unique("b_gross"), &mut b.gross, 110.0);
                            ui.label("€");
                        });
                        ui.end_row();
                        ui.label(muted(t("Steuersatz")));
                        vat_combo(ui, "b_vat", &mut b.vat);
                        ui.end_row();
                        if b.kind == BookingKind::Ausgabe {
                            ui.label("");
                            ui.checkbox(&mut b.reverse_charge, t("Ausländischer Lieferant, ich schulde die Umsatzsteuer (§ 13b)"))
                                .on_hover_text(t("z. B. Werbung bei Google oder Meta aus Irland: Die Rechnung hat keine Umsatzsteuer. Der Betrag ist netto; die Steuer wird geschuldet und gleichzeitig als Vorsteuer abgezogen."));
                            ui.end_row();
                        }
                        ui.label(muted(t("davon")));
                        if b.reverse_charge {
                            ui.label(tf!("netto {} · Steuer {} (geschuldet und abziehbar)", fmt_money(b.net()), fmt_money(b.reverse_charge_tax())));
                        } else {
                            ui.label(tf!(
                                "netto {} · Steuer {}",
                                fmt_money(b.net()),
                                fmt_money(b.tax())
                            ));
                        }
                        ui.end_row();
                        ui.label(muted(t("Zahlungsweg")));
                        ui.horizontal(|ui| {
                            for p in ["Bank", "Kasse", "Karte", "PayPal"] {
                                ui.selectable_value(&mut b.payment, p.to_string(), t(p));
                            }
                        });
                        ui.end_row();
                    });
            });
            if b.category == ASSET_CATEGORY {
                ui.label(muted(t("Anlagegüter werden nicht sofort als Ausgabe gezählt, sondern über die Nutzungsdauer abgeschrieben. Nach dem Speichern unter „Anlagen“ erfassen.")));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if !linked && primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(if linked { t("Schließen") } else { t("Abbrechen") }).clicked() {
                    keep = false;
                }
                if b.id != 0 && !linked {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger(ui, t("Löschen")).clicked() {
                            delete = true;
                        }
                    });
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save {
            if b.gross <= 0 {
                self.error(t("Bitte einen Betrag größer als 0 eingeben."));
            } else if let Ok(id) = self.run(Op::SaveBooking(b.clone())) {
                self.info(t("Buchung gespeichert."));
                self.booking_year = b.date.year();
                keep = false;
                // A fixed asset goes on into the asset register.
                if b.category == ASSET_CATEGORY && !self.data.assets.iter().any(|a| a.booking_id == id) {
                    self.books.tab = books::BooksTab::Anlagen;
                    self.books.asset_edit = Some(Asset {
                        name: b.text.clone(),
                        date: b.date,
                        cost: b.net(),
                        booking_id: id,
                        ..Asset::default()
                    });
                }
            }
        }
        if delete {
            self.confirm = Some((tf!("Buchung „{}“ löschen?", b.text), Op::DeleteBooking(b.id)));
        }
        if let Some(id) = open_doc {
            keep = false;
            self.open_doc(Some(id));
        }
        if let Some(id) = open_receipt {
            keep = false;
            self.open_receipt(id);
        }
        if keep {
            self.booking_edit = Some(b);
        }
    }
}

// ---- Change log ---------------------------------------------------------------

impl App {
    pub(super) fn log(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.log_search)
                    .hint_text(t("Benutzer, Aktion …"))
                    .desired_width(260.0),
            );
            ui.label(muted(t("Jede Änderung wird mit Zeit und Benutzer festgehalten.")));
        });
        ui.add_space(8.0);
        let q = self.log_search.to_lowercase();
        let cols = [col("Zeitpunkt", 1.2), col("Benutzer", 1.0), col("Aktion", 4.0)];
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &cols);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let list: Vec<&LogEntry> = self
                    .data
                    .log
                    .iter()
                    .rev()
                    .filter(|e| {
                        q.is_empty() || format!("{} {}", e.user, e.action).to_lowercase().contains(&q)
                    })
                    .take(500)
                    .collect();
                if list.is_empty() {
                    empty_hint(ui, t("Noch keine Änderungen."));
                }
                for e in list {
                    table_row(
                        ui,
                        &cols,
                        &[
                            Cell::Muted(e.time.clone()),
                            Cell::B(e.user.clone()),
                            Cell::T(e.action.clone()),
                        ],
                        false,
                    );
                }
            });
        });
    }
}
