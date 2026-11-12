"""Helpers for Kymo Verifier checks written in Python (standard library only).

A check is a function. It ends by calling one of the result functions, or by returning
True (fixed), False (the weakness is still there) or 0, 1 or 2. Returning nothing counts as
broken, because the author most likely forgot to decide.

    fixed()                     the weakness is gone
    vulnerable("message")       the weakness is still there (the message is for the organizer)
    broken("message")           something the check needs is missing
    remote("cmd", ...)          run a command on the target over SSH, returns (status, output)
    http(method, url, ...)      returns (status code, body); status 0 when nothing answered
"""
import os
import subprocess
import sys
import urllib.error
import urllib.request


def fixed():
    sys.exit(0)


def vulnerable(message=""):
    if message:
        print(message)
    sys.exit(1)


def broken(message=""):
    if message:
        print(message)
    sys.exit(2)


def exit_code(result):
    if result is True:
        return 0
    if result is False:
        return 1
    if isinstance(result, int) and result in (0, 1, 2):
        return result
    return 2


def env(name, default=""):
    return os.environ.get(name, default)


def http(method, url, data=None, headers=None, timeout=8):
    request = urllib.request.Request(url, data=data, headers=headers or {}, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read(65536).decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read(65536).decode("utf-8", "replace")
    except Exception:
        return 0, ""


def remote(*command):
    """Runs a command on the target over SSH, key first, then password (needs sshpass)."""
    opts = ["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR",
            "-o", "ConnectTimeout=6", "-p", env("SSH_PORT", "22")]
    run = list(command)
    if env("SSH_SUDO") == "1":
        run = ["sudo", "-n"] + run
    target = "%s@%s" % (env("SSH_USER"), env("SSH_HOST"))
    attempts = []
    if env("SSH_KEY_FILE"):
        attempts.append((["ssh"] + opts + ["-i", env("SSH_KEY_FILE"), target] + run, None))
    if env("SSH_PASSWORD"):
        no_batch = [o.replace("BatchMode=yes", "BatchMode=no") for o in opts]
        attempts.append((["sshpass", "-e", "ssh"] + no_batch + [target] + run,
                         dict(os.environ, SSHPASS=env("SSH_PASSWORD"))))
    status, output = 255, ""
    for argv, environment in attempts:
        try:
            done = subprocess.run(argv, capture_output=True, text=True, timeout=30, env=environment)
        except Exception:
            continue
        status, output = done.returncode, done.stdout
        if status != 255:
            break
    return status, output
