# Downstream browser-environment integration

This branch is an unofficial development variant of [liseami/CCBaga](https://github.com/liseami/CCBaga). Official releases remain the upstream owner's responsibility. It is not a new browser engine or a guarantee of service-side account/device independence.

The downstream layer adds named browser environments, explicit per-profile lifecycle checks, a unified selected-environment workspace, and read-only macOS browser discovery. Existing identity-linked behavior remains available as the built-in environment. Standalone environments do not switch Claude Code/Desktop identity.

The current Brave adapter is a compatibility path for an already-owned local profile. General browser import, cross-engine login preservation, per-environment report sessions, and multi-platform acceptance are not claimed. The UI does not display another profile's measurements as the selected environment's report.

## Maintenance

- Keep upstream fixes on focused branches that can be reviewed independently of this layer.
- Preserve the upstream default behavior and existing data locations. Share profile/process primitives rather than maintaining duplicate implementations.
- Fetch and preflight upstream only when explicitly preparing an update. A clean Git merge is not a build or functional acceptance result.
- Conflicts and failed checks stop the update; the installed application and browser data stay unchanged. No background watcher or scheduled job is required.
- Keep one known-good application rollback and remove temporary check artifacts after use.

Local macOS validation covers synthetic profiles, process ownership, native discovery and UI target routing. Linux/Windows validation for the downstream feature set remains outstanding; the focused upstream ownership fix is macOS-only.

## Explicit update preparation

The developer invokes `scripts/downstream/check_upstream_update.py` with a local JSON configuration only while fetching/preparing an update. It uses `git merge-tree` against the saved downstream ref; it does not merge into a checkout, install an application, or register a background task. A conflict stops the operation. A clean text merge still requires build and functional validation.

The configuration supplies the repository, expected upstream remote URL/branch, saved downstream ref, reviewed-source manifest and a temporary result directory. The latest result overwrites one `status.json`. On macOS an error alert is built only on demand and deleted after dismissal; successful checks do not display an alert. A local build environment with Python, Git and clang is required. This is developer-side update preparation, not a claim that the application's binary updater can merge source code.

`save_downstream_snapshot.py --reviewed` can record tested changes through a temporary index without changing the editing branch or its index. New files require explicit inclusion. The saved branch must not be checked out. Use its `--integrate-upstream` flag only after the new upstream source has actually been integrated and tested, so the saved merge ancestry reflects reality.

## Durable convergence (2026-10-28)

`downstream-overlay.json` records the exact official baseline and overlay contract. This branch merges upstream main with two-parent Git ancestry; `origin/main` remains pure upstream. PR #57 ownership primitives and #68 lock/unknown-metadata hardening are shared, while the downstream Brave identity adapter and per-environment lifecycle remain separate. Official 13 region presets are unchanged. Chicago is not silently mapped to Los Angeles and no additional preset is currently needed.

The project build entry must invoke `prepare_update.py` before packaging. It fetches the expected upstream, checks Git conflicts and source drift, makes a separate merge worktree for a clean new upstream, then runs Bun, targeted Rust/browser/region and native fixtures with a synthetic HOME. Every check has a log and a validation receipt. Text merges alone cannot authorize packaging. The multi edition also refuses official in-app binary updates.

State belongs in a persistent project `records/upstream-update` directory, not `_trash`. Schema v2 exposes `upstream_sha`, `downstream_sha`, `detection_points`, `repair_suggestions`, `event_id`, `last_handled_event_id` and `pending_event`. The existing daily maintenance task may read that JSON; after actually reporting/handling an event it invokes `check_upstream_update.py --config ... --ack-event EVENT_ID`. Repeated checks preserve acknowledgement. No scheduler or notification service is installed by this branch. Build/test failures stop before replacement.

`install_staged_app.py` accepts only the personal app identity and an exact-binary native acceptance receipt. It checks runtime occupancy, saves the old app by rename, journals the transaction and restores the old app on failure. Synthetic fixtures cover rollback and interrupted-rename recovery. This does not grant a real candidate GUI acceptance result. No profile migration or official A+B updater transaction is involved.

After verifying a clean candidate, `save_downstream_snapshot.py --reviewed --from-commit HEAD` can promote its actual commit to the saved downstream ref by fast-forward. It preserves two-parent upstream merge ancestry, refuses dirty candidates and non-fast-forward promotion, and leaves the editing HEAD/index untouched. The earlier explicit-new-file snapshot mode remains supported.
