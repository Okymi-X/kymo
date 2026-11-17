//! Puts the flags of a run into a virtual machine over SSH. The lab host connects as the
//! account the manifest names, with its key first and its password if the key is not accepted,
//! and works through one connection: the `before` commands, each flag written to a file or handed
//! to a command, the `after` commands, and last the deletion of every file marked for removal.
//! A flag goes through standard input, so it never appears in a command line or a process list.
//! Paths, owners and modes were checked when the manifest loaded and only contain safe characters,
//! so they are placed into the file write as they are; the administrator's own commands are
//! quoted as one word for the shell.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handler};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::time::{Instant, sleep, timeout};

use crate::inject::{FileWrite, GuestPlan};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const STEP_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// How long one of the administrator's commands may take, for example a service restart.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
/// How much of a failed command's output goes into the error.
const OUTPUT_KEPT: usize = 300;

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
    let plan = GuestPlan {
        files: files.to_vec(),
        ..GuestPlan::default()
    };
    deliver(target, &plan, patience).await
}

/// Does everything in the plan inside the machine. Connecting is retried while the guest is
/// still booting, until `patience` runs out; once connected the steps run exactly once, in
/// order, so a command is never repeated. Files marked for removal are deleted at the end even
/// when a step failed. The error never carries a flag value or a password.
pub async fn deliver(
    target: &SshTarget,
    plan: &GuestPlan,
    patience: Duration,
) -> Result<(), SshError> {
    let give_up = Instant::now() + patience;
    let mut session = loop {
        let why = match timeout(STEP_TIMEOUT * 4, connect(target)).await {
            Ok(Ok(session)) => break session,
            Ok(Err(SshError::Failed(why))) => why,
            Err(_) => "connecting to the guest took too long".into(),
        };
        if Instant::now() + RETRY_EVERY >= give_up {
            return Err(failed(why));
        }
        sleep(RETRY_EVERY).await;
    };
    let secrets = plan.secrets();
    let result = steps(&mut session, target, plan, &secrets).await;
    // The flags must not stay on the disk, so removal runs whatever happened above.
    let removal = remove_files(&mut session, target, plan, &secrets).await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    result?;
    removal
}

async fn connect(target: &SshTarget) -> Result<client::Handle<AcceptAny>, SshError> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(COMMAND_TIMEOUT),
        ..client::Config::default()
    });
    let address = (target.host.as_str(), target.port);
    let mut session = timeout(CONNECT_TIMEOUT, client::connect(config, address, AcceptAny))
        .await
        .map_err(|_| failed("the guest did not answer on its SSH port"))?
        .map_err(|e| failed(format!("could not connect: {e}")))?;
    authenticate(&mut session, target).await?;
    Ok(session)
}

async fn steps(
    session: &mut client::Handle<AcceptAny>,
    target: &SshTarget,
    plan: &GuestPlan,
    secrets: &[&str],
) -> Result<(), SshError> {
    for (n, command) in plan.before.iter().enumerate() {
        run_command(
            session,
            target,
            command,
            None,
            secrets,
            &format!("before command {}", n + 1),
        )
        .await?;
    }
    for file in &plan.files {
        write_one(session, target, file).await?;
    }
    for (n, pipe) in plan.pipes.iter().enumerate() {
        run_command(
            session,
            target,
            &pipe.command,
            Some(pipe.value.as_bytes()),
            secrets,
            &format!("flag command {}", n + 1),
        )
        .await?;
    }
    for (n, command) in plan.after.iter().enumerate() {
        run_command(
            session,
            target,
            command,
            None,
            secrets,
            &format!("after command {}", n + 1),
        )
        .await?;
    }
    Ok(())
}

async fn remove_files(
    session: &mut client::Handle<AcceptAny>,
    target: &SshTarget,
    plan: &GuestPlan,
    secrets: &[&str],
) -> Result<(), SshError> {
    let mut first_error = None;
    for file in plan.files.iter().filter(|f| f.remove) {
        // The path was checked against a strict character set when the manifest loaded.
        let script = format!("rm -f {}", file.path);
        if let Err(e) = run_command(
            session,
            target,
            &script,
            None,
            secrets,
            &format!("removing {}", file.path),
        )
        .await
        {
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
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

/// A string as one word for a POSIX shell: wrapped in single quotes, with any single quote
/// closed, escaped and reopened.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn wrap(target: &SshTarget, script: &str) -> String {
    let inner = format!("sh -c {}", shell_quote(script));
    if target.sudo {
        format!("sudo -n {inner}")
    } else {
        inner
    }
}

/// The last of a command's output with every flag value replaced, cut short, on one line.
fn scrub(output: &[u8], secrets: &[&str]) -> String {
    let mut text = String::from_utf8_lossy(output).into_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        text = text.replace(secret, "<flag>");
    }
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let skip = text.chars().count().saturating_sub(OUTPUT_KEPT);
    text.chars().skip(skip).collect()
}

/// Runs one command of the administrator's through the machine's shell, with `input` on its
/// standard input when given. A non zero exit is an error that carries what the command said,
/// with any flag value scrubbed out.
async fn run_command(
    session: &mut client::Handle<AcceptAny>,
    target: &SshTarget,
    script: &str,
    input: Option<&[u8]>,
    secrets: &[&str],
    what: &str,
) -> Result<(), SshError> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| failed(format!("could not open a channel: {e}")))?;
    channel
        .exec(true, wrap(target, script))
        .await
        .map_err(|e| failed(format!("could not run the {what}: {e}")))?;
    if let Some(input) = input {
        channel
            .data(input)
            .await
            .map_err(|e| failed(format!("could not send the flag to the {what}: {e}")))?;
    }
    channel
        .eof()
        .await
        .map_err(|e| failed(format!("could not finish the {what}: {e}")))?;
    let mut status = None;
    let mut output: Vec<u8> = Vec::new();
    while let Some(message) = timeout(COMMAND_TIMEOUT, channel.wait())
        .await
        .map_err(|_| failed(format!("the {what} took too long")))?
    {
        match message {
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Data { data } => output.extend_from_slice(&data),
            ChannelMsg::ExtendedData { data, .. } => output.extend_from_slice(&data),
            _ => {}
        }
    }
    match status {
        Some(0) => Ok(()),
        Some(code) => {
            let said = scrub(&output, secrets);
            Err(failed(if said.is_empty() {
                format!("the {what} failed on the guest (exit status {code})")
            } else {
                format!("the {what} failed on the guest (exit status {code}): {said}")
            }))
        }
        None => Err(failed(format!("the {what} did not finish"))),
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
            remove: false,
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
    fn a_command_is_one_quoted_word_for_the_shell() {
        assert_eq!(shell_quote("docker restart web"), "'docker restart web'");
        assert_eq!(shell_quote("echo 'hi'"), "'echo '\\''hi'\\'''");
        let t = SshTarget {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            key: None,
            password: Some("x".into()),
            sudo: true,
        };
        assert_eq!(
            wrap(&t, "docker cp /srv/f web:/f; rm /srv/f"),
            "sudo -n sh -c 'docker cp /srv/f web:/f; rm /srv/f'"
        );
    }

    #[test]
    fn output_loses_every_flag_value_and_is_cut_short() {
        let said = scrub(
            b"cat: KYMO{abc}: no such file\nKYMO{abc} again",
            &["KYMO{abc}"],
        );
        assert_eq!(said, "cat: <flag>: no such file <flag> again");
        let long = "x".repeat(1000);
        assert_eq!(scrub(long.as_bytes(), &[]).chars().count(), OUTPUT_KEPT);
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
