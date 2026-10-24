//! Verify release metadata using the existing publisher pin. No keychain,
//! network, file selection, shell execution or downloaded trust roots.
//! Same contract as scripts/companion-release-verify.swift: the envelope on
//! stdin, `{"valid":true}` and exit 0, exit 2 when malformed, 3 when unsigned.
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, UnparsedPublicKey};
use std::io::Read;

const PIN: &str = "G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM=";
const LIMIT: usize = 131072;

#[derive(Debug, PartialEq)]
enum Outcome { Valid, Malformed, Unsigned }

fn verify(input: &[u8], pin: &[u8]) -> Outcome {
    if input.len() > LIMIT { return Outcome::Malformed; }
    let Ok(serde_json::Value::Object(value)) = serde_json::from_slice::<serde_json::Value>(input) else { return Outcome::Malformed };
    if value.len() != 2 { return Outcome::Malformed; }
    let (Some(payload), Some(signature)) = (value.get("payload").and_then(|v| v.as_str()), value.get("signature").and_then(|v| v.as_str())) else { return Outcome::Malformed };
    let (Ok(body), Ok(signature)) = (STANDARD.decode(payload), STANDARD.decode(signature)) else { return Outcome::Malformed };
    if body.len() > 65536 || signature.len() != 64 || pin.len() != 32 { return Outcome::Malformed; }
    match UnparsedPublicKey::new(&ED25519, pin).verify(&body, &signature) { Ok(()) => Outcome::Valid, Err(_) => Outcome::Unsigned }
}

fn main() {
    let mut input = Vec::new();
    if std::io::stdin().take(LIMIT as u64 + 1).read_to_end(&mut input).is_err() { std::process::exit(2); }
    let pin = STANDARD.decode(PIN).unwrap_or_default();
    match verify(&input, &pin) {
        Outcome::Valid => println!("{{\"valid\":true}}"),
        Outcome::Malformed => std::process::exit(2),
        Outcome::Unsigned => std::process::exit(3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PUBLISHED: &[u8] = include_bytes!("fixtures/beta-25-130-release.json");
    fn pin() -> Vec<u8> { STANDARD.decode(PIN).unwrap() }
    #[test]
    fn the_published_beta_25_envelope_verifies_against_the_existing_pin() {
        assert_eq!(verify(PUBLISHED, &pin()), Outcome::Valid);
    }
    #[test]
    fn altered_payload_or_signature_is_rejected_as_unsigned() {
        let mut value: serde_json::Value = serde_json::from_slice(PUBLISHED).unwrap();
        let mut body = STANDARD.decode(value["payload"].as_str().unwrap()).unwrap();
        body[10] ^= 1; value["payload"] = STANDARD.encode(&body).into();
        assert_eq!(verify(&serde_json::to_vec(&value).unwrap(), &pin()), Outcome::Unsigned);
        let mut value: serde_json::Value = serde_json::from_slice(PUBLISHED).unwrap();
        value["signature"] = STANDARD.encode([0u8; 64]).into();
        assert_eq!(verify(&serde_json::to_vec(&value).unwrap(), &pin()), Outcome::Unsigned);
    }
    #[test]
    fn a_different_key_never_verifies_the_published_envelope() {
        let mut other = pin(); other[0] ^= 0x80;
        assert_ne!(verify(PUBLISHED, &other), Outcome::Valid);
    }
    #[test]
    fn malformed_extra_fields_and_oversized_input_are_rejected() {
        assert_eq!(verify(b"{}", &pin()), Outcome::Malformed);
        assert_eq!(verify(b"not json", &pin()), Outcome::Malformed);
        let mut value: serde_json::Value = serde_json::from_slice(PUBLISHED).unwrap();
        value["extra"] = 1.into();
        assert_eq!(verify(&serde_json::to_vec(&value).unwrap(), &pin()), Outcome::Malformed);
        assert_eq!(verify(&vec![b' '; LIMIT + 1], &pin()), Outcome::Malformed);
    }
}
