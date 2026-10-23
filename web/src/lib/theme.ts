import type { Appearance, FontChoice } from '../api/types'
import { resolveScheme } from './scheme'

export const DEFAULT_APPEARANCE: Appearance = {
  site_name: 'Kymo',
  accent_light: '#0a6fe0',
  accent_dark: '#4da3ff',
  radius: 8,
  font: 'sans',
  scheme: 'system',
  density: 'comfortable',
  tagline: 'Knockout brackets for CTF events, updated as results come in.',
  tint: 'neutral',
  avatars: true,
  logo: null,
  lines: 'square',
}

export const FONT_STACKS: Record<FontChoice, string> = {
  sans: "'Inter', ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
  serif: "'Iowan Old Style', 'Palatino Linotype', Palatino, Georgia, serif",
  mono: "'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, monospace",
}

export const FONT_LABELS: Record<FontChoice, string> = {
  sans: 'Sans serif',
  serif: 'Serif',
  mono: 'Monospace',
}

const HEX = /^#[0-9a-fA-F]{6}$/
export const isHex = (value: string) => HEX.test(value)

/** Builds the override stylesheet. Only validated values reach the CSS text. Mirrors `Appearance::css` on the server. */
export function themeCss(a: Appearance): string {
  const light = isHex(a.accent_light) ? a.accent_light : DEFAULT_APPEARANCE.accent_light
  const dark = isHex(a.accent_dark) ? a.accent_dark : DEFAULT_APPEARANCE.accent_dark
  const radius = Math.min(20, Math.max(0, Math.round(a.radius)))
  const font = FONT_STACKS[a.font] ?? FONT_STACKS.sans
  const size = a.density === 'compact' ? '14.5px' : '16px'
  return [
    `:root{--color-accent:${light};--radius-control:${radius}px;--radius-card:${radius + 4}px;--font-sans:${font};}`,
    `:root[data-scheme="dark"]{--color-accent:${dark};}`,
    `@media (prefers-color-scheme:dark){:root:not([data-scheme="light"]){--color-accent:${dark};}}`,
    `html{font-size:${size};}`,
    tintCss(a.tint),
    a.avatars === false ? '[data-avatar]{display:none !important;}' : '',
  ].join('')
}

const TINTS = {
  warm: { light: ['#faf8f5', '#fffefc', '#e8e4dd'], dark: ['#0d0c0b', '#161412', '#2d2a26'] },
  cool: { light: ['#f6f8fb', '#fcfdff', '#dfe5ee'], dark: ['#090c10', '#11151b', '#232a34'] },
} as const

const tintVars = ([bg, surface, line]: readonly string[]) => `--color-bg:${bg};--color-surface:${surface};--color-line:${line};`

function tintCss(tint: Appearance['tint']): string {
  if (tint !== 'warm' && tint !== 'cool') return ''
  const { light, dark } = TINTS[tint]
  return [
    `:root{${tintVars(light)}}`,
    `:root[data-scheme="dark"]{${tintVars(dark)}}`,
    `@media (prefers-color-scheme:dark){:root:not([data-scheme="light"]){${tintVars(dark)}}}`,
  ].join('')
}

export const MAX_LOGO_BYTES = 60_000
export const LOGO_TYPES = ['image/png', 'image/jpeg', 'image/webp']

/** Reads an image file into a data URI, or returns the reason it cannot be used. */
export function readLogo(file: File): Promise<{ uri: string } | { error: string }> {
  if (!LOGO_TYPES.includes(file.type)) return Promise.resolve({ error: 'Use a PNG, JPEG or WebP image.' })
  if (file.size > MAX_LOGO_BYTES) return Promise.resolve({ error: 'That image is over 60 KB. Resize it first.' })
  return new Promise((resolve) => {
    const reader = new FileReader()
    reader.onload = () => resolve({ uri: String(reader.result) })
    reader.onerror = () => resolve({ error: 'The file could not be read.' })
    reader.readAsDataURL(file)
  })
}

export interface Preset {
  id: string
  name: string
  values: Pick<Appearance, 'accent_light' | 'accent_dark' | 'radius' | 'font'>
}

/** Ready made looks. Every accent keeps at least 4.5:1 against the page for button text. */
export const PRESETS: Preset[] = [
  { id: 'kymo', name: 'Kymo', values: { accent_light: '#0a6fe0', accent_dark: '#4da3ff', radius: 8, font: 'sans' } },
  { id: 'graphite', name: 'Graphite', values: { accent_light: '#3f3f46', accent_dark: '#d4d4d8', radius: 6, font: 'sans' } },
  { id: 'ocean', name: 'Ocean', values: { accent_light: '#0369a1', accent_dark: '#38bdf8', radius: 10, font: 'sans' } },
  { id: 'forest', name: 'Forest', values: { accent_light: '#15803d', accent_dark: '#4ade80', radius: 8, font: 'sans' } },
  { id: 'ember', name: 'Ember', values: { accent_light: '#c2410c', accent_dark: '#fb923c', radius: 10, font: 'sans' } },
  { id: 'rose', name: 'Rose', values: { accent_light: '#be123c', accent_dark: '#fb7185', radius: 12, font: 'sans' } },
  { id: 'terminal', name: 'Terminal', values: { accent_light: '#15803d', accent_dark: '#4ade80', radius: 2, font: 'mono' } },
]

export function matchingPreset(a: Appearance): string | null {
  const found = PRESETS.find(
    (p) =>
      p.values.accent_light.toLowerCase() === a.accent_light.toLowerCase() &&
      p.values.accent_dark.toLowerCase() === a.accent_dark.toLowerCase() &&
      p.values.radius === a.radius &&
      p.values.font === a.font,
  )
  return found?.id ?? null
}

const STYLE_ID = 'kymo-theme'

export function applyAppearance(a: Appearance) {
  let el = document.getElementById(STYLE_ID)
  if (!el) {
    el = document.createElement('style')
    el.id = STYLE_ID
    document.head.appendChild(el)
  }
  el.textContent = themeCss(a)
  document.documentElement.dataset.scheme = resolveScheme(a.scheme)
  document.title = a.site_name
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const v = parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16) / 255
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
}

/** WCAG contrast ratio between two #rrggbb colors. */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

export const PAGE_BACKGROUND = { light: '#fafafa', dark: '#0b0b0c' }
export const AA = 4.5
