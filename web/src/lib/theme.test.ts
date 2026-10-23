import { describe, expect, it } from 'vitest'
import { DEFAULT_APPEARANCE, contrastRatio, themeCss } from './theme'

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
