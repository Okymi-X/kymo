import { useEffect } from 'react'
import { useAppearance } from '../components/ThemeLoader'
import { DEFAULT_APPEARANCE } from './theme'

/** Sets the browser tab title to "page, site name" and puts the site name back on leaving. */
export function usePageTitle(title: string | undefined) {
  const site = useAppearance().data?.site_name ?? DEFAULT_APPEARANCE.site_name
  useEffect(() => {
    document.title = title ? `${title} · ${site}` : site
    return () => {
      document.title = site
    }
  }, [title, site])
}
