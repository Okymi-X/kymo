import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Upload } from "lucide-react";
import { useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { EventCard } from "../../components/platform/EventCard";
import { platform } from "../../api/platform";
import {
  Empty,
  Gate,
  Loading,
  Panel,
  Toggle,
} from "../../components/platform/bits";
import {
  Button,
  ErrorText,
  Field,
  Input,
  PageTitle,
  Segmented,
  Select,
} from "../../components/ui";
import { EVENT_GROUPS, countByGroup, filterEvents, type EventGroup } from "../../lib/eventFilter";
import { labLabel } from "../../lib/platform";
import {
  DemoButton,
  ImportSetup,
  TournamentFields,
  defaultTournamentOptions,
  useCreateTournament,
  type TournamentOptions,
} from "../../components/TournamentCreate";
import { usePageTitle } from "../../lib/usePageTitle";

function CreateEvent() {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"jeopardy" | "tournament">("jeopardy");
  const [tournament, setTournament] = useState<TournamentOptions>(
    defaultTournamentOptions,
  );
  const [importError, setImportError] = useState<string | null>(null);
  const [mode, setMode] = useState<"solo" | "team">("team");
  // Where the instances run: chosen here so the event is set up in one go. Empty is this server.
  const [lab, setLab] = useState("");
  const [placeError, setPlaceError] = useState<{
    id: number;
    message: string;
  } | null>(null);
  const client = useQueryClient();
  const navigate = useNavigate();
  const labs = useQuery({ queryKey: ["labs"], queryFn: platform.labs });
  const choices = (labs.data?.labs ?? []).filter((l) => l.enabled);
  const createTournament = useCreateTournament(name, tournament);
  const create = useMutation({
    mutationFn: async () => {
      const event = await platform.createEvent({
        name: name.trim(),
        kind,
        mode,
      });
      const target =
        kind === "jeopardy" && lab !== ""
          ? choices.find((l) => l.id === Number(lab))
          : undefined;
      if (!target) return { event, failed: null as string | null };
      try {
        // A lab host with a gateway is used through it: players are not given open ports.
        await platform.setEventLab(
          event.id,
          target.id,
          target.vpn ? "vpn" : undefined,
        );
        return { event, failed: null };
      } catch (e) {
        return {
          event,
          failed:
            e instanceof Error ? e.message : "could not use that lab host",
        };
      }
    },
    onSuccess: ({ event, failed }) => {
      void client.invalidateQueries({ queryKey: ["admin-events"] });
      void client.invalidateQueries({ queryKey: ["labs"] });
      if (failed) {
        // The event exists; say what went wrong instead of hiding it behind a page change.
        setPlaceError({ id: event.id, message: failed });
        return;
      }
      navigate(`/organizer/events/${event.id}`);
    },
  });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (kind === "tournament") createTournament.mutate();
    else create.mutate();
  };
  const pending = create.isPending || createTournament.isPending;
  return (
    <form
      onSubmit={submit}
      className="flex flex-wrap items-end gap-3 rounded-card border border-line bg-surface p-4"
    >
      <Field label="New event">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Spring CTF 2026"
          className="w-64 max-w-full"
          maxLength={80}
        />
      </Field>
      <Field label="Type">
        <Select
          value={kind}
          onChange={(e) => setKind(e.target.value as "jeopardy" | "tournament")}
        >
          <option value="jeopardy">Jeopardy CTF</option>
          <option value="tournament">Tournament</option>
        </Select>
      </Field>
      {kind === "jeopardy" ? (
        <>
          <Field label="Players">
            <Select
              value={mode}
              onChange={(e) => setMode(e.target.value as "solo" | "team")}
            >
              <option value="team">Teams</option>
              <option value="solo">Solo</option>
            </Select>
          </Field>
          <Field label="Instances run on">
            <Select
              value={lab}
              onChange={(e) => setLab(e.target.value)}
              className="w-72 max-w-full"
            >
              <option value="">This server</option>
              {choices.map((l) => (
                <option key={l.id} value={l.id}>
                  {labLabel(l)}
                </option>
              ))}
            </Select>
          </Field>
        </>
      ) : (
        <TournamentFields value={tournament} onChange={setTournament} />
      )}
      <Button
        type="submit"
        variant="primary"
        disabled={!name.trim() || pending}
      >
        Create event
      </Button>
      {kind === "tournament" && <ImportSetup onError={setImportError} />}
      <ErrorText error={create.error ?? createTournament.error} />
      {importError && (
        <p role="alert" className="basis-full text-sm text-danger">
          {importError}
        </p>
      )}
      {placeError && (
        <p role="alert" className="basis-full text-sm text-danger">
          The event was created, but it could not be placed on that lab host:{" "}
          {placeError.message}.{" "}
          <Link
            to={`/organizer/events/${placeError.id}`}
            className="font-medium underline"
          >
            Open it to choose another place
          </Link>
          .
        </p>
      )}
    </form>
  );
}

function ImportBundle() {
  const input = useRef<HTMLInputElement>(null);
  const [keepTimes, setKeepTimes] = useState(false);
  const [keepState, setKeepState] = useState(false);
  const client = useQueryClient();
  const navigate = useNavigate();
  const run = useMutation({
    mutationFn: async (file: File) =>
      platform.importEvent(await file.text(), keepTimes, keepState),
    onSuccess: (e) => {
      void client.invalidateQueries({ queryKey: ["admin-events"] });
      navigate(`/organizer/events/${e.id}`);
    },
  });
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Import an event bundle</h2>
      <p className="text-sm text-muted">
        Creates an event from an exported file. Challenges start as drafts unless you keep their state.
      </p>
      <div className="flex flex-wrap items-center gap-4">
        <Toggle
          label="Keep the schedule"
          checked={keepTimes}
          onChange={setKeepTimes}
        />
        <Toggle
          label="Keep published challenges"
          checked={keepState}
          onChange={setKeepState}
        />
        <input
          ref={input}
          type="file"
          accept="application/json,.json"
          className="sr-only"
          aria-label="Event bundle file"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) run.mutate(file);
            e.target.value = "";
          }}
        />
        <Button onClick={() => input.current?.click()} disabled={run.isPending}>
          <Upload size={15} aria-hidden="true" /> Choose file
        </Button>
      </div>
      <ErrorText error={run.error} />
    </Panel>
  );
}

function Body() {
  const q = useQuery({
    queryKey: ["admin-events"],
    queryFn: platform.adminEvents,
  });
  const [group, setGroup] = useState<EventGroup>("all");
  const [text, setText] = useState("");
  if (q.isPending) return <Loading />;
  const events = q.data ?? [];
  const counts = countByGroup(events);
  const shown = filterEvents(events, group, text);
  return (
    <div className="flex flex-col gap-6">
      <CreateEvent />
      {events.length > 6 && (
        <div className="flex flex-wrap items-center gap-3">
          <Segmented<EventGroup>
            label="Show events"
            value={group}
            onChange={setGroup}
            options={EVENT_GROUPS.filter((g) => g.id === "all" || counts[g.id] > 0).map((g) => ({ value: g.id, label: `${g.label} ${counts[g.id]}` }))}
          />
          <Input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Search by name"
            aria-label="Search events"
            className="w-56 max-w-full"
          />
        </div>
      )}
      {events.length > 0 && shown.length === 0 && <Empty>No event matches.</Empty>}
      <ul className="grid auto-rows-fr gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {shown.map((e) => (
          <li key={e.id} className="min-w-0">
            <EventCard event={e} to={`/organizer/events/${e.id}`} footer={<span className="truncate font-mono text-xs text-muted">/{e.slug}</span>} />
          </li>
        ))}
      </ul>
      {(q.data ?? []).length === 0 && <DemoButton />}
      <ImportBundle />
    </div>
  );
}

export function AdminEvents() {
  usePageTitle("Manage events");
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Manage events</PageTitle>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  );
}
