//! Settings: where the original receipt files are kept (folder, database, S3, FTP, SFTP).

use super::*;
use crate::filestore::{FileKind, FileSettings};

#[derive(Default)]
pub struct StorageState {
    /// Settings being edited, whether a secret is stored, and the default folder.
    pub loaded: Option<(FileSettings, bool, String)>,
    pub error: Option<String>,
    pub copy: bool,
}

impl App {
    pub(super) fn settings_storage(&mut self, ui: &mut Ui) {
        let Backend::Remote(remote) = &self.backend else {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Belegablage"));
                ui.label(t("Hochgeladene Belege werden als Originaldatei unverändert aufbewahrt, mit SHA-256-Fingerabdruck. Im Einzelplatz-Betrieb liegen sie hier:"));
                ui.label(muted(data_path().with_file_name("Belege").display().to_string()));
                ui.label(muted(t("S3, FTP und SFTP stehen mit einem OpenKontor-Server zur Verfügung.")));
            });
            return;
        };
        if remote.role != Role::Admin {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Belegablage"));
                ui.label(t("Die Belegablage richtet ein Administrator ein."));
            });
            return;
        }
        if self.storage.loaded.is_none() && self.storage.error.is_none() {
            match remote.file_settings() {
                Ok(s) => self.storage.loaded = Some(s),
                Err(e) => self.storage.error = Some(tr(&e)),
            }
        }
        if let Some(e) = &self.storage.error {
            ui.label(RichText::new(e).color(RED));
        }
        let Some((s, secret_set, default_dir)) = &mut self.storage.loaded else { return };
        let mut action: Option<bool> = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Belegablage"));
            ui.label(muted(t("Hochgeladene und per E-Mail eingegangene Belege werden als Originaldatei unverändert aufbewahrt; ihr SHA-256-Fingerabdruck beweist das. Wo die Dateien liegen, legen Sie hier fest – je Firma.")));
            ui.add_space(6.0);
            for k in FileKind::ALL {
                ui.radio_value(&mut s.kind, k, t(k.name()));
            }
            ui.add_space(8.0);
            let secret_hint = if *secret_set { t("gespeichert – leer lassen") } else { "" };
            egui::Grid::new("storage_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| match s.kind {
                FileKind::Dateisystem => {
                    ui.label(muted(t("Ordner")));
                    ui.add_sized([420.0, 26.0], TextEdit::singleline(&mut s.folder).hint_text(default_dir.as_str()));
                    ui.end_row();
                }
                FileKind::Datenbank => {
                    ui.label("");
                    ui.label(muted(t("Die Dateien liegen in der PostgreSQL-Datenbank (bzw. neben der Datendatei) und sind in deren Sicherung enthalten.")));
                    ui.end_row();
                }
                FileKind::S3 => {
                    ui.label(muted(t("Adresse")));
                    ui.add_sized([420.0, 26.0], TextEdit::singleline(&mut s.endpoint).hint_text("https://s3.eu-central-1.amazonaws.com"));
                    ui.end_row();
                    ui.label(muted(t("Region")));
                    ui.add_sized([200.0, 26.0], TextEdit::singleline(&mut s.region).hint_text("eu-central-1"));
                    ui.end_row();
                    ui.label(muted(t("Bucket")));
                    ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.bucket));
                    ui.end_row();
                    ui.label(muted(t("Zugangsschlüssel (Access Key)")));
                    ui.add_sized([320.0, 26.0], TextEdit::singleline(&mut s.access_key));
                    ui.end_row();
                    ui.label(muted(t("Geheimer Schlüssel")));
                    ui.add_sized([320.0, 26.0], TextEdit::singleline(&mut s.secret_key).password(true).hint_text(secret_hint));
                    ui.end_row();
                    ui.label(muted(t("Unterordner")));
                    ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.remote_dir).hint_text(t("optional, z. B. belege")));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut s.path_style, t("Adresse im Pfad-Stil (MinIO, Hetzner, Ceph …)"));
                    ui.end_row();
                }
                FileKind::Ftp | FileKind::Sftp => {
                    ui.label(muted(t("Server")));
                    ui.horizontal(|ui| {
                        ui.add_sized([300.0, 26.0], TextEdit::singleline(&mut s.host).hint_text("ablage.meine-firma.de"));
                        ui.label(muted(t("Port")));
                        let mut port = s.port as u32;
                        if ui.add(egui::DragValue::new(&mut port).range(0..=65535)).changed() {
                            s.port = port as u16;
                        }
                        ui.label(muted(if s.kind == FileKind::Ftp { t("(0 = 21)") } else { t("(0 = 22)") }));
                    });
                    ui.end_row();
                    ui.label(muted(t("Benutzer")));
                    ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.username));
                    ui.end_row();
                    if s.kind == FileKind::Ftp {
                        ui.label(muted(t("Passwort")));
                        ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut s.password).password(true).hint_text(secret_hint));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut s.tls, t("Verschlüsselt (FTPS mit TLS) – dringend empfohlen"));
                        ui.end_row();
                    } else {
                        ui.label(muted(t("Schlüsseldatei")));
                        ui.add_sized([420.0, 26.0], TextEdit::singleline(&mut s.key_file).hint_text("/var/lib/openkontor/.ssh/id_ed25519"));
                        ui.end_row();
                    }
                    ui.label(muted(t("Ordner auf dem Server")));
                    ui.add_sized([300.0, 26.0], TextEdit::singleline(&mut s.remote_dir).hint_text("/belege"));
                    ui.end_row();
                }
            });
            match s.kind {
                FileKind::Sftp => {
                    ui.label(muted(t("SFTP nutzt das Programm „sftp“ (OpenSSH) des Servers mit einem Schlüssel ohne Passwort. Den öffentlichen Schlüssel auf dem SFTP-Server bei diesem Benutzer eintragen.")));
                }
                FileKind::Ftp if !s.tls => {
                    ui.label(RichText::new(t("Ohne TLS gehen Passwort und Belege unverschlüsselt durchs Netz – nur im eigenen Netz verwenden.")).color(ORANGE));
                }
                _ => {}
            }
            ui.add_space(8.0);
            ui.checkbox(&mut self.storage.copy, t("Vorhandene Belege in die neue Ablage übertragen"));
            ui.label(muted(t("Belege, die noch am alten Ort liegen, findet OpenKontor trotzdem weiter.")));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("Verbindung testen")).clicked() {
                    action = Some(true);
                }
                if primary(ui, t("Speichern")).clicked() {
                    action = Some(false);
                }
            });
        });
        if let Some(test_only) = action {
            let settings = s.clone();
            let copy = self.storage.copy;
            let Backend::Remote(remote) = &self.backend else { return };
            match remote.save_file_settings(&settings, copy && !test_only, test_only) {
                Ok(m) => {
                    self.info(tr(&m));
                    if !test_only {
                        self.storage = StorageState::default();
                    }
                }
                Err(e) => self.error(tr(&e)),
            }
        }
    }
}
