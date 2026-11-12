import { useQuery } from '@tanstack/react-query'
import { platform } from '../api/platform'
import { CustomHome } from '../components/home/CustomHome'
import { DefaultHome } from '../components/home/DefaultHome'
import { DEFAULT_HOME } from '../lib/home'
import { usePageTitle } from '../lib/usePageTitle'

/** The front page. It is either the default page, built from texts the organizer edits, or their own HTML and CSS.
 *  While the content loads, or if it cannot be fetched, the default page stands in so sign in is always one click away. */
export function Home() {
  usePageTitle(undefined)
  const q = useQuery({ queryKey: ['home'], queryFn: platform.home, staleTime: 30_000, retry: false })
  const home = q.data ?? DEFAULT_HOME
  if (home.mode === 'custom' && home.custom_html.trim() !== '') return <CustomHome html={home.custom_html} css={home.custom_css} />
  return <DefaultHome home={home} />
}
