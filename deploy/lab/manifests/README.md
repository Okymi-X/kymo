# Sample manifests

One example manifest per artifact kind the lab host supports. Copy the one that matches, rename
it to the manifest's own name, and edit it (`cp boot2root.json.example /etc/kymo-lab/manifests/my-box.json`).
Every field is explained by its own `_comment` (and `_comment_<field>` where one field needs more
detail). `kymo-lab check` validates a manifest after you install it.

| File | What it is for | Runtime |
| --- | --- | --- |
| `boot2root.json.example` | A virtual machine with two flags (user and root) written over SSH when a player starts it, HackTheBox style | `vm` |
| `container-with-login.json.example` | A container that needs a real login: sshd, sudo, a few services, with capabilities given back only for this image | `docker` |
| `vm-example.json.example` | A virtual machine attacked like any other box, no flag injection | `vm` |
| `vm-container-flag.json.example` | A virtual machine that runs a Docker container inside it, flags delivered into that container | `vm` |
| `dynamic-flag.json.example` | A container that receives its flag as the `FLAG` environment variable at start | `docker` |
| `flag-server.json.example` | A container whose flag is a file already in the image (the sample `lab/flag-server` image) | `docker` |
| `page-source.json.example` | A container serving a static page, the flag found in the page source or a linked file | `docker` |
| `tcp-banner.json.example` | A container with one raw TCP service, answered with a client such as `nc` | `docker` |
| `judge-c.json.example`, `judge-cpp.json.example`, `judge-node.json.example`, `judge-python.json.example` | One image per language for a coding challenge (a `judge` block: compile, run, timeouts) | `docker` |
| `responder.json.example` | A machine players must fix, with a Verifier that releases the flag once every check passes | `vm` or `docker` |

Missing for now: a Windows virtual machine example. Everything above targets a Linux target or a
Linux-only language image; a Windows `.ova` manifest (RDP or WinRM delivery, a Windows-shaped
`inject` block) is planned for later.
