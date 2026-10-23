export interface Command {
  id: string
  label: string
  hint?: string
  run: () => void
}

/** Case insensitive match where every typed word must appear in the label. */
export function filterCommands(commands: Command[], query: string): Command[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return commands
  return commands.filter((c) => words.every((w) => c.label.toLowerCase().includes(w)))
}
