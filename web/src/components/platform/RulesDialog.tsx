import { useQuery } from '@tanstack/react-query'
import { platform } from '../../api/platform'
import { parseRules } from '../../lib/home'
import { Button, ErrorText } from '../ui'
import { Modal, ModalBody, ModalFooter, ModalHeader } from '../Modal'
import { Loading } from './bits'

/** The rules text as plain text: nothing in it is ever treated as markup. */
export function RulesText({ text }: { text: string }) {
  return (
    <div className="flex flex-col gap-3 text-sm leading-relaxed">
      {parseRules(text).map((b, i) =>
        b.kind === 'heading' ? (
          <h3 key={i} className="mt-2 text-base font-semibold tracking-tight">
            {b.text}
          </h3>
        ) : b.kind === 'list' ? (
          <ul key={i} className="ml-5 list-disc space-y-1">
            {b.items.map((item, j) => (
              <li key={j}>{item}</li>
            ))}
          </ul>
        ) : (
          <p key={i} className="whitespace-pre-line">
            {b.text}
          </p>
        ),
      )}
    </div>
  )
}

/** The rules of one event, in a dialog on its page. */
export function RulesDialog({ slug, onClose }: { slug: string; onClose: () => void }) {
  const q = useQuery({ queryKey: ['rules', slug], queryFn: () => platform.eventRules(slug) })
  return (
    <Modal label="Rules" onClose={onClose} width="40rem">
      <ModalHeader title={q.data?.title ?? 'Rules'} />
      <ModalBody>
        {q.isPending && <Loading />}
        <ErrorText error={q.error} />
        {q.data && <RulesText text={q.data.body} />}
      </ModalBody>
      <ModalFooter>
        <Button variant="primary" onClick={onClose}>
          Close
        </Button>
      </ModalFooter>
    </Modal>
  )
}
