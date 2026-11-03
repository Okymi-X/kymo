//! Checks the certificates and revocation list against OpenSSL, which OpenVPN uses. Skipped
//! when the `openssl` command is not installed.

use std::path::Path;
use std::process::Command;

use kymo_vpn::{Ca, Revoked};

const NOW: i64 = 1_800_000_000;

fn openssl(args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("openssl").args(args).output().ok()?;
    Some((
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    ))
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.display().to_string()
}

#[test]
fn openssl_accepts_the_chain_for_the_roles_openvpn_checks_and_honours_the_revocation_list() {
    if openssl(&["version"]).is_none() {
        eprintln!("skipped: openssl is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let ca = Ca::load(Ca::generate(NOW, 3650).unwrap()).unwrap();
    let server = ca.issue_server("kymo-gateway", NOW, 365).unwrap();
    let good = ca.issue_client("u1", NOW, 60).unwrap();
    let gone = ca.issue_client("u2", NOW, 60).unwrap();
    let crl = ca
        .crl(
            &[Revoked {
                serial: gone.serial.clone(),
                at: NOW,
            }],
            1,
            NOW,
            365,
        )
        .unwrap();

    let ca_file = write(dir.path(), "ca.crt", ca.cert_pem());
    let server_file = write(dir.path(), "server.crt", &server.cert_pem);
    let good_file = write(dir.path(), "good.crt", &good.cert_pem);
    let gone_file = write(dir.path(), "gone.crt", &gone.cert_pem);
    let crl_file = write(dir.path(), "crl.pem", &crl);
    let at = "-attime";
    let when = NOW.to_string();

    // The chain verifies, for the purpose each side checks.
    for (cert, purpose) in [(&server_file, "sslserver"), (&good_file, "sslclient")] {
        let (ok, out) = openssl(&[
            "verify", at, &when, "-CAfile", &ca_file, "-purpose", purpose, cert,
        ])
        .unwrap();
        assert!(ok && out.contains("OK"), "{cert} for {purpose}: {out}");
    }
    // A client certificate is not a server certificate, and the other way round.
    let (ok, _) = openssl(&[
        "verify",
        at,
        &when,
        "-CAfile",
        &ca_file,
        "-purpose",
        "sslserver",
        &good_file,
    ])
    .unwrap();
    assert!(!ok, "a client certificate must not pass as a server");

    // The revocation list is signed by the authority and names exactly the revoked one.
    let (ok, out) = openssl(&[
        "crl", "-in", &crl_file, "-CAfile", &ca_file, "-noout", "-text",
    ])
    .unwrap();
    assert!(ok, "crl: {out}");
    assert!(
        out.contains("verify OK") || out.contains("Verification"),
        "{out}"
    );
    assert!(
        out.to_lowercase().contains(&gone.serial.to_lowercase()),
        "the revoked serial is listed: {out}"
    );
    assert!(!out.to_lowercase().contains(&good.serial.to_lowercase()));
    let args = |cert: &str| {
        vec![
            "verify".to_owned(),
            at.into(),
            when.clone(),
            "-crl_check".into(),
            "-CAfile".into(),
            ca_file.clone(),
            "-CRLfile".into(),
            crl_file.clone(),
            cert.to_owned(),
        ]
    };
    // `-CRLfile` needs the list to be passed with `-CAfile` too; combine them as OpenVPN does.
    let combined = format!("{}{}", ca.cert_pem(), crl);
    let bundle = write(dir.path(), "bundle.pem", &combined);
    let verify = |cert: &str| {
        let mut a = args(cert);
        let i = a.iter().position(|x| x == "-CAfile").unwrap();
        a[i + 1] = bundle.clone();
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        openssl(&refs).unwrap()
    };
    let (ok, out) = verify(&good_file);
    assert!(ok, "a good certificate passes the revocation check: {out}");
    let (ok, out) = verify(&gone_file);
    assert!(
        !ok && out.contains("revoked"),
        "a revoked certificate fails: {out}"
    );

    // The keys are plain PKCS#8 that OpenVPN's OpenSSL can read, and match their certificates.
    let key_file = write(dir.path(), "good.key", &good.key_pem);
    let (ok, out) = openssl(&["pkey", "-in", &key_file, "-noout", "-check"]).unwrap();
    assert!(ok, "key: {out}");
    let (_, from_key) = openssl(&["pkey", "-in", &key_file, "-pubout"]).unwrap();
    let (_, from_cert) = openssl(&["x509", "-in", &good_file, "-noout", "-pubkey"]).unwrap();
    assert_eq!(
        from_key.trim(),
        from_cert.trim(),
        "the key belongs to the certificate"
    );
}
