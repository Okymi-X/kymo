// How a challenge reaches the player, and what each way needs. Pure: the editor shows these
// answers and the same rules are what the lab host would otherwise refuse later.

import type { LabArtifact, LabCatalog } from '../api/platformTypes'

/** `none`: the flag is in the files or the description. `container`: a Docker or Podman service.
 *  `vm`: a virtual machine (OVA, qcow2, VMDK) booted on a lab host. */
export type Delivery = 'none' | 'container' | 'vm'

export const deliveryOf = (requiresInstance: boolean, machine: boolean): Delivery => (!requiresInstance ? 'none' : machine ? 'vm' : 'container')

export const isMachine = (runtime: string): boolean => runtime === 'vm'

export function runtimeLabel(runtime: string): string {
  return runtime === 'vm' ? 'Virtual machine' : runtime === 'podman' ? 'Podman' : runtime === 'docker' ? 'Docker' : runtime
}

/** The artifacts that fit the chosen way: machines for `vm`, containers for `container`. */
export function artifactsFor(delivery: Delivery, artifacts: LabArtifact[]): LabArtifact[] {
  if (delivery === 'none') return []
  return artifacts.filter((a) => isMachine(a.runtime) === (delivery === 'vm'))
}

export const findArtifact = (artifacts: LabArtifact[], image: string): LabArtifact | undefined => artifacts.find((a) => a.artifact === image.trim())

export interface DeliveryContext {
  delivery: Delivery
  image: string
  /** The lab host this challenge runs on (its own or the event's); null is this server's own runtime. */
  labId: number | null
  /** Null while it is loading or when there is no lab host to ask. */
  catalog: LabCatalog | null
}

export interface DeliveryProblems {
  labHost?: string
  image?: string
}

/** What the lab host would refuse, found while editing. Nothing here blocks a lab host that cannot be read. */
export function deliveryProblems(c: DeliveryContext): DeliveryProblems {
  const p: DeliveryProblems = {}
  if (c.delivery === 'none') return p
  if (c.delivery === 'vm') {
    if (c.labId === null) {
      p.labHost = 'A virtual machine runs on a lab host. Choose one here or on the event.'
      return p
    }
    if (c.catalog?.ok && c.catalog.runtimes && !c.catalog.runtimes.includes('vm')) p.labHost = 'This lab host cannot run virtual machines.'
  }
  const catalog = c.catalog
  if (!catalog?.ok || c.image.trim() === '') return p
  const found = findArtifact(catalog.artifacts ?? [], c.image)
  if (found) {
    if (isMachine(found.runtime) !== (c.delivery === 'vm')) {
      p.image = found.runtime === 'vm' ? 'This is a virtual machine. Choose Virtual machine above.' : 'This is a container. Choose Container above.'
    }
  } else if (catalog.mode === 'manifest') {
    p.image = 'This lab host has no manifest for that name, so it would refuse to start it.'
  }
  return p
}

export interface VmDefaults {
  port: string
  cpu: string
  memory: string
  pids: string
}

/** The numbers to send for a machine: what its manifest gives, so a small default never shrinks it. */
export function manifestDefaults(a: LabArtifact, fallback: VmDefaults): VmDefaults {
  return {
    port: String(a.port > 0 ? a.port : fallback.port),
    cpu: String(a.cpu_millis ?? fallback.cpu),
    memory: String(a.memory_mb ?? fallback.memory),
    pids: String(a.pids ?? fallback.pids),
  }
}

/** How players are told to reach it, in a sentence. A machine defaults to its address with every port open. */
export function displayText(a: LabArtifact): string {
  const machine = a.display ? a.display === 'machine' : isMachine(a.runtime)
  return machine ? 'Players get the address, with every port open to scan.' : 'Players get the address and port of the service.'
}

/** Flag names a challenge uses that the manifest does not deliver, so there is nowhere to put them. */
export function undeliveredFlags(names: string[], artifact: LabArtifact | undefined): string[] {
  if (!artifact?.flags || artifact.flags.length === 0) return []
  return names.filter((n) => n.trim() !== '' && !artifact.flags?.includes(n.trim()))
}

type TypedChallenge = {
  kind: 'flag' | 'dfir' | 'code'
  responder: boolean
  multi_flag: boolean
  flag_mode: 'static' | 'dynamic'
  requires_instance: boolean
  flag: { kind: 'static' | 'regex' | 'per_team' }
}

/** What kind of challenge this is, in the words the editor uses, for lists. */
export function challengeType(c: TypedChallenge): string {
  if (c.kind === 'dfir') return 'Multi-question'
  if (c.kind === 'code') return 'Coding'
  const flag = c.responder
    ? 'Responder'
    : c.multi_flag
      ? c.flag_mode === 'dynamic'
        ? 'Dynamic flags'
        : 'Several flags'
      : c.flag_mode === 'dynamic'
        ? 'Dynamic flag'
        : c.flag.kind === 'regex'
          ? 'Pattern flag'
          : c.flag.kind === 'per_team'
            ? 'Per-team flag'
            : 'Flag'
  return c.requires_instance && !c.responder ? `${flag} + instance` : flag
}
