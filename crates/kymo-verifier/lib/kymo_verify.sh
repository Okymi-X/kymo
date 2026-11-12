# Helpers for Kymo Verifier checks written in bash. Source this file is done for you: a check
# only defines functions. Each function ends by calling one of the three result functions, or
# by returning 0 (fixed), 1 (the weakness is still there) or 2 (broken).
#
#   fixed                    the weakness is gone
#   vulnerable "message"     the weakness is still there (the message is for the organizer)
#   broken "message"         something the check needs is missing
#   remote COMMAND...        run a command on the target over SSH (key first, then password)
#   http_status URL [curl options]   print the HTTP status code of a request, 000 if none

fixed() { exit 0; }
vulnerable() { [ -n "$1" ] && echo "$1"; exit 1; }
broken() { [ -n "$1" ] && echo "$1"; exit 2; }

http_status() {
  local url="$1"; shift
  curl -s -o /dev/null -w '%{http_code}' --max-time 8 "$@" "$url" || printf '000'
}

# Prints the command's output and returns its exit status. Without credentials, or when the
# login fails, it returns 255 like ssh does.
remote() {
  local opts=(-o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
              -o LogLevel=ERROR -o ConnectTimeout=6 -p "${SSH_PORT:-22}")
  local run=("$@")
  if [ "${SSH_SUDO:-0}" = 1 ]; then run=(sudo -n "$@"); fi
  if [ -n "${SSH_KEY_FILE:-}" ]; then
    ssh "${opts[@]}" -i "$SSH_KEY_FILE" "${SSH_USER}@${SSH_HOST}" "${run[@]}"
    local code=$?
    if [ "$code" -ne 255 ] || [ -z "${SSH_PASSWORD:-}" ]; then return "$code"; fi
  fi
  if [ -n "${SSH_PASSWORD:-}" ] && command -v sshpass >/dev/null 2>&1; then
    SSHPASS="$SSH_PASSWORD" sshpass -e ssh "${opts[@]/BatchMode=yes/BatchMode=no}" \
      "${SSH_USER}@${SSH_HOST}" "${run[@]}"
    return $?
  fi
  return 255
}
