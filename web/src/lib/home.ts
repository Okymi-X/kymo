import type { HomeContent } from '../api/platformTypes'

/** The home page as the server ships it. Used while the real one loads or when it cannot be fetched,
 *  so a visitor always gets the sign in and sign up buttons. Keep in step with `Home::default` on the server. */
export const DEFAULT_HOME: HomeContent = {
  mode: 'default',
  headline: 'Compete, learn and climb the scoreboard',
  subheadline: 'Sign in to join events, start your challenge machines and follow your progress.',
  signin_label: 'Sign in',
  signup_label: 'Create account',
  welcome: 'Welcome back',
  continue_label: 'Go to events',
  banner: '',
  about_title: 'About',
  about_text: '',
  show_events: true,
  show_scoreboards: true,
  footer: '',
  custom_html: '',
  custom_css: '',
}

/** Where a public scoreboard link goes for someone who is signed in (their own pages) or not (the public page). */
export function scoreboardPath(kind: 'jeopardy' | 'tournament', slug: string, signedIn: boolean): string {
  if (!signedIn) return `/s/${encodeURIComponent(slug)}`
  return kind === 'tournament' ? `/t/${encodeURIComponent(slug)}` : `/e/${encodeURIComponent(slug)}?view=scoreboard`
}

/** The rules text as blocks: `# Title` lines are headings, `- ` lines are list items, and blank lines separate paragraphs. */
export type RulesBlock = { kind: 'heading'; text: string } | { kind: 'list'; items: string[] } | { kind: 'paragraph'; text: string }

export function parseRules(text: string): RulesBlock[] {
  const blocks: RulesBlock[] = []
  let paragraph: string[] = []
  let list: string[] = []
  const flush = () => {
    if (paragraph.length > 0) blocks.push({ kind: 'paragraph', text: paragraph.join('\n') })
    if (list.length > 0) blocks.push({ kind: 'list', items: list })
    paragraph = []
    list = []
  }
  for (const raw of text.replace(/\r\n/g, '\n').split('\n')) {
    const line = raw.trimEnd()
    const heading = /^#{1,3}\s+(.+)$/.exec(line)
    const item = /^\s*[-*]\s+(.+)$/.exec(line)
    if (line.trim() === '') flush()
    else if (heading) {
      flush()
      blocks.push({ kind: 'heading', text: heading[1] })
    } else if (item) {
      if (paragraph.length > 0) flush()
      list.push(item[1])
    } else {
      if (list.length > 0) flush()
      paragraph.push(line)
    }
  }
  flush()
  return blocks
}
