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

## Form controls
- One height for inputs, selects and buttons (40px), so rows align. Labels sit above in ink at 70 percent.
- Field border uses `field-line` (stronger than card borders) so inputs read in dark mode. Placeholder at 60 percent muted so it never looks like a value.
- Focus: accent border plus a 3px accent ring at 25 percent. Select uses a custom chevron. Disabled primary buttons turn neutral instead of dimming the accent.

## Brand
Logo: `web/public/logo.png` (wordmark with bracket nodes). Favicon and app icon: `favicon-32.png`, `favicon-192.png`, `apple-touch-icon.png`, cropped from the supplied rounded square K mark. Header logo height 40px.

## Home
Large display title (tracking -0.03em, leading 1.05), one line of context, a pill status filter with counts, then a card grid. Card: name and format, status pill, match progress bar (live and finished), avatar stack with team count. Press feedback scales to 0.99. Loading uses skeleton cards, empty state is a dashed panel with a next step.

## Icons
lucide-react only. Font Awesome was considered and not added: a second icon set would duplicate the first and add weight for no gain.
