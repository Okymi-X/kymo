# Kymo PRD

## Problem
CTF organizers run knockout events with spreadsheets or general tournament tools (Bracket, Challonge) that know nothing about CTF rounds, team seeding by score, or live public boards. Kymo is an open source tournament manager built for CTF knockout events.

## Users
- Organizers: create and edit tournaments, seed teams, record round results, fix mistakes fast.
- Players and teams: see their bracket and next match.
- Spectators: watch a public live bracket without signing in.

## Goals (v1)
- Single and double elimination brackets, round robin leagues, byes, seeding (manual, random, by score).
- Teams, tournaments, rounds, matches, per match scores and winners.
- Easy editing: drag and drop seeding, inline score edit, undo last result, re-generate bracket before start.
- Public live bracket view, shareable URL, auto refresh via server sent events.
- Self host in one command: single binary, SQLite by default.
- Admin auth with a single organizer account per instance plus API tokens.
- Customizable look saved in the database: site name, accent colors, radius, typeface. Professional, restrained design.

## Non goals (v1)
- Hosting CTF challenges or flag submission.
- Swiss and group stage into knockout (planned v2). Round robin is built.
- Multi tenant SaaS, payments.

## Acceptance criteria
- Create a 16 team single elimination tournament and play it to a champion through the UI.
- Same for double elimination with 8 teams, including grand final reset.
- Non power of two team counts produce correct byes.
- Editing a finished match re-propagates winners downstream or is refused with a clear reason.
- Public view works without auth and updates live.
- `docker compose up` or one binary serves API and UI.
