# Lab host guide

How challenge machines run: on this server with Docker or Podman, or on a separate **lab host** that runs containers and virtual machines for the platform. It covers what each kind of challenge needs, the manifest files, installing a lab host, its configuration, the lab API and its security. Sample manifests are in [`deploy/lab/manifests/`](../deploy/lab/manifests).

[Back to the README](../README.md)

## Challenge instances with Docker or Podman

Set `KYMO_INSTANCES=docker`, `KYMO_INSTANCE_HOST` (what players connect to) and `KYMO_INSTANCE_BIND` (where container ports are published; `127.0.0.1` for a local test). The server talks to the Docker API. With Docker, your user needs access to its socket (the `docker` group). With rootless Podman, run `systemctl --user enable --now podman.socket` and set `DOCKER_HOST=unix:///run/user/$UID/podman/podman.sock`.

An instance is shown to the player as *starting* until its port really accepts connections (checked through the gateway for a private lab); if it never does, or the container later exits, it is stopped, marked failed with a reason and does not use the player's start budget. `lab/` holds three tiny example challenge images (busybox web servers and a raw TCP service). Build them with `podman build -t localhost/kymo-lab/flag-server:1 lab/flag-server` (or `docker build`), and likewise `page-source`, then point a challenge's instance image at them. To check the runtime against a real daemon: `KYMO_TEST_IMAGE=localhost/kymo-lab/flag-server:1 cargo test -p kymo-instances --features docker --test docker`.

## Separate lab host

By default the machine that runs Kymo also runs the challenge containers. A challenge is software that players attack, so if one is broken into, the attacker is on the machine that holds your accounts, flags and database. A **lab host** moves the containers to another machine. The platform asks it, over an authenticated and encrypted API, to start and stop them, and an event chooses where its instances run.

```
players ──► platform (web, accounts, scores)  ──── HTTPS + token ────►  lab host (Docker, challenge images)
   └──────────────────── connect to the instance on the lab host's public address ───────────────────►
```

- **Same host** (the default): the event uses this machine's own runtime and address.
- **Separate host**: add the lab host in the admin pages (Lab hosts) and pick it in the event's *Where instances run*. Players are told the lab host's address.
- **Lab hosts are global**: you add each lab host once (Lab hosts page) and it is available to every event. Each Jeopardy event then picks where its instances run (this server or any lab host), so different events can use different hosts. Any number of lab hosts. An event on a lab host can be moved only while none of its instances run.
- **Per event or per challenge.** An event picks one host for all its instances (*Instances run on*). A single challenge can override it (*Runs on* in the challenge editor), so different challenges of the same event run on different lab hosts; "Same as the event" follows the event again. The event's network mode has to work on every host it uses (a VPN event needs a gateway on each), and a challenge moves only while none of its instances run. Players are told which host a challenge uses, so they fetch that host's VPN profile when they start it.
- **Choosing the host is the organizer's call.** Pick it when you create a Jeopardy event (*Instances run on*) or later on the event's Overview. Each choice shows how busy that host is (events using it, instances running), so you can put several events on one host that has resources to spare, or give a big event a host of its own. A host never takes more than its `KYMO_LAB_MAX_INSTANCES` at once (further starts get "the lab host is full") and clamps every instance to its memory and CPU caps, so events sharing a host cannot starve it.
- **Solo and team events** behave the same; only the owner of an instance changes.
- **Private VPN on a lab host**: see [Players reach a lab host through its VPN](#players-reach-a-lab-host-through-its-vpn). With it nothing vulnerable is published on the internet.

### What each kind of challenge needs

Most challenges need nothing from a lab host. Only a challenge that runs something does. This is the whole list, so you know what to prepare before an event:

| Challenge | What you prepare | On the lab host | Manifest |
| --- | --- | --- | --- |
| **Flag only**, **multi-question** | Files to attach. No lab host. | Nothing | None |
| **Dynamic flag, no instance** | A text file with `{{flag}}` in it, attached as a per-player template | Nothing | None |
| **Container** (Docker or Podman) | A built image: a `Dockerfile` (see `lab/`) | The image, built or pulled there (`docker build -t localhost/ctf/web:1 .`) | `artifact` (the image name), `port`, limits. `deploy/lab/manifests/flag-server.json.example` |
| **Virtual machine** | A disk image: `.ova`, `.vmdk`, `.vdi`, `.qcow2`, `.img` or `.raw` | QEMU installed, the file in `/var/lib/kymo-lab/images/`. KVM makes it fast | `runtime: "vm"`, `file`. `vm-example.json.example` |
| **Dynamic flag in an instance** | Where the flag goes (an environment variable, or a file in the machine) | Same as the container or machine | add `inject`. `dynamic-flag.json.example`, `boot2root.json.example` |
| **Responder** (patch the machine, a Verifier gives the flag) | The machine or container, and a Verifier definition: a folder with `verifier.json` and check scripts | The folder in `/etc/kymo-lab/verifiers/`, the Verifier image | add `verifier`. `responder.json.example` |
| **Coding** | One image per language (`lab/judge/`) | The language images, built or pulled there | `judge` block: source file, compile and run commands. `judge-python.json.example`, `judge-cpp.json.example` |

The artifact name in the manifest is the name a challenge uses, and in `manifest` mode (the default) the lab host runs **only** what a manifest lists. `kymo-lab check` tells you what is missing, and the Lab hosts page shows each manifest's runtime and whether it is ready.

### Players reach a lab host through its VPN

Published ports are the weak point of a CTF server: every instance is a vulnerable machine on an open port, reachable by anyone on the internet who finds it. A lab host avoids that with a **gateway of its own**. It is on by default in the installer (`--no-vpn` turns it off).

```
player ── OpenVPN (UDP 1194 by default) ──► lab host gateway ──► the player's private lab network ──► their instances
                                      (nothing else is open; no instance has a public port)
```

- **One port faces players**: the gateway's UDP port. The platform's API port is for the platform only. Instances, containers and virtual machines alike, sit on a private network per player or team that no one else can reach, and are not published on any port.
- **The platform keeps the authority**: it issues each player's certificate, decides who may reach which lab, and pushes the result to the lab host (`PUT /v1/vpn/files`, attach, firewall rules, kick). The lab host only runs the gateway container and checks every value it is given (paths, network names, address range).
- **One authority per lab host.** A certificate for one lab host opens nothing on another, and a new profile for one host does not revoke your profile for another.
- **Machines are machines.** On a lab network a virtual machine is a tap device on the network's bridge with the address the platform reserved for it (a small built-in DHCP server hands the guest that address). Every port, TCP and UDP, and ping, is open on that one address, so players scan it with `nmap` like a HackTheBox machine; nothing is mapped or listed. A manifest chooses how it is shown with `"display"`: `machine` (just the address, the default for a virtual machine) or `service` (address and port, the default for a container, like a HackTheBox challenge). A container always has its own address on the network and every port it listens on is reachable.
- **Audit**: every instance start, restart and stop (and every failure), and every VPN profile download, is recorded with the player, the address it came from, the challenge, the lab host and the private address given. Monitor, Audit has search (address, player, challenge), a filter fed by what was recorded, and paging.
- **Virtual machines need a rootful Docker** on the lab host and the right to make a network device: the installer's service unit grants exactly `CAP_NET_ADMIN` and `CAP_NET_RAW`. With rootless Podman the lab network's bridge lives inside the user's network namespace, out of reach. Containers work with either. The DHCP exchange is read from a packet socket, so no host firewall rule is needed.
- **Players** open the event, download the profile for it (`POST /vpn/profile?lab_host=ID`, shown on the event page), connect, and start an instance; its private address appears when it answers.
- **VPN only**: a lab host with a gateway refuses to publish a port at all (`KYMO_LAB_VPN_ONLY`, default on), so a mistake in an event's settings cannot expose a machine. The Lab hosts page shows each host's gateway, and the event settings choose the host and the network mode together.
- **Firewall**: open the UDP port printed by the installer (1194 by default) to players and TCP 7400 to the platform. Nothing else needs opening.
- **Platform setup**: `KYMO_VPN=1` works with `KYMO_INSTANCES=labs` (no gateway on the platform itself, so no `KYMO_VPN_HOST` needed). The platform's `KYMO_VPN_LABS` must lie inside the lab host's `KYMO_LAB_VPN_POOL`; the platform says so if it does not.

### Install a lab host

On the new machine (Linux with systemd) download `kymo-lab-vX.Y.Z-linux-x86_64.tar.gz` from the releases page, unpack it and run the installer:

```sh
sudo ./install-lab.sh --public-host 203.0.113.7 --allow-from 198.51.100.10
```

The installer detects the system (Debian, Ubuntu, Fedora, Arch and their derivatives) and installs what is missing: Docker Engine from Docker's own repository on Debian, Ubuntu and Fedora (with the buildx and compose plugins; the distribution's `docker.io` or `moby-engine` is the fallback if the repository cannot be reached) or the `docker` package on Arch, which has no Docker repository, then enabled and started (`--no-docker-install` skips it). It assumes nothing is present on a fresh machine: it checks every program it and the service use (`curl`, `openssl`, `tar`, `ip`, `ss`, `modprobe`, `useradd`, `runuser`, `kill`, `iptables` and others) and installs the missing ones with the right package names for the system, creates an empty Arch package database if needed, restores SELinux labels when it is enforcing, and opens the API and VPN ports in firewalld or ufw when one is active (`--uninstall` closes them again). The rule that keeps players off the host needs `iptables`, so it is always installed with the VPN and QEMU with the UEFI firmware for virtual machines (`--no-vm` skips it). It also loads the `tun` module. Another system gets a clear message to install Docker and QEMU yourself. A rootless Docker is detected and virtual machines are left off, because they need a rootful one.

`--public-host` is the lab host's address as players reach it, `--allow-from` is the platform's address (the API then answers only that address; omit it to allow any address that has the token). The installer creates a service user, a self-signed certificate and a token, installs and starts the `kymo-lab` service, and prints what to enter in the platform:

```
API address:             https://203.0.113.7:7400
Token:                   ...
Certificate fingerprint: A4:7C:...
Public address:          203.0.113.7
```

In the platform open **Lab hosts**, **Add a lab host** and paste those four values. It is saved and tested at once, and the answer says plainly what is wrong if the lab host cannot be reached. Running the installer again upgrades the program and keeps the token, certificate and manifests. `sudo ./install-lab.sh --uninstall [--purge]` removes it.

On a separate lab machine, the VPN keeps the default UDP port 1194. If the platform and lab host
share one machine and 1194 is already occupied, a fresh lab install automatically selects the next
free UDP port and prints it. Rerunning the installer also detects and repairs an existing collision.
`--vpn-port PORT` makes the first-install choice explicit and refuses to overwrite an existing
listener. If the lab host was installed first, configure the platform's `KYMO_VPN_PORT` to a
different port before enabling its local gateway; the lab host's printed port stays unchanged.

Then, on the lab host:

1. **Open the ports.** TCP 7400 (the API) to the platform only, and the VPN gateway's UDP port printed by the installer (1194 by default) to players. That is all: with the VPN no instance is published on any port. Only if you installed with `--no-vpn` do instance ports (Docker's high range, `cat /proc/sys/net/ipv4/ip_local_port_range`) have to be opened to players, and then anyone who reaches the machine can reach them.
2. **Put the challenge images there** (`docker pull`, or `docker build -t localhost/kymo-lab/flag-server:1 lab/flag-server`) and describe each in a **manifest**, then restart the service. `kymo-lab check` verifies the settings, Docker access and manifests.

### Manifests

A manifest is a JSON file in `/etc/kymo-lab/manifests/` that describes one image. In the default `manifest` mode the lab host runs **only** images it has a manifest for, so even a stolen platform token cannot make it run something else:

```json
{
  "artifact": "localhost/kymo-lab/flag-server:1",
  "runtime": "docker",
  "port": 8000,
  "env": { "MODE": "ctf" },
  "env_allow": ["FLAG"],
  "cpu_millis": 500, "memory_mb": 128, "pids": 64, "max_ttl_secs": 3600
}
```

`artifact` is the image reference the challenge uses. The manifest decides the port and limits; the platform can only ask for less. It can set only the environment variables named in `env_allow`. `deploy/lab/manifests/` has examples for the demo images. `KYMO_LAB_MODE=open` accepts any valid image within the caps instead, which is convenient on a private network and weaker.

### Containers that need a real login

A container is started with **no capabilities**, **no new privileges** and a read-only root. That is what keeps a broken-into challenge on a shared machine from reaching the lab host, and it also means `sshd`, `sudo` or a group of services cannot run in it. When a challenge needs them, the organizer can relax it **per image, in the manifest only** (the platform cannot ask for it); see `deploy/lab/manifests/container-with-login.json.example`, which explains each field in comments.

| Manifest field | Meaning |
| --- | --- |
| `cap_add` | Capabilities added back after all are dropped. Only Docker's default set is allowed: `AUDIT_WRITE`, `CHOWN`, `DAC_OVERRIDE`, `FOWNER`, `FSETID`, `KILL`, `MKNOD`, `NET_BIND_SERVICE`, `NET_RAW`, `SETFCAP`, `SETGID`, `SETPCAP`, `SETUID`, `SYS_CHROOT`. Anything else is refused when the manifest loads |
| `no_new_privileges` | `false` lets setuid programs such as `sudo` and `su` work. Default `true` |
| `read_only` | `false` lets the container write to its own filesystem. Default `true` |
| `hostname` | The container's host name (1 to 63 characters: lowercase letters, digits, `-`), instead of Docker's random id. It relaxes nothing |

What stays locked whatever the manifest says: the container is never privileged and has no devices, no host mounts, no Docker socket and no host namespaces; its limits (memory, CPU, processes) apply; it sits on the player's private lab network with no route out; and `SYS_ADMIN`, `NET_ADMIN`, `SYS_PTRACE`, `SYS_MODULE`, `DAC_READ_SEARCH` and the other powerful capabilities cannot be granted at all. `kymo-lab check` prints a warning for every image that relaxes anything. Even so, a container shares the host's kernel, so keep a lab host that runs such images a machine of its own, and prefer a virtual machine (`runtime` `vm`) when you can.

### Configuration of the lab host

The installer writes `/etc/kymo-lab/lab.env`; `deploy/lab/lab.env.example` documents every variable.

| Variable | Default | Purpose |
| --- | --- | --- |
| `KYMO_LAB_TOKEN` | required | At least 24 characters; the platform authenticates with it (`KYMO_LAB_TOKEN_FILE` reads it from a file) |
| `KYMO_LAB_ADDR` | `0.0.0.0:7400` | Where the API listens |
| `KYMO_LAB_NAME` | hostname | Name shown in the platform |
| `KYMO_LAB_MODE` | `manifest` | `manifest` or `open` |
| `KYMO_LAB_RUNTIMES` | what is installed | `docker`, `vm` or both |
| `KYMO_LAB_MAX_INSTANCES` | `100` | Most instances at once; further starts get "the lab host is full" |
| `KYMO_LAB_ALLOW_FROM` | any | Comma separated addresses or CIDR ranges allowed to call the API |
| `KYMO_LAB_TLS` | `on` | `off` serves plain http, only behind a proxy or on a private network |
| `KYMO_LAB_TLS_NAMES` | hostname | Extra names and addresses for the generated certificate |
| `KYMO_LAB_TLS_CERT`, `KYMO_LAB_TLS_KEY` | generated | Use your own certificate and key |
| `KYMO_LAB_BIND` | `0.0.0.0` | Address instance ports are published on (without the VPN) |
| `KYMO_LAB_VPN_HOST` | off | Set it to run the lab host's own VPN gateway: the name or address players connect to |
| `KYMO_LAB_VPN_PORT` | `1194` | The gateway's UDP port |
| `KYMO_LAB_VPN_POOL` | `10.77.0.0/16` | The range lab networks are made from |
| `KYMO_LAB_VPN_ONLY` | `1` | Refuse every instance that is not on a lab network |
| `KYMO_LAB_VPN_IMAGE` | `localhost/kymo/vpn-gateway:1` | The gateway image (the installer builds it from `deploy/vpn-gateway`) |
| `KYMO_LAB_DATA` | `/var/lib/kymo-lab` | State file and certificate |
| `KYMO_LAB_MANIFESTS` | `/etc/kymo-lab/manifests` | Manifest directory |
| `KYMO_LAB_VERIFIERS` | `/etc/kymo-lab/verifiers` | Verifier definitions, one directory per definition |
| `KYMO_LAB_VERIFIER_IMAGE` | `localhost/kymo/verifier:1` | The Verifier container image (the installer builds it) |
| `KYMO_LAB_JUDGE_DIR` | `/var/lib/kymo-lab/judge` | Where coding submissions are unpacked while they run. Keep it a generic directory |
| `KYMO_LAB_JUDGE_PARALLEL` | `2` | How many submissions run at once |
| `KYMO_LAB_JUDGE_RUNTIME` | Docker's default | A sandboxed OCI runtime such as `runsc` (gVisor) or Kata for submissions. See the [challenge guide](CHALLENGES.md#coding-challenges) |
| `KYMO_LAB_NAME_PREFIX` | `kymo-` | Prefix of the containers and networks this lab host creates |
| `KYMO_LAB_VPN_DIR` | `$KYMO_LAB_DATA/vpn` | Where the gateway keeps its certificates and profiles |
| `KYMO_LAB_VPN_BIND_OPTIONS` | empty | Extra mount options for that directory in the gateway container, for example `z` with SELinux |

Commands: `kymo-lab` runs it, `kymo-lab check` verifies the setup, `kymo-lab prepare` unpacks virtual machine images now, `kymo-lab fingerprint` prints the certificate fingerprint, `kymo-lab version`.

### Virtual machines

A lab host can also run whole machines, such as the OVA exports of VulnHub boxes, with QEMU (KVM-accelerated when the host has it). A challenge's instance then gives each player or team a private, pristine copy of the machine, and on the VPN players are given **just its address**: every port is open on it, and they scan it to find what runs there, like a HackTheBox machine. You never list its ports.

Set it up on the lab host:

1. Install QEMU: `sudo ./install-lab.sh` does it by default on Debian, Ubuntu, Fedora and Arch (`--no-vm` skips it; it also adds the service user to the `kvm` group). On a cloud server KVM needs nested virtualization; without `/dev/kvm` machines still run, in plain emulation that is many times slower, and the Lab hosts page says so.
2. Copy the image (`.ova`, `.vmdk`, `.vdi`, `.qcow2`, `.img` or `.raw`) to `/var/lib/kymo-lab/images/`.
3. Describe it in a manifest, `runtime` `vm`:

```json
{
  "artifact": "gaara",
  "runtime": "vm",
  "file": "Gaara.ova",
  "memory_mb": 1024,
  "cpu_millis": 2000,
  "boot_timeout_secs": 240,
  "max_ttl_secs": 3600
}
```

4. Restart the service. The image is unpacked once, in the background (`kymo-lab prepare` does it on the spot; about ten seconds for a 1 GB OVA); until then a start is "try again shortly" and costs the player nothing. `kymo-lab check` shows what is ready.
5. In the challenge, set the instance image to the **artifact name** (`gaara`). The manifest decides the rest (the port in the challenge's instance settings is ignored for a machine). On the VPN nothing else is needed. The machine port range (`KYMO_LAB_VM_PORTS`) only matters for a lab host without a gateway.

| Manifest field | Meaning |
| --- | --- |
| `file` | The image, a plain file name in the images directory |
| `ports` | **Not needed for a machine.** Leave it out and the machine has no listed ports: it is given as an address with everything open, and counts as ready once it answers on the network. List up to 8 services only when `display` is `service`, or for a lab host without a gateway, where they are what gets published (primary first) |
| `memory_mb`, `cpu_millis` | Memory and CPU cores (1000 per core), clamped to the lab host's caps (`KYMO_LAB_MAX_MEMORY_MB`, `KYMO_LAB_MAX_CPUS`) |
| `boot_timeout_secs` | How long the platform waits for the machine to answer (10 to 1800, default 180) |
| `disk` | `sata` (default, works with nearly everything), `ide` or `virtio` (fastest, needs guest drivers) |
| `nic` | `e1000` (default), `virtio` or `rtl8139` |
| `firmware` | `bios` (default, what VirtualBox exports use) or `uefi` |
| `display` | `machine` (players get just the address, every port open on it, no ports in the manifest: the default for a virtual machine) or `service` (address and port, like a HackTheBox challenge: the default for a container, which keeps its `port`) |

How it behaves:

- **Every instance is disposable.** The image is converted once into a base that is never written to, and each instance runs from its own copy-on-write overlay that is deleted when it ends. A player can wreck the machine; the next start is pristine, and starting costs almost nothing.
- **A machine is shown as ready only when its guest is really on the network.** On the VPN the gateway checks the machine's address (it answers ARP once its address is up, whatever it blocks above that), or, when the manifest lists a port, that the port accepts a connection; the platform waits for that (up to `boot_timeout_secs`). Players see "starting" until then. Without a gateway, the lab host asks QEMU whether the guest took a connection on a forwarded port.
- **Isolation.** On a lab network the machine sits on the player's own private network, which has no route to the internet and which no other player can reach. Without a gateway it runs in QEMU's user mode with `restrict=on`: only the listed ports lead in and the guest reaches nothing. Machines run as the unprivileged service user.
- **VirtualBox exports just work.** The network card is placed where VirtualBox puts it, so a guest that configures `enp0s3` (the usual VulnHub case) finds it and gets its address.
- **See what the guest is doing.** In the Lab hosts page, *What is running*, a machine has a **Console** button with a screenshot of its screen. A login prompt means it booted; anything else tells you where it is stuck.
- **Time limits and restarts.** The lab host enforces the machine's time limit itself. Machines survive a restart of the `kymo-lab` service and are found again.
- **Limits today.** The first disk of an image is used; one machine is one instance (no private network between several); CPU is the number of cores given to the guest, not a hard share of the host.

### How the platform and the lab host cooperate

- **The platform builds the name** of every instance (`kymo-c<challenge>-<u|t><owner>-<random>`) and records the run in its database *before* asking the lab host to start it. The lab host validates the name (characters, prefix, not in use) and the request, and refuses anything else.
- **Nothing is shown to a player until the service answers.** The platform connects to the lab host's public address and port, the way a player will, so a firewall in the way is caught too. If it never answers, the run fails with a reason and does not use the player's start budget.
- **Reconciliation.** The platform decides what should run; the lab host knows what does run. Every 30 seconds the platform's reaper removes lab host containers it has no run for (listing first and reading its runs second, so a start under way is never taken for an orphan), and **Clean up** on a lab host card does it on demand. The lab host also enforces expiry on its own clock, so a platform that is down cannot leave containers running, and it remembers its instances across restarts.
- **Isolation on the lab host.** Each instance has its own network and runs with no capabilities, no privilege escalation, a read-only root and CPU, memory and process limits. The lab host labels its containers in its own namespace (`kymo.managed=lab`), so it can share a Docker daemon with a platform without either touching the other's containers.
- **Health.** The platform checks every lab host every 30 seconds. Operations lists them and counts an unreachable one as *degraded*.

### Security

- The API needs the bearer token (compared in constant time); ten wrong tokens from an address lock it out for a minute; `KYMO_LAB_ALLOW_FROM` restricts the source addresses.
- The connection is HTTPS. The installer's certificate is self-signed, so the platform **pins its fingerprint** instead of trusting a certificate authority: nobody can stand in for the lab host, and no domain name is needed. A lab host behind a proxy with an ordinary certificate needs no fingerprint. The platform warns on any unencrypted connection.
- The token is stored in the platform's database (back-ups include it, treat them as secrets) and is never shown again once saved.
- The service user belongs to the `docker` group, which is equivalent to root on that machine. That is why a lab host should be a machine of its own that holds nothing but challenges.

### The lab API

Everything the platform does is available to scripts. JSON over HTTPS, `Authorization: Bearer <token>`, `-k` or `--pinnedpubkey` for the self-signed certificate:

| Request | Does |
| --- | --- |
| `GET /healthz` | Liveness, no token |
| `GET /v1/health` | Version, runtime, mode, load, uptime |
| `GET /v1/artifacts` | The images this lab host will run, with their ports and (for machines) whether the image is ready |
| `POST /v1/instances` | Create and start `{name, instance_id, spec, meta}`; `201` with the instance, `409` for a used name, `400` for a refused request, `503` when full |
| `GET /v1/instances` | Every instance with its state, port, owner metadata and expiry |
| `GET /v1/instances/{id}` | One instance, by name, container id or platform instance id |
| `POST /v1/instances/{id}/restart` | Remove it and start it again under the same name |
| `GET /v1/instances/{id}/screen` | A virtual machine's screen as a PNG |
| `GET /v1/ready?port=N` | Whether the service behind a published port answers (for a machine, whether the guest took a connection) |
| `POST /v1/instances/{id}/stop`, `DELETE /v1/instances/{id}` | Remove it; removing a missing instance succeeds |
| `POST /v1/reconcile` | `{keep: [instance ids], grace_secs}`: remove everything not listed |
| `POST /v1/cleanup` | Remove expired instances now |
| `GET /v1/log` | The lab host's recent actions |

```sh
curl -sk -H "Authorization: Bearer $TOKEN" https://203.0.113.7:7400/v1/instances
```

The platform side is the admin API (`/api/v1/admin/labs`: list, add, update, delete, test, list instances, reconcile, and `PUT /admin/events/{id}/lab` to choose a lab host for an event), and the same operations are in the **Lab hosts** page.
