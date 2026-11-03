//! What a challenge asks the runtime to start, and the limits an organizer allows.

use serde::{Deserialize, Serialize};

use crate::error::SpecError;

const MAX_IMAGE_LEN: usize = 200;
const MAX_ENV_ENTRIES: usize = 32;
const MAX_ENV_VALUE: usize = 1024;

/// Hard ceilings set by the platform, applied to every challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCaps {
    pub max_cpu_millis: u32,
    pub max_memory_mb: u32,
    pub max_pids: u32,
    pub max_ttl_secs: i64,
}

impl Default for ResourceCaps {
    fn default() -> Self {
        Self {
            max_cpu_millis: 4000,
            max_memory_mb: 2048,
            max_pids: 512,
            max_ttl_secs: 4 * 3600,
        }
    }
}

/// One container, as configured on a challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceSpec {
    pub image: String,
    /// TCP port the service listens on inside the container.
    pub port: u16,
    pub cpu_millis: u32,
    pub memory_mb: u32,
    pub pids: u32,
    pub ttl_secs: i64,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    #[serde(default = "yes")]
    pub read_only: bool,
}

fn yes() -> bool {
    true
}

impl InstanceSpec {
    pub fn validate(&self, caps: &ResourceCaps) -> Result<(), SpecError> {
        valid_image(&self.image)?;
        if self.port == 0 {
            return Err(SpecError::Port);
        }
        within(
            "cpu_millis",
            self.cpu_millis as i64,
            caps.max_cpu_millis as i64,
        )?;
        within(
            "memory_mb",
            self.memory_mb as i64,
            caps.max_memory_mb as i64,
        )?;
        within("pids", self.pids as i64, caps.max_pids as i64)?;
        within("ttl_secs", self.ttl_secs, caps.max_ttl_secs)?;
        if self.env.len() > MAX_ENV_ENTRIES {
            return Err(SpecError::Env("too many variables".into()));
        }
        for (key, value) in &self.env {
            valid_env(key, value)?;
        }
        Ok(())
    }
}

fn within(field: &'static str, value: i64, max: i64) -> Result<(), SpecError> {
    if value < 1 || value > max {
        Err(SpecError::Limit { field, max })
    } else {
        Ok(())
    }
}

/// Image references only: no leading dash (it would read as a command line option) and no
/// whitespace or shell characters.
fn valid_image(image: &str) -> Result<(), SpecError> {
    let ok = !image.is_empty()
        && image.len() <= MAX_IMAGE_LEN
        && !image.starts_with(['-', '.', '/', ':'])
        && image
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._/:@-".contains(c));
    if ok { Ok(()) } else { Err(SpecError::Image) }
}

fn valid_env(key: &str, value: &str) -> Result<(), SpecError> {
    let mut chars = key.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_uppercase() || c == '_');
    if !first_ok
        || key.len() > 64
        || !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(SpecError::Env(format!("bad name {key}")));
    }
    if value.len() > MAX_ENV_VALUE || value.contains('\0') {
        return Err(SpecError::Env(format!("bad value for {key}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> InstanceSpec {
        InstanceSpec {
            image: "ghcr.io/acme/web-1:1.0".into(),
            port: 8000,
            cpu_millis: 500,
            memory_mb: 256,
            pids: 128,
            ttl_secs: 1800,
            env: vec![("FLAG".into(), "KYMO{x}".into())],
            read_only: true,
        }
    }

    #[test]
    fn a_normal_spec_is_valid() {
        assert_eq!(spec().validate(&ResourceCaps::default()), Ok(()));
    }

    #[test]
    fn images_cannot_look_like_options_or_contain_shell_characters() {
        for bad in ["", "-v", "a b", "img;rm", "$(id)", "UPPER", "/abs", "x\n"] {
            let mut s = spec();
            s.image = bad.into();
            assert_eq!(
                s.validate(&ResourceCaps::default()),
                Err(SpecError::Image),
                "{bad:?}"
            );
        }
        let mut long = spec();
        long.image = "a".repeat(201);
        assert_eq!(
            long.validate(&ResourceCaps::default()),
            Err(SpecError::Image)
        );
    }

    #[test]
    fn limits_are_positive_and_capped() {
        let caps = ResourceCaps::default();
        for tweak in [
            |s: &mut InstanceSpec| s.cpu_millis = 0,
            |s: &mut InstanceSpec| s.cpu_millis = 4001,
            |s: &mut InstanceSpec| s.memory_mb = 2049,
            |s: &mut InstanceSpec| s.pids = 513,
            |s: &mut InstanceSpec| s.ttl_secs = 0,
            |s: &mut InstanceSpec| s.ttl_secs = 4 * 3600 + 1,
        ] {
            let mut s = spec();
            tweak(&mut s);
            assert!(matches!(s.validate(&caps), Err(SpecError::Limit { .. })));
        }
        let mut edge = spec();
        edge.cpu_millis = 4000;
        edge.ttl_secs = 4 * 3600;
        assert_eq!(edge.validate(&caps), Ok(()));
    }

    #[test]
    fn port_zero_is_refused() {
        let mut s = spec();
        s.port = 0;
        assert_eq!(s.validate(&ResourceCaps::default()), Err(SpecError::Port));
    }

    #[test]
    fn env_names_and_values_are_checked() {
        let caps = ResourceCaps::default();
        for (k, v) in [("lower", "x"), ("1A", "x"), ("A-B", "x"), ("A", "nul\0")] {
            let mut s = spec();
            s.env = vec![(k.into(), v.into())];
            assert!(matches!(s.validate(&caps), Err(SpecError::Env(_))), "{k}");
        }
        let mut many = spec();
        many.env = (0..33).map(|i| (format!("K{i}"), "v".into())).collect();
        assert!(matches!(many.validate(&caps), Err(SpecError::Env(_))));
    }
}
