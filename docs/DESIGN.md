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

## Polish pass (2026-10-23)
- Header: sticky translucent material (blur), search collapses to an icon below 640px, admin links move to the palette on small screens, no wrapped labels.
- One `PageTitle` for every page (4xl to 5xl, tracking -0.03em, leading 1.05). Headings use text-wrap balance, paragraphs pretty.
- Depth: `--shadow-card` at rest, `--shadow-lift` on hover and for dialogs. Dark mode drops the rest shadow and uses a hairline plus a deeper lift.
- Dialogs enter with opacity and scale 0.96 over 180ms (strong ease out), backdrop fades. Reduced motion collapses durations.
- Bracket fits the available width by default (floor 60 percent, so text stays readable); the percentage button returns to fit, plus and minus override.
- Touch targets in the roster editor are at least 40px. Tournament header shows matches played.

## Data views
Tournament page tabs (URL `?view=`): Bracket, Standings, Matches, Teams. Standings: rank, team, seed, qualifier score, P, W, L, point difference, status (In, Out, Champion). Matches: Up next, Waiting for teams, Played, each row opens the result dialog for organizers. Teams: cards that open a team dialog with record, points and the path through the bracket. Hovering a team in the bracket highlights it everywhere. Standings and results export as CSV (formula characters are neutralized). Bracket area is wider than the page column (up to 1680px) so a double elimination bracket fits at full size.

## Round robin views
Standings first (P, W, D, L, +/-, Pts), then Schedule (cards grouped by round, no accent ring because every match is open), Matches, Teams. Ranking explanation sits above the table.

## Groups then knockout views
Groups tab: one standings table per group (top N advancing rows tinted with an accent rank) and a folded Fixtures panel. Knockout tab shows only the elimination bracket and keeps the wide area only when the bracket is wider than the page column. After the groups finish the page opens on Knockout.

## Navigation
Header: logo, primary link (Tournaments, accent underline when active), a centered context slot, search (icon only on phones), organizer menu or sign in. When a tournament page's own title scrolls under the header, the title, status and progress fade into the context slot (hidden below 768px). The organizer menu is a popover with arrow key navigation, Escape returns focus to the trigger.

## Activity and following
Activity tab: a vertical timeline with an icon per kind, relative time with the exact time on hover, the round, and a red icon for corrections. Following a team is stored in localStorage per tournament and shown as an accent tinted chip with the team's next match; the team is highlighted in the bracket.
