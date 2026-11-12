# Check functions written in bash. Each one ends by calling fixed, vulnerable or broken (or by
# returning 0, 1 or 2). See kymo_verify.sh for the helpers: remote, http_status.
#
# The target is in the environment: TARGET_IP, TARGET_PORTS, and the SSH settings of
# verifier.json (SSH_USER, SSH_KEY_FILE, SSH_PASSWORD).
#
# Prefer checks that repeat the original attack from here and expect it to fail. A player with
# root can make the machine say anything over SSH, but cannot change what happens on the wire.

# The health function runs first. If it fails, the step is Broken, not Fixed: the player broke
# something this step needs (the website itself, a user, a database).
health_web() {
  [ "$(http_status "http://$TARGET_IP/")" = 200 ]
}

# The original weakness: a file that should never be served. Fixed when it is no longer there.
check_exposed_file() {
  case "$(http_status "http://$TARGET_IP/backup/dump.sql")" in
    200) vulnerable "the dump is still served" ;;
    000) broken "the web server did not answer" ;;
    *)   fixed ;;
  esac
}
