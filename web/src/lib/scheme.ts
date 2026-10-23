import type { Scheme } from '../api/types'

const KEY = 'kymo.scheme'

export function readSchemeOverride(): 'light' | 'dark' | null {
  try {
    const value = localStorage.getItem(KEY)
    return value === 'light' || value === 'dark' ? value : null
  } catch {
    return null
  }
}

export function writeSchemeOverride(value: 'light' | 'dark' | null) {
  try {
    if (value === null) localStorage.removeItem(KEY)
    else localStorage.setItem(KEY, value)
  } catch {
    /* storage unavailable: the choice lasts until the page closes */
  }
}

/** The scheme to paint: a forced site scheme wins, then the visitor's own choice, then the device. */
export function resolveScheme(site: Scheme): Scheme {
  if (site !== 'system') return site
  return readSchemeOverride() ?? 'system'
}

export function deviceIsDark(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches
}
