//! Settings for bookkeeping, sign-in security and companies (Mandanten).

use super::*;

/// Lets the user choose a logo file and turns it into plain RGB on white.
pub(super) fn pick_logo() -> Option<Result<Logo, String>> {
    let path = rfd::FileDialog::new().add_filter(t("Bild"), &["png", "jpg", "jpeg"]).pick_file()?;
    Some((|| {
        let img = image::open(&path).map_err(|e| tf!("Das Bild ist nicht lesbar: {}", e))?;
        // Enough for print at the size of the letterhead, small in the data.
        let img = img.thumbnail(900, 300).to_rgba8();
        let (w, h) = img.dimensions();
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for p in img.pixels() {
            let a = p[3] as u32;
            for c in 0..3 {
                rgb.push(((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        Ok(crate::pdf::make_logo(w, h, &rgb))
    })())
}

/// Draws a QR code (for the authenticator app).
fn qr_code(ui: &mut Ui, text: &str, size: f32) {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else {
        return;
    };
    let n = code.width();
    let quiet = 2;
    let cells = n + 2 * quiet;
    let cell = (size / cells as f32).floor().max(1.0);
    let side = cell * cells as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, Color32::WHITE);
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % n + quiet) as f32, (i / n + quiet) as f32);
            p.rect_filled(
                egui::Rect::from_min_size(rect.min + vec2(x * cell, y * cell), vec2(cell, cell)),
                CornerRadius::ZERO,
                Color32::BLACK,
            );
        }
    }
}

/// Signing in to a Microsoft 365 mailbox as an app (instead of a password).
pub(super) fn microsoft_app_editor(ui: &mut Ui, salt: &str, app: &mut crate::oauth_mail::OAuthApp, secret_set: bool, permission: &str) {
    let flag = Id::unique(("ms_on", salt));
    let mut on = !app.tenant.is_empty() || !app.client_id.is_empty() || ui.data(|d| d.get_temp::<bool>(flag)).unwrap_or(false);
    ui.push_id(salt, |ui| {
        if ui.checkbox(&mut on, t("Microsoft 365 ohne Passwort (Anmeldung als App)")).changed() {
            ui.data_mut(|d| d.insert_temp(flag, on));
            if !on {
                *app = Default::default();
            }
        }
        if !on {
            return;
        }
        egui::Grid::new("ms_app").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            ui.label(muted(t("Verzeichnis-ID (Mandant)")));
            ui.add_sized([320.0, 26.0], TextEdit::singleline(&mut app.tenant).hint_text("11111111-2222-3333-4444-555555555555"));
            ui.end_row();
            ui.label(muted(t("Anwendungs-ID")));
            ui.add_sized([320.0, 26.0], TextEdit::singleline(&mut app.client_id));
            ui.end_row();
            ui.label(muted(t("Geheimer Schlüssel")));
            ui.add_sized(
                [320.0, 26.0],
                TextEdit::singleline(&mut app.client_secret).password(true).hint_text(if secret_set { t("gespeichert – leer lassen") } else { "" }),
            );
            ui.end_row();
        });
        ui.label(muted(tf!(
            "In Microsoft Entra eine App registrieren, einen geheimen Schlüssel anlegen und die Berechtigung „{}“ (Office 365 Exchange Online) erteilen; das Postfach mit Add-MailboxPermission bzw. New-ServicePrincipal freigeben. Benutzername = Adresse des Postfachs.",
            permission
        )));
    });
}

impl App {
    pub(super) fn settings_accounting(&mut self, ui: &mut Ui) {
        let mut c = self.company_edit.take().unwrap_or_else(|| self.data.company.clone());
        let can = self.backend.can_write() && self.backend.allows(OpArea::Settings);
        let (mut save, mut reset) = (false, false);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Gewinnermittlung und Kontenrahmen"));
            ui.add_enabled_ui(can, |ui| {
                ui.radio_value(&mut c.accounting, Accounting::Euer, t("Einnahmen-Überschuss-Rechnung (Freiberufler, Einzelunternehmen, kleine Firmen)"));
                ui.radio_value(&mut c.accounting, Accounting::Bilanz, t("Doppelte Buchführung mit Bilanz (z. B. GmbH, UG, eingetragene Kaufleute)"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(muted(t("Kontenrahmen")));
                    ui.radio_value(&mut c.chart, Chart::Skr03, "SKR 03");
                    ui.radio_value(&mut c.chart, Chart::Skr04, "SKR 04");
                });
                ui.label(muted(t("Gilt für den DATEV-Export, GuV und Bilanz. Bitte mit Ihrer Steuerberatung abstimmen.")));
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("DATEV"));
            ui.add_enabled_ui(can, |ui| {
                egui::Grid::new("datev").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    ui.label(muted(t("Beraternummer")));
                    ui.add_sized([160.0, 28.0], TextEdit::singleline(&mut c.datev_advisor));
                    ui.end_row();
                    ui.label(muted(t("Mandantennummer")));
                    ui.add_sized([160.0, 28.0], TextEdit::singleline(&mut c.datev_client));
                    ui.end_row();
                });
                ui.label(muted(t("Beide Nummern bekommen Sie von Ihrer Steuerberatung.")));
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Kasse"));
            ui.add_enabled_ui(can && self.data.cash_closings.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(muted(t("Anfangsbestand")));
                    money_edit(ui, Id::unique("cash_opening"), &mut c.cash_opening, 110.0);
                    ui.label("€");
                });
            });
            ui.label(muted(t("Bargeld in der Kasse vor der ersten Kassenbuchung. Nach dem ersten Kassenabschluss nicht mehr änderbar.")));
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
        if save && self.run(Op::SaveCompany(c.clone())).is_ok() {
            self.info(t("Gespeichert."));
            reset = true;
        }
        if !reset {
            self.company_edit = Some(c);
        }
    }

    pub(super) fn settings_security(&mut self, ui: &mut Ui) {
        let Backend::Remote(remote) = &self.backend else {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Zwei-Faktor-Anmeldung"));
                ui.label(t("Die Anmeldung mit Bestätigungscode gibt es, wenn Sie mit einem OpenKontor-Server arbeiten."));
            });
            return;
        };
        let mut msg: Option<Result<String, String>> = None;
        let mut enabled_now = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            ui.horizontal(|ui| {
                icon(ui, Icon::Lock, 18.0, ACCENT);
                card_title(ui, t("Zwei-Faktor-Anmeldung"));
            });
            ui.label(muted(t("Zusätzlich zum Passwort fragt der Server nach einem Code aus einer Authenticator-App auf dem Handy (z. B. Microsoft Authenticator, Google Authenticator, FreeOTP). So hilft ein gestohlenes Passwort allein nicht.")));
            ui.add_space(6.0);
            if remote.totp_enabled {
                ui.label(RichText::new(t("Für Ihr Konto eingeschaltet.")).color(GREEN).strong());
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut self.security.password).password(true).hint_text(t("Passwort zur Bestätigung")).desired_width(220.0));
                    if ui.button(t("Ausschalten")).clicked() {
                        msg = Some(remote.totp_disable("", &self.security.password).map(|_| t("Zwei-Faktor-Anmeldung ausgeschaltet.").to_string()));
                        if matches!(msg, Some(Ok(_))) {
                            enabled_now = Some(false);
                        }
                    }
                });
            } else if let Some(setup) = &self.security.setup {
                ui.horizontal_top(|ui| {
                    qr_code(ui, &setup.uri, 200.0);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(t("1. Diesen QR-Code mit der Authenticator-App scannen."));
                        ui.label(muted(tf!("Oder den Schlüssel von Hand eingeben: {}", setup.secret)));
                        ui.add_space(8.0);
                        ui.label(t("2. Den angezeigten Code hier eingeben:"));
                        ui.horizontal(|ui| {
                            ui.add(TextEdit::singleline(&mut self.security.code).hint_text("123456").desired_width(100.0));
                            if primary(ui, t("Einschalten")).clicked() {
                                msg = Some(remote.totp_enable(&self.security.code).map(|_| t("Zwei-Faktor-Anmeldung eingeschaltet.").to_string()));
                                if matches!(msg, Some(Ok(_))) {
                                    enabled_now = Some(true);
                                }
                            }
                        });
                    });
                });
            } else if primary(ui, t("Einrichten …")).clicked() {
                match remote.totp_start() {
                    Ok(s) => self.security.setup = Some(s),
                    Err(e) => msg = Some(Err(e)),
                }
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Verbindung zum Server"));
            if remote.base.starts_with("https://") {
                ui.label(RichText::new(t("Die Verbindung ist verschlüsselt (HTTPS).")).color(GREEN));
            } else {
                ui.label(RichText::new(t("Die Verbindung ist nicht verschlüsselt. Im eigenen Netz ist das üblich; über das Internet bitte HTTPS verwenden (Server mit --tls-cert und --tls-key starten).")).color(ORANGE));
            }
        });
        if let (Some(on), Backend::Remote(r)) = (enabled_now, &mut self.backend) {
            r.totp_enabled = on;
            self.security = super::settings::SecurityState::default();
        }
        match msg {
            Some(Ok(m)) => self.info(m),
            Some(Err(e)) => self.error(tr(&e)),
            None => {}
        }
    }

    pub(super) fn settings_tenants(&mut self, ui: &mut Ui) {
        let Backend::Remote(remote) = &self.backend else {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Mandanten"));
                ui.label(t("Mehrere Firmen auf einem Server gibt es mit einem OpenKontor-Server. Im Einzelplatz-Betrieb arbeitet man mit einer Firma je Datenbestand."));
            });
            return;
        };
        let mut msg: Option<Result<String, String>> = None;
        if self.security.tenants.is_none() {
            self.security.tenants = remote.tenants().ok();
        }
        let current = remote.tenant.clone();
        let is_admin = remote.role == Role::Admin;
        let mut switch = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Firmen auf diesem Server"));
            ui.label(muted(t("Jede Firma hat eigene Daten, Benutzer, Nummernkreise und E-Mail-Einstellungen. Bei der Anmeldung wählt man die Firma.")));
            ui.add_space(6.0);
            for x in self.security.tenants.clone().unwrap_or_default() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&x.name).strong());
                    if x.id == current {
                        ui.label(muted(t("(angemeldet)")));
                    } else if ui.small_button(t("Wechseln")).clicked() {
                        switch = Some(x.id.clone());
                    }
                });
            }
            if is_admin {
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new(t("Neue Firma anlegen")).strong());
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut self.security.new_tenant).hint_text(t("Firmenname")).desired_width(260.0));
                    if primary(ui, t("Anlegen")).clicked() {
                        let name = self.security.new_tenant.trim().to_string();
                        msg = Some(remote.create_tenant(&name).map(|list| {
                            self.security.tenants = Some(list);
                            tf!("Firma „{}“ angelegt. Sie sind dort Administrator mit Ihrem jetzigen Passwort.", name)
                        }));
                    }
                });
            }
        });
        if matches!(msg, Some(Ok(_))) {
            self.security.new_tenant.clear();
        }
        match msg {
            Some(Ok(m)) => self.info(m),
            Some(Err(e)) => self.error(tr(&e)),
            None => {}
        }
        if let Some(id) = switch {
            self.cfg.tenant = id;
            self.cfg.save();
            self.sign_out();
            self.info(t("Bitte für die andere Firma anmelden."));
        }
    }
}
