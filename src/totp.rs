//! Two-factor sign-in with time-based one-time codes (RFC 6238), as shown by
//! authenticator apps (Microsoft Authenticator, Google Authenticator, FreeOTP …).

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = ALPHABET.iter().position(|a| *a as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// A new random secret (160 bits), base32.
pub fn new_secret() -> String {
    let mut key = [0u8; 20];
    getrandom::fill(&mut key).expect("Zufallszahlen nicht verfügbar");
    base32_encode(&key)
}

/// The six-digit code for a 30-second time step.
pub fn code_at(secret: &str, step: u64) -> Option<u32> {
    let key = base32_decode(secret)?;
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &key);
    let tag = ring::hmac::sign(&k, &step.to_be_bytes());
    let h = tag.as_ref();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = ((h[off] as u32 & 0x7f) << 24) | ((h[off + 1] as u32) << 16) | ((h[off + 2] as u32) << 8) | h[off + 3] as u32;
    Some(bin % 1_000_000)
}

pub fn current_step() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 30).unwrap_or(0)
}

/// Checks a code, allowing one step of clock difference. Returns the step it matched,
/// so the same code cannot be used twice.
pub fn verify(secret: &str, code: &str, now_step: u64) -> Option<u64> {
    let code: u32 = code.trim().replace(' ', "").parse().ok()?;
    [now_step, now_step.saturating_sub(1), now_step + 1].into_iter().find(|s| code_at(secret, *s) == Some(code))
}

/// The address the authenticator app reads from the QR code.
pub fn uri(secret: &str, account: &str, issuer: &str) -> String {
    let enc = crate::server::url_encode;
    format!("otpauth://totp/{}:{}?secret={secret}&issuer={}&digits=6&period=30", enc(issuer), enc(account), enc(issuer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector() {
        // RFC 6238 test key "12345678901234567890", T = 59 s → step 1 → 94287082 (8 digits) → 287082.
        let secret = base32_encode(b"12345678901234567890");
        assert_eq!(code_at(&secret, 1), Some(287_082));
        assert_eq!(base32_decode(&secret).unwrap(), b"12345678901234567890");
        assert_eq!(verify(&secret, "287 082", 2), Some(1));
        assert_eq!(verify(&secret, "287082", 5), None);
    }
}
