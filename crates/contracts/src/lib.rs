//! Shared boundaries for HTTP, jobs and encrypted provider configuration.
use aes_gcm::{
    aead::{Aead, AeadCore, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use uuid::Uuid;

pub type OrganizationId = Uuid;
pub type UserId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Manager,
    Reviewer,
}
impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Self::Owner),
            "manager" => Some(Self::Manager),
            "reviewer" => Some(Self::Reviewer),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
            Self::Reviewer => "reviewer",
        }
    }
    pub fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Manager)
    }
    pub fn is_owner(self) -> bool {
        self == Self::Owner
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Success<T> {
    pub data: T,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub error: ApiError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderResult {
    #[serde(rename = "busy")]
    Busy,
    #[serde(rename = "answered")]
    Answered,
    #[serde(rename = "bridged")]
    Bridged,
    #[serde(rename = "not answered")]
    NotAnswered,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct CryptoError;
impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("configuration_encryption_error")
    }
}
impl std::error::Error for CryptoError {}

/// Versioned AES-256-GCM envelope. Authentication failure never exposes input.
pub fn seal(value: &Value, key: &[u8; 32]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let payload = serde_json::to_vec(value).map_err(|_| CryptoError)?;
    let encrypted = cipher
        .encrypt(&nonce, payload.as_slice())
        .map_err(|_| CryptoError)?;
    let mut envelope = nonce.to_vec();
    envelope.extend(encrypted);
    Ok(format!("v1.{}", STANDARD.encode(envelope)))
}

pub fn unseal(envelope: &str, key: &[u8; 32]) -> Result<Value, CryptoError> {
    let body = envelope.strip_prefix("v1.").ok_or(CryptoError)?;
    let bytes = STANDARD.decode(body).map_err(|_| CryptoError)?;
    if bytes.len() < 28 {
        return Err(CryptoError);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError)?;
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
        .map_err(|_| CryptoError)?;
    serde_json::from_slice(&decrypted).map_err(|_| CryptoError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn encryption_is_randomized_and_authenticated() {
        let key = [7; 32];
        let v = json!({"secret":"synthetic-secret"});
        let first = seal(&v, &key).unwrap();
        let second = seal(&v, &key).unwrap();
        assert_ne!(first, second);
        assert!(!first.contains("synthetic-secret"));
        assert_eq!(unseal(&first, &key).unwrap(), v);
        assert!(unseal(&first, &[8; 32]).is_err());
        let mut bytes = STANDARD.decode(first.strip_prefix("v1.").unwrap()).unwrap();
        bytes[14] ^= 1;
        assert!(unseal(&format!("v1.{}", STANDARD.encode(bytes)), &key).is_err());
        assert!(unseal("v2.invalid", &key).is_err());
    }
    #[test]
    fn roles_do_not_allow_reviewer_mutation() {
        assert!(!Role::Reviewer.can_write());
        assert!(!Role::Manager.is_owner());
        assert!(Role::Owner.is_owner());
        assert_eq!(Role::parse("admin"), None);
    }
}
