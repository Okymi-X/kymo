import { describe, expect, it } from 'vitest'
import { DEFAULT_APPEARANCE, accentProps, PAGE_BACKGROUND, PRESETS, contrastRatio, matchingPreset, themeCss } from './theme'

describe('themeCss', () => {
  it('writes accent, radius and font for both color schemes', () => {
    const css = themeCss({ ...DEFAULT_APPEARANCE, accent_light: '#aa2200', accent_dark: '#ff8866', radius: 14, font: 'serif' })
    expect(css).toContain('--color-accent:#aa2200')
    expect(css).toContain('--color-accent:#ff8866')
    expect(css).toContain('--radius-control:14px')
    expect(css).toContain('--radius-card:18px')
    expect(css).toContain('Georgia')
  })

  it('falls back to defaults instead of emitting untrusted text', () => {
    const css = themeCss({ ...DEFAULT_APPEARANCE, accent_light: 'red;}body{display:none', radius: 999 })
    expect(css).not.toContain('display:none')
    expect(css).toContain('--color-accent:#0a6fe0')
    expect(css).toContain('--radius-control:20px')
  })
})

describe('contrastRatio', () => {
  it('is 21 for black on white and 1 for identical colors', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 0)
    expect(contrastRatio('#336699', '#336699')).toBe(1)
  })

  it('flags a pale accent on a light page', () => {
    expect(contrastRatio('#ffee88', '#fafafa')).toBeLessThan(4.5)
  })
})

describe('scheme and density', () => {
  it('writes dark rules that respect a forced light scheme', () => {
    const css = themeCss(DEFAULT_APPEARANCE)
    expect(css).toContain(':root[data-scheme="dark"]{--color-accent:#4da3ff;}')
    expect(css).toContain('@media (prefers-color-scheme:dark){:root:not([data-scheme="light"])')
  })

  it('shrinks the root size for compact density', () => {
    expect(themeCss({ ...DEFAULT_APPEARANCE, density: 'compact' })).toContain('html{font-size:14.5px;}')
    expect(themeCss(DEFAULT_APPEARANCE)).toContain('html{font-size:16px;}')
  })
})

describe('presets', () => {
  it('keep button text readable in both color schemes', () => {
    for (const p of PRESETS) {
      expect(contrastRatio(p.values.accent_light, PAGE_BACKGROUND.light), `${p.name} light`).toBeGreaterThanOrEqual(4.5)
      expect(contrastRatio(p.values.accent_dark, PAGE_BACKGROUND.dark), `${p.name} dark`).toBeGreaterThanOrEqual(4.5)
    }
  })

  it('are recognised from the current values', () => {
    const ember = PRESETS.find((p) => p.id === 'ember')!
    expect(matchingPreset({ ...DEFAULT_APPEARANCE, ...ember.values })).toBe('ember')
    expect(matchingPreset({ ...DEFAULT_APPEARANCE, accent_light: '#123456' })).toBeNull()
  })
})

describe('tint and avatars', () => {
  it('adds nothing for the neutral tint with avatars shown', () => {
    expect(themeCss(DEFAULT_APPEARANCE)).not.toContain('--color-bg')
    expect(themeCss(DEFAULT_APPEARANCE)).not.toContain('data-avatar')
  })

  it('tints the page and hides avatars on request', () => {
    const css = themeCss({ ...DEFAULT_APPEARANCE, tint: 'cool', avatars: false })
    expect(css).toContain('--color-bg:#f6f8fb')
    expect(css).toContain('[data-avatar]{display:none !important;}')
  })
})

describe('accentProps', () => {
  it('scopes a valid hex and ignores anything else', () => {
    expect(accentProps('#aa2200')).toMatchObject({ 'data-tournament-accent': '', style: { '--tournament-accent': '#aa2200' } })
    expect(accentProps(null)).toEqual({})
    expect(accentProps('red;}</style>')).toEqual({})
  })
})
