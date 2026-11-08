use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Visitor,
    Player,
    Referee,
    Monitor,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ViewPublic,
    ViewScoreboardPublic,
    PlayEvent,
    ManageTeam,
    SubmitFlag,
    StartInstance,
    Referee,
    ManageEvents,
    ManageChallenges,
    ManageUsers,
    ManageSettings,
    PreviewAsPlayer,
    Monitor,
    ModerateBans,
    ViewAudit,
    /// See a scoreboard that is not published yet.
    ViewHiddenScoreboard,
    /// Change which role may do what.
    ManagePermissions,
}

impl Permission {
    pub const ALL: [Permission; 17] = [
        Permission::ViewPublic,
        Permission::ViewScoreboardPublic,
        Permission::PlayEvent,
        Permission::ManageTeam,
        Permission::SubmitFlag,
        Permission::StartInstance,
        Permission::Referee,
        Permission::ManageEvents,
        Permission::ManageChallenges,
        Permission::ManageUsers,
        Permission::ManageSettings,
        Permission::PreviewAsPlayer,
        Permission::Monitor,
        Permission::ModerateBans,
        Permission::ViewAudit,
        Permission::ViewHiddenScoreboard,
        Permission::ManagePermissions,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::ViewPublic => "view_public",
            Permission::ViewScoreboardPublic => "view_scoreboard_public",
            Permission::PlayEvent => "play_event",
            Permission::ManageTeam => "manage_team",
            Permission::SubmitFlag => "submit_flag",
            Permission::StartInstance => "start_instance",
            Permission::Referee => "referee",
            Permission::ManageEvents => "manage_events",
            Permission::ManageChallenges => "manage_challenges",
            Permission::ManageUsers => "manage_users",
            Permission::ManageSettings => "manage_settings",
            Permission::PreviewAsPlayer => "preview_as_player",
            Permission::Monitor => "monitor",
            Permission::ModerateBans => "moderate_bans",
            Permission::ViewAudit => "view_audit",
            Permission::ViewHiddenScoreboard => "view_hidden_scoreboard",
            Permission::ManagePermissions => "manage_permissions",
        }
    }
}

impl FromStr for Permission {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Permission::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| CoreError::Unknown {
                kind: "permission",
                value: s.to_string(),
            })
    }
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Visitor,
        Role::Player,
        Role::Referee,
        Role::Monitor,
        Role::Admin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Visitor => "visitor",
            Role::Player => "player",
            Role::Referee => "referee",
            Role::Monitor => "monitor",
            Role::Admin => "admin",
        }
    }

    /// Roles whose permissions an organizer cannot change: the admin keeps everything so
    /// nobody can lock themselves out, and a visitor has no account to grant anything to.
    pub fn is_locked(self) -> bool {
        matches!(self, Role::Admin | Role::Visitor)
    }

    /// What this role may do out of the box, before an organizer changes anything.
    pub fn default_allows(self, permission: Permission) -> bool {
        use Permission::*;
        match self {
            Role::Admin => true,
            Role::Visitor => matches!(permission, ViewPublic),
            Role::Player => matches!(
                permission,
                ViewPublic
                    | ViewScoreboardPublic
                    | PlayEvent
                    | ManageTeam
                    | SubmitFlag
                    | StartInstance
            ),
            Role::Referee => matches!(
                permission,
                ViewPublic | ViewScoreboardPublic | PlayEvent | Referee | ViewHiddenScoreboard
            ),
            Role::Monitor => matches!(permission, ViewPublic | Monitor | ModerateBans | ViewAudit),
        }
    }

    /// True when the permission cannot be switched off for this role: everything of a locked
    /// role, and public viewing for everyone.
    pub fn is_forced(self, permission: Permission) -> bool {
        self.is_locked() || permission == Permission::ViewPublic
    }
}

impl FromStr for Role {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| CoreError::Unknown {
                kind: "role",
                value: s.to_string(),
            })
    }
}

/// A scoreboard shows when it is published, or to a role that may see hidden ones.
pub fn scoreboard_visible(published: bool, may_see_hidden: bool) -> bool {
    published || may_see_hidden
}

/// Why a change to the permission matrix was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MatrixError {
    #[error("the {} role is fixed and cannot be changed", .0.as_str())]
    Locked(Role),
    #[error("{} must stay allowed", .0.as_str())]
    Required(Permission),
}

/// Which role holds which permission. Starts as the defaults in `Role::default_allows`; an
/// organizer can switch permissions on or off for the roles that are not locked. Only the
/// differences from the defaults are worth storing (see `overrides`), so a permission added
/// in a later version follows its default until someone decides otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionMatrix {
    grants: BTreeMap<Role, BTreeSet<Permission>>,
}

impl Default for PermissionMatrix {
    fn default() -> Self {
        let grants = Role::ALL
            .into_iter()
            .map(|role| {
                let set = Permission::ALL
                    .into_iter()
                    .filter(|p| role.default_allows(*p))
                    .collect();
                (role, set)
            })
            .collect();
        Self { grants }
    }
}

impl PermissionMatrix {
    pub fn can(&self, role: Role, permission: Permission) -> bool {
        self.grants
            .get(&role)
            .is_some_and(|set| set.contains(&permission))
    }

    /// The role's permissions in declaration order.
    pub fn granted(&self, role: Role) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|p| self.can(role, *p))
            .collect()
    }

    /// Replaces everything a role may do. A locked role cannot be changed, and public viewing
    /// must stay on.
    pub fn set_role(&mut self, role: Role, permissions: &[Permission]) -> Result<(), MatrixError> {
        if role.is_locked() {
            return Err(MatrixError::Locked(role));
        }
        let set: BTreeSet<Permission> = permissions.iter().copied().collect();
        for p in Permission::ALL {
            if role.is_forced(p) && !set.contains(&p) {
                return Err(MatrixError::Required(p));
            }
        }
        self.grants.insert(role, set);
        Ok(())
    }

    /// Builds the matrix from stored differences. Entries for locked roles and forced
    /// permissions are ignored, so a hand edited row cannot lock the admin out.
    pub fn from_overrides(overrides: impl IntoIterator<Item = (Role, Permission, bool)>) -> Self {
        let mut matrix = Self::default();
        for (role, permission, allowed) in overrides {
            if role.is_forced(permission) {
                continue;
            }
            if let Some(set) = matrix.grants.get_mut(&role) {
                if allowed {
                    set.insert(permission);
                } else {
                    set.remove(&permission);
                }
            }
        }
        matrix
    }

    /// Every place this matrix differs from the defaults, for storing and for the audit log.
    pub fn overrides(&self) -> Vec<(Role, Permission, bool)> {
        let mut out = Vec::new();
        for role in Role::ALL {
            for p in Permission::ALL {
                let now = self.can(role, p);
                if now != role.default_allows(p) {
                    out.push((role, p, now));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::*;

    fn granted(role: Role) -> Vec<Permission> {
        PermissionMatrix::default().granted(role)
    }

    #[test]
    fn visitor_matrix() {
        assert_eq!(granted(Role::Visitor), vec![ViewPublic]);
    }

    #[test]
    fn player_matrix() {
        assert_eq!(
            granted(Role::Player),
            vec![
                ViewPublic,
                ViewScoreboardPublic,
                PlayEvent,
                ManageTeam,
                SubmitFlag,
                StartInstance
            ]
        );
    }

    #[test]
    fn referee_matrix() {
        assert_eq!(
            granted(Role::Referee),
            vec![
                ViewPublic,
                ViewScoreboardPublic,
                PlayEvent,
                Referee,
                ViewHiddenScoreboard
            ]
        );
        assert!(!Role::Referee.default_allows(SubmitFlag));
        assert!(!Role::Referee.default_allows(StartInstance));
        assert!(!Role::Referee.default_allows(ManageTeam));
    }

    #[test]
    fn monitor_matrix() {
        assert_eq!(
            granted(Role::Monitor),
            vec![ViewPublic, Monitor, ModerateBans, ViewAudit]
        );
    }

    #[test]
    fn admin_has_everything() {
        assert_eq!(granted(Role::Admin), Permission::ALL.to_vec());
    }

    #[test]
    fn everyone_can_view_public() {
        let m = PermissionMatrix::default();
        for r in Role::ALL {
            assert!(m.can(r, ViewPublic));
        }
    }

    #[test]
    fn defaults_store_nothing() {
        assert!(PermissionMatrix::default().overrides().is_empty());
    }

    #[test]
    fn a_role_can_be_changed_and_round_trips_through_overrides() {
        let mut m = PermissionMatrix::default();
        m.set_role(Role::Monitor, &[ViewPublic, Monitor, ManageEvents])
            .unwrap();
        assert!(m.can(Role::Monitor, ManageEvents));
        assert!(!m.can(Role::Monitor, ModerateBans));
        let again = PermissionMatrix::from_overrides(m.overrides());
        assert_eq!(again, m);
        // Only the differences are stored.
        assert_eq!(
            m.overrides(),
            vec![
                (Role::Monitor, ManageEvents, true),
                (Role::Monitor, ModerateBans, false),
                (Role::Monitor, ViewAudit, false),
            ]
        );
    }

    #[test]
    fn locked_roles_and_public_viewing_cannot_be_removed() {
        let mut m = PermissionMatrix::default();
        assert_eq!(
            m.set_role(Role::Admin, &[ViewPublic]),
            Err(MatrixError::Locked(Role::Admin))
        );
        assert_eq!(
            m.set_role(Role::Visitor, &Permission::ALL),
            Err(MatrixError::Locked(Role::Visitor))
        );
        assert_eq!(
            m.set_role(Role::Player, &[PlayEvent]),
            Err(MatrixError::Required(ViewPublic))
        );
        assert_eq!(m, PermissionMatrix::default());
    }

    #[test]
    fn stored_overrides_cannot_lock_the_admin_out() {
        let m = PermissionMatrix::from_overrides([
            (Role::Admin, ManageSettings, false),
            (Role::Admin, ManagePermissions, false),
            (Role::Player, ViewPublic, false),
            (Role::Visitor, ManageUsers, true),
        ]);
        assert_eq!(m, PermissionMatrix::default());
    }

    #[test]
    fn a_new_permission_follows_its_default() {
        // Nothing is stored for it, so it keeps the default for every role.
        let m = PermissionMatrix::from_overrides([(Role::Player, SubmitFlag, false)]);
        assert!(m.can(Role::Referee, ViewHiddenScoreboard));
        assert!(!m.can(Role::Player, SubmitFlag));
    }

    #[test]
    fn scoreboard_visibility() {
        assert!(scoreboard_visible(true, false));
        assert!(scoreboard_visible(false, true));
        assert!(!scoreboard_visible(false, false));
    }

    #[test]
    fn role_str_and_serde() {
        for r in Role::ALL {
            assert_eq!(r.as_str().parse::<Role>(), Ok(r));
        }
        for p in Permission::ALL {
            assert_eq!(p.as_str().parse::<Permission>(), Ok(p));
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.as_str())
            );
        }
        assert!("root".parse::<Role>().is_err());
        assert_eq!(serde_json::to_string(&Role::Admin).unwrap(), "\"admin\"");
    }
}
