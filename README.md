# Lerke

Homelab incident monitor and Discord notification hub. Receives alert webhooks from Grafana and Uptime Kuma, tracks incident lifecycle (firing/resolved), posts Discord embeds with threaded updates, and provides a web UI for viewing incidents. Also exposes a REST API so other homelab services can post Discord messages without their own Discord integration.

Named after the Danish word for lark (the bird), because it watches and alerts.

## Features

- **Grafana webhook receiver** -- creates incidents from Grafana alerts, tracks firing/resolved lifecycle
- **Uptime Kuma webhook receiver** -- creates incidents from Uptime Kuma down/up heartbeats
- **Discord notifications** -- colored embed per incident (red=firing, green=resolved), threaded updates
- **Web UI** -- homepage shows active incidents (calm "all clear" when empty), history page grouped by date, incident detail with metadata/events timeline
- **REST API** -- other homelab services can send Discord messages via simple HTTP calls
- **Dead man's switch** -- heartbeats to an Uptime Kuma push monitor every 5s
- **Prometheus metrics** -- exposed at `/api/metrics`
- **Label variable substitution** -- alert titles and annotations support `{{ label_name }}` template syntax
- **URL annotations** -- annotation keys ending in `_url` are automatically rendered as links
- **Grafana URL rewriting** -- replaces Grafana's internal `localhost:3000` URLs with your external domain

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `DISCORD_BOT_TOKEN` | Yes | -- | Discord bot token. Create a bot at https://discord.com/developers/applications |
| `DISCORD_CHANNEL_ID` | Yes | -- | Discord channel ID (u64) where incidents and messages are posted |
| `UPTIME_KUMA_PUSH_URL` | Yes | -- | Uptime Kuma push monitor URL, hit every 5s as a dead man's switch |
| `DATABASE_URL` | No | `sqlite:data/lerke.db?mode=rwc` | SQLite database path |
| `BIND_ADDRESS` | No | `0.0.0.0:8080` | HTTP server bind address |
| `GRAFANA_URL` | No | -- | External Grafana base URL (e.g. `https://grafana.home.coolkev.com`). Rewrites `localhost:3000` URLs from webhooks |
| `LERKE_URL` | No | -- | External Lerke base URL (e.g. `https://lerke.home.coolkev.com`). Adds "Lerke" link in Discord embeds pointing to the web UI |
| `UPTIME_KUMA_URL` | No | -- | External Uptime Kuma base URL (e.g. `https://kuma.home.coolkev.com`). Constructs dashboard links for Uptime Kuma incidents |

## Configuring Integrations

### Discord Bot

1. Go to https://discord.com/developers/applications and create a new application
2. Go to Bot settings, create a bot, and copy the token -> `DISCORD_BOT_TOKEN`
3. Enable the "Message Content Intent" under Privileged Gateway Intents
4. Go to OAuth2 -> URL Generator, select scopes `bot`, and permissions: `Send Messages`, `Manage Messages`, `Create Public Threads`, `Send Messages in Threads`, `Embed Links`, `Read Message History`
5. Use the generated URL to invite the bot to your server
6. Right-click the target channel -> Copy Channel ID -> `DISCORD_CHANNEL_ID`

### Grafana

1. In Grafana, go to Alerting -> Contact Points -> New Contact Point
2. Set type to **Webhook**
3. Set URL to `http://<lerke-host>:8080/api/webhooks/grafana`
4. Leave method as POST, no auth needed on LAN
5. Assign the contact point to your notification policies

Grafana sends alerts with `dashboardURL`, `panelURL`, `silenceURL`, and `generatorURL` fields. If `GRAFANA_URL` is set, Lerke rewrites the domain portion of these URLs. Non-filtered alert labels are also appended as `&var-<label>=<value>` query params to dashboard and panel URLs for Grafana variable pre-selection.

**Alert title templating:** If your Grafana alert rule name contains `{{ label_name }}`, Lerke substitutes it with the label value. For example, an alert named `{{ job_template }} failed` with label `job_template=backup-daily` becomes `backup-daily failed`.

**Annotation templating:** Annotation values also support `{{ label_name }}` substitution. This is useful for constructing dynamic URLs, e.g. an annotation `jobfather_url` set to `https://jobs.example.com/templates/{{ namespace }}/{{ job_template }}` will have both placeholders replaced.

**URL annotations:** Any annotation key ending in `_url` is rendered as a clickable link (not shown in metadata). The link label is the key with the `_url` suffix removed, in sentence case. For example, `jobfather_url` becomes a link labeled "Jobfather".

**Filtered labels:** The labels `alertname`, `grafana_folder`, and `priority` are excluded from the Discord embed and web UI metadata displays.

### Uptime Kuma

1. In Uptime Kuma, go to Settings -> Notifications -> Setup Notification
2. Set type to **Webhook**
3. Set URL to `http://<lerke-host>:8080/api/webhooks/uptime-kuma`
4. Content Type: `application/json`
5. Assign the notification to your monitors

Incidents are created when `heartbeat.status == 0` (down) and resolved when `heartbeat.status == 1` (up). If `UPTIME_KUMA_URL` is set, a "Kuma" link is added to the Discord embed and web UI pointing to `{UPTIME_KUMA_URL}/dashboard/{monitor_id}`. If the monitored URL is non-empty (and not just `https://`), it is shown as a "Site" link.

### Dead Man's Switch

Create a **Push** type monitor in Uptime Kuma with a heartbeat interval of 10s (or similar). Copy the push URL and set it as `UPTIME_KUMA_PUSH_URL`. Lerke hits this URL every 5 seconds. If Lerke goes down, Uptime Kuma will alert after the monitor's timeout.

## REST API

All endpoints post to the Discord channel configured by `DISCORD_CHANNEL_ID`. No authentication required (designed for trusted LAN use).

### Create a message

```
POST /api/messages
Content-Type: application/json

{
  "system": "Backup",          // required - shown as embed title
  "text": "Nightly backup started",  // required - shown as embed body
  "color": "#58a6ff"           // optional - hex color, defaults to grey
}

Response: { "message_id": "1234567890" }
```

### Update a message

```
PUT /api/messages/{message_id}
Content-Type: application/json

{
  "system": "Backup",
  "text": "Backup completed successfully",
  "color": "#3fb950"
}

Response: { "ok": true }
```

### Reply in a thread

Creates a thread on the message if one doesn't exist, then posts the reply.

```
POST /api/messages/{message_id}/reply
Content-Type: application/json

{
  "text": "Processing step 2 of 5..."
}

Response: { "message_id": "1234567891", "thread_id": "1234567890" }
```

### Delete a message

```
DELETE /api/messages/{message_id}

Response: { "ok": true }
```

### Example: long-running job notification

```bash
# Start
MSG=$(curl -s -X POST http://lerke:8080/api/messages \
  -H 'Content-Type: application/json' \
  -d '{"system":"Backup","text":"Starting nightly backup","color":"#58a6ff"}')
ID=$(echo $MSG | jq -r .message_id)

# Progress updates in thread
curl -s -X POST http://lerke:8080/api/messages/$ID/reply \
  -H 'Content-Type: application/json' \
  -d '{"text":"Dumping database..."}'

curl -s -X POST http://lerke:8080/api/messages/$ID/reply \
  -H 'Content-Type: application/json' \
  -d '{"text":"Uploading to S3..."}'

# Update embed to show success
curl -s -X PUT http://lerke:8080/api/messages/$ID \
  -H 'Content-Type: application/json' \
  -d '{"system":"Backup","text":"Nightly backup completed (2.3 GB, 45s)","color":"#3fb950"}'
```

## Web UI

| Route | Description |
|---|---|
| `/` | Redirects to `/incidents` |
| `/incidents` | Active (firing) incidents. Shows calm "all clear" when empty |
| `/incidents/{id}` | Incident detail: summary, description, metadata, Grafana/custom links, event timeline |
| `/history` | All incidents grouped by date (Eastern time) |
| `/debug/webhooks` | Last 50 raw webhook payloads received (auto-refreshes) |
| `/debug/purge` | (POST) Deletes all bot messages from Discord channel and all data from the database |
| `/api/metrics` | Prometheus metrics |

## Webhook Endpoints

| Route | Source | Description |
|---|---|---|
| `POST /api/webhooks/grafana` | Grafana | Receives Grafana alerting webhooks |
| `POST /api/webhooks/uptime-kuma` | Uptime Kuma | Receives Uptime Kuma notification webhooks |

## Architecture

- **Rust** with actix-web 4, maud templates, HTMX + idiomorph for the web UI
- **SQLite** (via sqlx) with WAL mode for the incident database, stored at `data/lerke.db`
- **serenity** for Discord REST API (HTTP-only, no gateway connection)
- **Prometheus** metrics via OpenTelemetry
- **tokio::select!** runs the HTTP server and dead man's switch heartbeat loop concurrently

### Project Structure

```
src/
  main.rs           -- entry point, server setup, tokio::select!
  config.rs         -- environment variable parsing
  error.rs          -- central error type (AppError)
  metrics.rs        -- Prometheus counters
  deadman.rs        -- Uptime Kuma heartbeat loop
  lib.rs            -- serve_static_file! macro
  api/messages.rs   -- REST API for Discord messages
  db/models.rs      -- Incident, IncidentEvent, AppState
  db/queries.rs     -- database CRUD operations
  discord/notifier.rs -- Discord embed/thread management
  webhooks/grafana.rs -- Grafana webhook handler + incident lifecycle
  webhooks/uptime_kuma.rs -- Uptime Kuma webhook handler
  web/              -- HTMX web UI (incidents, history, detail, debug)
migrations/         -- SQLite schema migrations
static/             -- CSS, htmx.min.js, idiomorph
.deploy/            -- Kubernetes manifests for homelab deployment
```

## Development

```bash
# Required env vars for local dev
export DISCORD_BOT_TOKEN="your-bot-token"
export DISCORD_CHANNEL_ID="your-channel-id"
export UPTIME_KUMA_PUSH_URL="https://kuma.example.com/api/push/xxx"

# Optional
export GRAFANA_URL="https://grafana.home.coolkev.com"
export LERKE_URL="http://localhost:8080"
export UPTIME_KUMA_URL="https://kuma.home.coolkev.com"

cargo run
```

The web UI is at `http://localhost:8080`. Send a test Grafana webhook:

```bash
curl -X POST http://localhost:8080/api/webhooks/grafana \
  -H 'Content-Type: application/json' \
  -d '{
    "status": "firing",
    "alerts": [{
      "status": "firing",
      "labels": {"alertname": "TestAlert", "instance": "localhost"},
      "annotations": {"summary": "Test alert firing"},
      "fingerprint": "test123",
      "startsAt": "2024-01-01T00:00:00Z",
      "dashboardURL": "http://localhost:3000/d/test",
      "panelURL": "http://localhost:3000/d/test?viewPanel=1"
    }]
  }'
```

## Deployment

Deployed to the homelab k3s cluster via the CICD controller. Push to `master` triggers GitHub Actions to build the Docker image, then the CICD controller picks up the `.deploy/` config and deploys to `lerke.home.coolkev.com`.

Secrets are managed via 1Password Connect (`OnePasswordItem` CRD), persistent data stored on NFS at `/data/services/lerke`.
