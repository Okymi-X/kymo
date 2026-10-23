import { Download } from 'lucide-react'
import { Button } from './ui'

export function ExportButton({ filename, build, label = 'Export CSV', type = 'text/csv;charset=utf-8' }: { filename: string; build: () => string; label?: string; type?: string }) {
  const download = () => {
    const url = URL.createObjectURL(new Blob([build()], { type }))
    const link = document.createElement('a')
    link.href = url
    link.download = filename
    link.click()
    URL.revokeObjectURL(url)
  }
  return (
    <Button onClick={download}>
      <Download size={16} aria-hidden="true" />
      {label}
    </Button>
  )
}
