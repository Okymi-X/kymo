//! How much of the container lock down an image may give up, as a lab host manifest declares
//! it. Kept apart from the Docker runtime so the manifest can be checked anywhere.

/// Capabilities a lab host manifest may give a container: Docker's own default set. Anything
/// else (SYS_ADMIN, NET_ADMIN, SYS_PTRACE, SYS_MODULE, DAC_READ_SEARCH, SYS_RAWIO, SYS_TIME and
/// the rest) is refused, because those are what turn a container escape into a host takeover.
pub const ALLOWED_CAPS: &[&str] = &[
    "AUDIT_WRITE",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "KILL",
    "MKNOD",
    "NET_BIND_SERVICE",
    "NET_RAW",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_CHROOT",
];

/// How much of the usual container lock down one image gives up. The default is all of it: no
/// capabilities and no new privileges. An organizer can relax it per image in the lab host
/// manifest, for a challenge that needs a real login (sshd, sudo, a few services). The platform
/// can never ask for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardening {
    /// Capabilities added back after all are dropped. Only `ALLOWED_CAPS`.
    pub cap_add: Vec<String>,
    /// Whether a process may gain privileges through setuid programs (`sudo`, `su`).
    pub no_new_privileges: bool,
    /// The container's host name, instead of Docker's random one. Not a security setting: it
    /// only changes what the machine calls itself.
    pub hostname: Option<String>,
}

impl Default for Hardening {
    fn default() -> Self {
        Self {
            cap_add: Vec::new(),
            no_new_privileges: true,
            hostname: None,
        }
    }
}

impl Hardening {
    /// Whether every capability is one a manifest may grant, and the host name is a valid one.
    pub fn check(&self) -> Result<(), String> {
        if let Some(name) = &self.hostname {
            let label_ok = !name.is_empty()
                && name.len() <= 63
                && !name.starts_with('-')
                && !name.ends_with('-')
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !label_ok {
                return Err(
                    "hostname is 1 to 63 characters: lowercase letters, digits and -, not starting or ending with -".into(),
                );
            }
        }
        for cap in &self.cap_add {
            if !ALLOWED_CAPS.contains(&cap.as_str()) {
                return Err(format!(
                    "capability {cap} is not allowed; a manifest may grant {}",
                    ALLOWED_CAPS.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// True when any lock down is relaxed.
    pub fn is_relaxed(&self) -> bool {
        !self.cap_add.is_empty() || !self.no_new_privileges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_relaxes_nothing() {
        let h = Hardening::default();
        assert!(h.cap_add.is_empty() && h.no_new_privileges && !h.is_relaxed());
        assert!(h.check().is_ok());
    }

    #[test]
    fn a_host_name_must_be_a_valid_label() {
        let named = |n: &str| Hardening {
            hostname: Some(n.into()),
            ..Hardening::default()
        };
        assert!(named("atlas-intranet").check().is_ok());
        for bad in [
            "",
            "Atlas",
            "a b",
            "-x",
            "x-",
            "a.b",
            "has_underscore",
            &"a".repeat(64),
        ] {
            assert!(named(bad).check().is_err(), "{bad:?}");
        }
        // A host name alone relaxes nothing.
        assert!(!named("atlas-intranet").is_relaxed());
    }

    #[test]
    fn only_the_allowlist_passes() {
        let ok = Hardening {
            cap_add: vec!["SETUID".into(), "SYS_CHROOT".into()],
            no_new_privileges: false,
            hostname: None,
        };
        assert!(ok.check().is_ok() && ok.is_relaxed());
        let bad = Hardening {
            cap_add: vec!["SYS_ADMIN".into()],
            no_new_privileges: true,
            hostname: None,
        };
        assert!(bad.check().unwrap_err().contains("SYS_ADMIN"));
    }
}
