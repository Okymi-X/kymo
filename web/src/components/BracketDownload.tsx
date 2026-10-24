import { Image as ImageIcon } from 'lucide-react'
import type { Bracket, Slot } from '../api/types'
import { bracketImage } from '../lib/bracketImage'
import { Button } from './ui'

function save(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}

/** Saves the whole bracket as an image to post in chat or print, as SVG or PNG. */
export function BracketDownload({ bracket, teamName, title, slug }: { bracket: Bracket; teamName: (slot: Slot) => string; title: string; slug: string }) {
  const dark = () => document.documentElement.dataset.scheme === 'dark'
  const svg = () => {
    const { svg: markup } = bracketImage(bracket, teamName, title, dark())
    save(new Blob([markup], { type: 'image/svg+xml' }), `${slug}-bracket.svg`)
  }
  const png = () => {
    const { svg: markup, width, height } = bracketImage(bracket, teamName, title, dark())
    // A data URI, because the page policy allows data images but not blob images.
    const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(markup)}`
    const image = new Image()
    image.onload = () => {
      const scale = 2
      const canvas = document.createElement('canvas')
      canvas.width = width * scale
      canvas.height = height * scale
      const ctx = canvas.getContext('2d')
      if (ctx) {
        ctx.scale(scale, scale)
        ctx.drawImage(image, 0, 0, width, height)
        canvas.toBlob((blob) => blob && save(blob, `${slug}-bracket.png`))
      }
    }
    image.src = url
  }
  return (
    <div className="flex items-center gap-2">
      <Button onClick={png}>
        <ImageIcon size={16} aria-hidden="true" />
        Save image
      </Button>
      <Button variant="ghost" onClick={svg}>
        SVG
      </Button>
    </div>
  )
}
