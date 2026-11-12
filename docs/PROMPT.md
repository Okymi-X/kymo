## P0: Define and build challenge-based tournament matches

- [ ] Set the tournament challenge rules before implementation. Specify whether challenges are assigned per match, per round or from a shared pool; whether admins assign them manually or Kymo selects them; and how many challenges each match gets. Allow solo and team events to configure their challenge count independently, with solo defaulting to one.
- [ ] Offer the same challenge types in every kind of event. Flag-based, instance-based and DFIR challenges must be configurable in both Jeopardy and tournament events, through the same authoring screens and the same rules (a DFIR challenge in Jeopardy works exactly like one in a tournament: several questions, per-question answers and points, partial credit capped at the challenge total). Only the scoring context differs: Jeopardy ranks players by the points they collect, a tournament turns the points of one match into that match's score. Keep Jeopardy's existing flag scoring (static or dynamic) intact.
- [ ] Add DFIR challenges (available in Jeopardy and tournament events alike) with multiple question-and-answer items. Give each item its own answer validation and point value. Define whether partial credit is awarded per correct answer, how retries and feedback work, and ensure the earned points cannot exceed the challenge’s configured total.
- [ ] Connect challenge results to the tournament match score. Use challenge points to produce each side’s match score, then let the tournament format’s existing rules decide wins, draws, standings, and advancement. Tournament challenges must not use arbitrary Jeopardy-style point values as standalone event scores.
- [ ] Define match completion and tie handling. Specify when a match is final, whether a team must finish all assigned challenges, how incomplete or abandoned matches are handled, and how ties resolve in formats that require a winner.
- [ ] Support team submissions consistently. Define whether any team member can submit, how teammates’ answers contribute to one shared match score, and how duplicate or simultaneous correct submissions are handled.
- [ ] Add tournament challenge administration and player views. Admins need to configure challenge pools and assignments; players need to see the challenges for their current match, submit answers, and see match progress and results.
- [ ] Preserve auditability and event data portability. Record who submitted each answer, when, the verdict and points awarded. Include tournament challenges and their configuration in event export/import without exporting player submissions or secrets.

## P1: Let admins manage event participants

- [ ] Add, remove and update participants from the admin event page, regardless of whether registration is open or closed.
- [ ] In solo events, let admins find and add a player account. In team events, define whether admins add an existing team, add a player to an event roster, or both. Do not change a player’s global team membership as an accidental side effect.
- [ ] Validate event mode, duplicate registrations, team size rules and roster locks. Show a clear reason when an override is rejected.
- [ ] Define what removal does to existing submissions, scores, match assignments, instances and audit history. Preserve historical records; make removal from current standings and future matches explicit.
- [ ] Record admin additions and removals in the audit log, including the actor and affected player or team.
- [ ] Handle closed, live, paused and ended events explicitly. Decide whether archived events are frozen or can be edited after unarchiving.

## P1: Make event state and schedule behavior consistent

- [ ] Separate registration status from event live status. A registration-open event remains “Registration open” until its scheduled start. If organizers can start early, make that an explicit override and show that it started ahead of schedule.
- [ ] Add an explicit registration-close policy: close at the event start, close at a configured time, or remain open until an organizer closes registration. Keep this setting editable after event creation. Ensure the API and UI use the same policy.
- [ ] Show paused status everywhere. Align event cards, event pages, admin pages and tournament views so a paused event never appears live. In admin controls, label the action Resume when paused.
- [ ] Distinguish scheduled ending from manual ending. A scheduled end should block new submissions and instance starts at the end time. If an organizer extends the schedule, players should see the event become live again without stale “ended” state or cached UI.
- [ ] Keep manually ended events ended. Changing the schedule alone should not silently reopen an event that an organizer explicitly ended. Provide a deliberate reopen or resume action if reopening is allowed.
- [ ] Confirm end-of-event instance behavior. Block new starts immediately; define whether existing instances stop immediately or are allowed to expire, and make the player UI reflect that policy.
- [ ] Validate schedule combinations. Reject end times before start times and invalid registration windows; handle missing times, date edits, browser time zones and daylight-saving transitions consistently.
- [ ] Cover lifecycle boundaries with server and browser checks: just before and at registration open/close, event start/end, pause/resume, end-time extension, manual end and reopen.

## P1: Move tournament administration into Manage Events

- [ ] Make /admin/events/:id the complete admin interface for both Jeopardy and tournament events. Add tournament format/setup, participants, teams/rosters, bracket or match configuration, challenge configuration, schedules, scoring and other organizer controls currently on /t/:slug.
- [ ] Remove organizer-only controls from /t/:slug for admins. Keep the route and match experience available to players and authorized scoring roles.
- [ ] Keep player match actions on the public tournament page. Moving administration must not hide match viewing, player reporting or referee scoring actions that belong there.
- [ ] Update every admin link and shortcut that points to /t/:slug, including cards, command palette entries, embeds, reports and stage-related navigation. Public and player links should continue to target the player experience.
- [ ] Preserve authorization and existing tournament data. Direct navigation to old admin URLs should not expose organizer controls; either redirect admins to the event admin page or show a clear route transition.
- [ ] Avoid split sources of truth. Tournament settings, tournament status and the linked event’s status must remain synchronized after edits, state changes and imports.

## P2: Add score progression to the Ladder page

- [ ] Add a progression graph to /ladder showing cumulative points over time, with team selection and a readable current total.
- [ ] Provide chronological data from the server. The current ladder endpoint returns aggregate standings; it does not provide dated point changes. Define how wins, draws, titles, corrections, forfeits, resets and reopened tournaments affect the history.
- [ ] Define team identity across tournaments. The current ladder merges teams by case-insensitive name, which can combine unrelated teams or split renamed teams. Decide whether the graph keeps that rule or uses stable team identity, and label the behavior clearly.
- [ ] Make the graph usable on small screens and without visual interpretation alone. Include an accessible data table or equivalent, clear axes and date labels, empty states, and correct handling of teams with no history.



## Cross-cutting acceptance criteria

- [ ] Keep authorization checks and lifecycle/scoring rules on the server; do not rely on hiding UI controls to prevent actions.
- [ ] Ensure score updates and match advancement are atomic and idempotent, including concurrent submissions and retries.
- [ ] Add migration-safe persistence for new challenge, assignment, answer and participant data.
- [ ] Test solo/team events, all supported tournament formats, partial DFIR answers, duplicate submissions, paused/ended events, schedule edits, admin participant overrides, imports and score corrections.
