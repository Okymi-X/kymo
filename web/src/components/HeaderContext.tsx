import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import type { Status } from '../api/types'

export interface HeaderInfo {
  title: string
  status: Status
  progress?: { played: number; total: number }
}

interface Value {
  info: HeaderInfo | null
  setInfo: (info: HeaderInfo | null) => void
}

const Context = createContext<Value>({ info: null, setInfo: () => undefined })

export function HeaderProvider({ children }: { children: ReactNode }) {
  const [info, setInfo] = useState<HeaderInfo | null>(null)
  const value = useMemo(() => ({ info, setInfo }), [info])
  return <Context.Provider value={value}>{children}</Context.Provider>
}

export const useHeaderInfo = () => useContext(Context).info

/** Lets a page put a short summary in the header, for example once its own title scrolls away. */
export function usePublishHeader(info: HeaderInfo | null) {
  const { setInfo } = useContext(Context)
  const title = info?.title
  const status = info?.status
  const played = info?.progress?.played
  const total = info?.progress?.total
  useEffect(() => {
    if (title === undefined || status === undefined) {
      setInfo(null)
      return
    }
    setInfo({ title, status, progress: total === undefined || played === undefined ? undefined : { played, total } })
  }, [title, status, played, total, setInfo])
  useEffect(() => () => setInfo(null), [setInfo])
}
