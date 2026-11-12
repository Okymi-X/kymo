//! Challenge instance service: the order of checks and side effects around the pure rules in
//! kymo-instances and the container runtime. A start reserves its row under the write lock,
//! releases the lock, and only then talks to the container platform, so a slow image pull
//! never blocks bracket writes.

use kymo_core::EventState;
use kymo_instances::{
    Action, Denied, InstanceSpec, InstanceState, Placement, RuntimeStatus, StartExtras,
    StartRequest, Usage, authorize, effective_state, expires_at,
};
use serde_json::{Value, json};

use crate::challenge_store::{self, ChallengeRow};
use crate::error::ApiError;
use crate::event_store;
use crate::instance_store::{self as store, InstanceRow, NewRun};
use crate::labs;
use crate::run_flags;
use crate::state::{AppState, InstanceSetup};
use crate::submit::{Play, open_for_play};
use crate::vpn;

fn setup(state: &AppState) -> Result<&InstanceSetup, ApiError> {
    state
        .instance_setup()
        .ok_or_else(|| ApiError::Unavailable("instances are not enabled on this server".into()))
}

fn denied(reason: Denied) -> ApiError {
    ApiError::Conflict(reason.to_string())
}

/// The container a challenge asks for, checked against the platform ceilings.
pub fn spec_of(challenge: &ChallengeRow, setup: &InstanceSetup) -> Result<InstanceSpec, ApiError> {
    if !challenge.requires_instance {
        return Err(ApiError::NotFound);
    }
    let config = challenge
        .instance_config
        .as_deref()
        .ok_or_else(|| ApiError::Unavailable("this challenge has no instance configured".into()))?;
    let spec: InstanceSpec = serde_json::from_str(config).map_err(|_| ApiError::Internal)?;
    spec.validate(&setup.caps)
        .map_err(|_| ApiError::Unavailable("this challenge has no valid instance setup".into()))?;
    Ok(spec)
}

/// The endpoints the lab host reported for a run, as they were stored.
fn endpoints(stored: Option<&str>) -> Value {
    stored
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]))
}

/// The other published services of a machine, by name, for the player to connect to.
fn services(extra: Option<&str>) -> Value {
    json!(
        store::extra_services(extra)
            .into_iter()
            .map(|p| json!({ "name": p.name, "port": p.host }))
            .collect::<Vec<_>>()
    )
}

/// What a player sees. The container handle and environment are never included.
pub fn view(setup: &InstanceSetup, row: &InstanceRow, starts_used: u32, now: i64) -> Value {
    let state = row
        .record()
        .map(|r| effective_state(&r, now))
        .unwrap_or(InstanceState::Failed);
    // The address is shown only once the service answers, never before and never after.
    let connect = state == InstanceState::Running && row.host_port.is_some();
    // A lab instance is reached at its private address through the player's VPN.
    let host = row
        .lab_ip
        .clone()
        .or_else(|| row.public_host.clone())
        .unwrap_or_else(|| setup.public_host.clone());
    json!({
        "id": row.id,
        "state": state.as_str(),
        "ready": state == InstanceState::Running,
        "failure": row.failure,
        "network": if row.lab_ip.is_some() { "vpn" } else { "published" },
        "host": connect.then_some(host),
        // A machine is given as an address alone: every port is open on it, so the player scans it.
        "machine": row.machine,
        "port": (connect && !row.machine).then_some(row.host_port).flatten(),
        "services": if connect && !row.machine { services(row.extra_ports.as_deref()) } else { json!([]) },
        // Where the machine and, for a Responder, its Verifier can be reached.
        "endpoints": if connect { endpoints(row.endpoints.as_deref()) } else { json!([]) },
        "started_at": row.started_at,
        "expires_at": row.expires_at,
        "remaining_secs": if state.is_live() { (row.expires_at - now).max(0) } else { 0 },
        "starts_used": starts_used,
        "starts_max": setup.policy.max_starts,
    })
}

pub async fn current(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
) -> Result<Value, ApiError> {
    let setup = setup(state)?;
    spec_of(challenge, setup)?;
    match store::latest(&state.pool, challenge.id, play.scorer).await? {
        Some(row) => {
            // Settle the run first, so the start budget below already reflects a failure.
            let row = refresh(state, setup, row).await?;
            let used = store::starts_used(&state.pool, challenge.id, play.scorer).await?;
            Ok(view(setup, &row, used, state.now()))
        }
        None => Ok(json!({
            "state": "none",
            "starts_used": store::starts_used(&state.pool, challenge.id, play.scorer).await?,
            "starts_max": setup.policy.max_starts,
        })),
    }
}

/// How long a started container may take to answer before the start counts as failed.
pub const READY_TIMEOUT_SECS: i64 = 60;

/// Whether the service of a started instance accepts connections: through the gateway for a
/// private lab, straight to the published port otherwise.
async fn probe(state: &AppState, setup: &InstanceSetup, row: &InstanceRow) -> bool {
    match (
        &row.lab_ip,
        row.host_port.and_then(|p| u16::try_from(p).ok()),
    ) {
        (Some(ip), Some(port)) => {
            // A lab host may still be setting the instance up (writing its flags): not ready.
            if let (Some(handle), Ok(target)) = (
                row.handle.as_deref(),
                labs::target(state, setup, row.lab_host_id).await,
            ) && !target.runtime.ready(handle).await
            {
                return false;
            }
            vpn::lab_reachable(state, row.lab_host_id.unwrap_or(0), ip, port).await
        }
        // The runtime the run lives on checks its own way: a remote lab host is reached the way
        // a player will reach it, from this server to its public address.
        (None, Some(port)) => match labs::target(state, setup, row.lab_host_id).await {
            Ok(target) => target.runtime.reachable(port).await,
            Err(_) => false,
        },
        _ => false,
    }
}

/// Brings a live run's stored state in line with reality: a `starting` run becomes `running`
/// when its service answers and fails when it never does; a run whose container exited or
/// vanished fails. Safe to call from anywhere, any number of times.
async fn refresh(
    state: &AppState,
    setup: &InstanceSetup,
    row: InstanceRow,
) -> Result<InstanceRow, ApiError> {
    if row.state != "starting" && row.state != "running" {
        return Ok(row);
    }
    let now = state.now();
    if now >= row.expires_at {
        return Ok(row);
    }
    // A virtual machine says how long it needs to boot; a container gets the usual minute.
    let ready_limit = row
        .ready_secs
        .unwrap_or(READY_TIMEOUT_SECS)
        .max(READY_TIMEOUT_SECS);
    let Some(handle) = row.handle.clone() else {
        // The container is still being created. A start stuck here for too long is dropped.
        if now - row.started_at > READY_TIMEOUT_SECS {
            store::mark_failed(&state.pool, row.id, now, "never_ready").await?;
            return store::by_id(&state.pool, row.id).await;
        }
        return Ok(row);
    };
    // A runtime that cannot be asked right now says nothing about the container.
    let Ok(target) = labs::target(state, setup, row.lab_host_id).await else {
        return Ok(row);
    };
    if let Ok(RuntimeStatus::Exited | RuntimeStatus::Missing) = target.runtime.status(&handle).await
    {
        let _ = target.runtime.stop(&handle).await;
        store::mark_failed(&state.pool, row.id, now, "exited").await?;
        return store::by_id(&state.pool, row.id).await;
    }
    if row.state == "starting" {
        if probe(state, setup, &row).await {
            store::mark_ready(&state.pool, row.id).await?;
        } else if now - row.started_at > ready_limit {
            let _ = target.runtime.stop(&handle).await;
            store::mark_failed(&state.pool, row.id, now, "never_ready").await?;
        }
        return store::by_id(&state.pool, row.id).await;
    }
    Ok(row)
}

/// Gives a freshly started container a few seconds to answer, so a quick service is reported
/// running in the same response. A slow one stays `starting` and the page polls.
async fn wait_ready(state: &AppState, setup: &InstanceSetup, id: i64) -> Result<(), ApiError> {
    for _ in 0..10 {
        let row = store::by_id(&state.pool, id).await?;
        if row.state != "starting" {
            return Ok(());
        }
        if probe(state, setup, &row).await {
            return store::mark_ready(&state.pool, id).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    Ok(())
}

/// Ends a run: the container first, then the row. If the platform refuses, the row stays
/// live so the reaper retries.
async fn end_run(
    state: &AppState,
    setup: &InstanceSetup,
    row: &InstanceRow,
    outcome: InstanceState,
) -> Result<(), ApiError> {
    if let Some(handle) = &row.handle {
        let target = labs::target(state, setup, row.lab_host_id).await?;
        target.runtime.stop(handle).await.map_err(|e| {
            tracing::warn!("stopping instance {}: {e}", row.id);
            ApiError::Unavailable("could not stop the instance, try again".into())
        })?;
    }
    store::mark_ended(&state.pool, row.id, outcome, state.now()).await
}

pub async fn start(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    user_id: i64,
) -> Result<Value, ApiError> {
    begin(state, play, challenge, user_id, Action::Start).await
}

pub async fn restart(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    user_id: i64,
) -> Result<Value, ApiError> {
    begin(state, play, challenge, user_id, Action::Restart).await
}

async fn begin(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    user_id: i64,
    action: Action,
) -> Result<Value, ApiError> {
    let setup = setup(state)?;
    let spec = spec_of(challenge, setup)?;
    let now = state.now();
    open_for_play(play, challenge, now)?;
    let scorer = play.scorer;

    // Where this run will live: the challenge's own lab host, else the event's, else this machine's runtime.
    let lab_host_id = challenge.host(play.event.lab_host_id);
    if let Some(id) = lab_host_id {
        labs::usable(state, id).await?;
    }
    if lab_host_id.is_none() && !setup.runtime.can_start() {
        return Err(ApiError::Unavailable(
            "this server runs no challenge containers itself: an organizer must choose a lab host for this event or challenge".into(),
        ));
    }
    let target = labs::target(state, setup, lab_host_id).await?;
    // The platform builds the name, and records it before asking anyone to start anything.
    let name = format!(
        "kymo-c{}-{}{}-{}",
        challenge.id,
        &scorer.kind()[..1],
        scorer.id(),
        kymo_lab::naming::random_suffix()
    );

    // VPN mode: the player's private network exists and the gateway is on it before we start.
    let vpn_lab = if play.event.network_mode == "vpn" {
        if state.vpn_setup().is_none() {
            return Err(ApiError::Unavailable(
                "this event needs the VPN, which is not enabled on this server".into(),
            ));
        }
        Some(vpn::lab_for(state, play.event.id, scorer, lab_host_id.unwrap_or(0)).await?)
    } else {
        None
    };

    // Flags this run needs (dynamic flags, or the one a Verifier will release). Only a lab host
    // can hand them out, and a Responder needs the private network for its Verifier.
    let flags = run_flags::plan(&state.pool, challenge).await?;
    if (!flags.is_empty() || challenge.responder) && lab_host_id.is_none() {
        return Err(ApiError::Unavailable(
            "this challenge needs a lab host: an organizer must choose one for the event".into(),
        ));
    }
    if (!flags.is_empty() || challenge.responder)
        && let Some(id) = lab_host_id
    {
        labs::require_flag_support(state, id).await?;
    }
    if challenge.responder && vpn_lab.is_none() {
        return Err(ApiError::Unavailable(
            "this challenge needs the private VPN network".into(),
        ));
    }
    let (run_id, previous, lab_ip, verifier_ip) = {
        let _guard = state.write_lock.lock().await;
        let latest = store::latest(&state.pool, challenge.id, scorer).await?;
        let record = latest.as_ref().map(InstanceRow::record).transpose()?;
        let usage = Usage {
            starts_used: store::starts_used(&state.pool, challenge.id, scorer).await?,
            live_elsewhere: store::live_elsewhere(
                &state.pool,
                play.event.id,
                challenge.id,
                scorer,
                now,
            )
            .await?,
        };
        authorize(&setup.policy, record.as_ref(), usage, action, now).map_err(denied)?;
        let (lab_ip, verifier_ip) = match &vpn_lab {
            // A Responder takes two addresses: the machine and its Verifier.
            Some(lab) if challenge.responder => {
                let mut ips = vpn::free_lab_ips(state, lab, 2).await?.into_iter();
                (ips.next(), ips.next())
            }
            Some(lab) => (Some(vpn::free_lab_ip(state, lab).await?), None),
            None => (None, None),
        };
        let run_id = store::insert(
            &state.pool,
            &NewRun {
                event_id: play.event.id,
                challenge_id: challenge.id,
                scorer,
                user_id,
                started_at: now,
                expires_at: expires_at(now, spec.ttl_secs),
                state: "starting",
                lab_ip: lab_ip.as_deref(),
                lab_host_id,
                name: &name,
                public_host: &target.host,
                verifier_ip: verifier_ip.as_deref(),
            },
        )
        .await?;
        run_flags::keep(&state.pool, run_id, &flags).await?;
        let previous = latest.filter(|r| r.state == "running" || r.state == "starting");
        (run_id, previous, lab_ip, verifier_ip)
    };

    if let Some(old) = previous {
        let outcome = old
            .record()
            .map(|r| effective_state(&r, now))
            .ok()
            .filter(|s| *s == InstanceState::Expired)
            .unwrap_or(InstanceState::Stopped);
        if let Err(e) = end_run(state, setup, &old, outcome).await {
            store::mark_failed(&state.pool, run_id, state.now(), "start_failed").await?;
            return Err(e);
        }
    }

    let placement = match (&vpn_lab, &lab_ip) {
        (Some(lab), Some(ip)) => Placement::Lab {
            network: lab.network.clone(),
            ip: ip.clone(),
            // The network's own address: where a virtual machine's ports are forwarded.
            gateway: state
                .vpn_setup()
                .and_then(|s| vpn::lab_subnet(s, lab).ok())
                .and_then(|n| n.host(1))
                .map(|a| a.to_string()),
        },
        _ => Placement::Published,
    };
    if vpn_lab.is_some() {
        // Let the player's tunnel reach the lab before the container is up.
        if let Err(e) = vpn::sync_host(state, lab_host_id.unwrap_or(0)).await {
            store::mark_failed(&state.pool, run_id, state.now(), "start_failed").await?;
            return Err(e);
        }
    }
    let request = StartRequest {
        instance_id: run_id.to_string(),
        spec,
        placement,
        name: Some(name.clone()),
        meta: vec![
            ("event".into(), play.event.id.to_string()),
            ("challenge".into(), challenge.id.to_string()),
            ("owner".into(), format!("{}:{}", scorer.kind(), scorer.id())),
            ("user".into(), user_id.to_string()),
        ],
    };
    let extras = StartExtras {
        flags: flags.iter().map(|f| f.flag.clone()).collect(),
        verifier_ip: verifier_ip.clone(),
    };
    match target.runtime.start_with(&request, &extras).await {
        Ok(outcome) => {
            store::mark_started(&state.pool, run_id, &outcome.started).await?;
            store::set_endpoints(&state.pool, run_id, &outcome.endpoints).await?;
            wait_ready(state, setup, run_id).await?;
        }
        Err(e) => {
            tracing::warn!("starting instance {run_id}: {e}");
            store::mark_failed(&state.pool, run_id, state.now(), "start_failed").await?;
            return Err(ApiError::Unavailable(
                "could not start the instance, try again later".into(),
            ));
        }
    }
    let row = store::by_id(&state.pool, run_id).await?;
    let used = store::starts_used(&state.pool, challenge.id, scorer).await?;
    Ok(view(setup, &row, used, state.now()))
}

pub async fn stop(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
) -> Result<Value, ApiError> {
    let setup = setup(state)?;
    spec_of(challenge, setup)?;
    let now = state.now();
    let _guard = state.write_lock.lock().await;
    let latest = store::latest(&state.pool, challenge.id, play.scorer).await?;
    let record = latest.as_ref().map(InstanceRow::record).transpose()?;
    authorize(
        &setup.policy,
        record.as_ref(),
        Usage::default(),
        Action::Stop,
        now,
    )
    .map_err(denied)?;
    let row = latest.ok_or(ApiError::NotFound)?;
    end_run(state, setup, &row, InstanceState::Stopped).await?;
    let ended = store::by_id(&state.pool, row.id).await?;
    let used = store::starts_used(&state.pool, challenge.id, play.scorer).await?;
    Ok(view(setup, &ended, used, now))
}

/// Why a live run must end now, if it must: its time is up, the challenge closed, or the
/// event is over.
async fn due(
    state: &AppState,
    setup: &InstanceSetup,
    row: &InstanceRow,
    now: i64,
) -> Result<Option<InstanceState>, ApiError> {
    if now >= row.expires_at.saturating_add(setup.reap_grace_secs) {
        return Ok(Some(InstanceState::Expired));
    }
    let event = event_store::by_id(&state.pool, row.event_id).await?;
    if matches!(
        event.effective_state(now)?,
        EventState::Ended | EventState::Archived
    ) {
        return Ok(Some(InstanceState::Stopped));
    }
    let challenge = challenge_store::challenge_by_id(&state.pool, row.challenge_id).await?;
    let (_, retire) = challenge.window();
    if retire.is_some_and(|t| now >= t) {
        return Ok(Some(InstanceState::Stopped));
    }
    Ok(None)
}

/// Stops runs that are due and removes containers no live run owns. Safe to call at any time
/// and from several places; failures on one run do not stop the others.
pub async fn reap(state: &AppState) -> Result<usize, ApiError> {
    let result = reap_inner(state).await;
    if state.instance_setup().is_some() {
        state.ops().record_reaper(state.now(), &result);
    }
    result
}

async fn reap_inner(state: &AppState) -> Result<usize, ApiError> {
    let Some(setup) = state.instance_setup() else {
        return Ok(0);
    };
    let now = state.now();
    let mut ended = 0;
    for row in store::live(&state.pool).await? {
        let row = match refresh(state, setup, row.clone()).await {
            Ok(fresh) => fresh,
            Err(_) => row,
        };
        if row.state != "starting" && row.state != "running" {
            continue;
        }
        let Some(outcome) = due(state, setup, &row, now).await? else {
            continue;
        };
        match end_run(state, setup, &row, outcome).await {
            Ok(()) => ended += 1,
            Err(_) => tracing::warn!("instance {} is due but could not be stopped", row.id),
        }
    }
    // Lab networks nobody uses any more go once their instances are gone.
    if let Err(e) = vpn::collect_idle_labs(state).await {
        tracing::warn!("collecting idle labs: {e:?}");
    }
    labs::refresh_health(state).await;
    ended += remove_orphans(state, setup).await?;
    Ok(ended)
}

/// Containers a runtime has that no live run owns: left behind by a crash, or started and then
/// forgotten. Looked for on this machine and on every lab host. The platform decides what should
/// run; a run's row exists before its container does, so one being created is never an orphan.
async fn remove_orphans(state: &AppState, setup: &InstanceSetup) -> Result<usize, ApiError> {
    let mut places: Vec<Option<i64>> = vec![None];
    places.extend(
        crate::lab_store::enabled(&state.pool)
            .await?
            .into_iter()
            .map(|l| Some(l.id)),
    );
    let mut removed = 0;
    for place in places {
        let Ok(target) = labs::target(state, setup, place).await else {
            continue;
        };
        // List first, read the runs second: a run's row is written before its container exists,
        // so every container seen here already has its row by the time the rows are read.
        let managed = match target.runtime.list_managed().await {
            Ok(managed) => managed,
            // A lab host that is down is not an orphan hunt; its health shows in Operations.
            Err(e) => {
                tracing::warn!("listing instances ({place:?}): {e}");
                continue;
            }
        };
        let owned: std::collections::HashSet<String> = store::live(&state.pool)
            .await?
            .iter()
            .filter(|r| r.lab_host_id == place)
            .map(|r| r.id.to_string())
            .collect();
        for orphan in managed.iter().filter(|m| !owned.contains(&m.instance_id)) {
            if target.runtime.stop(&orphan.handle).await.is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// Admin: stop one run regardless of its owner.
pub async fn force_stop(state: &AppState, id: i64) -> Result<InstanceRow, ApiError> {
    let setup = setup(state)?;
    let _guard = state.write_lock.lock().await;
    let row = store::by_id(&state.pool, id).await?;
    if row.state == "running" || row.state == "starting" {
        end_run(state, setup, &row, InstanceState::Stopped).await?;
    }
    store::by_id(&state.pool, id).await
}

/// The instance a player has going right now, whichever event it is in, for the header.
pub async fn mine(state: &AppState, user_id: i64) -> Result<Value, ApiError> {
    let now = state.now();
    let row = store::live_of_user(&state.pool, user_id, now).await?;
    Ok(json!({
        "instance": row.map(|(id, challenge_id, title, run_state, slug, expires_at)| json!({
            "id": id,
            "challenge_id": challenge_id,
            "title": title,
            "state": if run_state == "running" { "running" } else { "starting" },
            "event_slug": slug,
            "remaining_secs": (expires_at - now).max(0),
        })),
    }))
}

/// A player's running and starting instances in an event, for the "your instances" list.
pub async fn list_for(state: &AppState, play: &Play) -> Result<Value, ApiError> {
    let setup = setup(state)?;
    let now = state.now();
    let mut out = Vec::new();
    for row in store::live_of(&state.pool, play.event.id, play.scorer).await? {
        let state_name = if row.expires_at <= now {
            InstanceState::Expired
        } else if row.state == "running" {
            InstanceState::Running
        } else {
            InstanceState::Starting
        };
        let shown = state_name == InstanceState::Running && row.host_port.is_some();
        let host = row
            .lab_ip
            .clone()
            .or_else(|| row.public_host.clone())
            .unwrap_or_else(|| setup.public_host.clone());
        out.push(json!({
            "id": row.id,
            "challenge_id": row.challenge_id,
            "title": row.title,
            "state": state_name.as_str(),
            "network": if row.lab_ip.is_some() { "vpn" } else { "published" },
            "host": shown.then_some(host),
            "machine": row.machine,
            "port": (shown && !row.machine).then_some(row.host_port).flatten(),
            "services": if shown && !row.machine { services(row.extra_ports.as_deref()) } else { json!([]) },
            "remaining_secs": (row.expires_at - now).max(0),
        }));
    }
    Ok(json!({ "instances": out }))
}
