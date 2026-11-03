use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Visitor,
    Player,
    Referee,
    Monitor,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

impl Permission {
    pub const ALL: [Permission; 15] = [
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
    ];
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

    pub fn can(self, permission: Permission) -> bool {
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
                ViewPublic | ViewScoreboardPublic | PlayEvent | Referee
            ),
            Role::Monitor => matches!(permission, ViewPublic | Monitor | ModerateBans | ViewAudit),
        }
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

/// Admins and referees always see the scoreboard, others only once published.
pub fn scoreboard_visible(published: bool, role: Role) -> bool {
    published || matches!(role, Role::Admin | Role::Referee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::*;

    fn granted(role: Role) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|p| role.can(*p))
            .collect()
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
            vec![ViewPublic, ViewScoreboardPublic, PlayEvent, Referee]
        );
        assert!(!Role::Referee.can(SubmitFlag));
        assert!(!Role::Referee.can(StartInstance));
        assert!(!Role::Referee.can(ManageTeam));
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
        for r in Role::ALL {
            assert!(r.can(ViewPublic));
        }
    }

    #[test]
    fn scoreboard_visibility() {
        for r in Role::ALL {
            assert!(scoreboard_visible(true, r));
            let expected = matches!(r, Role::Admin | Role::Referee);
            assert_eq!(scoreboard_visible(false, r), expected, "{r:?}");
        }
    }

    #[test]
    fn role_str_and_serde() {
        for r in Role::ALL {
            assert_eq!(r.as_str().parse::<Role>(), Ok(r));
        }
        assert!("root".parse::<Role>().is_err());
        assert_eq!(serde_json::to_string(&Role::Admin).unwrap(), "\"admin\"");
        assert_eq!(
            serde_json::to_string(&ViewScoreboardPublic).unwrap(),
            "\"view_scoreboard_public\""
        );
    }
}
