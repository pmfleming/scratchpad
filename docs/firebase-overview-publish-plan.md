# Firebase Overview Publish Plan

## Purpose

Publish the current Scratchpad measurement overview to Firebase Hosting as a
static snapshot. Firebase should not regenerate metrics, run local scripts, read
the app package, or call the local dashboard API. It should only host the
webpages and JSON/SVG artifacts that already exist at publish time.

The local dashboard remains the source of refresh and measurement generation.
Publishing is a separate copy-and-deploy step that takes the current local
viewer state and moves it to the web.

## Target Behavior

- The deployed site opens the existing `viewer/` dashboard.
- The dashboard reads uploaded artifacts from `target/analysis/`.
- All normal static interactions continue to work: tabs, filters, tables,
  charts, map controls, run-log display from uploaded `measurement_runs.json`,
  and flamegraph display when SVGs were uploaded.
- Refresh controls are disabled or hidden on Firebase.
- The hosted viewer makes no `/api/...` requests.
- Firebase Hosting is static-only; no Firebase Functions are required for the
  first version.

## Non-Goals

- Do not regenerate JSON artifacts during Firebase publish.
- Do not expose local filesystem paths, local session package APIs, or buffer
  clearing operations on Firebase.
- Do not make Firebase the measurement runner.
- Do not require a database, authentication, or cloud scheduler for the initial
  publish flow.

## Static Bundle Layout

Use a generated publish directory, for example `firebase-public/`:

```text
firebase-public/
  viewer/
    index.html
    data-viewer.js
    styles.css
  target/
    analysis/
      measurement_catalog.json
      measurement_runs.json
      hotspots.json
      clones.json
      rust_escape_hatches.json
      locality_metrics.json
      leverage_metrics.json
      slowspots.json
      search_speed.json
      capacity_report.json
      resource_profiles.json
      speed_efficiency_report.json
      performance_review.json
      correctness_review.json
      test_catalog.json
      map.json
      project_code_metrics.json
      flamegraphs.json
      flamegraphs/
        *.svg
```

This layout intentionally preserves the viewer's existing relative JSON fetch
shape. From `/viewer/`, paths like `../target/analysis/hotspots.json` resolve to
the uploaded Firebase asset at `/target/analysis/hotspots.json`.

## Firebase Project Files

Add Firebase Hosting configuration once the target project is known:

```text
firebase.json
.firebaserc
```

Expected `firebase.json` shape:

```json
{
  "hosting": {
    "public": "firebase-public",
    "ignore": [
      "firebase.json",
      "**/.*",
      "**/node_modules/**"
    ],
    "cleanUrls": true,
    "trailingSlash": false
  }
}
```

`firebase.json` and `.firebaserc` are normally safe to commit after review
because they should contain project identity and hosting settings, not private
keys. Review them before staging.

## Credential And Key Handling

Before downloading or creating Firebase keys, update `.gitignore` so generated
credentials cannot be accidentally committed.

Recommended ignore entries:

```gitignore
# Firebase local state and generated static publish output
.firebase/
firebase-public/

# Firebase credentials and local deploy environment
.env
.env.*
!.env.example
firebase-service-account*.json
*firebase-adminsdk*.json
serviceAccount*.json
google-application-credentials*.json
```

Notes:

- Firebase CLI login for local deploy usually stores credentials outside the
  repo, so no checked-in key file should be needed.
- If a service account key is created for CI, keep it out of the repository and
  store it in the CI secret manager.
- If a web app config is needed, treat it as publish configuration rather than a
  private admin key. Commit only after confirming it contains no service account
  private key or deploy token.
- Never copy credential files into `viewer/`, `target/analysis/`, or
  `firebase-public/`.

## Hosted Mode

Add a hosted/static mode to the viewer. It can be implemented either by writing a
small generated config file into the publish bundle or by patching the copied
`viewer/index.html` during the publish script.

Preferred generated file:

```html
<script>
  window.SCRATCHPAD_VIEWER_HOSTED = true;
</script>
```

Hosted mode should do the following:

- Disable or hide all elements with `data-run`, `data-run-category`, and
  `data-run-item`.
- Disable or hide `#app-package-refresh`.
- Disable or hide `#app-package-clear-buffers`.
- Skip event listener registration for refresh and clear actions.
- Skip the five-second `refreshRuns` API polling interval.
- Skip default loading of `/api/app-package` unless a static app-package export
  is added later.
- Replace local-refresh copy with static-snapshot copy, for example:
  `Static Firebase snapshot. Refresh locally, then publish again.`

## API Calls To Remove In Hosted Mode

The hosted Firebase viewer must not call these local dashboard endpoints:

```text
/api/runs
/api/run/all
/api/run/category/*
/api/run/item/*
/api/run/*/log
/api/app-package
/api/app-package/clear-buffers
```

For run logs, the first Firebase version can rely on the already uploaded
`measurement_runs.json` metadata and show "log unavailable in hosted snapshot"
when a user selects a run. A later version can copy
`target/analysis/logs/*.log` and teach the viewer to load those static text
files instead.

## Publish Script

Add a script such as:

```text
scripts/publish-overview-firebase.ps1
```

Script responsibilities:

1. Resolve the repo root.
2. Verify `viewer/index.html`, `viewer/data-viewer.js`, and `viewer/styles.css`
   exist.
3. Verify `target/analysis/` exists.
4. Remove and recreate `firebase-public/`.
5. Copy `viewer/` into `firebase-public/viewer/`.
6. Copy `target/analysis/` into `firebase-public/target/analysis/`.
7. Add hosted mode to the copied viewer.
8. Optionally write a snapshot metadata file, for example
   `firebase-public/target/analysis/publish_snapshot.json`, with publish time,
   git branch, git commit, and artifact counts.
9. Run a local static validation pass.
10. Run `firebase deploy --only hosting` when validation passes.

The script should not run measurement producers. If the JSON artifacts are stale,
the user should refresh locally first with `scripts/open-overview.ps1` or the
dashboard refresh controls, then run the publish script.

## Local Validation

Before deploying, serve `firebase-public/` with a plain static server:

```powershell
.venv\Scripts\python.exe -m http.server 8080 -d firebase-public
```

Then inspect:

```text
http://127.0.0.1:8080/viewer/
```

Validation checklist:

- The Overview tab loads.
- At least one quality, performance, correctness, and map artifact renders.
- Refresh buttons are hidden or disabled.
- No network requests target `/api/...`.
- Missing optional artifacts, such as flamegraphs, degrade quietly.
- Flamegraph SVGs load when `target/analysis/flamegraphs/` was copied.
- Browser console has no repeated fetch loop failures.

## Firebase Deploy Flow

Initial setup:

```powershell
firebase login
firebase init hosting
```

Use `firebase-public` as the public directory. Do not configure as a single-page
app unless the viewer routing changes; the dashboard currently uses real static
paths.

Normal publish:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\publish-overview-firebase.ps1
```

Manual fallback:

```powershell
firebase deploy --only hosting
```

## Implementation Phases

### Phase 1: Static Bundle

- Add `.gitignore` entries for Firebase local state, generated publish output,
  and credential files.
- Add `firebase.json` after choosing the Firebase project.
- Add the publish script that copies the current viewer and current
  `target/analysis/` artifacts without regenerating anything.
- Confirm local static serving can load `/viewer/`.

### Phase 2: Hosted Mode

- Add `window.SCRATCHPAD_VIEWER_HOSTED` handling in `viewer/data-viewer.js`.
- Disable or hide refresh buttons in hosted mode.
- Skip all local dashboard API calls in hosted mode.
- Update hosted load-status text so users understand they are viewing a
  published snapshot.

### Phase 3: Static Run Logs

- Decide whether run logs matter on the public snapshot.
- If yes, copy `target/analysis/logs/*.log` into the bundle.
- Change hosted `loadRunLog` to fetch copied log files from static paths.
- If no, keep run-log metadata visible and show a hosted-mode message when logs
  are selected.

### Phase 4: Deploy And Document

- Run the publish script locally.
- Validate with a static server.
- Deploy to Firebase Hosting.
- Add the Firebase URL to the README or measurement docs once stable.

## Acceptance Criteria

- `firebase-public/` can be deleted and recreated by script.
- Firebase deployment contains the same dashboard pages and current JSON
  artifacts available locally at publish time.
- Hosted dashboard does not expose working refresh controls.
- Hosted dashboard does not call local-only `/api/...` routes.
- No Firebase keys, service account JSON files, tokens, `.env` files, or
  generated publish output are staged.
- A future refresh remains a two-step workflow: refresh locally, then publish the
  snapshot.
