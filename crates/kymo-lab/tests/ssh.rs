//! The SSH writer against an in process SSH server: it must connect, log in with the key first
//! and the password only when the key is refused, write each file through standard input, and
//! never leak a flag value or a password into an error.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use kymo_lab::inject::FileWrite;
use kymo_lab::ssh::{SshTarget, write_files};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Msg, Server as _, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

#[derive(Default)]
struct Seen {
    logins: Vec<String>,
    commands: Vec<String>,
    inputs: Vec<Vec<u8>>,
}

#[derive(Clone)]
struct Fake {
    seen: Arc<Mutex<Seen>>,
    accept_key: bool,
    password: Option<String>,
    exit_status: u32,
}

impl russh::server::Server for Fake {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl russh::server::Handler for Fake {
    type Error = russh::Error;

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        _key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        self.seen.lock().unwrap().logins.push(format!("key:{user}"));
        Ok(if self.accept_key {
            Auth::Accept
        } else {
            Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        })
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.seen
            .lock()
            .unwrap()
            .logins
            .push(format!("password:{user}"));
        Ok(if self.password.as_deref() == Some(password) {
            Auth::Accept
        } else {
            Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        })
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let mut seen = self.seen.lock().unwrap();
        seen.commands
            .push(String::from_utf8_lossy(data).into_owned());
        seen.inputs.push(Vec::new());
        session.channel_success(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(last) = self.seen.lock().unwrap().inputs.last_mut() {
            last.extend_from_slice(data);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.exit_status_request(channel, self.exit_status)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

async fn server(
    accept_key: bool,
    password: Option<&str>,
    exit_status: u32,
) -> (u16, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mut fake = Fake {
        seen: Arc::clone(&seen),
        accept_key,
        password: password.map(str::to_owned),
        exit_status,
    };
    let config = Arc::new(russh::server::Config {
        keys: vec![PrivateKey::random(&mut rand010::rng(), Algorithm::Ed25519).unwrap()],
        auth_rejection_time: Duration::from_millis(0),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..Default::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = fake.run_on_socket(config, &listener).await;
    });
    (port, seen)
}

fn key_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("verify.key");
    PrivateKey::random(&mut rand010::rng(), Algorithm::Ed25519)
        .unwrap()
        .write_openssh_file(&path, LineEnding::LF)
        .unwrap();
    path
}

fn files() -> Vec<FileWrite> {
    vec![
        FileWrite {
            path: "/home/ctf/user.txt".into(),
            owner: Some("ctf:ctf".into()),
            mode: "0640".into(),
            value: "KYMO{user_9f3a}".into(),
        },
        FileWrite {
            path: "/root/root.txt".into(),
            owner: None,
            mode: "0600".into(),
            value: "KYMO{root_44be}".into(),
        },
    ]
}

fn target(port: u16, key: Option<std::path::PathBuf>, password: Option<&str>) -> SshTarget {
    SshTarget {
        host: "127.0.0.1".into(),
        port,
        user: "root".into(),
        key,
        password: password.map(str::to_owned),
        sudo: false,
    }
}

#[tokio::test]
async fn the_key_is_used_first_and_each_flag_goes_through_standard_input() {
    let dir = tempfile::tempdir().unwrap();
    let (port, seen) = server(true, Some("never-used"), 0).await;
    let t = target(port, Some(key_file(&dir)), Some("never-used"));
    write_files(&t, &files(), Duration::from_secs(10))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.logins,
        vec!["key:root".to_owned()],
        "the password was not tried"
    );
    assert_eq!(seen.commands.len(), 2);
    assert!(seen.commands[0].contains("cat > /home/ctf/user.txt"));
    assert!(seen.commands[0].contains("chown ctf:ctf /home/ctf/user.txt"));
    assert!(seen.commands[1].contains("chmod 0600 /root/root.txt"));
    assert_eq!(seen.inputs[0], b"KYMO{user_9f3a}");
    assert_eq!(seen.inputs[1], b"KYMO{root_44be}");
    for command in &seen.commands {
        assert!(!command.contains("KYMO"), "no flag in a command line");
    }
}

#[tokio::test]
async fn a_refused_key_falls_back_to_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let (port, seen) = server(false, Some("s3cret"), 0).await;
    let t = target(port, Some(key_file(&dir)), Some("s3cret"));
    write_files(&t, &files(), Duration::from_secs(10))
        .await
        .unwrap();
    let logins = seen.lock().unwrap().logins.clone();
    assert_eq!(logins[0], "key:root");
    assert!(logins.contains(&"password:root".to_owned()));
}

#[tokio::test]
async fn a_password_alone_works() {
    let (port, seen) = server(false, Some("s3cret"), 0).await;
    write_files(
        &target(port, None, Some("s3cret")),
        &files(),
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert_eq!(
        seen.lock().unwrap().logins,
        vec!["password:root".to_owned()]
    );
}

#[tokio::test]
async fn a_wrong_password_fails_and_the_error_has_no_secret() {
    let (port, _) = server(false, Some("right"), 0).await;
    let err = write_files(
        &target(port, None, Some("wrong-one")),
        &files(),
        Duration::from_secs(3),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("password"), "{err}");
    assert!(!err.contains("wrong-one") && !err.contains("KYMO"), "{err}");
}

#[tokio::test]
async fn a_failing_write_is_an_error() {
    let (port, _) = server(false, Some("pw"), 1).await;
    let err = write_files(
        &target(port, None, Some("pw")),
        &files(),
        Duration::from_secs(3),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("exit status 1"), "{err}");
    assert!(!err.contains("KYMO"), "{err}");
}

#[tokio::test]
async fn a_guest_that_never_answers_gives_up_after_the_patience() {
    // A port nothing listens on.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let started = std::time::Instant::now();
    let err = write_files(
        &target(port, None, Some("pw")),
        &files(),
        Duration::from_secs(3),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(started.elapsed() < Duration::from_secs(12));
    assert!(err.contains("connect") || err.contains("answer"), "{err}");
}

#[tokio::test]
async fn an_unreadable_key_is_reported_without_its_path_contents() {
    let (port, _) = server(true, None, 0).await;
    let err = write_files(
        &target(port, Some("/does/not/exist.key".into()), None),
        &files(),
        Duration::from_secs(3),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("key"), "{err}");
}
