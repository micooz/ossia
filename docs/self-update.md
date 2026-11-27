# Self-update

The desktop management process checks the selected source, downloads the asset
for the current platform, verifies it, and replaces the application. The default
source is the built-in Ossia feed; GitHub Releases for `micooz/ossia` remain
selectable. The score page and local backend do not contact either source.

## Access

Ossia checks and downloads work anonymously. A fine-grained GitHub token with
read-only **Contents** access to `micooz/ossia` is optional and is used only
when the GitHub source is selected. The token is stored in
`<data-directory>/preferences.json` and is never returned to the UI. The
management window shows only a masked suffix.

When configured, the token is sent only to `https://api.github.com`. GitHub
release assets are requested through the asset API; the signed redirect is
followed without the token. The Ossia source never sends an authorization
header. Only HTTPS is accepted outside loopback test servers.

The feed contract and a self-hosted mirror guide are in
[`update-mirror.md`](update-mirror.md).

## Channels and build eligibility

- `stable` accepts only non-prerelease releases.
- `beta` accepts beta releases and stable releases.

Stable is the default. A beta build defaults to beta. Debug builds can manually
check and download the latest release in the selected channel, even when its
version is equal to or older than the build version. Downloads use the normal
asset selection and checksum verification; debug builds cannot install them.

Distributable packages set the build stamp from their package version; an
explicit `OSSIA_BUILD_STAMP` must match it. Development builds that bypass the
package script fall back to an exact Git tag or `git describe`. Only non-debug
builds with an exact release tag can replace themselves. Other non-debug builds
report `source`.

## Lifecycle

The GUI performs one launch check five seconds after startup. While the
management process remains running, it checks every 24 hours on the stable
channel and every 6 hours on the beta channel. Manual checks are always
available. Changing the source, channel or automatic-download preference takes
effect on the next check; it does not start one immediately. Debug builds skip
scheduled checks; a manual check downloads when automatic download is enabled.
Other source builds stop the scheduler after reporting `source`.

Automatic download is enabled by default; a found release downloads immediately
and waits for the user. Closing or restarting Ossia without choosing **Update**
discards the download and keeps the current version.

Downloads can be cancelled from the About page or title bar. Cancellation removes
the partial download and allows changing the source; a later check uses the
selected source.

Only one check, download, or install may run at a time. A verified download
survives a failed later check, but not a process restart. A scheduled check that
finds another operation in progress retries after one hour. Failed checks retry
after 1, 2, 4, 8, 16, then 24 hours; a successful check resets the delay.

The last completed check is shared with Agent status in
`<data-directory>/update/status.json`. It includes the checked version, source,
channel and Unix check time, but no credentials or release notes. MCP
`ossia_status` and CLI `status` mark results older than the channel's check
interval as stale. Changing the source or channel invalidates the result; on
desktop startup, a different running build also invalidates it.

## Verification and installation

The updater matches the complete release asset name for the current target:

| Platform       | Asset                     |
| -------------- | ------------------------- |
| macOS arm64    | `ossia-darwin-arm64.zip`  |
| macOS Intel    | `ossia-darwin-amd64.zip`  |
| Windows x86_64 | `ossia-windows-amd64.exe` |

DMGs are not selected. GitHub's asset `digest` is
used when present; otherwise the updater reads `SHA256SUMS`. No digest and no
matching checksum entry is an error.

- macOS: `ditto` extracts the ZIP into `.ossia-update` beside the `.app`, then
  swaps the bundle and restores the old bundle if the new one cannot be moved
  into place.
- Windows: the staged NSIS installer starts with `/S /UPDATE`; a manual update
  adds `/R` and preserves arguments with `/ARGS`.

Before an install, Ossia stops the local backend and cancels running tasks;
uncommitted source files are preserved. A failed install restarts the backend if
it was running before. A successful install schedules the new executable with
the original arguments and exits. The relaunch log is
`<data-directory>/logs/relaunch.log`.

## Release notes

The cache is `<data-directory>/update/releases.json`. It retains the current
source's recent release bodies and selects the range from the installed version
to the target, capped at five releases. Source and debug builds show the newest
five visible releases; on-demand note checks in debug builds include prereleases
even when the saved channel is stable. If no notes are cached, **Open** loads
them from the selected source.

Release bodies use `<!-- lang:zh -->` to separate English and Chinese and stop at
`### Installation` or `### 安装说明`. Releases marked
`<!-- ossia:internal-only -->` are retained for range selection but excluded from
the dialog; an entirely maintenance-only range shows one localized notice.

The dialog renders a restricted Markdown subset. Images are reduced to alt text,
non-HTTPS links are removed, and HTTPS links open through the system browser.

## Tauri surface

Only the `management` window has these commands:

- `update_state`
- `update_check`
- `update_cancel`
- `release_notes`
- `update_apply`
- `update_token_set`
- `open_external_url`

Snapshots are emitted on `ossia://update`. The states are `idle`, `checking`,
`auth_required`, `latest`, `source`, `no_release`, `found`, `downloading`,
`ready`, `installing`, and `error`.

## Limits

There is no resumable download or embedded release credential. macOS and
Windows packages are unsigned; Gatekeeper or SmartScreen may still warn.
