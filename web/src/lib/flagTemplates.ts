// What an author writes so a dynamic flag reaches the player, for each way a challenge is
// delivered. Pure text: the editor shows these as copy-ready blocks next to the flags, with the
// flag names the author chose already filled in.

import type { Delivery } from './delivery'

export interface FlagTemplate {
  id: string
  title: string
  /** Where it goes and what to do with it. */
  note: string
  language: 'json' | 'text' | 'shell' | 'python'
  text: string
}

interface Input {
  delivery: Delivery
  responder: boolean
  names: string[]
}

const envName = (name: string) => `FLAG_${name.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`
const clean = (names: string[]) => {
  const named = names.map((n) => n.trim()).filter(Boolean)
  return named.length > 0 ? named : ['flag']
}

function flagOnly(names: string[]): FlagTemplate[] {
  const lines = names.length === 1 ? [`Recovered secret: {{flag}}`] : names.map((n) => `${n}: {{flag:${n}}}`)
  return [
    {
      id: 'file',
      title: 'A text file with the flag in it',
      note: 'Attach it to the challenge and tick "Per-player template". Each player downloads their own copy, with their own flag in place of the token. It must be a text file: a binary cannot hold a token. The token {{flag}} is the first flag, {{flag:name}} a named one.',
      language: 'text',
      text: ['Backup restored at 02:13.', ...lines, 'End of log.'].join('\n'),
    },
    {
      id: 'make',
      title: 'Make a file that is not plain text',
      note: 'For a log, a config, a page of source or a document you build, write it as text with the token, attach it as a template, and let players do the rest. For an image, archive or binary, hide a static flag in it instead, or serve a container that builds it per player.',
      language: 'text',
      text: '<?php // index.php\n$secret = "{{flag}}";\n// a web page source players download and read',
    },
  ]
}

function container(names: string[]): FlagTemplate[] {
  const flags = names.map((n) => ({ name: n, env: envName(n) }))
  return [
    {
      id: 'manifest',
      title: 'Lab host manifest',
      note: 'Add this to the manifest of the image on the lab host. Kymo passes each flag to the container as an environment variable of that name when it starts it.',
      language: 'json',
      text: JSON.stringify({ artifact: 'localhost/ctf/web:1', runtime: 'docker', port: 8000, inject: { flags } }, null, 2),
    },
    {
      id: 'read',
      title: 'Read it in the challenge',
      note: 'The variable is set when the container starts and is different for every player. Never write it into the image: build the image once without a flag.',
      language: 'python',
      text: names.map((n) => `${n.replace(/\W/g, '_')} = os.environ["${envName(n)}"]`).join('\n'),
    },
    {
      id: 'shell',
      title: 'Or write it to a file at start',
      note: 'For a challenge that reads a flag file, make the entrypoint write it before the service starts.',
      language: 'shell',
      text: ['#!/bin/sh', ...names.map((n) => `printf '%s' "$${envName(n)}" > /srv/${n}.txt`), 'exec "$@"'].join('\n'),
    },
  ]
}

function machine(names: string[]): FlagTemplate[] {
  const flags = names.map((n) => ({ name: n, path: `/home/user/${n}.txt`, owner: 'user:user', mode: '0640' }))
  return [
    {
      id: 'manifest',
      title: 'Lab host manifest',
      note: 'The lab host logs into the machine over SSH after it boots and writes each flag where you say. The account needs write access to those paths (or set sudo). The machine image must already accept this key.',
      language: 'json',
      text: JSON.stringify(
        {
          artifact: 'kymera',
          runtime: 'vm',
          file: 'kymera.qcow2',
          ports: [{ name: 'ssh', port: 22 }],
          inject: { user: 'root', ssh_key: '/etc/kymo-lab/keys/kymera.key', flags },
        },
        null,
        2,
      ),
    },
    {
      id: 'service',
      title: 'A flag inside a container in the machine',
      note: 'When the flag must end up inside a container on the machine, pipe it to a command instead of a path. The flag goes in on standard input, never touches the disk and never appears in a command line.',
      language: 'json',
      text: JSON.stringify({ inject: { flags: [{ name: names[0] ?? 'flag', command: "docker exec -i web sh -c 'cat > /flag.txt'" }] } }, null, 2),
    },
  ]
}

function responder(names: string[]): FlagTemplate[] {
  return [
    {
      id: 'verifier',
      title: 'Verifier block of the manifest',
      note: 'Players fix the machine; the Verifier runs your checks and shows the flag only when all pass. The flag is held by the lab host, never put on the machine. A Responder has exactly one flag.',
      language: 'json',
      text: JSON.stringify({ verifier: { definition: 'my-challenge', port: 8080 } }, null, 2),
    },
    {
      id: 'check',
      title: 'A check in verifier.json',
      note: `Each check is a script that exits 0 when the player fixed that thing. The flag ${names[0] ?? 'flag'} appears once every check passes.`,
      language: 'json',
      text: JSON.stringify({ version: 1, checks: [{ id: 'service-patched', title: 'The login no longer accepts the old password', script: 'check_login.sh' }] }, null, 2),
    },
  ]
}

/** The snippets for a delivery, with the author's flag names filled in. */
export function dynamicTemplates({ delivery, responder: isResponder, names }: Input): FlagTemplate[] {
  const list = clean(names)
  if (delivery === 'none') return flagOnly(list)
  if (isResponder) return responder(list)
  return delivery === 'vm' ? machine(list) : container(list)
}
