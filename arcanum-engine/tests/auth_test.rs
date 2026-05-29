use arcanum_engine::auth::{AuthMiddleware, ApiKeyClaims};

#[test]
fn test_valid_api_key_authenticates() {
    let auth = AuthMiddleware::new("secret-signing-key");
    let key = auth.generate_api_key("user-1", vec!["collection-a".to_string()]);
    let claims = auth.validate_api_key(&key).unwrap();
    assert_eq!(claims.user_id, "user-1");
    assert!(claims.allowed_collections.contains(&"collection-a".to_string()));
}

#[test]
fn test_invalid_api_key_rejected() {
    let auth = AuthMiddleware::new("secret-signing-key");
    let result = auth.validate_api_key("invalid.key.here");
    assert!(result.is_err());
}
