import { describe, expect, it } from 'vitest'
import { embedSnippet, embedUrl } from './EmbedDialog'

const base = { view: 'bracket' as const, theme: 'auto' as const, transparent: false, title: true }

describe('embedUrl', () => {
  it('stays clean when every option is the default', () => {
    expect(embedUrl('https://k.example', 'spring-cup', base, 'bracket')).toBe('https://k.example/embed/spring-cup')
  })

  it('adds only the options that differ', () => {
    const url = embedUrl('https://k.example', 'spring-cup', { view: 'standings', theme: 'dark', transparent: true, title: false }, 'bracket')
    expect(url).toBe('https://k.example/embed/spring-cup?view=standings&theme=dark&transparent=1&title=0')
  })

  it('encodes unusual slugs', () => {
    expect(embedUrl('https://k.example', '東京 大会', base, 'bracket')).toContain('/embed/%E6%9D%B1%E4%BA%AC%20%E5%A4%A7%E4%BC%9A')
  })
})

describe('embedSnippet', () => {
  it('builds an iframe and keeps hostile titles from breaking the attribute', () => {
    const html = embedSnippet('https://k.example/embed/x', 'A "quoted" <b>cup</b>')
    expect(html.startsWith('<iframe src="https://k.example/embed/x"')).toBe(true)
    expect(html).toContain('loading="lazy"')
    expect(html).not.toContain('"quoted"')
    expect(html).not.toContain('<b>')
  })
})
