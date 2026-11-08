//! Offers, invoices and credit notes: list, editor and paper preview.

use super::*;

const DOC_COLS: [Col; 7] = [
    col("Nummer", 1.3),
    col("Art", 1.0),
    col("Kontakt", 2.3),
    col("Datum", 1.0),
    col("Fällig", 1.0),
    col("Status", 1.1),
    rcol("Betrag brutto", 1.2),
];

impl App {
    pub(super) fn documents(&mut self, ui: &mut Ui) {
        let small = self.data.company.small_business;
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(!self.doc_view_recurring && self.doc_kind.is_none(), t("Alle")).clicked() {
                self.doc_kind = None;
                self.doc_view_recurring = false;
            }
            for (k, label) in [
                (DocKind::Angebot, t("Angebote")),
                (DocKind::Auftragsbestaetigung, t("Aufträge")),
                (DocKind::Lieferschein, t("Lieferscheine")),
                (DocKind::Rechnung, t("Rechnungen")),
                (DocKind::Gutschrift, t("Gutschriften")),
                (DocKind::Bestellung, t("Bestellungen")),
            ] {
                if ui.selectable_label(!self.doc_view_recurring && self.doc_kind == Some(k), label).clicked() {
                    self.doc_kind = Some(k);
                    self.doc_view_recurring = false;
                }
            }
            let due = self.data.due_recurring(self.today);
            let label = if due > 0 { tf!("Wiederkehrend ({} fällig)", due) } else { t("Wiederkehrend").to_string() };
            if ui.selectable_label(self.doc_view_recurring, label).clicked() {
                self.doc_view_recurring = true;
            }
        });
        if self.doc_view_recurring {
            self.recurring_list(ui);
            return;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("doc_status")
                .selected_text(self.doc_status.map(|s| s.label()).unwrap_or(t("Jeder Status")))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.doc_status, None, t("Jeder Status"));
                    for s in [
                        DocStatus::Entwurf,
                        DocStatus::Offen,
                        DocStatus::Bezahlt,
                        DocStatus::Angenommen,
                        DocStatus::Abgelehnt,
                        DocStatus::Storniert,
                    ] {
                        ui.selectable_value(&mut self.doc_status, Some(s), s.label());
                    }
                });
            ui.add(
                TextEdit::singleline(&mut self.doc_search)
                    .hint_text(t("Nummer, Kunde …"))
                    .desired_width(200.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button(t("+ Weitere …"), |ui| {
                    for k in [DocKind::Auftragsbestaetigung, DocKind::Lieferschein, DocKind::Gutschrift, DocKind::Bestellung] {
                        if ui.button(k.label()).clicked() {
                            self.doc_edit = Some(self.new_doc(k));
                        }
                    }
                });
                if ui.button(t("+ Angebot")).clicked() {
                    self.doc_edit = Some(self.new_doc(DocKind::Angebot));
                }
                let overdue = self.data.documents.iter().filter(|d| d.is_overdue(self.today)).count();
                if overdue > 0 && ui.button(tf!("Mahnlauf ({} überfällig) …", overdue)).clicked() {
                    self.open_dunning();
                }
            });
        });
        ui.add_space(8.0);
        let q = self.doc_search.to_lowercase();
        let mut open = None;
        let mut list: Vec<&Document> = self
            .data
            .documents
            .iter()
            .filter(|d| self.doc_kind.is_none_or(|k| d.kind == k))
            .filter(|d| self.doc_status.is_none_or(|s| d.status == s))
            .filter(|d| {
                q.is_empty()
                    || format!("{} {} {}", d.number, self.data.contact_name(d.contact_id), d.title)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect();
        list.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let sum: Cents = list
            .iter()
            .filter(|d| d.kind.is_billing() && d.status != DocStatus::Storniert && d.status != DocStatus::Entwurf)
            .map(|d| d.to_eur(d.totals(small).gross))
            .sum();
        let count = list.len();
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &DOC_COLS);
            ScrollArea::vertical()
                .auto_shrink(false)
                .max_height(ui.available_height() - 34.0)
                .show(ui, |ui| {
                    if list.is_empty() {
                        empty_hint(ui, t("Keine Belege gefunden."));
                    }
                    for d in &list {
                        let cells = [
                            if d.number.is_empty() {
                                Cell::Muted(t("Entwurf").into())
                            } else {
                                Cell::B(d.number.clone())
                            },
                            Cell::T(d.kind.label().into()),
                            Cell::T(self.data.contact_name(d.contact_id)),
                            Cell::T(fmt_date(d.date)),
                            if d.kind == DocKind::Rechnung {
                                Cell::T(fmt_date(d.due_date()))
                            } else {
                                Cell::Muted("–".into())
                            },
                            Cell::Chip(status_label(d, self.today).into()),
                            if d.kind == DocKind::Lieferschein {
                                Cell::Muted("–".into())
                            } else {
                                if d.is_foreign() {
                                    Cell::RightText(fmt_money_cur(crate::i18n::lang(), d.totals(small).gross, d.currency()))
                                } else {
                                    Cell::Money(d.totals(small).gross)
                                }
                            },
                        ];
                        if table_row(ui, &DOC_COLS, &cells, false).clicked() {
                            open = Some(d.id);
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label(muted(tf!("{} Belege", count)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(fmt_money(sum)).strong());
                    ui.label(muted(t("Summe Rechnungen und Gutschriften:")));
                });
            });
        });
        if open.is_some() {
            self.open_doc(open);
        }
    }

    pub(super) fn document_page(&mut self, ui: &mut Ui) {
        let mut doc = self.doc_edit.take().unwrap();
        let editable = doc.status == DocStatus::Entwurf && self.backend.can_write();
        let mut close = false;
        let mut action: Option<Op> = None;
        let mut save = false;
        let mut finalize = false;
        let mut print = false;
        let mut open_mail: Option<MailKind> = None;
        let mut save_as = false;
        let mut save_xml = false;

        // Header with the actions that fit the document's state.
        ui.horizontal(|ui| {
            if ui.button(t("← Zurück")).clicked() {
                close = true;
            }
            let title = if doc.number.is_empty() {
                tf!("{} (Entwurf)", doc.kind.label())
            } else {
                format!("{} {}", doc.kind.label(), doc.number)
            };
            ui.label(RichText::new(title).size(19.0).strong());
            chip(ui, status_label(&doc, self.today));
        });
        ui.add_space(4.0);
        // Actions; they wrap onto a second line on narrow windows.
        ui.horizontal_wrapped(|ui| {
            if doc.status == DocStatus::Entwurf {
                if primary_enabled(ui, editable, t("Fertigstellen"))
                    .on_hover_text(t("Vergibt die fortlaufende Nummer. Danach bleibt der Beleg unverändert."))
                    .clicked()
                {
                    finalize = true;
                }
                if ui.add_enabled(editable, egui::Button::new(t("Speichern"))).clicked() {
                    save = true;
                }
            } else {
                let sales = self.backend.allows(OpArea::Sales);
                let has_advances = self
                    .data
                    .documents
                    .iter()
                    .any(|d| d.source_id == Some(doc.id) && d.part == InvoicePart::Abschlag && !matches!(d.status, DocStatus::Entwurf | DocStatus::Storniert));
                match (doc.kind, doc.status) {
                    (DocKind::Angebot | DocKind::Auftragsbestaetigung, DocStatus::Offen | DocStatus::Angenommen) if sales => {
                        if has_advances {
                            if primary(ui, t("Schlussrechnung")).on_hover_text(t("Rechnet alles ab und zieht die Abschlagsrechnungen ab.")).clicked() {
                                action = Some(Op::CreateFinalInvoice(doc.id));
                            }
                        } else if primary(ui, t("In Rechnung umwandeln")).clicked() {
                            action = Some(Op::ConvertDocument(doc.id, DocKind::Rechnung));
                        }
                        if doc.kind == DocKind::Angebot && ui.button(t("Auftragsbestätigung")).clicked() {
                            action = Some(Op::ConvertDocument(doc.id, DocKind::Auftragsbestaetigung));
                        }
                        if ui.button(t("Lieferschein")).clicked() {
                            action = Some(Op::ConvertDocument(doc.id, DocKind::Lieferschein));
                        }
                        ui.menu_button(t("Abschlagsrechnung …"), |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut self.advance_pct).range(1.0..=100.0).speed(1.0).suffix(" %"));
                                if ui.button(t("erstellen")).clicked() {
                                    action = Some(Op::CreateAdvanceInvoice(doc.id, self.advance_pct));
                                    ui.close();
                                }
                            });
                            ui.label(muted(t("Anteil des Auftrags, der jetzt berechnet wird")));
                        });
                        if doc.kind == DocKind::Angebot && doc.status == DocStatus::Offen {
                            if ui.button(t("Angenommen")).clicked() {
                                action = Some(Op::SetDocStatus(doc.id, DocStatus::Angenommen));
                            }
                            if ui.button(t("Abgelehnt")).clicked() {
                                action = Some(Op::SetDocStatus(doc.id, DocStatus::Abgelehnt));
                            }
                        }
                    }
                    (DocKind::Lieferschein, DocStatus::Offen) if sales => {
                        if primary(ui, t("In Rechnung umwandeln")).clicked() {
                            action = Some(Op::ConvertDocument(doc.id, DocKind::Rechnung));
                        }
                        if ui.button(t("Als geliefert markieren")).clicked() {
                            action = Some(Op::SetDocStatus(doc.id, DocStatus::Erledigt));
                        }
                    }
                    (DocKind::Lieferschein, DocStatus::Erledigt) if sales => {
                        if primary(ui, t("In Rechnung umwandeln")).clicked() {
                            action = Some(Op::ConvertDocument(doc.id, DocKind::Rechnung));
                        }
                    }
                    (DocKind::Bestellung, DocStatus::Offen) if sales => {
                        if primary(ui, t("Ware erhalten"))
                            .on_hover_text(t("Bucht die bestellten Artikel ins Lager ein."))
                            .clicked()
                        {
                            action = Some(Op::ReceivePurchase(doc.id));
                        }
                    }
                    (_, DocStatus::Bezahlt) => {
                        if ui.button(t("Zahlung zurücknehmen")).clicked() {
                            action = Some(Op::MarkUnpaid(doc.id));
                        }
                    }
                    _ => {}
                }
                if icon_button(ui, Icon::Mail, t("Per E-Mail senden …")).clicked() {
                    open_mail = Some(MailKind::for_doc(doc.kind));
                }
                if doc.kind == DocKind::Rechnung && doc.status == DocStatus::Offen {
                    let level = crate::mail::next_reminder_level(&doc);
                    let label = tf!("{} …", t(reminder_title(level)));
                    let resp = if doc.is_overdue(self.today) {
                        ui.add(egui::Button::new(RichText::new(label).color(RED)).stroke(Stroke::new(1.0, RED)))
                    } else {
                        ui.button(label)
                    };
                    if resp.clicked() {
                        open_mail = Some(MailKind::Mahnung(level));
                    }
                }
            }
            if ui.button(t("PDF anzeigen / drucken")).clicked() {
                print = true;
            }
            if ui.button(t("PDF speichern …")).clicked() {
                save_as = true;
            }
            if doc.status == DocStatus::Entwurf {
                if doc.id != 0
                    && ui
                        .add_enabled(editable, egui::Button::new(RichText::new(t("Löschen")).color(RED)))
                        .clicked()
                {
                    self.confirm = Some((t("Diesen Entwurf löschen?").into(), Op::DeleteDocument(doc.id)));
                }
            } else {
                ui.menu_button(t("Weitere …"), |ui| {
                    if matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift)
                        && ui.button(t("Als XRechnung (XML) speichern …")).clicked()
                    {
                        save_xml = true;
                    }
                    if ui.button(t("Kopie als neuer Entwurf")).clicked() {
                        action = Some(Op::DuplicateDocument(doc.id));
                    }
                    if doc.kind == DocKind::Rechnung && ui.button(t("Als wiederkehrende Rechnung einrichten …")).clicked() {
                        self.recurring_edit = Some(Recurring {
                            name: format!("{} – {}", self.data.contact_name(doc.contact_id), doc.title),
                            template: doc.clone(),
                            next_date: doc.date.checked_add_months(chrono::Months::new(1)).unwrap_or(doc.date),
                            ..Recurring::default()
                        });
                    }
                    if doc.kind == DocKind::Rechnung && doc.status != DocStatus::Storniert {
                        if ui.button(t("Gutschrift / Storno erstellen")).clicked() {
                            action = Some(Op::CreateCredit(doc.id));
                        }
                        if doc.status == DocStatus::Offen && ui.button(t("Als storniert markieren")).clicked() {
                            self.confirm = Some((
                                tf!("Rechnung {} als storniert markieren?", doc.number),
                                Op::SetDocStatus(doc.id, DocStatus::Storniert),
                            ));
                        }
                    }
                });
            }
        });

        // Recording a payment for an open invoice or credit note.
        if doc.kind.is_billing() && doc.status == DocStatus::Offen && self.backend.allows(OpArea::Accounting) {
            ui.add_space(6.0);
            let small = self.data.company.small_business;
            let open = doc.open_amount(small);
            ui.horizontal_wrapped(|ui| {
                ui.label(muted(t("Zahlung am")));
                if date_edit(ui, Id::unique("paid_date"), &mut self.doc_paid_date) {
                    self.doc_paid_discount = doc.discount_on(self.doc_paid_date);
                }
                ui.label(muted(t("Betrag")));
                money_edit(ui, Id::unique("pay_amount"), &mut self.doc_pay_amount, 90.0);
                egui::ComboBox::from_id_salt("pay_method").width(80.0).selected_text(t(if self.doc_pay_method == CASH { "Kasse" } else { "Bank" })).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.doc_pay_method, "Bank".to_string(), t("Bank"));
                    ui.selectable_value(&mut self.doc_pay_method, CASH.to_string(), t("Kasse"));
                });
                if !doc.discounts.is_empty() {
                    let allowed = doc.discount_on(self.doc_paid_date);
                    egui::ComboBox::from_id_salt("paid_discount")
                        .selected_text(if self.doc_paid_discount > 0.0 {
                            tf!("abzgl. {} % Skonto", fmt_qty(self.doc_paid_discount))
                        } else {
                            t("ohne Abzug").to_string()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.doc_paid_discount, 0.0, t("ohne Abzug"));
                            for s in &doc.discounts {
                                let label = tf!("abzgl. {} % Skonto", fmt_qty(s.percent));
                                ui.selectable_value(&mut self.doc_paid_discount, s.percent, label).on_hover_text(
                                    if s.percent <= allowed { t("Frist eingehalten") } else { t("Frist überschritten") },
                                );
                            }
                        });
                }
                let part = self.doc_pay_amount > 0 && self.doc_pay_amount < open.abs() && self.doc_paid_discount == 0.0;
                if primary(ui, if part { t("Teilzahlung buchen") } else { t("Als bezahlt buchen") }).clicked() {
                    let sign = if open < 0 { -1 } else { 1 };
                    action = Some(if part || self.doc_pay_method != "Bank" {
                        let discount = if part {
                            0
                        } else {
                            let gross = doc.totals(small).gross;
                            ((gross as f64 * self.doc_paid_discount / 100.0).round() as Cents).clamp(-open.abs(), open.abs())
                        };
                        let amount = if part { sign * self.doc_pay_amount } else { open - discount };
                        Op::AddPayment(
                            doc.id,
                            Payment { date: self.doc_paid_date, amount, discount, method: self.doc_pay_method.clone(), bank_tx: None, eur: None },
                        )
                    } else if self.doc_paid_discount > 0.0 {
                        Op::MarkPaidWithDiscount(doc.id, self.doc_paid_date, self.doc_paid_discount)
                    } else {
                        Op::MarkPaid(doc.id, self.doc_paid_date)
                    });
                }
            });
            if !doc.payments.is_empty() {
                ui.label(muted(tf!("Bereits bezahlt {} · noch offen {}", fmt_money_cur(crate::i18n::lang(), doc.paid_total().abs(), doc.currency()), fmt_money_cur(crate::i18n::lang(), open.abs(), doc.currency()))));
            }
        }
        ui.add_space(10.0);

        let total_w = ui.available_width();
        if !total_w.is_finite() || total_w < 100.0 {
            self.doc_edit = Some(doc);
            return;
        }
        let preview_w = (total_w * 0.38).clamp(320.0, 470.0);
        let form_w = total_w - preview_w - 16.0;
        let mut switch_to = None;
        egui::Panel::right("doc_preview_panel")
            .exact_size(preview_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 16, ..Margin::ZERO }))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("doc_preview")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        self.document_preview(ui, &doc, preview_w - 30.0);
                    });
            });
        ScrollArea::vertical()
            .id_salt("doc_form")
            .auto_shrink(false)
            .show(ui, |ui| {
                switch_to = self.document_form(ui, &mut doc, editable, form_w - 12.0);
            });

        // Apply what was clicked.
        if save || finalize {
            if let Ok(id) = self.run(Op::SaveDocument(doc.clone())) {
                if let Some(id) = id {
                    doc.id = id;
                }
                if finalize {
                    if self.run(Op::FinalizeDocument(doc.id)).is_ok() {
                        self.info(tf!("{} fertiggestellt.", doc.kind.label()));
                    }
                } else {
                    self.info(t("Entwurf gespeichert."));
                }
                if let Some(d) = self.data.documents.iter().find(|d| d.id == doc.id) {
                    doc = d.clone();
                }
            }
        }
        if let Some(op) = action {
            let opens_new = matches!(
                op,
                Op::ConvertOffer(_)
                    | Op::CreateCredit(_)
                    | Op::DuplicateDocument(_)
                    | Op::ConvertDocument(..)
                    | Op::CreateAdvanceInvoice(..)
                    | Op::CreateFinalInvoice(_)
            );
            if let Ok(id) = self.run(op) {
                let target = if opens_new { id.unwrap_or(doc.id) } else { doc.id };
                if opens_new {
                    self.info(t("Neuer Entwurf angelegt."));
                }
                if let Some(d) = self.data.documents.iter().find(|d| d.id == target) {
                    doc = d.clone();
                }
                self.doc_pay_amount = doc.open_amount(self.data.company.small_business).abs();
            }
        }
        if let Some(id) = switch_to
            && let Some(d) = self.data.documents.iter().find(|d| d.id == id)
        {
            doc = d.clone();
        }
        if print {
            self.print_doc(&doc);
        }
        if let Some(kind) = open_mail {
            self.open_mail(doc.id, kind);
        }
        if save_xml {
            self.save_xrechnung(&doc);
        }
        if save_as {
            self.save_doc_as(&doc);
        }
        if !close {
            self.doc_edit = Some(doc);
        }
    }

    /// The editable (or read-only) form. Returns a document to switch to, if a link was clicked.
    fn document_form(&mut self, ui: &mut Ui, doc: &mut Document, editable: bool, width: f32) -> Option<u64> {
        let small = self.data.company.small_business;
        let mut switch_to = None;
        card(ui, |ui| {
            ui.set_width(width - 34.0);
            ui.add_enabled_ui(editable, |ui| {
                egui::Grid::new("doc_head")
                    .num_columns(4)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(muted(t("Kontakt")));
                        let mut contacts: Vec<&Contact> = self.data.contacts.iter().collect();
                        let want = if doc.kind == DocKind::Bestellung { ContactKind::Lieferant } else { ContactKind::Kunde };
                        contacts.sort_by_key(|c| (c.kind != want, c.display_name().to_lowercase()));
                        let before = doc.contact_id;
                        egui::ComboBox::from_id_salt("doc_contact")
                            .width(250.0)
                            .selected_text(if doc.contact_id.is_some() {
                                self.data.contact_name(doc.contact_id)
                            } else {
                                t("Bitte wählen …").into()
                            })
                            .show_ui(ui, |ui| {
                                for c in contacts {
                                    ui.selectable_value(
                                        &mut doc.contact_id,
                                        Some(c.id),
                                        format!("{} ({})", c.display_name(), c.number),
                                    );
                                }
                            });
                        if doc.contact_id != before
                            && let Some(c) = self.data.contact(doc.contact_id)
                        {
                            super::follow_contact_language(doc, c.language);
                        }
                        if doc.contact_id != before && doc.kind == DocKind::Rechnung {
                            doc.apply_term(&self.data.term_for(doc.contact_id));
                        }
                        if doc.contact_id != before
                            && let Some(c) = self.data.contact(doc.contact_id)
                        {
                            doc.vat_case = c.vat_case;
                            if doc.currency != c.currency {
                                doc.currency = c.currency.clone();
                                doc.exchange_rate = 0.0;
                            }
                        }
                        ui.label(muted(t("Datum")));
                        date_edit(ui, Id::unique("doc_date"), &mut doc.date);
                        ui.end_row();

                        ui.label(muted(t("Titel")));
                        ui.add(TextEdit::singleline(&mut doc.title).desired_width(250.0));
                        if doc.kind == DocKind::Angebot {
                            ui.label(muted(t("Gültig (Tage)")));
                            ui.add(egui::DragValue::new(&mut doc.due_days).range(0..=365).suffix(t(" Tage")));
                        }
                        ui.end_row();

                        ui.label(muted(t("Leistungszeitraum")));
                        ui.add(
                            TextEdit::singleline(&mut doc.service_date)
                                .hint_text(t("z. B. Oktober 2026"))
                                .desired_width(250.0),
                        );
                        match doc.kind {
                            DocKind::Angebot => {
                                ui.label(muted(t("Gültig bis")));
                                ui.label(fmt_date(doc.due_date()));
                            }
                            DocKind::Rechnung => {
                                ui.label(muted(t("Fällig am")));
                                ui.label(fmt_date(doc.due_date()));
                            }
                            _ => {
                                ui.label("");
                                ui.label("");
                            }
                        }
                        ui.end_row();
                        if doc.kind != DocKind::Lieferschein {
                            ui.label(muted(t("Währung")));
                            ui.horizontal(|ui| {
                                let before = doc.currency().to_string();
                                let mut cur = before.clone();
                                egui::ComboBox::from_id_salt("doc_currency").width(80.0).selected_text(&cur).show_ui(ui, |ui| {
                                    for c in CURRENCIES {
                                        ui.selectable_value(&mut cur, c.to_string(), c);
                                    }
                                });
                                if cur != before {
                                    doc.currency = if cur == "EUR" { String::new() } else { cur };
                                    doc.exchange_rate = 0.0;
                                }
                                if doc.is_foreign() {
                                    ui.label(muted(t("1 EUR =")));
                                    ui.add(egui::DragValue::new(&mut doc.exchange_rate).range(0.0..=100_000.0).speed(0.001).max_decimals(4));
                                    ui.label(doc.currency().to_string());
                                    if ui.button(t("Kurs der EZB")).on_hover_text(t("Holt den heutigen Referenzkurs der Europäischen Zentralbank")).clicked() {
                                        match export::ecb_rate(doc.currency()) {
                                            Ok(r) => doc.exchange_rate = r,
                                            Err(e) => self.toast = Some((tr(&e), true, 0.0)),
                                        }
                                    }
                                }
                            });
                            ui.end_row();
                        }
                        if !small && doc.kind != DocKind::Lieferschein {
                            ui.label(muted(t("Umsatzsteuer")));
                            egui::ComboBox::from_id_salt("doc_vat_case").width(250.0).selected_text(doc.vat_case.label()).show_ui(ui, |ui| {
                                for v in VatCase::ALL {
                                    ui.selectable_value(&mut doc.vat_case, v, v.label());
                                }
                            });
                            ui.end_row();
                        }
                    });
                if doc.kind == DocKind::Rechnung && editable {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(muted(t("Zahlungsbedingung")));
                        let mut chosen = self
                            .data
                            .company
                            .payment_terms
                            .iter()
                            .find(|x| x.name == doc.term_name)
                            .map(|x| x.id);
                        if term_combo(ui, "doc_term", &self.data.company.payment_terms, &mut chosen, Some(t("Individuell")))
                            && let Some(term) = self.data.company.term(chosen)
                        {
                            doc.apply_term(&term.clone());
                        }
                    });
                    if discount_editor(ui, Id::unique("doc_discounts"), &mut doc.discounts, &mut doc.due_days) {
                        doc.term_name = "Individuell".into();
                    }
                    for line in payment_lines(doc, small, crate::i18n::lang()) {
                        ui.label(muted(line).size(12.5));
                    }
                    ui.separator();
                }
                ui.add_space(6.0);
                ui.label(muted(t("Einleitungstext")));
                ui.add(TextEdit::multiline(&mut doc.intro).desired_rows(2).desired_width(f32::INFINITY));
            });
            if !editable && doc.kind == DocKind::Rechnung {
                ui.add_space(4.0);
                if !doc.term_name.is_empty() {
                    ui.label(muted(tf!("Zahlungsbedingung: {}", tr(&doc.term_name))));
                }
                for line in payment_lines(doc, small, crate::i18n::lang()) {
                    ui.label(muted(line).size(12.5));
                }
            }
        });
        ui.add_space(12.0);

        card(ui, |ui| {
            ui.set_width(width - 34.0);
            card_title(ui, t("Positionen"));
            let text_w = (width - 34.0 - 560.0).max(130.0);
            let mut remove = None;
            let mut move_up = None;
            egui::Grid::new("doc_lines")
                .num_columns(9)
                .spacing([6.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in [
                        "Pos.",
                        "Bezeichnung",
                        "Menge",
                        "Einheit",
                        "Einzelpreis",
                        "Rabatt %",
                        if small { "" } else { t("USt.") },
                        "Gesamt netto",
                        "",
                    ] {
                        ui.label(muted(t(h)).size(12.5));
                    }
                    ui.end_row();
                    let cur = doc.currency().to_string();
                    for (i, l) in doc.lines.iter_mut().enumerate() {
                        let id = Id::unique(("line", i));
                        ui.label(muted(format!("{}", i + 1)));
                        if editable {
                            ui.add(
                                TextEdit::singleline(&mut l.text)
                                    .desired_width(text_w)
                                    .hint_text(t("Bezeichnung")),
                            );
                            qty_edit(ui, id.with("q"), &mut l.qty, 52.0);
                            ui.add(TextEdit::singleline(&mut l.unit).desired_width(62.0));
                            money_edit(ui, id.with("p"), &mut l.price_net, 82.0);
                            qty_edit(ui, id.with("r"), &mut l.discount_pct, 40.0);
                            if small {
                                ui.label("");
                            } else {
                                vat_combo(ui, ("line_vat", i), &mut l.vat);
                            }
                        } else {
                            ui.label(&l.text);
                            ui.label(fmt_qty(l.qty));
                            ui.label(&l.unit);
                            ui.label(fmt_money_cur(crate::i18n::lang(), l.price_net, &cur));
                            ui.label(if l.discount_pct > 0.0 { fmt_qty(l.discount_pct) } else { "–".into() });
                            ui.label(if small { String::new() } else { format!("{} %", l.vat) });
                        }
                        ui.label(RichText::new(fmt_money_cur(crate::i18n::lang(), l.net(), &cur)).strong());
                        if editable {
                            ui.horizontal(|ui| {
                                if i > 0 && crate::icons::icon_only_button(ui, crate::icons::Icon::ArrowUp, t("nach oben")).clicked() {
                                    move_up = Some(i);
                                }
                                if crate::icons::icon_only_button(ui, crate::icons::Icon::Close, t("Position entfernen")).clicked() {
                                    remove = Some(i);
                                }
                            });
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = remove {
                doc.lines.remove(i);
            }
            if let Some(i) = move_up {
                doc.lines.swap(i, i - 1);
            }
            if editable {
                ui.horizontal(|ui| {
                    if ui.button(t("+ Freie Position")).clicked() {
                        doc.lines.push(Line::default());
                    }
                    let mut pick: Option<Article> = None;
                    egui::ComboBox::from_id_salt("add_article")
                        .selected_text(t("+ Artikel einfügen …"))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for a in &self.data.articles {
                                if ui
                                    .selectable_label(false, format!("{} – {}", a.name, fmt_money(a.price_net)))
                                    .clicked()
                                {
                                    pick = Some(a.clone());
                                }
                            }
                        });
                    if let Some(a) = pick {
                        // Replace an empty first line instead of leaving it behind.
                        if doc.lines.len() == 1 && doc.lines[0].text.is_empty() && doc.lines[0].price_net == 0 {
                            doc.lines.clear();
                        }
                        doc.lines.push(Line {
                            article_id: Some(a.id),
                            text: a.name.clone(),
                            qty: 1.0,
                            unit: a.unit.clone(),
                            price_net: a.price_net,
                            vat: a.vat,
                            discount_pct: 0.0,
                        });
                    }
                });
            }
            ui.add_space(8.0);
            let tot = doc.totals(small);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 330.0).max(0.0));
                egui::Grid::new("doc_sums")
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(muted(t("Summe netto")));
                        right_label(ui, 130.0, RichText::new(fmt_money_cur(crate::i18n::lang(), tot.net, doc.currency())));
                        ui.end_row();
                        for (rate, (_n, tax)) in &tot.by_rate {
                            if *rate > 0 {
                                ui.label(muted(tf!("Umsatzsteuer {} %", rate)));
                                right_label(ui, 130.0, RichText::new(fmt_money_cur(crate::i18n::lang(), *tax, doc.currency())));
                                ui.end_row();
                            }
                        }
                        ui.label(RichText::new(t("Gesamtbetrag")).strong().size(15.0));
                        right_label(ui, 130.0, RichText::new(fmt_money_cur(crate::i18n::lang(), tot.gross, doc.currency())).strong().size(15.0));
                        ui.end_row();
                    });
            });
            if small {
                ui.label(muted(t("Kleinunternehmer: Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.")));
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(width - 34.0);
            ui.add_enabled_ui(editable, |ui| {
                ui.label(muted(t("Schlusstext")));
                ui.add(TextEdit::multiline(&mut doc.outro).desired_rows(2).desired_width(f32::INFINITY));
            });
            if let Some(src) = doc
                .source_id
                .and_then(|id| self.data.documents.iter().find(|d| d.id == id))
                && ui
                    .link(tf!("Erstellt aus {} {}", src.kind.label(), src.number))
                    .clicked()
            {
                switch_to = Some(src.id);
            }
            if doc.payments.len() > 1 || (doc.status == DocStatus::Offen && !doc.payments.is_empty()) {
                ui.label(RichText::new(t("Zahlungen")).strong());
                for p in &doc.payments {
                    ui.label(muted(tf!("{}: {} per {}", fmt_date(p.date), fmt_money_cur(crate::i18n::lang(), p.amount.abs(), doc.currency()), t(if p.method == CASH { "Kasse" } else { "Bank" }))));
                }
            }
            if doc.part != InvoicePart::Normal {
                ui.label(muted(match doc.part {
                    InvoicePart::Abschlag => t("Abschlagsrechnung – wird in der Schlussrechnung abgezogen."),
                    _ => t("Schlussrechnung – die Abschläge sind abgezogen."),
                }));
            }
            if doc.status == DocStatus::Bezahlt
                && let Some(p) = doc.paid_date
            {
                ui.label(
                    RichText::new(tf!(
                        "Bezahlt am {} – automatisch als Einnahme gebucht.",
                        fmt_date(p)
                    ))
                    .color(GREEN),
                );
                if doc.paid_discount_pct > 0.0 {
                    ui.label(RichText::new(tf!("Abzüglich {} % Skonto gezahlt.", fmt_qty(doc.paid_discount_pct))).color(GREEN));
                }
            }
            if doc.status == DocStatus::Entwurf {
                ui.label(muted(
                    t("Entwürfe bekommen ihre Nummer beim Fertigstellen. Fertige Belege bleiben unverändert erhalten (GoBD)."),
                ));
            }
            self.einvoice_status(ui, doc);
            self.mail_history(ui, doc);
        });
        switch_to
    }

    /// A small paper view of how the document will look when printed.
    fn document_preview(&self, ui: &mut Ui, doc: &Document, width: f32) {
        let c = &self.data.company;
        let small = c.small_business;
        let tot = doc.totals(small);
        Frame::new()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, BORDER))
            .shadow(egui::Shadow {
                offset: [0, 3],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(22),
            })
            .inner_margin(Margin::same(22))
            .show(ui, |ui| {
                ui.set_width(width - 46.0);
                ui.set_min_height((width - 46.0) * 1.35);
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.label(RichText::new(&c.name).size(14.0).strong().color(ACCENT));
                let (r, _) = ui.allocate_exact_size(vec2(width - 46.0, 2.0), egui::Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::ZERO, ACCENT);
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("{} · {} · {}", c.name, c.street, c.zip_city))
                        .size(8.5)
                        .color(MUTED)
                        .underline(),
                );
                if let Some(k) = self.data.contact(doc.contact_id) {
                    for line in [&k.company, &k.person, &k.street, &k.zip_city] {
                        if !line.is_empty() {
                            ui.label(RichText::new(line).size(11.0));
                        }
                    }
                } else {
                    ui.label(muted(t("Empfänger fehlt")).size(11.0));
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new(if doc.title.is_empty() { doc.kind.label() } else { &doc.title })
                        .size(13.5)
                        .strong(),
                );
                let number = if doc.number.is_empty() { t("ENTWURF").to_string() } else { doc.number.clone() };
                ui.label(
                    RichText::new(tf!("Nr. {} · Datum {}", number, fmt_date(doc.date)))
                        .size(10.0)
                        .color(MUTED),
                );
                ui.label(RichText::new(&doc.intro).size(10.5));
                ui.add_space(6.0);
                let inner = width - 46.0;
                egui::Grid::new("pv_lines")
                    .num_columns(3)
                    .spacing([8.0, 3.0])
                    .min_col_width(10.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(t("Bezeichnung")).size(9.5).strong());
                        ui.label(RichText::new(t("Menge")).size(9.5).strong());
                        ui.label(RichText::new(t("Gesamt")).size(9.5).strong());
                        ui.end_row();
                        for l in &doc.lines {
                            ui.allocate_ui_with_layout(
                                vec2(inner * 0.55, 14.0),
                                Layout::left_to_right(Align::Center),
                                |ui| ui.add(egui::Label::new(RichText::new(&l.text).size(10.0)).truncate()),
                            );
                            ui.label(RichText::new(format!("{} {}", fmt_qty(l.qty), l.unit)).size(10.0));
                            ui.label(RichText::new(fmt_money_cur(crate::i18n::lang(), l.net(), doc.currency())).size(10.0));
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
                ui.with_layout(Layout::top_down(Align::Max), |ui| {
                    ui.label(RichText::new(tf!("Netto {}", fmt_money_cur(crate::i18n::lang(), tot.net, doc.currency()))).size(10.5));
                    if !small {
                        for (rate, (_n, tax)) in &tot.by_rate {
                            if *rate > 0 {
                                ui.label(RichText::new(tf!("USt. {} % {}", rate, fmt_money_cur(crate::i18n::lang(), *tax, doc.currency()))).size(10.5));
                            }
                        }
                    }
                    ui.label(RichText::new(tf!("Gesamt {}", fmt_money_cur(crate::i18n::lang(), tot.gross, doc.currency()))).size(12.0).strong());
                });
                ui.add_space(6.0);
                for line in payment_lines(doc, small, self.data.contact(doc.contact_id).map(|k| k.language).unwrap_or_default()) {
                    ui.label(RichText::new(line).size(10.0));
                }
                ui.label(RichText::new(&doc.outro).size(10.5));
                if small {
                    ui.label(RichText::new(t("Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.")).size(9.0).color(MUTED));
                }
                ui.add_space(16.0);
                ui.separator();
                ui.label(
                    RichText::new(tf!("{} · {} · Steuer-Nr. {}", c.name, c.email, c.tax_number))
                        .size(8.0)
                        .color(MUTED),
                );
                ui.label(
                    RichText::new(format!("{} · IBAN {} · BIC {}", c.bank, c.iban, c.bic))
                        .size(8.0)
                        .color(MUTED),
                );
            });
    }
}
