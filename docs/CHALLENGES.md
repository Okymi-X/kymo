# Challenge guide

What an organizer needs to know to build challenges: coding challenges that are judged automatically, dynamic flags so every player gets their own, Responder challenges where players patch a machine, and how Kymo helps catch cheating. Start with "What each kind of challenge needs" in the [lab host guide](LAB_HOST.md#what-each-kind-of-challenge-needs).

[Back to the README](../README.md)

## Coding challenges

A coding challenge needs a lab host of version 0.5.0 or later. It gives players a text box, a language and a set of tests. They write a program that reads standard input and prints standard output; Kymo runs it against every test on a lab host and the challenge is solved when all of them pass. It is scored like any other challenge (fixed or dynamic points, attempt limit, hints).

**Languages come from the lab host.** A language is an image plus a manifest with a `judge` block. The platform only names the manifest's artifact, so it can never choose an image or a command. Samples are in `lab/judge/` (Python, Node, C, C++) and `deploy/lab/manifests/judge-*.json.example`:

```json
{
  "artifact": "localhost/kymo-judge/cpp:1",
  "runtime": "docker",
  "judge": {
    "label": "C++ (g++)",
    "file": "main.cpp",
    "compile": ["g++", "-O2", "-std=c++20", "-o", "main", "main.cpp"],
    "run": ["./main"]
  }
}
```

Build the image on the lab host (`docker build -t localhost/kymo-judge/cpp:1 lab/judge/cpp`), copy the manifest to `/etc/kymo-lab/manifests/`, restart, and the language shows up in the challenge editor. An interpreted language leaves `compile` out; the official `python` and `node` images work as they are. Run Python with `-u` (unbuffered): a program killed at its time limit otherwise loses everything it printed, and the player sees an empty output.

**The author** sets the languages, the tests (input and expected output; up to five can be shown to players as examples, the rest stay hidden), the time and memory per test and how output is compared (by line, by word, exactly). *Try a reference solution* runs a program against the saved tests, hidden ones included, and records nothing. A coding challenge cannot be published before it has tests, and its event needs a lab host.

**What keeps it safe.** Every run is a disposable container: no network, a read-only root, all capabilities dropped, `no-new-privileges`, an unprivileged user, and hard limits on memory, CPU, processes, open files, file size and output. The program is killed at its time limit, and a program that hits a limit is not run on the remaining tests. The container is always removed, a crashed lab host removes leftovers when it restarts, and the source lives in a private directory that is emptied after each run. A player can run one submission at a time; a compile error does not use an attempt.

On the lab host, `KYMO_LAB_JUDGE_DIR` is where submissions are unpacked (default `/var/lib/kymo-lab/judge`) and `KYMO_LAB_JUDGE_PARALLEL` how many run at once (default 2). A busy lab host answers "try again in a moment" instead of queueing without end.

Submissions run in containers with no network, no capabilities, a read-only root, a stricter seccomp profile than Docker's default (no `ptrace`, `process_vm_readv` or `process_vm_writev`) and a private IPC namespace. They still share the host kernel. For a public event set `KYMO_LAB_JUDGE_RUNTIME` to a sandboxed OCI runtime the Docker daemon knows, such as `runsc` (gVisor) or a Kata runtime, and keep `KYMO_LAB_JUDGE_DIR` a generic directory: a submission can read its own mount path.

## Dynamic flags and several flags

By default a challenge has one static flag that every player submits. Two options change that, both for **instance challenges** and both chosen per challenge in the *New challenge* form:

- **Dynamic flags.** Kymo generates a different flag for every instance run: the *prefix* and *closing text* you type, with a random hex part (4 to 32 characters, default 8) between them, for example `NDH{b2r_user_9f3a1c7d}`. The flag is handed to the lab host in the create request and put into the instance, so a flag from another player never works. A wrong flag that was generated for someone else leaves a `flag_shared` note in the audit log, naming both players (never the flag). A flag from any earlier run of the same player or team stays valid after a reset, once per flag.
- **Several flags** (for example `user` and `root` of a boot2root machine). Players use the same flag input for all of them and see a `found/total` count. Each flag has a weight, and the challenge's points are split between the flags by weight (static points only). Hint costs are taken from the awards in the order the flags are found. The challenge counts as solved with its last flag. Static flags work in a multi flag challenge too.

Once a run or a solve exists, the flags of a challenge can no longer change, only their labels. A lab host must be version 0.4.0 or later to hand out flags; Kymo refuses to start such a challenge on an older one rather than start it without its flag. In tournament matches a challenge can use dynamic flags, several flags and be a Responder: each side gets its own run of the instance and its own flags. Each flag is worth the points the organizer sets on it (1 by default), found once per side, and the match goes to the side with the most points.

### Where the lab host puts the flags

The platform sends the flags; the **manifest** says where each goes, so the platform never knows paths or credentials. Examples are in `deploy/lab/manifests/` (`dynamic-flag.json.example`, `boot2root.json.example`, `responder.json.example`).

```json
"inject": {
  "port": 22,
  "user": "root",
  "ssh_key": "/etc/kymo-lab/keys/boot2root-box.key",
  "ssh_password": "optional, tried only if the key is refused",
  "flags": [
    { "name": "user", "path": "/home/ctf/user.txt", "owner": "ctf:ctf", "mode": "0640" },
    { "name": "root", "path": "/root/root.txt", "owner": "root:root", "mode": "0600" }
  ]
}
```

- **Containers** put a flag in an environment variable (`{ "name": "flag", "env": "FLAG" }`).
- **Virtual machines** get a file written over SSH after the guest answers (VirtualBox exports, `.ova`, `.qcow2` and the other image types work the same). If both a key and a password are declared the key is used first. The value is sent on standard input, never in a command line. The instance reports as starting until every file is written, and is removed if it cannot be done in time, so no player gets a machine without its flag.
- The flag names in the challenge must match the manifest's. A name the manifest has no place for is refused.
- Manifests may hold an SSH password, so the installer makes them readable by the service group only. Keep keys in `/etc/kymo-lab/keys/`.

## Responder challenges and the Verifier

A **Responder** is an instance challenge where the player *patches* a machine instead of attacking it. A **Verifier** checks the repair, and when every check passes it shows the flag, which the player pastes into the usual flag input. Tick *Responder* under *Instance* in the challenge form.

- **Two addresses.** The player sees the machine and the Verifier, both on their own private VPN lab network. Nothing is public and there is no login: only that player's (or team's) tunnel can reach their lab. The Verifier takes a second address from the lab network, allocated by Kymo like the machine's.
- **One Verifier per instance.** The lab host starts a container next to each instance and removes it with the instance. It is read-only, has no capabilities and no route out.
- **The flag stays off the machine.** A Responder has exactly one flag, static or dynamic. The lab host holds it and only the Verifier container can show it, and only while the latest run had every check fixed.
- **You write the checks.** Each challenge has a Verifier definition: a directory with `verifier.json` and the scripts it names, under `/etc/kymo-lab/verifiers/`. A check has an id, the title players see, a language (`bash`, `python` or `exec` for any prebuilt program), a file and function, an optional health function and a timeout. The function's exit status is the answer: 0 fixed, 1 the weakness is still there, 2 broken (a crash or timeout counts as broken). Copy `deploy/lab/verifiers/template` (installed as `/etc/kymo-lab/verifiers/template`) and see `docs/schema/verifier.schema.json`.
- **Name it in the manifest:** `"verifier": { "definition": "ghost-files", "port": 8080 }`.
- **Test it with no player:** `kymo-lab verifier test /etc/kymo-lab/verifiers/ghost-files --target 10.77.0.42 --ports 22,80` shows each check red on a fresh machine and green after your fix. `kymo-lab check` finds mistakes in the definitions and the manifests first.
- **A player with root can lie.** Checks that only read a file over SSH can be fooled by a determined player. Prefer checks that repeat the original attack from the Verifier and expect it to fail.
- **See what happened.** On the event's *Instances* tab, *Details* shows the flags generated for a run (the reading is audited) and what the Verifier recorded.

The lab host needs Docker, the VPN gateway and the Verifier image (the installer builds it). The installer also adds a firewall rule that keeps the lab networks away from the lab host itself, because a player may have root in a challenge machine on a bridge whose address is that host. The full design is in `docs/DYNAMIC_FLAGS_AND_RESPONDER.md`.

## Integrity and disqualification

Kymo reports suspicion and leaves the decision to an organizer.

- **Flag sharing.** With dynamic flags each player or team has their own value. Submitting another participant's flag is recorded as sharing, and the player is told so.
- **Copy detection.** Accepted coding solutions are fingerprinted, so renamed variables and changed spacing do not hide a copy. Pairs of different participants above a similarity threshold appear under Integrity in the event, with both programs side by side, and can be dismissed or acted on.
- **Disqualify.** An organizer can remove a participant from one event with a reason. They leave the scoreboard and cannot play it any more, they are told why, and their account stays. It can be reversed. Every disqualification, dismissal and flag read is in the audit log.
