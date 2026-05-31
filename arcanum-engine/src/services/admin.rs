use arcanum_core::{Result, ArcanumError};
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::AdminRole;

#[derive(Debug)]
pub struct AdminService {
    audit: Arc<AuditLogger>,
}

impl AdminService {
    pub fn new(audit: Arc<AuditLogger>) -> Self {
        Self { audit }
    }

    /// Check that `caller_role` is at least as privileged as `required`.
    /// Role hierarchy: Admin > Operator > Tester.
    pub fn require_role(caller_role: &AdminRole, required: &AdminRole) -> Result<()> {
        let rank = |r: &AdminRole| match r {
            AdminRole::Admin    => 2,
            AdminRole::Operator => 1,
            AdminRole::Tester   => 0,
        };
        if rank(caller_role) >= rank(required) {
            Ok(())
        } else {
            Err(ArcanumError::Auth(format!(
                "requires {:?} role, caller has {:?}", required, caller_role
            )))
        }
    }

    pub async fn rotate_keys(&self, admin_user_id: &str) -> Result<()> {
        self.audit.log(AuditEntry {
            operation: "rotate_keys".into(),
            user_id: admin_user_id.to_string(),
            collection_id: String::new(),
            result: "ok".into(),
        }).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_role_admin_passes_all() {
        assert!(AdminService::require_role(&AdminRole::Admin, &AdminRole::Admin).is_ok());
        assert!(AdminService::require_role(&AdminRole::Admin, &AdminRole::Operator).is_ok());
        assert!(AdminService::require_role(&AdminRole::Admin, &AdminRole::Tester).is_ok());
    }

    #[test]
    fn test_require_role_tester_fails_operator() {
        assert!(AdminService::require_role(&AdminRole::Tester, &AdminRole::Operator).is_err());
    }

    #[tokio::test]
    async fn test_rotate_keys_logs_audit() {
        let audit = Arc::new(AuditLogger::new());
        let svc = AdminService::new(audit.clone());
        svc.rotate_keys("admin-1").await.unwrap();
        let logs = audit.query(10).await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].entry.operation, "rotate_keys");
    }
}
