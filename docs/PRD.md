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
- Swiss (planned). Round robin and groups then knockout are built.
- Multi tenant SaaS, payments.

## Event day features
- Public activity log of every result, correction, forfeit and reset, so disputes can be checked.
- Stage mode for a projector, and a followable team with its next match.
- Sample data (`seed-demo`) to try every format and state.

## Acceptance criteria
- Create a 16 team single elimination tournament and play it to a champion through the UI.
- Same for double elimination with 8 teams, including grand final reset.
- Non power of two team counts produce correct byes.
- Editing a finished match re-propagates winners downstream or is refused with a clear reason.
- Public view works without auth and updates live.
- `docker compose up` or one binary serves API and UI.

## Platform scope (v2, decided 2026-11-02)
Kymo grows from a bracket manager into a CTF platform with two competition types, each solo or team:
- Tournament events: single and double elimination, round robin, groups then knockout, Swiss (built).
- Jeopardy events: categories, flags, hints, attachments, static or dynamic points, scoreboard.
Events have states: draft, registration_open, live, paused, ended, archived, with registration and start/end times and scheduled challenge releases. Time rules gate challenge access, flag submission and instances.
Roles: visitor (home, rules, register, login, published scoreboard), player (profile, teams, challenges, submissions), referee (limited interface), admin (events, challenges, participants, scoring, settings, appearance, preview as player), monitor (admin only request view, per endpoint rate limits, auto and manual bans, flag submission history, moderation audit).
Instances: player started, restarted and stopped containers (Docker first, behind a runtime trait), duration and resource limits, isolation, secret handling, expiry reaper, abandoned cleanup.
Operations: event health view, error visibility, backup and restore of event data, role based access everywhere.
Three engines stay separate and share event, user, team and reporting data: bracket (kymo-engine), jeopardy (kymo-jeopardy), instances (kymo-instances).
