use arcanum_engine::auth::{AuthMiddleware, AdminClaims, AdminRole};

#[test]
fn test_admin_claims_role_parsing() {
    let claims = AdminClaims {
        sub: "admin-user".to_string(),
        role: AdminRole::Admin,
        exp: u64::MAX,
    };
    assert_eq!(claims.role, AdminRole::Admin);
    assert_eq!(claims.sub, "admin-user");

    let op = AdminClaims { sub: "op".to_string(), role: AdminRole::Operator, exp: 0 };
    assert_eq!(op.role, AdminRole::Operator);

    let tester = AdminClaims { sub: "t".to_string(), role: AdminRole::Tester, exp: 0 };
    assert_eq!(tester.role, AdminRole::Tester);
}

#[test]
fn test_validate_admin_jwt_requires_rs256_key() {
    let auth = AuthMiddleware::new("secret-signing-key-32-chars-long!!");
    let result = auth.validate_admin_jwt("some.jwt.token");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("RS256 public key not configured"));
}

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
