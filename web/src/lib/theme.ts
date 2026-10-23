import type { Appearance, FontChoice } from '../api/types'

export const DEFAULT_APPEARANCE: Appearance = {
  site_name: 'Kymo',
  accent_light: '#0a6fe0',
  accent_dark: '#4da3ff',
  radius: 8,
  font: 'sans',
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

/** Builds the override stylesheet. Only validated values reach the CSS text. */
export function themeCss(a: Appearance): string {
  const light = isHex(a.accent_light) ? a.accent_light : DEFAULT_APPEARANCE.accent_light
  const dark = isHex(a.accent_dark) ? a.accent_dark : DEFAULT_APPEARANCE.accent_dark
  const radius = Math.min(20, Math.max(0, Math.round(a.radius)))
  const font = FONT_STACKS[a.font] ?? FONT_STACKS.sans
  return [
    `:root{--color-accent:${light};--radius-control:${radius}px;--radius-card:${radius + 4}px;--font-sans:${font};}`,
    `@media (prefers-color-scheme:dark){:root{--color-accent:${dark};}}`,
  ].join('')
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
