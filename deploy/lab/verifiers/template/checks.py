"""Check functions written in Python (standard library only). Each one ends by calling
fixed(), vulnerable() or broken(), or by returning True (fixed) or False (still vulnerable).

The target is in the environment: TARGET_IP, TARGET_PORTS, and the SSH settings of
verifier.json (see env("SSH_USER")). kymo_verify has remote() and http().
"""
import kymo_verify as kv


def health_login():
    # The login page still answers. If not, the player broke the application.
    status, _ = kv.http("GET", "http://%s/login" % kv.env("TARGET_IP"))
    return status == 200


def check_weak_credential():
    # The original weakness: the default password still works.
    body = b"user=admin&password=admin"
    status, _ = kv.http(
        "POST", "http://%s/login" % kv.env("TARGET_IP"), data=body,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    if status == 0:
        kv.broken("no answer")
    if status in (200, 302):
        kv.vulnerable("the default password is still accepted")
    kv.fixed()
