use arcanum_core::{Result, ArcanumError};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
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

pub struct AuthMiddleware {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
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
        }
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
