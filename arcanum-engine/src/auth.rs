use arcanum_core::{Result, ArcanumError};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyClaims {
    pub user_id: String,
    pub allowed_collections: Vec<String>,
    pub exp: usize,
}

pub struct AuthMiddleware {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthMiddleware {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn generate_api_key(&self, user_id: &str, collections: Vec<String>) -> String {
        let claims = ApiKeyClaims {
            user_id: user_id.to_string(),
            allowed_collections: collections,
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
        claims.allowed_collections.is_empty()
            || claims.allowed_collections.iter().any(|c| c == collection)
    }
}
