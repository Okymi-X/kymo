import { useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import { applyAppearance } from '../lib/theme'

export const appearanceKey = ['appearance']

export function useAppearance() {
  return useQuery({ queryKey: appearanceKey, queryFn: api.getAppearance, staleTime: 60_000 })
}

/** Applies the saved look to the whole app as soon as it loads. */
export function ThemeLoader() {
  const { data } = useAppearance()
  useEffect(() => {
    if (data) applyAppearance(data)
  }, [data])
  return null
}
