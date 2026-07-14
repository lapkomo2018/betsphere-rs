//! The authenticated identity a use case acts on behalf of.

use domain::entities::{Role, UserId};

use crate::ports::AccessClaims;

/// Who is invoking a use case, independent of transport. Adapters (HTTP,
/// bots, jobs) build one from whatever credentials they verify; use cases
/// check it against the domain's authorization policies.
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub user_id: UserId,
    pub role: Role,
}

impl From<AccessClaims> for Actor {
    fn from(claims: AccessClaims) -> Self {
        Self {
            user_id: claims.user_id,
            role: claims.role,
        }
    }
}
