//! Settings: company data, number ranges, users and data backup.

use super::*;

/// Two-factor sign-in and companies on the server, as shown in the settings.
#[derive(Default)]
pub struct SecurityState {
    pub setup: Option<crate::server::TotpStart>,
    pub code: String,
    pub password: String,
    pub tenants: Option<Vec<crate::server::TenantInfo>>,
    pub new_tenant: String,
}

impl App {
    pub(super) fn settings(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Firma, t("Firma & Belege"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Buchhaltung, t("Buchhaltung"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Benutzer, t("Benutzer & Passwort"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Sicherheit, t("Sicherheit"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Mandanten, t("Mandanten"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Vorlagen, t("Vorlagen & Gestaltung"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Ablage, t("Belegablage"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Daten, t("Verbindung & Datensicherung"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Email, t("E-Mail"));
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Sprache, t("Sprache"));
        });
        ui.add_space(10.0);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.settings_tab {
            SettingsTab::Firma => self.settings_company(ui),
            SettingsTab::Buchhaltung => self.settings_accounting(ui),
            SettingsTab::Benutzer => self.settings_users(ui),
            SettingsTab::Sicherheit => self.settings_security(ui),
            SettingsTab::Mandanten => self.settings_tenants(ui),
            SettingsTab::Ablage => self.settings_storage(ui),
            SettingsTab::Daten => self.settings_data(ui),
            SettingsTab::Sprache => self.settings_language(ui),
            SettingsTab::Email => self.settings_mail(ui),
            SettingsTab::Vorlagen => self.settings_templates(ui),
        });
    }

    fn settings_company(&mut self, ui: &mut Ui) {
        let mut c = self
            .company_edit
            .take()
            .unwrap_or_else(|| self.data.company.clone());
        let mut save = false;
        let mut reset = false;
        let mut msg_remove_default = false;
        let can = self.backend.can_write();
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Firmendaten"));
            ui.label(muted(t("Erscheinen im Kopf und in der Fußzeile Ihrer Angebote und Rechnungen.")));
            ui.add_space(4.0);
            ui.add_enabled_ui(can, |ui| {
                // The fields share the width left next to the labels.
                let field = fit(ui, 290.0, 300.0, 640.0) / 2.0;
                egui::Grid::new("company")
                    .num_columns(4)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        let pairs: [(&str, &mut String, &str, &mut String); 5] = [
                            ("Firmenname", &mut c.name, "Inhaber/in", &mut c.owner),
                            ("Straße", &mut c.street, "PLZ und Ort", &mut c.zip_city),
                            ("E-Mail", &mut c.email, "Telefon", &mut c.phone),
                            ("Steuernummer", &mut c.tax_number, "USt-IdNr.", &mut c.vat_id),
                            ("Bank", &mut c.bank, "BIC", &mut c.bic),
                        ];
                        for (l1, v1, l2, v2) in pairs {
                            ui.label(muted(t(l1)));
                            ui.add_sized([field, 28.0], TextEdit::singleline(v1));
                            ui.label(muted(t(l2)));
                            ui.add_sized([field, 28.0], TextEdit::singleline(v2));
                            ui.end_row();
                        }
                        ui.label(muted(t("IBAN")));
                        ui.add_sized([field, 28.0], TextEdit::singleline(&mut c.iban));
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.checkbox(
                    &mut c.small_business,
                    t("Kleinunternehmer nach § 19 UStG (keine Umsatzsteuer auf Rechnungen)"),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(muted(t("Rechnungsversand (Standard)")));
                    egui::ComboBox::from_id_salt("einvoice_default")
                        .width(300.0)
                        .selected_text(t(c.einvoice_default.name()))
                        .show_ui(ui, |ui| {
                            for f in crate::xrechnung::EInvoiceFormat::ALL {
                                ui.selectable_value(&mut c.einvoice_default, f, t(f.name()));
                            }
                        });
                });
                ui.label(muted(t(
                    "ZUGFeRD: PDF mit eingebetteten Rechnungsdaten – für die meisten Geschäftskunden. XRechnung: reine XML-Datei, vor allem für Behörden (mit Leitweg-ID). Pro Kontakt änderbar.",
                )));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(muted(t("Logo")));
                    match &c.logo {
                        Some(l) => {
                            ui.label(tf!("{} × {} Pixel", l.width, l.height));
                            if ui.button(t("Entfernen")).clicked() {
                                c.logo = None;
                            }
                        }
                        None => {
                            ui.label(muted(t("kein Logo")));
                        }
                    }
                    if ui.button(t("Logo wählen …")).on_hover_text(t("PNG oder JPG; erscheint oben rechts auf Angeboten und Rechnungen.")).clicked() {
                        match super::settings_more::pick_logo() {
                            Some(Ok(logo)) => c.logo = Some(logo),
                            Some(Err(e)) => self.toast = Some((e, true, 0.0)),
                            None => {}
                        }
                    }
                });
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Nummernkreise"));
            ui.add_enabled_ui(can, |ui| number_ranges(ui, &mut c, self.today));
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Zahlungsbedingungen"));
            ui.label(muted(t(
                "Stehen bei Rechnungen und Kunden zur Auswahl. Skonto-Stufen erscheinen mit Datum und Betrag auf der Rechnung.",
            )));
            ui.add_enabled_ui(can, |ui| {
                ui.horizontal(|ui| {
                    ui.label(muted(t("Standard für neue Rechnungen")));
                    let mut default = Some(c.default_term_id);
                    if term_combo(ui, "default_term", &c.payment_terms.clone(), &mut default, None)
                        && let Some(id) = default
                    {
                        c.default_term_id = id;
                    }
                });
                ui.add_space(6.0);
                let mut remove = None;
                for (i, term) in c.payment_terms.iter_mut().enumerate() {
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.add_sized([360.0, 26.0], TextEdit::singleline(&mut term.name).hint_text(t("Bezeichnung")));
                                ui.label(muted(term.summary()));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button(t("Entfernen")).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                            discount_editor(ui, Id::unique(("term", term.id)), &mut term.discounts, &mut term.net_days);
                        });
                    ui.add_space(4.0);
                }
                if let Some(i) = remove {
                    if c.payment_terms[i].id == c.default_term_id {
                        msg_remove_default = true;
                    } else {
                        c.payment_terms.remove(i);
                    }
                }
                if ui.button(t("+ Zahlungsbedingung")).clicked() {
                    let id = c.payment_terms.iter().map(|x| x.id).max().unwrap_or(0) + 1;
                    c.payment_terms.push(PaymentTerm { id, name: t("Neue Zahlungsbedingung").into(), ..PaymentTerm::default() });
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_enabled(ui, can, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Verwerfen")).clicked() {
                    reset = true;
                }
            });
        });
        if msg_remove_default {
            self.error(t("Die Standard-Zahlungsbedingung kann nicht entfernt werden. Wählen Sie zuerst eine andere als Standard."));
        }
        if save {
            for term in &mut c.payment_terms {
                term.discounts.sort_by_key(|s| s.days);
            }
        }
        if save && self.run(Op::SaveCompany(c.clone())).is_ok() {
            self.info(t("Firmendaten gespeichert."));
            reset = true;
        }
        if !reset {
            self.company_edit = Some(c);
        }
    }

    fn settings_users(&mut self, ui: &mut Ui) {
        let Backend::Remote(remote) = &self.backend else {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Benutzer"));
                ui.label(t(
                    "Benutzer und Rechte gibt es, wenn Sie mit einem OpenKontor-Server arbeiten. \
                     Im Einzelplatz-Betrieb arbeitet nur dieser Rechner mit den Daten.",
                ));
            });
            return;
        };
        let is_admin = remote.role == Role::Admin;
        let mut msg: Option<Result<String, String>> = None;
        let mut reload = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Eigenes Passwort ändern"));
            egui::Grid::new("pw").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(muted(t("Bisheriges Passwort")));
                ui.add_sized([280.0, 28.0], TextEdit::singleline(&mut self.pw_old).password(true));
                ui.end_row();
                ui.label(muted(t("Neues Passwort")));
                ui.add_sized([280.0, 28.0], TextEdit::singleline(&mut self.pw_new).password(true));
                ui.end_row();
            });
            if primary(ui, t("Passwort ändern")).clicked() {
                msg = Some(
                    remote
                        .change_password(&self.pw_old, &self.pw_new)
                        .map(|_| t("Passwort geändert.").to_string()),
                );
            }
        });
        if matches!(msg, Some(Ok(_))) {
            self.pw_old.clear();
            self.pw_new.clear();
        }
        ui.add_space(12.0);
        if is_admin {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                ui.horizontal(|ui| {
                    card_title(ui, t("Benutzer auf dem Server"));
                    if ui.button(t("Aktualisieren")).clicked() {
                        reload = true;
                    }
                });
                if self.users.is_empty() && self.users_error.is_none() {
                    reload = true;
                }
                if let Some(e) = &self.users_error {
                    ui.label(RichText::new(e).color(RED));
                }
                let mut delete = None;
                let mut change_role = None;
                egui::Grid::new("users")
                    .num_columns(4)
                    .spacing([24.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for u in &self.users {
                            ui.label(RichText::new(&u.name).strong());
                            let mut role = u.role;
                            egui::ComboBox::from_id_salt(("role", &u.name))
                                .selected_text(role.label())
                                .show_ui(ui, |ui| {
                                    for r in Role::ALL {
                                        ui.selectable_value(&mut role, r, r.label()).on_hover_text(r.hint());
                                    }
                                });
                            if role != u.role {
                                change_role = Some((u.name.clone(), role, u.sso_only));
                            }
                            if u.sso_only {
                                ui.label(muted(t("Microsoft 365")));
                            } else if u.totp {
                                if ui.small_button(t("2-Faktor aus")).on_hover_text(t("Für Kollegen, die ihr Handy verloren haben")).clicked() {
                                    msg = Some(remote.totp_disable(&u.name, "").map(|_| tf!("Zwei-Faktor-Anmeldung für {} ausgeschaltet.", u.name)));
                                    reload = true;
                                }
                            } else {
                                ui.label("");
                            }
                            if u.name == remote.user {
                                ui.label(muted(t("(Sie)")));
                            } else if ui.small_button(t("Entfernen")).clicked() {
                                delete = Some(u.name.clone());
                            }
                            ui.end_row();
                        }
                    });
                if let Some((name, role, sso_only)) = change_role {
                    msg = Some(
                        remote
                            .save_user(&name, role, "", sso_only)
                            .map(|_| tf!("Rolle von {} geändert.", name)),
                    );
                    reload = true;
                }
                if let Some(name) = delete {
                    msg = Some(remote.delete_user(&name).map(|_| tf!("{} entfernt.", name)));
                    reload = true;
                }
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new(t("Neuen Benutzer anlegen oder Passwort zurücksetzen")).strong());
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.new_user.0)
                            .hint_text(t("Benutzername"))
                            .desired_width(150.0),
                    );
                    if !self.new_user_sso {
                        ui.add(
                            TextEdit::singleline(&mut self.new_user.2)
                                .hint_text(t("Passwort (mind. 6 Zeichen)"))
                                .password(true)
                                .desired_width(190.0),
                        );
                    }
                    egui::ComboBox::from_id_salt("new_role")
                        .selected_text(self.new_user.1.label())
                        .show_ui(ui, |ui| {
                            for r in Role::ALL {
                                ui.selectable_value(&mut self.new_user.1, r, r.label()).on_hover_text(r.hint());
                            }
                        });
                    if primary(ui, t("Speichern")).clicked() {
                        let (n, r, p) = self.new_user.clone();
                        let sso = self.new_user_sso && remote.sso_enabled;
                        msg = Some(remote.save_user(&n, r, &p, sso).map(|_| tf!("Benutzer {} gespeichert.", n)));
                        reload = true;
                    }
                });
                if remote.sso_enabled {
                    ui.checkbox(
                        &mut self.new_user_sso,
                        t("Nur mit Microsoft 365 anmelden (ohne Passwort) – als Benutzername die Microsoft-Anmeldeadresse eintragen, z. B. max@firma.de"),
                    );
                }
                ui.label(muted(
                    t("Rollen: Administrator (alles) · Buchhaltung (alles außer Benutzern) · Vertrieb (Kontakte, Artikel, Angebote, Rechnungen, Lager) · Belegerfassung (nur Belege einreichen) · Nur lesen."),
                ));
            });
        }
        if reload && is_admin {
            match remote.users() {
                Ok(u) => {
                    self.users = u;
                    self.users_error = None;
                }
                Err(e) => self.users_error = Some(e),
            }
        }
        if matches!(msg, Some(Ok(_))) {
            self.new_user.0.clear();
            self.new_user.2.clear();
        }
        match msg {
            Some(Ok(m)) => self.info(m),
            Some(Err(e)) => self.error(e),
            None => {}
        }
    }

    fn settings_language(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Sprache der Oberfläche"));
            ui.label(muted(t("Gilt für diesen Arbeitsplatz und wird gespeichert.")));
            ui.add_space(4.0);
            self.language_picker(ui);
            ui.add_space(10.0);
            card_title(ui, t("Sprache der Belege"));
            ui.label(muted(t(
                "Angebote, Rechnungen und Gutschriften werden in der Sprache des Kontakts erstellt. \
                 Sie stellen sie im Kontakt unter „Sprache der Belege“ ein.",
            )));
        });
    }

    fn settings_data(&mut self, ui: &mut Ui) {
        self.import_card(ui);
        ui.add_space(12.0);
        let mut import: Option<Data> = None;
        let mut sign_out = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Verbindung"));
            match &self.backend {
                Backend::Remote(r) => {
                    ui.label(tf!("Verbunden mit {} ({})", r.server_name, r.base));
                    ui.label(muted(tf!("Angemeldet als {} · Rolle: {}", r.user, r.role.label())));
                    ui.label(muted(
                        t("Der Server legt jeden Tag automatisch eine Sicherung im Ordner „Sicherungen“ an."),
                    ));
                }
                Backend::Local => {
                    ui.label(t("Einzelplatz – die Daten liegen auf diesem Rechner:"));
                    ui.label(muted(data_path().display().to_string()));
                }
                Backend::Demo => {
                    ui.label(RichText::new(t("Demo-Modus: Änderungen werden nicht gespeichert.")).color(ORANGE));
                }
            }
            ui.add_space(6.0);
            if ui.button(t("Abmelden / Verbindung wechseln")).clicked() {
                sign_out = true;
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Updates"));
            ui.label(muted(tf!("Installierte Version: {}", env!("CARGO_PKG_VERSION"))));
            if ui.checkbox(&mut self.cfg.check_updates, t("Beim Start nach neuen Versionen suchen (GitHub)")).changed() {
                self.cfg.save();
            }
            ui.horizontal(|ui| {
                if ui.button(t("Jetzt suchen")).clicked() {
                    self.update_rx = Some(export::check_for_update());
                }
                match &self.update {
                    Some((v, url)) => {
                        if ui.link(tf!("Version {} herunterladen", v)).clicked() {
                            export::open_url(url);
                        }
                    }
                    None if self.update_rx.is_none() => {
                        ui.label(muted(t("Keine neuere Version gefunden.")));
                    }
                    None => {
                        ui.spinner();
                    }
                }
            });
        });
        ui.add_space(12.0);
        let admin = self.backend.is_admin();
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Datensicherung"));
            ui.label(muted(
                t("Eine Sicherung enthält alle Kontakte, Artikel, Belege, Buchungen und Firmendaten in einer Datei."),
            ));
            ui.horizontal(|ui| {
                if ui.button(t("Sicherung speichern …")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name(tf!(
                            "OpenKontor-Sicherung-{}.json",
                            self.today.format("%Y-%m-%d")
                        ))
                        .add_filter(t("OpenKontor-Sicherung"), &["json"])
                        .save_file()
                {
                    let result = serde_json::to_string_pretty(&self.data)
                        .map_err(|e| e.to_string())
                        .and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => self.info(t("Sicherung gespeichert.")),
                        Err(e) => self.error(e),
                    }
                }
                if ui
                    .add_enabled(admin, egui::Button::new(t("Sicherung einspielen …")))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter(t("OpenKontor-Sicherung"), &["json"])
                        .pick_file()
                {
                    let result = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|t| serde_json::from_str::<Data>(&t).map_err(|e| e.to_string()));
                    match result {
                        Ok(d) => import = Some(d),
                        Err(e) => self.error(tf!("Die Datei ist keine gültige Sicherung: {}", e)),
                    }
                }
                if ui.add_enabled(admin, egui::Button::new(t("Beispielfirma laden"))).clicked() {
                    import = Some(sample_data());
                }
            });
            if !admin {
                ui.label(muted(t("Einspielen dürfen nur Administratoren.")));
            }
        });
        if let Some(d) = import {
            self.confirm = Some((
                t("Der gesamte aktuelle Datenbestand wird ersetzt. Am besten vorher eine Sicherung speichern. Fortfahren?")
                    .into(),
                Op::ReplaceAll(Box::new(d)),
            ));
        }
        if sign_out {
            self.sign_out();
        }
    }
}

/// The number range editor: one kind at a time, with a preview of every range below.
fn number_ranges(ui: &mut Ui, c: &mut Company, today: NaiveDate) {
    ui.label(muted(t(
        "Wählen Sie, wie die Nummern aufgebaut sind. Eine Nummer, die es schon gibt, wird nie ein zweites Mal vergeben.",
    )));
    ui.add_space(4.0);
    let id = Id::unique("number_kind");
    let mut kind: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);
    ui.horizontal_wrapped(|ui| {
        for (i, (_, label)) in NUMBER_KINDS.iter().enumerate() {
            ui.selectable_value(&mut kind, i, t(label));
        }
    });
    ui.data_mut(|d| d.insert_temp(id, kind));
    let key = NUMBER_KINDS[kind.min(NUMBER_KINDS.len() - 1)].0;
    let old = c.number_format(key);
    let mut f = old.clone();
    ui.add_space(6.0);
    let field = fit(ui, 300.0, 120.0, 240.0);
    egui::Grid::new("number_format")
        .num_columns(4)
        .spacing([14.0, 8.0])
        .show(ui, |ui| {
            ui.label(muted(t("Präfix")));
            ui.add_sized([field, 28.0], TextEdit::singleline(&mut f.prefix));
            ui.label(muted(t("Datum in der Nummer")));
            egui::ComboBox::from_id_salt("number_date")
                .width(field)
                .selected_text(t(f.date.name()))
                .show_ui(ui, |ui| {
                    for d in DatePart::ALL {
                        ui.selectable_value(&mut f.date, d, t(d.name()));
                    }
                });
            ui.end_row();
            ui.label(muted(t("Trennzeichen")));
            ui.add_enabled(
                f.date != DatePart::Keine,
                TextEdit::singleline(&mut f.separator).desired_width(field).char_limit(3),
            )
            .on_hover_text(t("Steht zwischen Datum und laufender Nummer."));
            ui.label(muted(t("Stellen der Nummer")));
            ui.add(egui::DragValue::new(&mut f.digits).range(0..=10))
                .on_hover_text(t("Wird vorne mit Nullen aufgefüllt, z. B. 4 Stellen: 0007. 0 = ohne Auffüllen."));
            ui.end_row();
            ui.label(muted(t("Suffix")));
            ui.add_sized([field, 28.0], TextEdit::singleline(&mut f.suffix));
            ui.label(muted(t("Zählung")));
            egui::ComboBox::from_id_salt("number_restart")
                .width(field)
                .selected_text(t(f.restart.name()))
                .show_ui(ui, |ui| {
                    for r in Restart::ALL {
                        ui.selectable_value(&mut f.restart, r, t(r.name()));
                    }
                });
            ui.end_row();
            ui.label(muted(t("Startwert")));
            ui.add(egui::DragValue::new(&mut f.start).range(1..=99_999_999))
                .on_hover_text(t("Damit beginnt die Zählung nach einem Neubeginn."));
            ui.label(muted(t("nächste Nummer")));
            let mut next = f.next_for(today);
            if ui.add(egui::DragValue::new(&mut next).range(1..=99_999_999)).changed() {
                f.next = next;
                f.period = String::new();
            }
            ui.end_row();
        });
    f.separator = f.separator.chars().filter(|ch| !ch.is_control()).collect();
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(muted(t("So sieht die nächste Nummer aus:")));
        ui.label(RichText::new(f.preview(today)).monospace().strong());
    });
    if let Some(w) = f.warning() {
        ui.colored_label(Color32::from_rgb(190, 110, 20), t(w));
    }
    if f != old {
        c.numbers.insert(key.to_string(), f);
    }
    ui.add_space(8.0);
    ui.label(muted(t("Alle Nummernkreise")));
    egui::Grid::new("number_overview")
        .num_columns(2)
        .spacing([18.0, 4.0])
        .show(ui, |ui| {
            for (key, label) in NUMBER_KINDS {
                ui.label(muted(t(label)));
                ui.label(RichText::new(c.number_format(key).preview(today)).monospace());
                ui.end_row();
            }
        });
}
