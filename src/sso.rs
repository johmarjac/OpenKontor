//! Optional sign-in with Microsoft 365 (Microsoft Entra ID).
//!
//! The workplace signs the user in at Microsoft (authorization code flow with PKCE in the
//! system browser) and hands the resulting ID token to the server. The server checks it here:
//! RS256 signature against the tenant's published keys, issuer, audience, tenant, expiry and
//! nonce. Only then does it open a session for the matching OpenKontor user.

use crate::server::Role;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_AUTHORITY: &str = "https://login.microsoftonline.com";
/// Clock difference tolerated between this server and Microsoft.
const LEEWAY_SECS: i64 = 300;

/// Server settings for Microsoft 365 sign-in.
#[derive(Clone, Debug)]
pub struct SsoConfig {
    /// Directory (tenant) ID of the Microsoft 365 organisation, a GUID.
    pub tenant: String,
    /// Application (client) ID of the app registration.
    pub client_id: String,
    /// Sign-in address; other clouds (e.g. login.microsoftonline.us) can be set here.
    pub authority: String,
    /// Role for colleagues who sign in for the first time; `None` = only users an admin created.
    pub auto_role: Option<Role>,
}

/// What the server tells workplaces so they can start the sign-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SsoInfo {
    pub tenant: String,
    pub client_id: String,
    pub authority: String,
}

impl SsoConfig {
    pub fn info(&self) -> SsoInfo {
        SsoInfo {
            tenant: self.tenant.clone(),
            client_id: self.client_id.clone(),
            authority: self.authority.clone(),
        }
    }

    pub fn issuer(&self) -> String {
        format!("{}/{}/v2.0", self.authority.trim_end_matches('/'), self.tenant)
    }

    fn keys_url(&self) -> String {
        format!("{}/{}/discovery/v2.0/keys", self.authority.trim_end_matches('/'), self.tenant)
    }
}

pub fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(&parts).all(|(n, p)| p.len() == *n)
        && s.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
}

/// The person Microsoft confirmed.
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    /// Sign-in name, e.g. "max.muster@firma.de" (lower case).
    pub username: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Claims {
    pub iss: String,
    pub aud: String,
    pub tid: String,
    pub exp: i64,
    pub nbf: i64,
    pub nonce: String,
    pub preferred_username: String,
    pub email: String,
    pub upn: String,
    pub name: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

/// Checks ID tokens; keeps Microsoft's signing keys in memory.
pub struct Verifier {
    pub cfg: SsoConfig,
    keys: HashMap<String, (Vec<u8>, Vec<u8>)>,
    fetched: Option<Instant>,
}

impl Verifier {
    pub fn new(cfg: SsoConfig) -> Verifier {
        Verifier { cfg, keys: HashMap::new(), fetched: None }
    }

    fn refresh_keys(&mut self) -> Result<(), String> {
        // Microsoft rotates keys now and then; don't ask more than once a minute.
        if self.fetched.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return Ok(());
        }
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let jwks: Jwks = agent
            .get(self.cfg.keys_url())
            .call()
            .map_err(|e| format!("Schlüssel von Microsoft nicht abrufbar: {e}"))?
            .body_mut()
            .read_json()
            .map_err(|e| format!("Schlüssel von Microsoft unlesbar: {e}"))?;
        self.keys.clear();
        for k in jwks.keys.into_iter().filter(|k| k.kty == "RSA") {
            if let (Some(kid), Some(n), Some(e)) = (k.kid, k.n, k.e) {
                self.keys.insert(kid, (b64url_decode(&n)?, b64url_decode(&e)?));
            }
        }
        self.fetched = Some(Instant::now());
        Ok(())
    }

    /// Checks an ID token and returns who signed in.
    pub fn verify(&mut self, id_token: &str, nonce: &str) -> Result<Identity, String> {
        let (header, claims, message, signature) = split_token(id_token)?;
        if header.alg != "RS256" {
            return Err("Unerwartetes Signaturverfahren im Anmeldenachweis.".into());
        }
        let kid = header.kid.unwrap_or_default();
        if !self.keys.contains_key(&kid) {
            self.refresh_keys()?;
        }
        let (n, e) = self
            .keys
            .get(&kid)
            .ok_or("Der Anmeldenachweis ist mit einem unbekannten Schlüssel signiert.")?;
        verify_rs256(message.as_bytes(), &signature, n, e)?;
        check_claims(&claims, &self.cfg, nonce, now())
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn split_token(token: &str) -> Result<(Header, Claims, String, Vec<u8>), String> {
    let parts: Vec<&str> = token.trim().split('.').collect();
    if parts.len() != 3 {
        return Err("Der Anmeldenachweis ist unvollständig.".into());
    }
    let bad = |_| "Der Anmeldenachweis ist beschädigt.".to_string();
    let header: Header = serde_json::from_slice(&b64url_decode(parts[0])?).map_err(bad)?;
    let claims: Claims = serde_json::from_slice(&b64url_decode(parts[1])?).map_err(bad)?;
    let signature = b64url_decode(parts[2])?;
    Ok((header, claims, format!("{}.{}", parts[0], parts[1]), signature))
}

pub fn verify_rs256(message: &[u8], signature: &[u8], n: &[u8], e: &[u8]) -> Result<(), String> {
    ring::signature::RsaPublicKeyComponents { n, e }
        .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, message, signature)
        .map_err(|_| "Die Signatur des Anmeldenachweises ist ungültig.".to_string())
}

/// Issuer, audience, tenant, time and nonce must all match.
pub fn check_claims(c: &Claims, cfg: &SsoConfig, nonce: &str, now: i64) -> Result<Identity, String> {
    if c.iss != cfg.issuer() || !c.tid.eq_ignore_ascii_case(&cfg.tenant) {
        return Err("Der Anmeldenachweis stammt nicht aus Ihrer Microsoft-365-Organisation.".into());
    }
    if c.aud != cfg.client_id {
        return Err("Der Anmeldenachweis ist für eine andere Anwendung ausgestellt.".into());
    }
    if c.exp + LEEWAY_SECS < now || (c.nbf != 0 && c.nbf - LEEWAY_SECS > now) {
        return Err("Der Anmeldenachweis ist abgelaufen. Bitte erneut anmelden.".into());
    }
    if nonce.is_empty() || c.nonce != nonce {
        return Err("Der Anmeldenachweis gehört nicht zu dieser Anmeldung.".into());
    }
    let username = [&c.preferred_username, &c.email, &c.upn]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .ok_or("Microsoft hat keinen Anmeldenamen geliefert.")?
        .trim()
        .to_lowercase();
    Ok(Identity { display_name: if c.name.is_empty() { username.clone() } else { c.name.clone() }, username })
}

// ---- base64url (RFC 4648 §5, without padding) -------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64url_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..chunk.len() + 1 {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return Err("Der Anmeldenachweis ist beschädigt.".into()),
        };
        buf = buf << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SsoConfig {
        SsoConfig {
            tenant: "11111111-2222-3333-4444-555555555555".into(),
            client_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            authority: DEFAULT_AUTHORITY.into(),
            auto_role: None,
        }
    }

    fn claims() -> Claims {
        let c = cfg();
        Claims {
            iss: c.issuer(),
            aud: c.client_id.clone(),
            tid: c.tenant.clone(),
            exp: 2_000,
            nbf: 1_000,
            nonce: "n1".into(),
            preferred_username: "Max.Muster@Firma.de".into(),
            name: "Max Muster".into(),
            ..Claims::default()
        }
    }

    #[test]
    fn base64url_round_trip() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0xff, 0xfe, 0x00, 0x10]] {
            assert_eq!(b64url_decode(&b64url_encode(data)).unwrap(), data);
        }
        assert_eq!(b64url_encode(b"\xfb\xff"), "-_8");
    }

    #[test]
    fn accepts_a_matching_token() {
        let id = check_claims(&claims(), &cfg(), "n1", 1_500).unwrap();
        assert_eq!(id.username, "max.muster@firma.de");
    }

    #[test]
    fn rejects_wrong_tenant_audience_time_or_nonce() {
        let c = cfg();
        let mut x = claims();
        x.tid = "99999999-2222-3333-4444-555555555555".into();
        assert!(check_claims(&x, &c, "n1", 1_500).is_err());
        let mut x = claims();
        x.aud = "other".into();
        assert!(check_claims(&x, &c, "n1", 1_500).is_err());
        assert!(check_claims(&claims(), &c, "n1", 3_000).is_err());
        assert!(check_claims(&claims(), &c, "n2", 1_500).is_err());
        assert!(check_claims(&claims(), &c, "", 1_500).is_err());
    }

    #[test]
    fn guid_check() {
        assert!(is_guid("11111111-2222-3333-4444-555555555555"));
        assert!(!is_guid("firma.onmicrosoft.com"));
    }
}
