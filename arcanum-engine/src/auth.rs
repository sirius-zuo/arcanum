use arcanum_core::{Result, ArcanumError};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, Algorithm};
use serde::{Deserialize, Serialize};
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyClaims {
    pub user_id: String,
    pub allowed_collections: Vec<String>,
    /// Explicit admin flag — grants access to all collections.
    /// Must be set deliberately; absence or empty allowed_collections defaults to deny.
    #[serde(default)]
    pub is_admin: bool,
    pub exp: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminRole {
    Admin,
    Operator,
    Tester,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminClaims {
    pub sub: String,
    pub role: AdminRole,
    pub exp: u64,
}

pub struct AuthMiddleware {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    rs256_public_key_pem: Option<String>,
}

impl std::fmt::Debug for AuthMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMiddleware").finish_non_exhaustive()
    }
}

impl AuthMiddleware {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            rs256_public_key_pem: None,
        }
    }

    pub fn with_rs256_public_key(mut self, pem: &str) -> Self {
        self.rs256_public_key_pem = Some(pem.to_string());
        self
    }

    pub fn validate_admin_jwt(&self, token: &str) -> Result<AdminClaims> {
        let pem = self.rs256_public_key_pem.as_deref()
            .ok_or_else(|| ArcanumError::Auth("RS256 public key not configured".to_string()))?;
        let key = DecodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|e| ArcanumError::Auth(format!("invalid RS256 key: {}", e)))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        decode::<AdminClaims>(token, &key, &validation)
            .map(|d| d.claims)
            .map_err(|e| ArcanumError::Auth(format!("admin JWT invalid: {}", e)))
    }

    pub fn generate_api_key(&self, user_id: &str, collections: Vec<String>) -> String {
        self.generate_api_key_with_opts(user_id, collections, false)
    }

    pub fn generate_admin_key(&self, user_id: &str) -> String {
        self.generate_api_key_with_opts(user_id, vec![], true)
    }

    fn generate_api_key_with_opts(&self, user_id: &str, collections: Vec<String>, is_admin: bool) -> String {
        let claims = ApiKeyClaims {
            user_id: user_id.to_string(),
            allowed_collections: collections,
            is_admin,
            exp: (Utc::now().timestamp() + 86400 * 365) as usize,
        };
        encode(&Header::default(), &claims, &self.encoding_key).unwrap_or_default()
    }

    pub fn validate_api_key(&self, token: &str) -> Result<ApiKeyClaims> {
        decode::<ApiKeyClaims>(token, &self.decoding_key, &Validation::default())
            .map(|data| data.claims)
            .map_err(|e| ArcanumError::Auth(e.to_string()))
    }

    pub fn can_access_collection(&self, claims: &ApiKeyClaims, collection: &str) -> bool {
        // Explicit admin flag grants all access. Empty allowed_collections is NOT a wildcard.
        claims.is_admin || claims.allowed_collections.iter().any(|c| c == collection)
    }
}
