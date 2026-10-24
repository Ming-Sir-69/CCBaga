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
