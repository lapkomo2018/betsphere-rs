//! Access policies: which role may perform which action.
//!
//! Enforcement lives in the application layer's use cases so every adapter
//! (HTTP, bots, jobs) goes through the same checks; this module is the single
//! place the rules themselves are written down.

use crate::entities::Role;

/// Who may create and resolve markets.
pub fn can_manage_markets(role: Role) -> bool {
    role == Role::Admin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_admins_manage_markets() {
        assert!(can_manage_markets(Role::Admin));
        assert!(!can_manage_markets(Role::User));
    }
}
