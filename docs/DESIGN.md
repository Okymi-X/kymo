# Design

Direction: calm, precise, Apple level restraint. Content is the bracket; chrome stays quiet.

## Tokens
- Font: Inter variable (UI), JetBrains Mono (scores, seeds, ids). Tracking -0.01em on headings.
- Scale (px): 12, 14, 16, 20, 28, 40. Spacing: 4 base (4, 8, 12, 16, 24, 32, 48).
- Radius: 8 controls, 12 cards, 999 pills.
- Light: bg #FAFAFA, surface #FFFFFF, border #E5E5E7, text #1D1D1F, muted #6E6E73.
- Dark: bg #0B0B0C, surface #141416, border #2A2A2E, text #F5F5F7, muted #98989D.
- Accent: #0A84FF (focus, active match). Success #30D158 (winner). Danger #FF453A.
- Elevation: 1px border plus a soft shadow only on floating layers.

## Motion
- Springs, 200 to 300 ms, interruptible. Winner advance animates the slot into the next round.
- Reduced motion: cross fade only.

## Components
- Match card: two rows (team, score), winner row bolder with success tick, active match with accent ring.
- Bracket canvas: pan and zoom, rounds as columns, connector lines drawn in SVG.
- Inline score edit, drag handle seeding list, command palette for navigation.

## Accessibility
WCAG AA contrast, visible focus ring (2px accent), full keyboard control of match editing, live region for score updates.
