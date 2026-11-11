---
name: ossia
description: Manage, edit, and export scores in a local Ossia library using its MCP tools.
---

# Ossia

Use this workflow when the user asks to browse, import, inspect, edit, render, or version a score in Ossia.

1. Call `ossia_status` or `ossia_guide` to confirm the connected runtime and current capabilities.
2. Call `ossia_list` to inspect the library. Use `ossia_import` with an absolute path when the user wants to add a file.
3. Call `ossia_open` with the selected `documentId`. Open the returned one-time URL in the host's browser when available; otherwise give the URL to the user. Keep the returned `viewId` for this task.
4. Call `ossia_context` with that exact `viewId` to read the document, selection, source hash, and source path. Do not use another session's recent selection.
5. Call `ossia_edit` with action `begin`, the current `baseVersionId`, and the exact `expectedSourceHash`. Keep `editId` private.
6. Edit the returned `sourcePath` with your normal file tools. Do not write a second copy or bypass a host filesystem permission refusal.
7. Call `ossia_check` with the exact new source hash. Poll `ossia_job` until the check reaches a terminal state, and read compile or musical-difference diagnostics. An unavailable difference result is not proof of no change.
8. Call `ossia_commit` with the same `editId`, base version, source hash, and a short summary. If it conflicts, reread context and do not overwrite another task.
9. Report the committed version, what was actually checked, and any export path. Interrupted or abandoned work keeps the dirty file unless the user explicitly asks to discard it.

Lossy imports require the confirmation hash returned by the host. Refused imports cannot be forced. History and export use `versionId`; restore creates a new version and requires a clean workspace.

Source locations are UTF-8 byte spans, pages and measures are one-based, staves are zero-based, and page geometry uses SVG user units. Runtime restart invalidates old views and page tokens.
