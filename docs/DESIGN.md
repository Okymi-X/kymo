# Design

Direction: calm, precise, Apple level restraint. Content is the bracket; chrome stays quiet.

## Tokens
- Font: Inter variable (UI), JetBrains Mono (scores, seeds, ids). Tracking -0.01em on headings.
- Scale (px): 12, 14, 16, 20, 28, 40. Spacing: 4 base (4, 8, 12, 16, 24, 32, 48).
- Radius: 8 controls, 12 cards, 999 pills.
- Light: bg #FAFAFA, surface #FFFFFF, border #E5E5E7, text #1D1D1F, muted #6E6E73.
- Dark: bg #0B0B0C, surface #141416, border #2A2A2E, text #F5F5F7, muted #98989D.
- Accent: #0A84FF (focus, active match). Success #30D158 (winner). Warning #A86400 (light theme), #FFD166 (dark theme). Danger #FF453A.
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

## Stage mode
Route `/t/{slug}/stage`, outside the normal shell. Large title with status and progress, a rotating main panel (bracket then standings, or groups then knockout), a side column with the next four ready matches and the latest six results. Keys: space pauses, arrows switch, F toggles full screen. When a champion exists the panel becomes a full height champion screen with a large avatar and the up next list is hidden. Updates arrive over the same live event stream as the normal page.

## Bracket map
The bracket lives in a bounded viewport (`MapViewport`) instead of a tall page. Interactions: drag with the mouse or one finger (momentum on release, skipped for reduced motion), Ctrl or Cmd with the wheel and two finger pinch zoom around the pointer, plain wheel pans and hands the wheel back to the page once the map is at an edge, arrows pan, + and - zoom, 0 resets, F shows everything. A drag past 4px never counts as a click, so match cards stay safe to grab. Controls sit bottom left, the minimap bottom right (only when the map is larger than the viewport) and jumping on it recenters. Initial zoom fits the width with a floor of 60 percent; stage mode and embeds fill their parent and start zoomed out to show everything. Print drops the viewport and prints the canvas at half size.

## Back and wayfinding
Every sub page starts with a text back link to its parent: Events for an event or tournament, Home for the others. On phones the header also has a back chevron (history back, home if there is no history). The header has a live badge linking to Events, a wide search field, a light and dark switch that only the visitor sees (hidden when the organizer forces a scheme) and the organizer menu.

## Embed and print
`/embed/{slug}` has no navigation and accepts `view`, `theme`, `transparent=1` and `title=0`. It is the only route allowed in a frame. The Embed dialog builds the iframe code. Print hides the header, tabs and dialogs.

## Platform screens (phase 6)
Header (signed in only): Events, Ladder. A visitor sees only the logo and a Sign in button (player accounts, with Create account on the same page); the organizer password sign in stays at /login. The account menu (letter avatar) holds Profile and My team for players, Appearance and API tokens for admins, then Manage events, Users, Operations, and Monitor for monitors and admins, then Sign out. The organizer menu keeps its old label and item order (Appearance first) for the legacy password session.
Session: `session.who` (id, username, role) is stored beside the token for player accounts; `useIsAdmin` is true for the organizer password and for admin accounts, `useCanMonitor` for monitors and admins. Pages are wrapped in `Gate` (sign in prompt, then a role explanation); the server still enforces every permission.
Events (/events): cards with a state pill, kind and mode; Jeopardy events open /e/{slug}, tournaments /t/{slug}. Event page: title, state, schedule, a registration panel (sign in, create a team first, register, withdraw), tabs Challenges, Scoreboard and My progress (URL `?view=`). Challenges are grouped by category on cards (title, points, solves, solved tick, lock for closed); a card opens a dialog with the description as plain text, downloads (fetched with the session, then saved), hints (confirm the cost before unlocking), the instance panel (start, restart, stop, address with copy, live countdown, starts left) and the flag form (format hint from the prefix and suffix, attempts left, result announced politely; a solved challenge shows the points awarded). The board refreshes over the event stream.
Team (/team): create or join with a code; owner sees the invite code with copy and new code, can remove members; anyone can leave (confirm). Scoreboard: rank, avatar, name, solves, points, last solve, and a per challenge tick matrix on wide screens.
Admin: /admin/events (create, import a bundle with keep schedule and keep published options), /admin/events/{id} with tabs Overview (state buttons that mirror the server's state machine, details, public scoreboard toggle, tie-breakers and flag rate limit, export bundle), Challenges (categories, table, editor dialog with scoring, flag kind, accepted format, schedule, instance limits, hints, attachments, flag test, publish and retire), Participants, Submissions (filters, pager) and Instances (stop). /admin/users (search, role select, ban, sign-ups on or off). /admin/ops (health tiles, jobs, events, recent server errors, download or save a backup, clean up now). /monitor with tabs Traffic (summary, top addresses and endpoints, request log with filters), Rate limits (rule form), Bans (ban form, lift) and Audit (action filter).
All of it uses the existing tokens, `Modal`, `ConfirmDialog`, `Tabs`, `Pager`, one 40px control height, lucide icons, and tables in a bordered scroll container with a visually hidden caption.

## Private network screens (phase 7)
Event page: when an event is in VPN mode and the player is registered, a panel *Your private network* sits under the registration panel: what it is in two sentences (isolated network per player or team, only you can reach your instances), three steps (install OpenVPN 2.5 or newer, download and import the profile, connect then start an instance), a download button that says *Download my VPN profile* or *Download a new profile*, the player's tunnel address and the profile's expiry, and a note that a new profile replaces the old one at once. A server without the VPN says so instead of offering a download.
Instance panel: a VPN instance shows its private address with the copy button and a line that it only works while the VPN is connected.
Admin event Overview: *Instance network* (published ports or private VPN) with a one paragraph explanation of the difference; the VPN option is disabled and labelled when the server has no VPN. The server refuses a switch while instances run, the page shows that message.
Operations: a *VPN gateway* section with running state, the address players connect to, players with a profile out of capacity, private networks out of capacity, the two ranges, a table of live networks (range, owner, event, instances), *Sync the gateway now* and a revoke-by-user-id form. Without the VPN it explains how to turn it on.

## Dialogs (phase 8)
One framework for every dialog (`components/Modal.tsx`): `Modal`, `ModalHeader` (title, subtitle, actions, a close button named Close), `ModalBody` (the only scrolling part, one scrollbar), `ModalFooter` (actions always visible; a status note sits beside the buttons on a wide screen and above them on a phone), `ModalCancel` and `modalForm` for a `<form>` that wraps all three. The page behind stops scrolling while a dialog is open (nested dialogs share one lock). A backdrop click closes only when both the press and the release are on the backdrop, so selecting text cannot dismiss it, and forms turn backdrop closing off. Escape, the close button and Cancel all go through one guard: with unsaved changes the dialog asks "Discard your changes?" first. Ctrl or Cmd with Enter submits the form. The first field gets focus; a risky confirmation focuses its safe answer.
Challenge editor: sections (Basics, Points, Flag, Availability, Instance), helper text under fields, a one sentence summary for dynamic scoring, dates in two columns with a Clear button, hints, files and the flag test above the save bar with the note that they save at once. Validation runs as you type but only shows once a field is touched; a static flag that does not match its own accepted prefix or suffix is refused, because nobody could submit it. The save bar names the first problem and Save is off while one shows.
Challenge dialog (player): header with category, solves and points; body with the description, files, instance and hints; the flag form is pinned at the bottom so it is always in view.
Instance panel states: nothing running, starting (spinner, polls every 1.5 s, no address), running (address with copy, countdown, restart and stop), failed with a reason (could not start, never answered, stopped unexpectedly), expired. A private-network challenge warns and offers the download when the player has no VPN profile. The event page lists the player's instances with the challenge each belongs to.
Sign in and sign up: the form says up front when sign-ups are closed and falls back to signing in; username and password are checked as you type with the exact reason; the password can be shown. Controls that change a setting (sign-ups, public scoreboard, network mode) change on click and settle on what the server says.
Errors: a request that fails reads as a sentence. The server's own message is kept when it already says what is wrong; a bare 500 becomes "Something went wrong on the server"; an unreachable server says so; an ended session says to sign in again. A 503 keeps its message because the server writes those for people.

## Phone layout and wayfinding (audit)
- Under the sm breakpoint the header shows a menu button (Tournaments, Events, Ladder, Sign in when signed out) in place of the primary links. There is no history based back arrow: sub pages link to their parent.
- Search is labelled Search Kymo and covers events, tournaments and the pages the role can use.
- Wide tables fade at the edge and say to swipe; Users is a card list on a phone. Registration copy comes from `registrationStatus` and never offers Create account while sign-ups are closed.

## Score chart
Points over time sit above the scoreboard table: flat between solves, an S-curve rise into each one, a dot per solve, one colour per participant from `--series-1..10` (stable by participant id), a Top 5/10/20 control, hover and arrow keys read out every score at that moment, legend entries toggle and highlight lines. Lines draw in once; reduced motion turns that off. The table below is the accessible data.

## Lab hosts page
Admin only, reached from the account menu and the command palette. A short explanation and the install command come first, then one card per lab host: a state pill (Online, Unreachable, Switched off, Not checked yet), what players connect to, whether the connection is encrypted and pinned, events and running instances, the last error in words, and the actions Test connection, What is running (flags containers the platform does not know), Edit, Switch off or on, Clean up and Remove. The add and edit dialog suggests the public address from the API address, never shows the token again, and after saving shows the connection test result. Events get a Where instances run select next to the network settings.

## Services and machines
A player's instance shows its main address as before; a machine with more services lists the others by name beneath it (copy buttons), in the instance panel and in Your instances. Nothing is shown until the guest really answers. On the Lab hosts page an image that is still being unpacked reads preparing, a virtual machine is tagged vm, the connection test says whether machines run with KVM or in slow emulation, and each running machine has a Console button that shows a screenshot of its screen.
