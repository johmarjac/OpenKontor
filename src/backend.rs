//! Where the data lives: on this computer (single workplace) or on a server.

use crate::imap::InboxSettings;
use crate::mail::{MailSettings, SendRequest};
use crate::model::{self, Data, Op};
use crate::server::{
    ErrorReply, LoginReply, LoginRequest, OpReply, PasswordRequest, Role, SaveUserRequest,
    StateReply, UserInfo, url_encode,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::Arc;
use std::time::Duration;

/// Remembered between starts so a workplace reconnects with one click.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    pub use_server: bool,
    pub server_url: String,
    pub user: String,
    /// Interface language; `None` follows the operating system.
    pub language: Option<crate::i18n::Lang>,
    /// Company (Mandant) on the server; empty = the first one.
    pub tenant: String,
    /// Look on GitHub for a newer version at start.
    pub check_updates: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            use_server: false,
            server_url: format!("http://localhost:{}", crate::server::DEFAULT_PORT),
            user: "admin".into(),
            language: None,
            tenant: String::new(),
            check_updates: true,
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OpenKontor")
        .join("arbeitsplatz.json")
}

impl ClientConfig {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let p = config_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, t);
        }
    }
}

pub struct Remote {
    pub base: String,
    pub token: String,
    pub user: String,
    pub role: Role,
    pub server_name: String,
    /// The server offers Microsoft 365 sign-in.
    pub sso_enabled: bool,
    /// Two-factor sign-in is switched on for this user.
    pub totp_enabled: bool,
    pub tenant: String,
    pub tenant_name: String,
    agent: ureq::Agent,
    revision: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    updates: Receiver<Result<StateReply, String>>,
    pub online: bool,
}

pub enum Backend {
    /// Data in a file on this computer.
    Local,
    /// Example data in memory only; nothing is saved.
    Demo,
    Remote(Box<Remote>),
}

fn make_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    }
}

/// Marks the error "password right, code from the authenticator app needed".
pub const NEED_CODE: char = '\u{1}';

/// What the server tells before signing in: its name and companies.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ServerInfo {
    pub name: String,
    pub tenants: Vec<crate::server::TenantInfo>,
    pub version: String,
}

pub fn server_info(url: &str) -> Result<ServerInfo, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .http_status_as_error(false)
        .build();
    read(ureq::Agent::new_with_config(config).get(format!("{}/api/info", normalize_url(url))).call())
}

/// Reads a JSON reply, turning the server's error messages into plain text.
fn read<T: for<'de> Deserialize<'de>>(
    res: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<T, String> {
    let mut res = res.map_err(|e| format!("Server nicht erreichbar ({e})."))?;
    let status = res.status().as_u16();
    let text = res
        .body_mut()
        .with_config()
        .limit(500 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("Antwort unlesbar: {e}"))?;
    if status >= 400 {
        return Err(serde_json::from_str::<ErrorReply>(&text)
            .map(|e| if e.need_code { format!("{NEED_CODE}{}", e.error) } else { e.error })
            .unwrap_or_else(|_| format!("Fehler {status}")));
    }
    serde_json::from_str(&text).map_err(|e| format!("Antwort unlesbar: {e}"))
}

impl Remote {
    /// Logs in and fetches the current data.
    pub fn connect(
        url: &str,
        user: &str,
        password: &str,
        code: &str,
        tenant: &str,
        ctx: eframe::egui::Context,
    ) -> Result<(Remote, Data), String> {
        let base = normalize_url(url);
        let agent = make_agent();
        let login: LoginReply = read(agent.post(format!("{base}/api/login")).send_json(LoginRequest {
            user: user.into(),
            password: password.into(),
            code: code.into(),
            tenant: tenant.into(),
        }))?;
        Self::start(base, agent, login, ctx)
    }

    /// Signs in with the proof from Microsoft 365.
    pub fn connect_sso(
        url: &str,
        proof: &crate::sso_client::SsoResult,
        tenant: &str,
        ctx: eframe::egui::Context,
    ) -> Result<(Remote, Data), String> {
        let base = normalize_url(url);
        let agent = make_agent();
        let login: LoginReply = read(agent.post(format!("{base}/api/login/sso")).send_json(
            crate::server::SsoLoginRequest {
                id_token: proof.id_token.clone(),
                nonce: proof.nonce.clone(),
                tenant: tenant.into(),
            },
        ))?;
        Self::start(base, agent, login, ctx)
    }

    /// Fetches the data after signing in and starts watching for changes.
    fn start(
        base: String,
        agent: ureq::Agent,
        login: LoginReply,
        ctx: eframe::egui::Context,
    ) -> Result<(Remote, Data), String> {
        let state: StateReply = read(
            agent
                .get(format!("{base}/api/state?since=0"))
                .header("Authorization", format!("Bearer {}", login.token))
                .call(),
        )?;
        let data = state.data.unwrap_or_default();
        let revision = Arc::new(AtomicU64::new(state.revision));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();

        // Look for changes made at other workplaces every few seconds.
        {
            let (base, token) = (base.clone(), login.token.clone());
            let (revision, stop) = (revision.clone(), stop.clone());
            std::thread::spawn(move || {
                let agent = make_agent();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(3));
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let since = revision.load(Ordering::Relaxed);
                    let reply: Result<StateReply, String> = read(
                        agent
                            .get(format!("{base}/api/state?since={since}"))
                            .header("Authorization", format!("Bearer {token}"))
                            .call(),
                    );
                    let changed = !matches!(&reply, Ok(r) if r.data.is_none());
                    if tx.send(reply).is_err() {
                        break;
                    }
                    if changed {
                        ctx.request_repaint();
                    }
                }
            });
        }

        Ok((
            Remote {
                base,
                token: login.token,
                user: login.user,
                role: login.role,
                server_name: login.server_name,
                sso_enabled: login.sso_enabled,
                totp_enabled: login.totp_enabled,
                tenant: login.tenant,
                tenant_name: login.tenant_name,
                agent,
                revision,
                stop,
                updates: rx,
                online: true,
            },
            data,
        ))
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn send(&mut self, op: &Op) -> Result<(Option<u64>, Data), String> {
        let reply: OpReply = read(
            self.agent
                .post(format!("{}/api/op", self.base))
                .header("Authorization", self.auth())
                .send_json(op),
        )?;
        self.revision.store(reply.revision, Ordering::Relaxed);
        self.online = true;
        Ok((reply.id, reply.data))
    }

    /// Newest data from the server, if someone else changed something.
    pub fn poll(&mut self) -> Option<Data> {
        let mut newest = None;
        while let Ok(reply) = self.updates.try_recv() {
            match reply {
                Ok(r) => {
                    self.online = true;
                    if let Some(d) = r.data {
                        self.revision.store(r.revision, Ordering::Relaxed);
                        newest = Some(d);
                    }
                }
                Err(_) => self.online = false,
            }
        }
        newest
    }

    pub fn tenants(&self) -> Result<Vec<crate::server::TenantInfo>, String> {
        read(self.agent.get(format!("{}/api/tenants", self.base)).header("Authorization", self.auth()).call())
    }

    pub fn create_tenant(&self, name: &str) -> Result<Vec<crate::server::TenantInfo>, String> {
        read(
            self.agent
                .post(format!("{}/api/tenants", self.base))
                .header("Authorization", self.auth())
                .send_json(serde_json::json!({ "name": name })),
        )
    }

    pub fn totp_start(&self) -> Result<crate::server::TotpStart, String> {
        read(self.agent.post(format!("{}/api/totp/start", self.base)).header("Authorization", self.auth()).send_json(serde_json::json!({})))
    }

    pub fn totp_enable(&self, code: &str) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/totp/enable", self.base))
                .header("Authorization", self.auth())
                .send_json(serde_json::json!({ "code": code })),
        )
        .map(|_| ())
    }

    /// Switches two-factor sign-in off: for oneself with the password, or (administrators) for another user.
    pub fn totp_disable(&self, user: &str, password: &str) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/totp/disable", self.base))
                .header("Authorization", self.auth())
                .send_json(serde_json::json!({ "user": user, "password": password })),
        )
        .map(|_| ())
    }

    /// The DATEV package with the receipt files, made by the server.
    pub fn datev_export(&self, from: chrono::NaiveDate, to: chrono::NaiveDate) -> Result<Vec<u8>, String> {
        let mut res = self
            .agent
            .get(format!("{}/api/export/datev?from={}&to={}", self.base, from, to))
            .header("Authorization", self.auth())
            .call()
            .map_err(|e| format!("Server nicht erreichbar ({e})."))?;
        if res.status().as_u16() >= 400 {
            return Err(format!("Fehler {}", res.status().as_u16()));
        }
        res.body_mut().with_config().limit(2 * 1024 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())
    }

    pub fn users(&self) -> Result<Vec<UserInfo>, String> {
        read(
            self.agent
                .get(format!("{}/api/users", self.base))
                .header("Authorization", self.auth())
                .call(),
        )
    }

    pub fn save_user(&self, name: &str, role: Role, password: &str, sso_only: bool) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/users", self.base))
                .header("Authorization", self.auth())
                .send_json(SaveUserRequest { name: name.into(), role, password: password.into(), sso_only }),
        )
        .map(|_| ())
    }

    pub fn delete_user(&self, name: &str) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .delete(format!("{}/api/users/{}", self.base, url_encode(name)))
                .header("Authorization", self.auth())
                .call(),
        )
        .map(|_| ())
    }

    pub fn change_password(&self, old: &str, new: &str) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/password", self.base))
                .header("Authorization", self.auth())
                .send_json(PasswordRequest { old: old.into(), new: new.into() }),
        )
        .map(|_| ())
    }

    pub fn mail_settings(&self) -> Result<crate::server::MailSettingsReply, String> {
        read(
            self.agent
                .get(format!("{}/api/mail/settings", self.base))
                .header("Authorization", self.auth())
                .call(),
        )
    }

    pub fn save_mail_settings(&self, settings: &MailSettings) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/mail/settings", self.base))
                .header("Authorization", self.auth())
                .send_json(settings),
        )
        .map(|_| ())
    }

    pub fn test_mail(&self, to: &str) -> Result<(), String> {
        read::<serde_json::Value>(
            self.mail_agent()
                .post(format!("{}/api/mail/test", self.base))
                .header("Authorization", self.auth())
                .send_json(crate::mail::TestRequest { to: to.into() }),
        )
        .map(|_| ())
    }

    pub fn send_mail(&mut self, req: &SendRequest) -> Result<Data, String> {
        let reply: OpReply = read(
            self.mail_agent()
                .post(format!("{}/api/mail/send", self.base))
                .header("Authorization", self.auth())
                .send_json(req),
        )?;
        self.revision.store(reply.revision, Ordering::Relaxed);
        Ok(reply.data)
    }

    /// Uploads a receipt file; the server recognises it and returns the new data.
    pub fn upload_receipt(&mut self, name: &str, bytes: &[u8]) -> Result<(Option<u64>, Data), String> {
        let reply: OpReply = read(
            self.mail_agent()
                .post(format!("{}/api/receipts?name={}", self.base, url_encode(name)))
                .header("Authorization", self.auth())
                .header("Content-Type", "application/octet-stream")
                .send(bytes),
        )?;
        self.revision.store(reply.revision, Ordering::Relaxed);
        Ok((reply.id, reply.data))
    }

    pub fn receipt_file(&self, id: u64) -> Result<Vec<u8>, String> {
        let mut res = self
            .mail_agent()
            .get(format!("{}/api/receipts/{id}/file", self.base))
            .header("Authorization", self.auth())
            .call()
            .map_err(|e| format!("Server nicht erreichbar ({e})."))?;
        if res.status().as_u16() >= 400 {
            return Err("Die Belegdatei wurde nicht gefunden.".into());
        }
        res.body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| format!("Antwort unlesbar: {e}"))
    }

    pub fn recognize_receipt(&mut self, id: u64) -> Result<Data, String> {
        let reply: OpReply = read(
            self.mail_agent()
                .post(format!("{}/api/receipts/{id}/recognize", self.base))
                .header("Authorization", self.auth())
                .send_empty(),
        )?;
        self.revision.store(reply.revision, Ordering::Relaxed);
        Ok(reply.data)
    }

    pub fn inbox_settings(&self) -> Result<(InboxSettings, bool), String> {
        #[derive(Deserialize)]
        struct Reply {
            settings: InboxSettings,
            password_set: bool,
        }
        let r: Reply = read(
            self.agent
                .get(format!("{}/api/inbox/settings", self.base))
                .header("Authorization", self.auth())
                .call(),
        )?;
        Ok((r.settings, r.password_set))
    }

    pub fn save_inbox_settings(&self, settings: &InboxSettings) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/inbox/settings", self.base))
                .header("Authorization", self.auth())
                .send_json(settings),
        )
        .map(|_| ())
    }

    pub fn check_inbox(&self) -> Result<(), String> {
        read::<serde_json::Value>(
            self.agent
                .post(format!("{}/api/inbox/check", self.base))
                .header("Authorization", self.auth())
                .send_empty(),
        )
        .map(|_| ())
    }

    /// Sending goes through the mail server, which may take a while.
    fn mail_agent(&self) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build();
        ureq::Agent::new_with_config(config)
    }
}

/// Mail settings of a single workplace, kept in a file next to the data.
fn local_mail_path() -> PathBuf {
    model::data_path().with_file_name("mail.json")
}

fn load_local_mail() -> MailSettings {
    std::fs::read_to_string(local_mail_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_local_mail(settings: &MailSettings) -> Result<(), String> {
    let path = local_mail_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

const DEMO_NO_MAIL: &str = "Im Demo-Modus werden keine E-Mails versendet.";
const NEEDS_SERVER: &str = "Das Sammelpostfach gibt es nur mit einem OpenKontor-Server.";

/// Receipt files of a single workplace: a folder next to the data file.
fn local_receipt_path(id: u64) -> PathBuf {
    model::data_path().with_file_name("Belege").join(format!("{id}.bin"))
}

/// Receipt files in the demo live only in memory.
static DEMO_FILES: std::sync::Mutex<Vec<(u64, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

impl Drop for Remote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Backend {
    /// The mail server settings without the password, and whether a password is stored.
    pub fn mail_settings(&self) -> Result<(MailSettings, bool), String> {
        match self {
            Backend::Remote(r) => r.mail_settings().map(|m| (m.settings, m.password_set)),
            Backend::Local => {
                let mut s = load_local_mail();
                let set = !s.password.is_empty();
                s.password.clear();
                Ok((s, set))
            }
            Backend::Demo => Ok((MailSettings::default(), false)),
        }
    }

    /// Saves the mail server settings; an empty password keeps the stored one.
    pub fn save_mail_settings(&mut self, settings: &MailSettings) -> Result<(), String> {
        match self {
            Backend::Remote(r) => r.save_mail_settings(settings),
            Backend::Local => {
                let mut s = settings.clone();
                if s.password.is_empty() {
                    s.password = load_local_mail().password;
                }
                save_local_mail(&s)
            }
            Backend::Demo => Err(DEMO_NO_MAIL.into()),
        }
    }

    pub fn test_mail(&self, to: &str) -> Result<(), String> {
        match self {
            Backend::Remote(r) => r.test_mail(to),
            Backend::Local => crate::mail::send(
                &load_local_mail(),
                to,
                "",
                "OpenKontor: Test-E-Mail",
                "Diese Test-E-Mail wurde von OpenKontor verschickt. Der E-Mail-Versand funktioniert.",
                vec![],
            ),
            Backend::Demo => Err(DEMO_NO_MAIL.into()),
        }
    }

    /// Sends a document by e-mail and records it; `data` is updated on success.
    /// Adds a receipt file to the inbox and recognises it. Returns the receipt's id.
    pub fn add_receipt(&mut self, data: &mut Data, name: &str, bytes: &[u8]) -> Result<u64, String> {
        if let Backend::Remote(r) = self {
            let (id, new) = r.upload_receipt(name, bytes)?;
            *data = new;
            return id.ok_or_else(|| "Beleg konnte nicht angelegt werden.".into());
        }
        crate::inbox::check_new_file(data, name, bytes)?;
        let user = whoami();
        let receipt = crate::inbox::new_receipt(data, name, bytes, &format!("Hochgeladen von {user}"));
        let mut copy = data.clone();
        let id = copy.apply(Op::AddReceipt(Box::new(receipt)), &user)?.unwrap_or(0);
        match self {
            Backend::Local => {
                let path = local_receipt_path(id);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                model::save(&copy)?;
            }
            _ => DEMO_FILES.lock().unwrap().push((id, bytes.to_vec())),
        }
        *data = copy;
        Ok(id)
    }

    /// The original file of a receipt.
    pub fn receipt_file(&self, id: u64) -> Result<Vec<u8>, String> {
        match self {
            Backend::Remote(r) => r.receipt_file(id),
            Backend::Local => std::fs::read(local_receipt_path(id)).map_err(|_| "Die Belegdatei wurde nicht gefunden.".into()),
            Backend::Demo => DEMO_FILES
                .lock()
                .unwrap()
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| "Zu diesem Beispielbeleg gibt es keine Datei.".into()),
        }
    }

    /// Runs the recognition again on the stored file.
    pub fn recognize_receipt(&mut self, data: &mut Data, id: u64) -> Result<(), String> {
        if let Backend::Remote(r) = self {
            *data = r.recognize_receipt(id)?;
            return Ok(());
        }
        let bytes = self.receipt_file(id)?;
        let mut receipt = data.receipts.iter().find(|r| r.id == id).cloned().ok_or("Beleg nicht gefunden.")?;
        receipt.category.clear();
        crate::inbox::recognize(data, &mut receipt, &bytes);
        self.run(data, Op::SaveReceipt(Box::new(receipt))).map(|_| ())
    }

    pub fn inbox_settings(&self) -> Result<(InboxSettings, bool), String> {
        match self {
            Backend::Remote(r) => r.inbox_settings(),
            _ => Err(NEEDS_SERVER.into()),
        }
    }

    pub fn save_inbox_settings(&self, settings: &InboxSettings) -> Result<(), String> {
        match self {
            Backend::Remote(r) => r.save_inbox_settings(settings),
            _ => Err(NEEDS_SERVER.into()),
        }
    }

    pub fn check_inbox(&self) -> Result<(), String> {
        match self {
            Backend::Remote(r) => r.check_inbox(),
            _ => Err(NEEDS_SERVER.into()),
        }
    }

    pub fn send_mail(&mut self, data: &mut Data, req: &SendRequest) -> Result<(), String> {
        match self {
            Backend::Remote(r) => {
                *data = r.send_mail(req)?;
                Ok(())
            }
            Backend::Local => {
                let mut copy = data.clone();
                crate::mail::send_document(&mut copy, &load_local_mail(), req, &whoami())?;
                model::save(&copy)?;
                *data = copy;
                Ok(())
            }
            Backend::Demo => Err(DEMO_NO_MAIL.into()),
        }
    }

    /// Applies a change wherever the data lives. On success `data` is the new state.
    pub fn run(&mut self, data: &mut Data, op: Op) -> Result<Option<u64>, String> {
        match self {
            Backend::Local => {
                let deleted = if let Op::DeleteReceipt(rid) = op { Some(rid) } else { None };
                let mut copy = data.clone();
                let id = copy.apply(op, &whoami())?;
                model::save(&copy)?;
                *data = copy;
                if let Some(rid) = deleted {
                    let _ = std::fs::remove_file(local_receipt_path(rid));
                }
                Ok(id)
            }
            Backend::Demo => {
                let deleted = if let Op::DeleteReceipt(rid) = op { Some(rid) } else { None };
                let mut copy = data.clone();
                let id = copy.apply(op, "Demo")?;
                *data = copy;
                if let Some(rid) = deleted {
                    DEMO_FILES.lock().unwrap().retain(|(i, _)| *i != rid);
                }
                Ok(id)
            }
            Backend::Remote(r) => {
                let (id, new) = r.send(&op)?;
                *data = new;
                Ok(id)
            }
        }
    }

    pub fn can_write(&self) -> bool {
        match self {
            Backend::Remote(r) => r.role.can_write(),
            _ => true,
        }
    }

    pub fn is_admin(&self) -> bool {
        match self {
            Backend::Remote(r) => r.role == Role::Admin,
            _ => true,
        }
    }

    /// Whether the signed-in user may change things of this kind.
    pub fn allows(&self, area: model::OpArea) -> bool {
        match self {
            Backend::Remote(r) => r.role.allows(area),
            _ => true,
        }
    }

    /// The DATEV package (booking batch and receipt files) for a period.
    pub fn datev_package(&self, data: &Data, from: chrono::NaiveDate, to: chrono::NaiveDate) -> Result<Vec<u8>, String> {
        match self {
            Backend::Remote(r) => r.datev_export(from, to),
            _ => Ok(crate::exports::datev_package(data, from, to, &mut |id| self.receipt_file(id).ok())),
        }
    }
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "lokal".into())
}
