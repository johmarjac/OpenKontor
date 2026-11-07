//! Sending documents and reminders by e-mail, and the e-mail settings.

use super::*;
use crate::mail::{self, MailSettings, Security, SendRequest};

/// An e-mail being written in the send window.
pub struct MailDraft {
    pub doc_id: u64,
    pub kind: MailKind,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub fee: Cents,
    pub deadline_days: u32,
}

/// Settings tab state: server settings and the template being edited.
#[derive(Default)]
pub struct MailSettingsState {
    pub loaded: Option<(MailSettings, bool)>,
    pub error: Option<String>,
    pub test_to: String,
    pub tpl_kind: Option<MailKind>,
    pub tpl_lang: Lang,
    pub tpl_subject: String,
    pub tpl_body: String,
}

/// Common providers, so nobody has to look up server names.
const PROVIDERS: [(&str, &str, u16, Security); 7] = [
    ("Microsoft 365 / Outlook", "smtp.office365.com", 587, Security::StartTls),
    ("Google Workspace / Gmail", "smtp.gmail.com", 587, Security::StartTls),
    ("IONOS", "smtp.ionos.de", 587, Security::StartTls),
    ("STRATO", "smtp.strato.de", 465, Security::Tls),
    ("GMX", "mail.gmx.net", 587, Security::StartTls),
    ("WEB.DE", "smtp.web.de", 587, Security::StartTls),
    ("Telekom (T-Online)", "securesmtp.t-online.de", 465, Security::Tls),
];

impl App {
    /// Opens the send window for a document, filled from the template in the customer's language.
    pub(super) fn open_mail(&mut self, doc_id: u64, kind: MailKind) {
        let Some(doc) = self.data.documents.iter().find(|d| d.id == doc_id) else { return };
        let to = self.data.contact(doc.contact_id).map(|c| c.email.clone()).unwrap_or_default();
        let mut draft = MailDraft {
            doc_id,
            kind,
            to,
            cc: String::new(),
            subject: String::new(),
            body: String::new(),
            fee: 0,
            deadline_days: self.data.company.reminder_days,
        };
        self.fill_mail(&mut draft);
        self.mail_draft = Some(draft);
    }

    /// Puts the template text (and for reminders the fee) into the draft.
    fn fill_mail(&self, draft: &mut MailDraft) {
        let Some(doc) = self.data.documents.iter().find(|d| d.id == draft.doc_id) else { return };
        let lang = self.data.contact(doc.contact_id).map(|c| c.language).unwrap_or_default();
        if let MailKind::Mahnung(level) = draft.kind {
            draft.fee = self.data.company.reminder_fees.get(level as usize).copied().unwrap_or(0);
        }
        let deadline = self.today + chrono::Duration::days(draft.deadline_days as i64);
        let (subject, body) = mail::template(&self.data.company, draft.kind, lang);
        draft.subject = mail::render(&subject, &self.data, doc, draft.fee, deadline);
        draft.body = mail::render(&body, &self.data, doc, draft.fee, deadline);
    }

    pub(super) fn mail_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.mail_draft.take() else { return };
        let Some(doc) = self.data.documents.iter().find(|d| d.id == draft.doc_id).cloned() else { return };
        let mut keep = true;
        let mut send = false;
        let mut post_only = false;
        let mut preview = false;
        let mut refill = false;
        let modal = egui::Modal::new(Id::unique("mail")).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.horizontal(|ui| {
                icon(ui, Icon::Mail, 20.0, ACCENT);
                ui.label(RichText::new(tf!("{} {} per E-Mail senden", t(draft.kind.name()), doc.number)).size(18.0).strong());
            });
            ui.add_space(6.0);
            egui::Grid::new("mail_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                if let MailKind::Mahnung(level) = draft.kind {
                    ui.label(muted(t("Mahnstufe")));
                    let mut chosen = level;
                    egui::ComboBox::from_id_salt("mail_level")
                        .selected_text(t(reminder_title(level)))
                        .show_ui(ui, |ui| {
                            for l in 0..=3u8 {
                                ui.selectable_value(&mut chosen, l, t(reminder_title(l)));
                            }
                        });
                    if chosen != level {
                        draft.kind = MailKind::Mahnung(chosen);
                        refill = true;
                    }
                    ui.end_row();
                    ui.label(muted(t("Mahngebühr")));
                    ui.horizontal(|ui| {
                        if money_edit(ui, Id::unique("mail_fee"), &mut draft.fee, 90.0) {
                            refill = true;
                        }
                        ui.label("€");
                        ui.label(muted(t("Neue Frist")));
                        if ui.add(egui::DragValue::new(&mut draft.deadline_days).range(1..=60).suffix(t(" Tage"))).changed() {
                            refill = true;
                        }
                    });
                    ui.end_row();
                }
                ui.label(muted(t("An")));
                ui.add(TextEdit::singleline(&mut draft.to).hint_text("name@kunde.de").desired_width(480.0));
                ui.end_row();
                ui.label(muted(t("Kopie (CC)")));
                ui.add(TextEdit::singleline(&mut draft.cc).hint_text(t("optional, mehrere mit Komma")).desired_width(480.0));
                ui.end_row();
                ui.label(muted(t("Betreff")));
                ui.add(TextEdit::singleline(&mut draft.subject).desired_width(480.0));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.add(TextEdit::multiline(&mut draft.body).desired_rows(12).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                icon(ui, Icon::Document, 16.0, MUTED);
                let files = mail::attachment_names(&self.data, &doc, draft.kind);
                ui.label(muted(tf!("Anhang: {}", files.join(", "))));
            });
            if draft.to.trim().is_empty() {
                ui.label(RichText::new(t("Beim Kontakt ist keine E-Mail-Adresse hinterlegt – bitte oben eintragen.")).color(ORANGE));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::Mail, t("Senden")).clicked() {
                    send = true;
                }
                if ui.button(t("PDF-Vorschau")).clicked() {
                    preview = true;
                }
                if matches!(draft.kind, MailKind::Mahnung(_))
                    && ui
                        .button(t("Nur als PDF (per Post)"))
                        .on_hover_text(t("Vermerkt die Mahnung ohne E-Mail und öffnet das PDF zum Drucken."))
                        .clicked()
                {
                    post_only = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if refill {
            self.fill_mail(&mut draft);
        }
        let reminder = |draft: &MailDraft, today: NaiveDate| match draft.kind {
            MailKind::Mahnung(level) => Some(ReminderRecord {
                level,
                date: today,
                fee: draft.fee,
                due: today + chrono::Duration::days(draft.deadline_days as i64),
            }),
            _ => None,
        };
        if preview {
            let bytes = match reminder(&draft, self.today) {
                Some(r) => crate::pdf::reminder_pdf(&self.data, &doc, &r),
                None => crate::pdf::document_pdf(&self.data, &doc),
            };
            let path = std::env::temp_dir().join(format!("OpenKontor Vorschau {}.pdf", doc.number));
            if std::fs::write(&path, bytes).is_ok() {
                export::open_file(&path);
            }
        }
        if post_only && let Some(r) = reminder(&draft, self.today) {
            let bytes = crate::pdf::reminder_pdf(&self.data, &doc, &r);
            let path = std::env::temp_dir().join(crate::pdf::reminder_file_name(&doc, &r));
            if self.run(Op::RecordReminder(doc.id, r)).is_ok() {
                if std::fs::write(&path, bytes).is_ok() {
                    export::open_file(&path);
                }
                self.info(t("Mahnung vermerkt – PDF zum Drucken geöffnet."));
                keep = false;
            }
        }
        if send {
            let req = SendRequest {
                doc_id: draft.doc_id,
                kind: draft.kind,
                to: draft.to.clone(),
                cc: draft.cc.clone(),
                subject: draft.subject.clone(),
                body: draft.body.clone(),
                fee: draft.fee,
                deadline_days: draft.deadline_days,
            };
            match self.backend.send_mail(&mut self.data, &req) {
                Ok(()) => {
                    self.info(tf!("E-Mail an {} gesendet.", draft.to.trim()));
                    keep = false;
                }
                Err(e) => self.error(tr(&e)),
            }
        }
        if !keep {
            // Show the new history entry on the open document.
            if let Some(open) = &self.doc_edit
                && open.id == doc.id
                && let Some(fresh) = self.data.documents.iter().find(|d| d.id == doc.id)
            {
                self.doc_edit = Some(fresh.clone());
            }
        } else {
            self.mail_draft = Some(draft);
        }
    }

    /// Shows how the invoice goes out and what is still missing for the e-invoice.
    pub(super) fn einvoice_status(&self, ui: &mut Ui, doc: &Document) {
        use crate::xrechnung::{EInvoiceFormat, Profile};
        if !matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift) {
            return;
        }
        let format = self.data.einvoice_format(doc.contact_id);
        ui.add_space(6.0);
        ui.label(RichText::new(tf!("Versand: {}", t(format.name()))).strong());
        let profile = match format {
            EInvoiceFormat::Zugferd => Profile::En16931,
            EInvoiceFormat::XRechnung => Profile::XRechnung,
            EInvoiceFormat::NurPdf => {
                ui.label(muted(t("Für Geschäftskunden in Deutschland sind E-Rechnungen ab 2027/2028 Pflicht. Das Format stellen Sie beim Kontakt ein.")));
                return;
            }
        };
        let problems: Vec<String> = crate::xrechnung::check(&self.data, doc, profile)
            .into_iter()
            .filter(|p| !(doc.status == DocStatus::Entwurf && p.contains("fertiggestellt")))
            .collect();
        if problems.is_empty() {
            ui.label(RichText::new(t("✓ Alle Angaben für die E-Rechnung sind vorhanden.")).color(GREEN));
        } else {
            ui.label(RichText::new(t("Für die E-Rechnung fehlt noch:")).color(ORANGE));
            for p in problems {
                ui.label(RichText::new(format!("• {}", tr(&p))).color(ORANGE));
            }
        }
    }

    /// History of e-mails and reminders on a document.
    pub(super) fn mail_history(&self, ui: &mut Ui, doc: &Document) {
        if doc.emails.is_empty() && doc.reminders.is_empty() {
            return;
        }
        ui.add_space(6.0);
        ui.label(RichText::new(t("Versand und Mahnungen")).strong());
        for r in &doc.reminders {
            let fee = if r.fee > 0 { tf!(" · Gebühr {}", fmt_money(r.fee)) } else { String::new() };
            ui.label(muted(tf!("{} vom {} · neue Frist {}{}", t(reminder_title(r.level)), fmt_date(r.date), fmt_date(r.due), fee)));
        }
        for e in &doc.emails {
            ui.label(muted(tf!("{} · E-Mail „{}“ an {} ({})", e.time, e.subject, e.to, e.user)));
        }
    }

    // ---- Settings tab ---------------------------------------------------------------

    pub(super) fn settings_mail(&mut self, ui: &mut Ui) {
        let admin = self.backend.is_admin();
        if self.mail_state.loaded.is_none() && self.mail_state.error.is_none() && admin {
            match self.backend.mail_settings() {
                Ok(s) => self.mail_state.loaded = Some(s),
                Err(e) => self.mail_state.error = Some(tr(&e)),
            }
        }
        let mut save = false;
        let mut test = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Postausgang (SMTP)"));
            ui.label(muted(match &self.backend {
                Backend::Remote(_) => t("Der Server verschickt alle E-Mails. Das Passwort bleibt auf dem Server."),
                _ => t("Dieser Rechner verschickt die E-Mails direkt."),
            }));
            if !admin {
                ui.label(t("Den E-Mail-Versand richtet ein Administrator ein."));
                return;
            }
            if let Some(e) = &self.mail_state.error {
                ui.label(RichText::new(e).color(RED));
            }
            let Some((s, password_set)) = &mut self.mail_state.loaded else { return };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(muted(t("Anbieter-Vorlage")));
                egui::ComboBox::from_id_salt("provider").selected_text(t("Bitte wählen …")).show_ui(ui, |ui| {
                    for (name, host, port, sec) in PROVIDERS {
                        if ui.selectable_label(false, name).clicked() {
                            s.host = host.into();
                            s.port = port;
                            s.security = sec;
                        }
                    }
                });
            });
            egui::Grid::new("smtp").num_columns(4).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(muted(t("Mailserver")));
                ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.host).hint_text("smtp.example.de"));
                ui.label(muted(t("Port")));
                ui.add(egui::DragValue::new(&mut s.port).range(0..=65535));
                ui.end_row();
                ui.label(muted(t("Verschlüsselung")));
                egui::ComboBox::from_id_salt("security").selected_text(t(s.security.name())).show_ui(ui, |ui| {
                    for sec in Security::ALL {
                        ui.selectable_value(&mut s.security, sec, t(sec.name()));
                    }
                });
                ui.end_row();
                ui.label(muted(t("Benutzername")));
                ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.username));
                ui.label(muted(t("Passwort")));
                ui.add_sized(
                    [220.0, 26.0],
                    TextEdit::singleline(&mut s.password)
                        .password(true)
                        .hint_text(if *password_set { t("gespeichert – leer lassen") } else { "" }),
                );
                ui.end_row();
                ui.label(muted(t("Absendername")));
                ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.from_name).hint_text(self.data.company.name.clone()));
                ui.label(muted(t("Absenderadresse")));
                ui.add_sized([220.0, 26.0], TextEdit::singleline(&mut s.from_address).hint_text("rechnung@firma.de"));
                ui.end_row();
                ui.label(muted(t("Antwort an")));
                ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.reply_to).hint_text(t("optional")));
                ui.label(muted(t("Blindkopie (BCC)")));
                ui.add_sized([220.0, 26.0], TextEdit::singleline(&mut s.bcc).hint_text(t("z. B. für das Archiv")));
                ui.end_row();
            });
            ui.label(muted(t("Microsoft 365: Für das Postfach muss „Authentifiziertes SMTP“ erlaubt sein; bei Google ein App-Passwort verwenden.")));
            super::settings_more::microsoft_app_editor(ui, "mail_ms", &mut s.microsoft, *password_set, "SMTP.SendAsApp");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                ui.add_space(16.0);
                ui.add(TextEdit::singleline(&mut self.mail_state.test_to).hint_text(t("Test an …")).desired_width(200.0));
                if ui.button(t("Test-E-Mail senden")).clicked() {
                    test = true;
                }
            });
        });
        if save && let Some((s, _)) = &self.mail_state.loaded {
            let s = s.clone();
            match self.backend.save_mail_settings(&s) {
                Ok(()) => {
                    self.info(t("E-Mail-Einstellungen gespeichert."));
                    self.mail_state.loaded = None;
                }
                Err(e) => self.error(tr(&e)),
            }
        }
        if test {
            let to = self.mail_state.test_to.trim().to_string();
            match self.backend.test_mail(&to) {
                Ok(()) => self.info(tf!("Test-E-Mail an {} gesendet.", to)),
                Err(e) => self.error(tr(&e)),
            }
        }
        ui.add_space(12.0);
        self.settings_mail_templates(ui);
        ui.add_space(12.0);
        self.settings_inbox(ui);
    }

    fn settings_mail_templates(&mut self, ui: &mut Ui) {
        let can = self.backend.can_write();
        let st = &mut self.mail_state;
        let kind = *st.tpl_kind.get_or_insert(MailKind::Rechnung);
        if st.tpl_subject.is_empty() && st.tpl_body.is_empty() {
            let (s, b) = mail::template(&self.data.company, kind, st.tpl_lang);
            st.tpl_subject = s;
            st.tpl_body = b;
        }
        let mut reload = false;
        let mut save = false;
        let mut restore = false;
        let mut fees = self.data.company.reminder_fees.clone();
        fees.resize(4, 0);
        let mut days = self.data.company.reminder_days;
        let mut save_reminders = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("E-Mail-Texte"));
            ui.add_enabled_ui(can, |ui| {
                ui.horizontal(|ui| {
                    let mut k = kind;
                    egui::ComboBox::from_id_salt("tpl_kind").selected_text(t(k.name())).show_ui(ui, |ui| {
                        for x in MailKind::ALL {
                            ui.selectable_value(&mut k, x, t(x.name()));
                        }
                    });
                    let mut l = st.tpl_lang;
                    egui::ComboBox::from_id_salt("tpl_lang").selected_text(l.native_name()).show_ui(ui, |ui| {
                        for x in Lang::ALL {
                            ui.selectable_value(&mut l, x, x.native_name());
                        }
                    });
                    if k != kind || l != st.tpl_lang {
                        st.tpl_kind = Some(k);
                        st.tpl_lang = l;
                        reload = true;
                    }
                });
                ui.add(TextEdit::singleline(&mut st.tpl_subject).desired_width(f32::INFINITY));
                ui.add(TextEdit::multiline(&mut st.tpl_body).desired_rows(9).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if primary(ui, t("Text speichern")).clicked() {
                        save = true;
                    }
                    if ui.button(t("Standardtext wiederherstellen")).clicked() {
                        restore = true;
                    }
                });
                egui::CollapsingHeader::new(t("Platzhalter")).show(ui, |ui| {
                    egui::Grid::new("placeholders").num_columns(2).show(ui, |ui| {
                        for (key, what) in mail::PLACEHOLDERS {
                            ui.label(RichText::new(key).monospace());
                            ui.label(muted(t(what)));
                            ui.end_row();
                        }
                    });
                });
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Mahnwesen"));
            ui.add_enabled_ui(can, |ui| {
                egui::Grid::new("fees").num_columns(4).spacing([14.0, 8.0]).show(ui, |ui| {
                    for level in 0..4u8 {
                        ui.label(muted(t(reminder_title(level))));
                        ui.horizontal(|ui| {
                            money_edit(ui, Id::unique(("fee", level)), &mut fees[level as usize], 80.0);
                            ui.label(muted(t("€ Gebühr")));
                        });
                        if level % 2 == 1 {
                            ui.end_row();
                        }
                    }
                    ui.label(muted(t("Neue Zahlungsfrist")));
                    ui.add(egui::DragValue::new(&mut days).range(1..=60).suffix(t(" Tage")));
                    ui.end_row();
                });
                if primary(ui, t("Speichern")).clicked() {
                    save_reminders = true;
                }
            });
        });

        if reload {
            let st = &mut self.mail_state;
            let (s, b) = mail::template(&self.data.company, st.tpl_kind.unwrap(), st.tpl_lang);
            st.tpl_subject = s;
            st.tpl_body = b;
        }
        if save || restore {
            let st = &self.mail_state;
            let (kind, lang) = (st.tpl_kind.unwrap(), st.tpl_lang);
            let mut c = self.data.company.clone();
            c.email_templates.retain(|x| !(x.kind == kind && x.lang == lang));
            if save {
                c.email_templates.push(MailTemplate { kind, lang, subject: st.tpl_subject.clone(), body: st.tpl_body.clone() });
            }
            if self.run(Op::SaveCompany(c)).is_ok() {
                self.info(t("E-Mail-Text gespeichert."));
                let (s, b) = mail::template(&self.data.company, kind, lang);
                self.mail_state.tpl_subject = s;
                self.mail_state.tpl_body = b;
            }
        }
        if save_reminders {
            let mut c = self.data.company.clone();
            c.reminder_fees = fees;
            c.reminder_days = days;
            if self.run(Op::SaveCompany(c)).is_ok() {
                self.info(t("Einstellungen zum Mahnwesen gespeichert."));
            }
        }
    }
}
