//! Where the original receipt files are kept. The storage is exchangeable: a folder on
//! the server (default), the database, S3-compatible object storage (AWS, MinIO, Hetzner,
//! Wasabi, IONOS …), an FTP/FTPS server or an SFTP server. Files are never changed; each
//! receipt keeps its SHA-256 fingerprint, so a stored file can be checked at any time.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    /// A folder on the server.
    #[default]
    Dateisystem,
    /// Inside the PostgreSQL database (or next to the JSON file without database).
    Datenbank,
    S3,
    Ftp,
    Sftp,
}

impl FileKind {
    pub const ALL: [FileKind; 5] = [FileKind::Dateisystem, FileKind::Datenbank, FileKind::S3, FileKind::Ftp, FileKind::Sftp];

    pub fn name(self) -> &'static str {
        match self {
            FileKind::Dateisystem => "Ordner auf dem Server",
            FileKind::Datenbank => "In der Datenbank",
            FileKind::S3 => "S3-Speicher (AWS, MinIO, Hetzner …)",
            FileKind::Ftp => "FTP / FTPS",
            FileKind::Sftp => "SFTP",
        }
    }
}

/// Settings of the receipt storage; secrets stay on the server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSettings {
    pub kind: FileKind,
    /// Folder; empty = "Belege" next to the server data.
    pub folder: String,
    /// S3: address of the service, e.g. https://s3.eu-central-1.amazonaws.com.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    /// S3: address as endpoint/bucket/key (MinIO and most others) instead of bucket.endpoint/key.
    pub path_style: bool,
    /// FTP/SFTP server.
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// FTP: encrypt with TLS (explicit FTPS).
    pub tls: bool,
    /// SFTP: private key file on the server (sign-in with key, as SFTP batch mode needs).
    pub key_file: String,
    /// Folder on the FTP/SFTP server or prefix in the bucket.
    pub remote_dir: String,
}

impl FileSettings {
    /// Without the secrets, for showing to a workplace.
    pub fn without_secrets(&self) -> FileSettings {
        FileSettings { secret_key: String::new(), password: String::new(), ..self.clone() }
    }

    /// Keeps stored secrets where the new settings leave them empty.
    pub fn keep_secrets(&mut self, old: &FileSettings) {
        if self.secret_key.is_empty() {
            self.secret_key = old.secret_key.clone();
        }
        if self.password.is_empty() {
            self.password = old.password.clone();
        }
    }

    pub fn has_secret(&self) -> bool {
        !self.secret_key.is_empty() || !self.password.is_empty()
    }
}

/// A place for files, addressed by a key like "Belege/2026/123-rechnung.pdf".
pub trait FileBackend: Send {
    fn describe(&self) -> String;
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), String>;
    fn get(&mut self, key: &str) -> Result<Vec<u8>, String>;
    fn delete(&mut self, key: &str) -> Result<(), String>;
}

/// The key of a receipt's file: readable, sorted by year, unique by id.
pub fn key_for(id: u64, file_name: &str, received: &str) -> String {
    let year = received.split(['.', ' ']).nth(2).filter(|y| y.len() == 4).unwrap_or("ohne-jahr");
    let safe: String = file_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    format!("{year}/{id}-{safe}")
}

/// The storage for the settings; `None` means: in the database (the server's own store).
pub fn open(s: &FileSettings, default_dir: &std::path::Path) -> Result<Option<Box<dyn FileBackend>>, String> {
    Ok(match s.kind {
        FileKind::Datenbank => None,
        FileKind::Dateisystem => {
            let root = if s.folder.trim().is_empty() { default_dir.join("Belege") } else { PathBuf::from(s.folder.trim()) };
            Some(Box::new(FsBackend { root }))
        }
        FileKind::S3 => {
            if s.endpoint.trim().is_empty() || s.bucket.trim().is_empty() || s.access_key.trim().is_empty() {
                return Err("S3: Adresse, Bucket und Zugangsschlüssel angeben.".into());
            }
            Some(Box::new(S3Backend { s: s.clone() }))
        }
        FileKind::Ftp => {
            if s.host.trim().is_empty() {
                return Err("FTP: Servername angeben.".into());
            }
            Some(Box::new(FtpBackend { s: s.clone() }))
        }
        FileKind::Sftp => {
            if s.host.trim().is_empty() || s.key_file.trim().is_empty() {
                return Err("SFTP: Servername und Schlüsseldatei angeben.".into());
            }
            Some(Box::new(SftpBackend { s: s.clone() }))
        }
    })
}

/// Stores a test file, reads it back and removes it: proves the storage works.
pub fn check(b: &mut dyn FileBackend) -> Result<(), String> {
    let key = format!("openkontor-test-{}.txt", std::process::id());
    let content = b"OpenKontor Speichertest";
    b.put(&key, content)?;
    let back = b.get(&key)?;
    let _ = b.delete(&key);
    if back != content {
        return Err("Die Testdatei kam verändert zurück.".into());
    }
    Ok(())
}

fn join(dir: &str, key: &str) -> String {
    let dir = dir.trim().trim_matches('/');
    if dir.is_empty() { key.to_string() } else { format!("{dir}/{key}") }
}

// ---- Folder ----------------------------------------------------------------------

pub struct FsBackend {
    pub root: PathBuf,
}

impl FileBackend for FsBackend {
    fn describe(&self) -> String {
        format!("Ordner {}", self.root.display())
    }

    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.root.join(key);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Ordner {}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("tmp-upload");
        std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| format!("Datei {}: {e}", path.display()))
    }

    fn get(&mut self, key: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.root.join(key)).map_err(|_| "Die Belegdatei wurde nicht gefunden.".to_string())
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        match std::fs::remove_file(self.root.join(key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

// ---- S3 (Signature Version 4) ---------------------------------------------------------

pub struct S3Backend {
    pub s: FileSettings,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key);
    ring::hmac::sign(&k, data.as_bytes()).as_ref().to_vec()
}

/// Percent-encoding as S3 expects it in paths (slashes stay).
fn s3_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The Authorization header of AWS Signature Version 4.
/// `headers` are (lower-case name, value), including host and x-amz-*.
pub fn sigv4(
    method: &str,
    path: &str,
    headers: &[(String, String)],
    payload_hash: &str,
    amz_date: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
) -> String {
    let mut sorted: Vec<&(String, String)> = headers.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let canonical_headers: String = sorted.iter().map(|(k, v)| format!("{k}:{}\n", v.trim())).collect();
    let signed: Vec<&str> = sorted.iter().map(|(k, _)| k.as_str()).collect();
    let signed = signed.join(";");
    let canonical = format!("{method}\n{path}\n\n{canonical_headers}\n{signed}\n{payload_hash}");
    let day = &amz_date[..8];
    let scope = format!("{day}/{region}/s3/aws4_request");
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex(&Sha256::digest(canonical.as_bytes())));
    let k = hmac(format!("AWS4{secret_key}").as_bytes(), day);
    let k = hmac(&k, region);
    let k = hmac(&k, "s3");
    let k = hmac(&k, "aws4_request");
    let signature = hex(&hmac(&k, &to_sign));
    format!("AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed}, Signature={signature}")
}

impl S3Backend {
    fn request(&self, method: &str, key: &str, body: &[u8]) -> Result<Vec<u8>, String> {
        let s = &self.s;
        let endpoint = s.endpoint.trim().trim_end_matches('/');
        let (scheme, host) = endpoint.split_once("://").unwrap_or(("https", endpoint));
        let object = s3_encode(&join(&s.remote_dir, key));
        let bucket = s.bucket.trim();
        let (host, path) = if s.path_style {
            (host.to_string(), format!("/{bucket}/{object}"))
        } else {
            (format!("{bucket}.{host}"), format!("/{object}"))
        };
        let region = if s.region.trim().is_empty() { "us-east-1" } else { s.region.trim() };
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let payload_hash = hex(&Sha256::digest(body));
        let headers = vec![
            ("host".to_string(), host.clone()),
            ("x-amz-content-sha256".to_string(), payload_hash.clone()),
            ("x-amz-date".to_string(), amz_date.clone()),
        ];
        let auth = sigv4(method, &path, &headers, &payload_hash, &amz_date, region, s.access_key.trim(), &s.secret_key);
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let url = format!("{scheme}://{host}{path}");
        let res = match method {
            "PUT" => agent
                .put(&url)
                .header("x-amz-content-sha256", &payload_hash)
                .header("x-amz-date", &amz_date)
                .header("Authorization", &auth)
                .send(body),
            "DELETE" => agent
                .delete(&url)
                .header("x-amz-content-sha256", &payload_hash)
                .header("x-amz-date", &amz_date)
                .header("Authorization", &auth)
                .call(),
            _ => agent
                .get(&url)
                .header("x-amz-content-sha256", &payload_hash)
                .header("x-amz-date", &amz_date)
                .header("Authorization", &auth)
                .call(),
        };
        let mut res = res.map_err(|e| format!("S3 nicht erreichbar ({e})."))?;
        let status = res.status().as_u16();
        let bytes = res.body_mut().with_config().limit(512 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
        if status == 404 {
            return Err("Die Belegdatei wurde nicht gefunden.".into());
        }
        if status >= 300 {
            let text = String::from_utf8_lossy(&bytes);
            let code = text.split("<Code>").nth(1).and_then(|r| r.split('<').next()).unwrap_or("");
            return Err(format!("S3 meldet Fehler {status} {code}"));
        }
        Ok(bytes)
    }
}

impl FileBackend for S3Backend {
    fn describe(&self) -> String {
        format!("S3 {} / {}", self.s.endpoint.trim(), self.s.bucket.trim())
    }
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        self.request("PUT", key, bytes).map(|_| ())
    }
    fn get(&mut self, key: &str) -> Result<Vec<u8>, String> {
        self.request("GET", key, b"")
    }
    fn delete(&mut self, key: &str) -> Result<(), String> {
        match self.request("DELETE", key, b"") {
            Err(e) if e.contains("nicht gefunden") => Ok(()),
            r => r.map(|_| ()),
        }
    }
}

// ---- FTP / FTPS --------------------------------------------------------------------

pub struct FtpBackend {
    pub s: FileSettings,
}

trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

fn tls_client(host: &str, tcp: TcpStream) -> Result<Box<dyn Stream>, String> {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name).map_err(|e| e.to_string())?;
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

struct Ftp {
    ctrl: BufReader<Box<dyn Stream>>,
    host: String,
    tls: bool,
}

impl Ftp {
    fn reply(&mut self) -> Result<(u32, String), String> {
        let mut text = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.ctrl.read_line(&mut line).map_err(|e| format!("FTP: {e}"))? == 0 {
                return Err("FTP: Verbindung getrennt.".into());
            }
            text.push_str(&line);
            // A reply ends with "123 text"; "123-" lines continue it.
            if line.len() >= 4 && line.as_bytes()[3] == b' ' && line[..3].chars().all(|c| c.is_ascii_digit()) {
                return Ok((line[..3].parse().unwrap_or(0), text));
            }
        }
    }

    fn cmd(&mut self, c: &str) -> Result<(u32, String), String> {
        let w = self.ctrl.get_mut();
        w.write_all(format!("{c}\r\n").as_bytes()).and_then(|_| w.flush()).map_err(|e| format!("FTP: {e}"))?;
        self.reply()
    }

    fn expect(&mut self, c: &str, ok: &[u32]) -> Result<String, String> {
        let (code, text) = self.cmd(c)?;
        if ok.contains(&code) {
            Ok(text)
        } else {
            let shown = if c.starts_with("PASS") { "PASS ***" } else { c };
            Err(format!("FTP: {shown} → {}", text.trim()))
        }
    }

    fn connect(s: &FileSettings) -> Result<Ftp, String> {
        let host = s.host.trim().to_string();
        let port = if s.port == 0 { 21 } else { s.port };
        let tcp = TcpStream::connect((host.as_str(), port)).map_err(|e| format!("FTP-Server {host}:{port} nicht erreichbar: {e}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(60))).ok();
        let mut f = Ftp { ctrl: BufReader::new(Box::new(tcp.try_clone().map_err(|e| e.to_string())?)), host: host.clone(), tls: s.tls };
        let (code, text) = f.reply()?;
        if code != 220 {
            return Err(format!("FTP: {}", text.trim()));
        }
        if s.tls {
            f.expect("AUTH TLS", &[234])?;
            f.ctrl = BufReader::new(tls_client(&host, tcp)?);
        }
        let user = if s.username.trim().is_empty() { "anonymous" } else { s.username.trim() };
        let (code, text) = f.cmd(&format!("USER {user}"))?;
        if code == 331 {
            f.expect(&format!("PASS {}", s.password), &[230, 202])?;
        } else if code != 230 {
            return Err(format!("FTP: {}", text.trim()));
        }
        if s.tls {
            f.expect("PBSZ 0", &[200])?;
            f.expect("PROT P", &[200])?;
        }
        f.expect("TYPE I", &[200])?;
        Ok(f)
    }

    /// Opens the data connection (passive mode).
    fn data(&mut self) -> Result<Box<dyn Stream>, String> {
        let text = self.expect("PASV", &[227])?;
        let nums: Vec<u16> = text
            .split(['(', ')'])
            .nth(1)
            .unwrap_or("")
            .split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        if nums.len() != 6 {
            return Err("FTP: Antwort auf PASV unverständlich.".into());
        }
        // Use the control host: servers behind NAT often report an inner address.
        let port = nums[4] * 256 + nums[5];
        let tcp = TcpStream::connect((self.host.as_str(), port)).map_err(|e| format!("FTP-Datenverbindung: {e}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(120))).ok();
        if self.tls { tls_client(&self.host, tcp) } else { Ok(Box::new(tcp)) }
    }

    fn mkdirs(&mut self, path: &str) {
        let mut cur = String::new();
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for p in parts.iter().take(parts.len().saturating_sub(1)) {
            cur.push('/');
            cur.push_str(p);
            let _ = self.cmd(&format!("MKD {cur}"));
        }
    }
}

impl FileBackend for FtpBackend {
    fn describe(&self) -> String {
        format!("{}://{}/{}", if self.s.tls { "ftps" } else { "ftp" }, self.s.host.trim(), self.s.remote_dir.trim().trim_matches('/'))
    }

    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let path = format!("/{}", join(&self.s.remote_dir, key));
        let mut f = Ftp::connect(&self.s)?;
        f.mkdirs(&path);
        let mut data = f.data()?;
        f.expect(&format!("STOR {path}"), &[125, 150])?;
        data.write_all(bytes).and_then(|_| data.flush()).map_err(|e| format!("FTP: {e}"))?;
        drop(data);
        let (code, text) = f.reply()?;
        let _ = f.cmd("QUIT");
        if code != 226 && code != 250 {
            return Err(format!("FTP: {}", text.trim()));
        }
        Ok(())
    }

    fn get(&mut self, key: &str) -> Result<Vec<u8>, String> {
        let path = format!("/{}", join(&self.s.remote_dir, key));
        let mut f = Ftp::connect(&self.s)?;
        let mut data = f.data()?;
        let (code, _) = f.cmd(&format!("RETR {path}"))?;
        if code != 125 && code != 150 {
            return Err("Die Belegdatei wurde nicht gefunden.".into());
        }
        let mut out = Vec::new();
        // TLS servers may close without a close_notify; what arrived is kept.
        let _ = data.read_to_end(&mut out);
        drop(data);
        let _ = f.reply();
        let _ = f.cmd("QUIT");
        Ok(out)
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        let path = format!("/{}", join(&self.s.remote_dir, key));
        let mut f = Ftp::connect(&self.s)?;
        let _ = f.cmd(&format!("DELE {path}"));
        let _ = f.cmd("QUIT");
        Ok(())
    }
}

// ---- SFTP (via the OpenSSH client of the server) -----------------------------------

pub struct SftpBackend {
    pub s: FileSettings,
}

impl SftpBackend {
    /// Runs `sftp` in batch mode with the key file; commands come on stdin.
    fn run(&self, commands: &str) -> Result<(), String> {
        let s = &self.s;
        let target = if s.username.trim().is_empty() { s.host.trim().to_string() } else { format!("{}@{}", s.username.trim(), s.host.trim()) };
        let mut child = std::process::Command::new("sftp")
            .args(["-b", "-", "-P", &(if s.port == 0 { 22 } else { s.port }).to_string(), "-i", s.key_file.trim()])
            .args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=20"])
            .arg(target)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("SFTP: Programm „sftp“ nicht gefunden ({e}). Bitte OpenSSH-Client installieren."))?;
        child.stdin.take().map(|mut i| i.write_all(commands.as_bytes())).transpose().map_err(|e| e.to_string())?;
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("SFTP: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("Fehler")))
        }
    }

    fn quoted(p: &str) -> String {
        format!("\"{}\"", p.replace('"', ""))
    }

    fn temp(&self) -> PathBuf {
        let mut name = [0u8; 8];
        let _ = getrandom::fill(&mut name);
        std::env::temp_dir().join(format!("openkontor-sftp-{}", hex(&name)))
    }
}

impl FileBackend for SftpBackend {
    fn describe(&self) -> String {
        format!("sftp://{}/{}", self.s.host.trim(), self.s.remote_dir.trim().trim_matches('/'))
    }

    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let path = join(&self.s.remote_dir, key);
        let tmp = self.temp();
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        let mut cmds = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        let mut cur = String::new();
        for p in &parts[..parts.len() - 1] {
            if !cur.is_empty() || path.starts_with('/') {
                cur.push('/');
            }
            cur.push_str(p);
            // A leading "-" lets the batch go on if the folder exists already.
            cmds.push_str(&format!("-mkdir {}\n", Self::quoted(&cur)));
        }
        cmds.push_str(&format!("put {} {}\n", Self::quoted(&tmp.to_string_lossy()), Self::quoted(&path)));
        let r = self.run(&cmds);
        let _ = std::fs::remove_file(&tmp);
        r
    }

    fn get(&mut self, key: &str) -> Result<Vec<u8>, String> {
        let path = join(&self.s.remote_dir, key);
        let tmp = self.temp();
        self.run(&format!("get {} {}\n", Self::quoted(&path), Self::quoted(&tmp.to_string_lossy())))
            .map_err(|_| "Die Belegdatei wurde nicht gefunden.".to_string())?;
        let bytes = std::fs::read(&tmp).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&tmp);
        bytes
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        let path = join(&self.s.remote_dir, key);
        self.run(&format!("-rm {}\n", Self::quoted(&path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_example_signature() {
        // Example "GET Object" from the AWS documentation of Signature Version 4 for S3.
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let headers = vec![
            ("host".to_string(), "examplebucket.s3.amazonaws.com".to_string()),
            ("range".to_string(), "bytes=0-9".to_string()),
            ("x-amz-content-sha256".to_string(), empty.to_string()),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        let auth = sigv4(
            "GET",
            "/test.txt",
            &headers,
            empty,
            "20130524T000000Z",
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        assert!(auth.ends_with("Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"), "{auth}");
    }

    #[test]
    fn folder_storage() {
        let dir = std::env::temp_dir().join(format!("ok-fs-test-{}", std::process::id()));
        let mut b = FsBackend { root: dir.clone() };
        check(&mut b).unwrap();
        let key = key_for(42, "Rechnung März.pdf", "07.11.2026 14:03");
        assert_eq!(key, "2026/42-Rechnung_M_rz.pdf");
        b.put(&key, b"pdf").unwrap();
        assert_eq!(b.get(&key).unwrap(), b"pdf");
        b.delete(&key).unwrap();
        assert!(b.get(&key).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
