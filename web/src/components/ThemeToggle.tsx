import { Moon, Sun } from 'lucide-react'
import { useState } from 'react'
import { useAppearance } from './ThemeLoader'
import { deviceIsDark, readSchemeOverride, writeSchemeOverride } from '../lib/scheme'

/** Light or dark for this visitor only. Hidden when the organizer forced a scheme for everyone. */
export function ThemeToggle() {
  const site = useAppearance().data?.scheme ?? 'system'
  const [override, setOverride] = useState(readSchemeOverride)
  if (site !== 'system') return null
  const dark = (override ?? (deviceIsDark() ? 'dark' : 'light')) === 'dark'
  const toggle = () => {
    const next = dark ? 'light' : 'dark'
    writeSchemeOverride(next)
    setOverride(next)
    document.documentElement.dataset.scheme = next
  }
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={dark ? 'Switch to light mode' : 'Switch to dark mode'}
      className="flex size-9 items-center justify-center rounded-control text-muted transition-colors hover:bg-line/50 hover:text-ink"
    >
      {dark ? <Sun size={17} aria-hidden="true" /> : <Moon size={17} aria-hidden="true" />}
    </button>
  )
}
