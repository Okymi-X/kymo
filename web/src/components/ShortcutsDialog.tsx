import { Modal, ModalBody, ModalHeader } from './Modal'

const GROUPS: { title: string; items: [string, string][] }[] = [
  {
    title: 'Anywhere',
    items: [
      ['Ctrl K', 'Search events, tournaments and pages'],
      ['?', 'Show this list'],
    ],
  },
  {
    title: 'Bracket map',
    items: [
      ['Arrow keys', 'Move the map'],
      ['+ and -', 'Zoom in and out'],
      ['0', 'Fit everything'],
      ['Ctrl scroll', 'Zoom at the pointer'],
    ],
  },
  {
    title: 'Stage mode',
    items: [
      ['Left and Right', 'Previous and next screen'],
      ['Space', 'Pause or resume rotation'],
      ['F', 'Full screen'],
    ],
  },
]

export function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  return (
    <Modal label="Keyboard shortcuts" onClose={onClose} width="30rem">
      <ModalHeader title="Keyboard shortcuts" />
      <ModalBody>
        {GROUPS.map((g) => (
          <section key={g.title} aria-label={g.title} className="flex flex-col gap-2">
            <h3 className="text-xs font-medium uppercase tracking-wider text-muted">{g.title}</h3>
            <dl className="flex flex-col gap-1.5">
              {g.items.map(([keys, what]) => (
                <div key={keys} className="flex items-center justify-between gap-4 text-sm">
                  <dt className="text-muted">{what}</dt>
                  <dd>
                    <kbd className="rounded-control border border-line bg-surface px-2 py-0.5 font-mono text-xs">{keys}</kbd>
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </ModalBody>
    </Modal>
  )
}
