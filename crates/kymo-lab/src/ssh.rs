//! Writes the flags of a run into a virtual machine over SSH. The lab host connects as the
//! account the manifest names, with its key first and its password if the key is not accepted,
//! and writes each file through standard input, so a flag value never appears in a command line
//! or a process list. Paths, owners and modes were checked when the manifest loaded and only
//! contain safe characters, so they are placed into the command as they are.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handler};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::time::{Instant, sleep, timeout};

use crate::inject::FileWrite;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const STEP_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_EVERY: Duration = Duration::from_secs(2);

/// How to reach the guest.
#[derive(Debug, Clone)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// A private key file. Tried first when both a key and a password are declared.
    pub key: Option<PathBuf>,
    pub password: Option<String>,
    /// Run the write through `sudo -n`.
    pub sudo: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("{0}")]
    Failed(String),
}

fn failed(message: impl Into<String>) -> SshError {
    SshError::Failed(message.into())
}

/// The guest was built from a known image on a private network, so the first host key is taken
/// as it is. This is a deliberate choice, written in the design.
struct AcceptAny;

impl Handler for AcceptAny {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Writes every file, retrying while the guest is still coming up, until `patience` runs out.
/// The error names the last problem and never carries a flag value or a password.
pub async fn write_files(
    target: &SshTarget,
    files: &[FileWrite],
    patience: Duration,
) -> Result<(), SshError> {
    let give_up = Instant::now() + patience;
    loop {
        let why = match timeout(STEP_TIMEOUT * 4, try_write(target, files)).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(SshError::Failed(why))) => why,
            Err(_) => "writing the flags took too long".into(),
        };
        if Instant::now() + RETRY_EVERY >= give_up {
            return Err(failed(why));
        }
        sleep(RETRY_EVERY).await;
    }
}

async fn try_write(target: &SshTarget, files: &[FileWrite]) -> Result<(), SshError> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(STEP_TIMEOUT),
        ..client::Config::default()
    });
    let address = (target.host.as_str(), target.port);
    let mut session = timeout(CONNECT_TIMEOUT, client::connect(config, address, AcceptAny))
        .await
        .map_err(|_| failed("the guest did not answer on its SSH port"))?
        .map_err(|e| failed(format!("could not connect: {e}")))?;
    authenticate(&mut session, target).await?;
    for file in files {
        write_one(&mut session, target, file).await?;
    }
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(())
}

async fn authenticate(
    session: &mut client::Handle<AcceptAny>,
    target: &SshTarget,
) -> Result<(), SshError> {
    if let Some(path) = &target.key {
        let key = load_secret_key(path, None)
            .map_err(|_| failed("the SSH key in the manifest cannot be read"))?;
        let hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| failed(format!("SSH negotiation failed: {e}")))?
            .flatten();
        let result = session
            .authenticate_publickey(
                target.user.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            )
            .await
            .map_err(|e| failed(format!("SSH login failed: {e}")))?;
        if result.success() {
            return Ok(());
        }
        if target.password.is_none() {
            return Err(failed("the guest did not accept the SSH key"));
        }
    }
    if let Some(password) = &target.password {
        let result = session
            .authenticate_password(target.user.clone(), password.clone())
            .await
            .map_err(|e| failed(format!("SSH login failed: {e}")))?;
        if result.success() {
            return Ok(());
        }
        return Err(failed("the guest did not accept the SSH password"));
    }
    Err(failed("no SSH credentials are declared"))
}

/// The shell script for one file. Every part was checked against a strict character set.
fn script(file: &FileWrite) -> String {
    let dir = file
        .path
        .rsplit_once('/')
        .map_or("/", |(dir, _)| if dir.is_empty() { "/" } else { dir });
    let mut script = format!(
        "umask 077; mkdir -p {dir} && cat > {path} && chmod {mode} {path}",
        path = file.path,
        mode = file.mode
    );
    if let Some(owner) = &file.owner {
        script.push_str(&format!(" && chown {owner} {}", file.path));
    }
    script
}

fn command(target: &SshTarget, file: &FileWrite) -> String {
    let inner = script(file);
    if target.sudo {
        format!("sudo -n sh -c \"{inner}\"")
    } else {
        format!("sh -c \"{inner}\"")
    }
}

async fn write_one(
    session: &mut client::Handle<AcceptAny>,
    target: &SshTarget,
    file: &FileWrite,
) -> Result<(), SshError> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| failed(format!("could not open a channel: {e}")))?;
    channel
        .exec(true, command(target, file))
        .await
        .map_err(|e| failed(format!("could not run the write: {e}")))?;
    channel
        .data(file.value.as_bytes())
        .await
        .map_err(|e| failed(format!("could not send the flag: {e}")))?;
    channel
        .eof()
        .await
        .map_err(|e| failed(format!("could not finish the write: {e}")))?;
    let mut status = None;
    while let Some(message) = timeout(STEP_TIMEOUT, channel.wait())
        .await
        .map_err(|_| failed("the guest stopped answering during the write"))?
    {
        if let ChannelMsg::ExitStatus { exit_status } = message {
            status = Some(exit_status);
        }
    }
    match status {
        Some(0) => Ok(()),
        Some(code) => Err(failed(format!(
            "writing {} failed on the guest (exit status {code})",
            file.path
        ))),
        None => Err(failed(format!("writing {} did not finish", file.path))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(owner: Option<&str>) -> FileWrite {
        FileWrite {
            path: "/home/ctf/user.txt".into(),
            owner: owner.map(str::to_owned),
            mode: "0640".into(),
            value: "KYMO{secret}".into(),
        }
    }

    #[test]
    fn the_script_makes_the_folder_writes_from_stdin_and_sets_mode_and_owner() {
        let text = script(&file(Some("ctf:ctf")));
        assert_eq!(
            text,
            "umask 077; mkdir -p /home/ctf && cat > /home/ctf/user.txt && chmod 0640 /home/ctf/user.txt && chown ctf:ctf /home/ctf/user.txt"
        );
        assert!(
            !text.contains("KYMO"),
            "the value never goes into the command"
        );
        assert!(!script(&file(None)).contains("chown"));
    }

    #[test]
    fn a_file_in_the_root_folder_uses_slash() {
        let mut f = file(None);
        f.path = "/flag.txt".into();
        assert!(script(&f).starts_with("umask 077; mkdir -p / && cat > /flag.txt"));
    }

    #[test]
    fn sudo_wraps_the_whole_script() {
        let mut t = SshTarget {
            host: "10.77.0.42".into(),
            port: 22,
            user: "ctf".into(),
            key: None,
            password: Some("x".into()),
            sudo: true,
        };
        assert!(command(&t, &file(None)).starts_with("sudo -n sh -c \"umask 077;"));
        t.sudo = false;
        assert!(command(&t, &file(None)).starts_with("sh -c \"umask 077;"));
    }
}
