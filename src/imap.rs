//! Collecting receipts from a shared mailbox (e.g. belege@firma.de) over IMAP.
//!
//! Unread e-mails are fetched, their PDF/XML/image attachments become receipts, and the
//! e-mails are marked as read. Only what is needed of IMAP is spoken here.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImapSecurity {
    /// Port 993, encrypted from the start (all common providers).
    #[default]
    Tls,
    /// No encryption – only for a mail server in the own network.
    None,
}

impl ImapSecurity {
    pub const ALL: [ImapSecurity; 2] = [ImapSecurity::Tls, ImapSecurity::None];
    pub fn name(self) -> &'static str {
        match self {
            ImapSecurity::Tls => "SSL/TLS (Port 993)",
            ImapSecurity::None => "Unverschlüsselt (nur im eigenen Netz)",
        }
    }
}

/// The shared mailbox; kept on the server (contains the password).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InboxSettings {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub security: ImapSecurity,
    pub username: String,
    pub password: String,
    pub folder: String,
    pub interval_minutes: u32,
    /// Last check and its result, for display.
    pub last_check: String,
    pub last_result: String,
}

impl Default for InboxSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 993,
            security: ImapSecurity::Tls,
            username: String::new(),
            password: String::new(),
            folder: "INBOX".into(),
            interval_minutes: 5,
            last_check: String::new(),
            last_result: String::new(),
        }
    }
}

/// An attachment that should become a receipt.
pub struct Fetched {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub from: String,
    pub subject: String,
}

trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

struct Conn {
    stream: Box<dyn Stream>,
    buf: Vec<u8>,
    tag: u32,
}

/// Largest e-mail fetched (40 MB).
const MAX_MESSAGE: usize = 40 * 1024 * 1024;
/// E-mails handled per check, so one check never takes too long.
const MAX_PER_CHECK: usize = 25;

impl Conn {
    fn open(s: &InboxSettings) -> Result<Conn, String> {
        let host = s.host.trim();
        let port = if s.port == 0 { 993 } else { s.port };
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(|e| format!("Postfach-Server „{host}“ nicht gefunden: {e}"))?
            .next()
            .ok_or_else(|| format!("Postfach-Server „{host}“ nicht gefunden."))?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(20))
            .map_err(|e| format!("Postfach-Server nicht erreichbar: {e}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(60))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(60))).ok();
        let stream: Box<dyn Stream> = match s.security {
            ImapSecurity::None => Box::new(tcp),
            ImapSecurity::Tls => {
                let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_safe_default_protocol_versions()
                    .map_err(|e| e.to_string())?
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
                let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
                Box::new(rustls::StreamOwned::new(conn, tcp))
            }
        };
        let mut c = Conn { stream, buf: Vec::new(), tag: 0 };
        let greeting = c.line()?;
        if !greeting.starts_with(b"* OK") && !greeting.starts_with(b"* PREAUTH") {
            return Err("Der Postfach-Server antwortet nicht wie erwartet.".into());
        }
        Ok(c)
    }

    fn fill(&mut self) -> Result<(), String> {
        let mut chunk = [0u8; 16384];
        let n = self.stream.read(&mut chunk).map_err(|e| format!("Verbindung zum Postfach unterbrochen: {e}"))?;
        if n == 0 {
            return Err("Der Postfach-Server hat die Verbindung beendet.".into());
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    /// One line including the line break.
    fn line(&mut self) -> Result<Vec<u8>, String> {
        loop {
            if let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") {
                return Ok(self.buf.drain(..pos + 2).collect());
            }
            if self.buf.len() > MAX_MESSAGE {
                return Err("Antwort des Postfach-Servers ist zu groß.".into());
            }
            self.fill()?;
        }
    }

    fn exact(&mut self, n: usize) -> Result<Vec<u8>, String> {
        while self.buf.len() < n {
            self.fill()?;
        }
        Ok(self.buf.drain(..n).collect())
    }

    /// Sends a command and returns the untagged lines and literals until the tagged answer.
    fn command(&mut self, cmd: &str) -> Result<(Vec<String>, Vec<Vec<u8>>), String> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        self.stream
            .write_all(format!("{tag} {cmd}\r\n").as_bytes())
            .and_then(|_| self.stream.flush())
            .map_err(|e| format!("Verbindung zum Postfach unterbrochen: {e}"))?;
        let mut lines = Vec::new();
        let mut literals = Vec::new();
        loop {
            let line = self.line()?;
            let text = String::from_utf8_lossy(&line).trim_end().to_string();
            // A literal: "{123}" at the end announces 123 raw bytes.
            if let Some(open) = text.rfind('{')
                && text.ends_with('}')
                && let Ok(n) = text[open + 1..text.len() - 1].parse::<usize>()
            {
                if n > MAX_MESSAGE {
                    return Err("Eine E-Mail im Postfach ist zu groß (über 40 MB).".into());
                }
                literals.push(self.exact(n)?);
                lines.push(text);
                continue;
            }
            if let Some(rest) = text.strip_prefix(&format!("{tag} ")) {
                if rest.starts_with("OK") {
                    return Ok((lines, literals));
                }
                return Err(format!("Postfach-Server meldet: {}", rest.trim_start_matches("NO ").trim_start_matches("BAD ")));
            }
            lines.push(text);
        }
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn wanted(name: &str) -> bool {
    crate::inbox::is_supported(name) && !name.to_lowercase().ends_with(".txt")
}

/// Turns one e-mail into the receipt files it carries.
pub fn attachments(raw: &[u8]) -> Vec<Fetched> {
    let Some(msg) = mail_parser::MessageParser::default().parse(raw) else { return Vec::new() };
    let from = msg
        .from()
        .and_then(|f| f.first().and_then(|a| a.address().map(str::to_string)))
        .unwrap_or_default();
    let subject = msg.subject().unwrap_or("").to_string();
    let mut out = Vec::new();
    for part in msg.attachments() {
        let name = part.attachment_name().unwrap_or("").to_string();
        let is_pdf = part.is_content_type("application", "pdf");
        let is_xml = part.is_content_type("application", "xml") || part.is_content_type("text", "xml");
        let name = if name.is_empty() {
            match (is_pdf, is_xml) {
                (true, _) => "anhang.pdf".to_string(),
                (_, true) => "anhang.xml".to_string(),
                _ => continue,
            }
        } else {
            name
        };
        // Logos and pictures inside the text of the e-mail are not receipts.
        if part.content_id().is_some() && !is_pdf && !is_xml {
            continue;
        }
        if !wanted(&name) {
            continue;
        }
        out.push(Fetched { file_name: name, bytes: part.decoded().into_owned(), from: from.clone(), subject: subject.clone() });
    }
    out
}

/// Fetches unread e-mails, returns their receipt attachments and marks them as read.
pub fn fetch(settings: &InboxSettings) -> Result<(Vec<Fetched>, usize), String> {
    let mut c = Conn::open(settings)?;
    c.command(&format!("LOGIN {} {}", quote(settings.username.trim()), quote(&settings.password)))
        .map_err(|e| format!("Anmeldung am Postfach fehlgeschlagen ({e})."))?;
    let folder = if settings.folder.trim().is_empty() { "INBOX" } else { settings.folder.trim() };
    c.command(&format!("SELECT {}", quote(folder)))?;
    let (lines, _) = c.command("UID SEARCH UNSEEN")?;
    let uids: Vec<u64> = lines
        .iter()
        .filter_map(|l| l.strip_prefix("* SEARCH"))
        .flat_map(|rest| rest.split_whitespace().filter_map(|n| n.parse().ok()).collect::<Vec<u64>>())
        .take(MAX_PER_CHECK)
        .collect();
    let mut found = Vec::new();
    for uid in &uids {
        let (_, literals) = c.command(&format!("UID FETCH {uid} BODY.PEEK[]"))?;
        if let Some(raw) = literals.into_iter().max_by_key(|l| l.len()) {
            found.extend(attachments(&raw));
        }
        c.command(&format!("UID STORE {uid} +FLAGS (\\Seen)"))?;
    }
    let _ = c.command("LOGOUT");
    Ok((found, uids.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_pdf_attachments_and_skips_inline_logos() {
        let raw = concat!(
            "From: Lieferant <rechnung@lieferant.example>\r\n",
            "Subject: Ihre Rechnung\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=XX\r\n\r\n",
            "--XX\r\nContent-Type: text/plain\r\n\r\nAnbei die Rechnung.\r\n",
            "--XX\r\nContent-Type: image/png\r\nContent-ID: <logo>\r\n",
            "Content-Disposition: inline; filename=logo.png\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n",
            "--XX\r\nContent-Type: application/pdf; name=\"RE 4711.pdf\"\r\n",
            "Content-Disposition: attachment; filename=\"RE 4711.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n",
            "--XX--\r\n",
        );
        let found = attachments(raw.as_bytes());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name, "RE 4711.pdf");
        assert_eq!(found[0].bytes, b"%PDF-1.4\n");
        assert_eq!(found[0].from, "rechnung@lieferant.example");
    }

    #[test]
    fn quotes_imap_strings() {
        assert_eq!(quote(r#"pa"ss\word"#), r#""pa\"ss\\word""#);
    }
}
