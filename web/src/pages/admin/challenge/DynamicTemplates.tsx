import { useState } from 'react'
import { Button } from '../../../components/ui'
import { dynamicTemplates } from '../../../lib/flagTemplates'
import type { Delivery } from '../../../lib/delivery'

/** The text an author writes for the dynamic flags to reach the player, one block per step, to copy. */
export function DynamicTemplates({ delivery, responder, names }: { delivery: Delivery; responder: boolean; names: string[] }) {
  const [copied, setCopied] = useState<string | null>(null)
  const blocks = dynamicTemplates({ delivery, responder, names })
  const where = delivery === 'none' ? 'a template file' : responder ? 'the Verifier' : delivery === 'vm' ? 'the machine' : 'the container'
  return (
    <details className="rounded-control border border-line p-3 text-sm" open={delivery === 'none'}>
      <summary className="cursor-pointer font-medium">How the flag reaches {where}: template to copy</summary>
      <div className="mt-3 flex flex-col gap-4">
        {blocks.map((b) => (
          <div key={b.id} className="flex flex-col gap-1.5">
            <p className="text-sm font-medium">{b.title}</p>
            <p className="text-xs text-muted">{b.note}</p>
            <pre className="overflow-x-auto rounded-control border border-line bg-bg p-2 font-mono text-xs">{b.text}</pre>
            <Button
              className="self-start"
              onClick={() => {
                void navigator.clipboard?.writeText(b.text).then(() => {
                  setCopied(b.id)
                  window.setTimeout(() => setCopied(null), 1500)
                })
              }}
            >
              {copied === b.id ? 'Copied' : 'Copy'}
            </Button>
          </div>
        ))}
      </div>
    </details>
  )
}
