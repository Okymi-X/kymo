use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use kymo_core::{Permission, PermissionMatrix, Role};
use serde::{Deserialize, Serialize};

use crate::auth::{MaybeCaller, Require, needs};
use crate::error::ApiError;
use crate::monitor_store;
use crate::state::AppState;

#[derive(Serialize)]
pub struct RoleOut {
    role: Role,
    /// A locked role cannot be edited at all.
    locked: bool,
    granted: Vec<Permission>,
    defaults: Vec<Permission>,
    /// Permissions that stay on for this role whatever is chosen.
    forced: Vec<Permission>,
}

#[derive(Serialize)]
pub struct MatrixOut {
    permissions: Vec<Permission>,
    roles: Vec<RoleOut>,
}

fn describe(matrix: &PermissionMatrix) -> MatrixOut {
    let roles = Role::ALL
        .into_iter()
        .map(|role| RoleOut {
            role,
            locked: role.is_locked(),
            granted: matrix.granted(role),
            defaults: Permission::ALL
                .into_iter()
                .filter(|p| role.default_allows(*p))
                .collect(),
            forced: Permission::ALL
                .into_iter()
                .filter(|p| role.is_forced(*p))
                .collect(),
        })
        .collect();
    MatrixOut {
        permissions: Permission::ALL.to_vec(),
        roles,
    }
}

pub async fn show(
    _: Require<needs::ManagePermissions>,
    State(state): State<AppState>,
) -> Result<Json<MatrixOut>, ApiError> {
    Ok(Json(describe(&state.permission_matrix().await?)))
}

#[derive(Deserialize)]
pub struct UpdateBody {
    roles: BTreeMap<Role, Vec<Permission>>,
}

/// Records what changed for each role: one audit row per role, naming the permissions.
async fn audit_changes(
    state: &AppState,
    who: &crate::auth::Caller,
    before: &PermissionMatrix,
    after: &PermissionMatrix,
    action: &str,
) -> Result<(), ApiError> {
    for role in Role::ALL {
        let names = |on: bool| -> Vec<&'static str> {
            Permission::ALL
                .into_iter()
                .filter(|p| {
                    before.can(role, *p) != after.can(role, *p) && after.can(role, *p) == on
                })
                .map(Permission::as_str)
                .collect()
        };
        let granted = names(true);
        let revoked = names(false);
        if granted.is_empty() && revoked.is_empty() {
            continue;
        }
        let detail = format!(
            "granted: {}; revoked: {}",
            if granted.is_empty() {
                "none".into()
            } else {
                granted.join(", ")
            },
            if revoked.is_empty() {
                "none".into()
            } else {
                revoked.join(", ")
            },
        );
        monitor_store::audit(
            &state.pool,
            state.now(),
            Some(who),
            action,
            "role",
            role.as_str(),
            &detail,
        )
        .await?;
    }
    Ok(())
}

/// Replaces the permissions of the roles named in the body. Roles left out keep theirs. The
/// whole request is refused if any part of it is, so a half applied change cannot happen.
pub async fn update(
    who: Require<needs::ManagePermissions>,
    State(state): State<AppState>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<MatrixOut>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let before = state.permission_matrix().await?;
    let mut after = before.clone();
    for (role, permissions) in &body.roles {
        after
            .set_role(*role, permissions)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }
    if after != before {
        state.set_permission_matrix(after.clone()).await?;
        audit_changes(&state, &who.0, &before, &after, "permissions_updated").await?;
    }
    Ok(Json(describe(&after)))
}

pub async fn reset(
    who: Require<needs::ManagePermissions>,
    State(state): State<AppState>,
) -> Result<Json<MatrixOut>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let before = state.permission_matrix().await?;
    let after = PermissionMatrix::default();
    if after != before {
        state.set_permission_matrix(after.clone()).await?;
        audit_changes(&state, &who.0, &before, &after, "permissions_reset").await?;
    }
    Ok(Json(describe(&after)))
}

#[derive(Serialize)]
pub struct MineOut {
    role: Role,
    permissions: Vec<Permission>,
}

/// What the caller may do right now, so the web app can show only what will work. A visitor
/// gets the visitor's permissions. The server still checks every request.
pub async fn mine(
    MaybeCaller(caller): MaybeCaller,
    State(state): State<AppState>,
) -> Result<Json<MineOut>, ApiError> {
    let role = caller.map_or(Role::Visitor, |c| c.role);
    Ok(Json(MineOut {
        role,
        permissions: state.permission_matrix().await?.granted(role),
    }))
}
