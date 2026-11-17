import { describe, expect, it } from 'vitest'
import { dynamicTemplates } from './flagTemplates'

describe('dynamic flag templates', () => {
  it('a flag only challenge gets a text file with the token', () => {
    const t = dynamicTemplates({ delivery: 'none', responder: false, names: ['flag'] })
    expect(t[0]?.text).toContain('{{flag}}')
    const several = dynamicTemplates({ delivery: 'none', responder: false, names: ['user', 'root'] })
    expect(several[0]?.text).toContain('{{flag:user}}')
    expect(several[0]?.text).toContain('{{flag:root}}')
  })
  it('a container gets an environment variable per flag, in the manifest and the code', () => {
    const t = dynamicTemplates({ delivery: 'container', responder: false, names: ['web-key', 'db'] })
    const manifest = JSON.parse(t[0]?.text ?? '{}') as { inject: { flags: { name: string; env: string }[] } }
    expect(manifest.inject.flags).toEqual([
      { name: 'web-key', env: 'FLAG_WEB_KEY' },
      { name: 'db', env: 'FLAG_DB' },
    ])
    expect(t[1]?.text).toContain('os.environ["FLAG_WEB_KEY"]')
    expect(t[2]?.text).toContain('"$FLAG_DB"')
  })
  it('a machine gets a path per flag and every block is valid JSON', () => {
    const t = dynamicTemplates({ delivery: 'vm', responder: false, names: ['user', 'root'] })
    const manifest = JSON.parse(t[0]?.text ?? '{}') as { inject: { flags: { name: string; path: string }[] } }
    expect(manifest.inject.flags.map((f) => f.path)).toEqual(['/home/user/user.txt', '/home/user/root.txt'])
    for (const block of t) expect(() => JSON.parse(block.text)).not.toThrow()
  })
  it('a responder gets the verifier, whatever the delivery says', () => {
    const t = dynamicTemplates({ delivery: 'vm', responder: true, names: ['flag'] })
    expect(t.map((x) => x.id)).toEqual(['verifier', 'check'])
  })
  it('falls back to one flag called flag when none is named', () => {
    expect(dynamicTemplates({ delivery: 'none', responder: false, names: [] })[0]?.text).toContain('{{flag}}')
    expect(dynamicTemplates({ delivery: 'none', responder: false, names: ['', ' '] })[0]?.text).toContain('{{flag}}')
  })
})
