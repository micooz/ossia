# Releasing Ossia

## GitHub setup

The release workflows expect the repository `micooz/ossia`, with `main` as the release branch. Configure:

- `DEEPSEEK_API_KEY` repository secret for bilingual release notes.
- `OSSIA_BOT_CLIENT_ID` and `OSSIA_BOT_APP_ID` repository variables, plus
  `OSSIA_BOT_PRIVATE_KEY` as a repository secret, for automatic tagging.
- `OSSIA_RELEASES_URL`, `OSSIA_RELEASES_BUCKET`, `OSSIA_RELEASES_REGION`,
  `OSSIA_RELEASES_SECRET_ID` and `OSSIA_RELEASES_SECRET_KEY` repository secrets for
  the Ossia update feed. `OSSIA_RELEASES_PREFIX`, `OSSIA_RELEASES_SECURITY_TOKEN`
  and `OSSIA_RELEASES_ACL` are optional secrets.
- A GitHub App installed on the repository with **Contents: write** permission.

Set the DeepSeek key without putting it in shell history:

```sh
gh secret set DEEPSEEK_API_KEY --repo micooz/ossia
```

Set the GitHub App credentials after downloading its PEM private key:

```sh
gh variable set OSSIA_BOT_CLIENT_ID --repo micooz/ossia --body '<client-id>'
gh variable set OSSIA_BOT_APP_ID --repo micooz/ossia --body '<app-id>'
gh secret set OSSIA_BOT_PRIVATE_KEY --repo micooz/ossia < /path/to/ossia-bot.pem
```

`Check` runs on pull requests and `main` pushes. After a successful `main`
check, `Auto tag` creates the next patch tag for the latest commit. Include
`[skip release]` in the first line of that commit message to skip automatic tagging.
The first release tag must be created manually.

Include `[skip ci]` in a commit message to skip GitHub Actions workflows triggered
by that push or pull request, including `Check` and website deployment. Without
a successful `Check`, the commit cannot trigger automatic tagging or satisfy the
release check gate. Manually dispatched workflows are unaffected. Skipped required
checks can remain pending and block pull request merges.

## Tag format

Use annotated tags in one of these forms:

```text
v0.1.0
v0.1.0-beta.1
```

Stable tags become the Latest release. Beta tags become Pre-releases and never
become Latest. Tags are immutable; a failed or superseded tag is abandoned and
the next beta or patch version is used.

Automatic tagging allocates the next stable patch above every existing release
tag, including beta tags. During beta testing, create and push beta tags
manually and use `[skip release]` on commits that should not trigger a stable
release.

## Release flow

1. Push to `main`. `Check` validates on macOS arm64 and Windows x86_64,
   including browser regression tests on both platforms. `Auto tag` creates
   the next stable patch tag after the check succeeds.
2. `Release` validates the annotated tag. Each package build temporarily injects
   the tag version into the manifests and lockfile.
3. Release waits for the latest successful `main` Check for the exact tagged
   commit. DeepSeek notes and offline documentation generation run in parallel;
   all platform builds use the generated documentation artifact.
4. Each build installs or extracts its installable artifacts and runs the same
   packaged smoke check.
5. The publish job creates a draft release and verifies its exact asset set.
6. The publish job publishes the GitHub release as a Pre-release or Latest release.
7. The separate `Ossia update feed` workflow verifies the published release and
   uploads versioned assets for all targets, including the macOS DMG downloads.
   The EdgeOne function in `scripts/release/edge-latest.js` redirects each
   platform's `latest` download URL to the newest stable version in its manifest.
   Deploy the function to EdgeOne when its routes change.

Packaging requires a clean Git working tree. Release version injection happens
after this check and the original files are restored when packaging ends.

The release targets are macOS arm64, macOS x86_64 and Windows x86_64.

## Package verification

Every platform uses one post-package smoke check. The script restores an
installed payload from the final artifact, then runs the packaged executable
with an isolated temporary `--home`:

- `--version` reports the packaged app version.
- `doctor` confirms the bundled engine, converters and LilyPond can run.
- `serve` starts a resident backend and `/` returns the Ossia page.
- `status` reports a `runtime.webBaseUrl`, then the backend is stopped.

Each target is built and checked on its native runner, so this executes the
target platform's own binary.

Payload restoration:

- macOS verifies the copied `.app`, the app inside the mounted DMG and the ZIP.
- Windows silently installs the NSIS package into a temporary directory, then
  uninstalls it.

The check also verifies the installed license and source materials. It does not
automate GUI startup, code signing or notarization.

## Release assets

Release assets:

- `ossia-darwin-arm64.dmg`
- `ossia-darwin-arm64.zip`
- `ossia-darwin-amd64.dmg`
- `ossia-darwin-amd64.zip`
- `ossia-windows-amd64.exe`
- `SHA256SUMS`

GitHub provides source archives for the release tag. Each installation includes
its platform's license tree, `SOURCE-MANIFEST.json`, `THIRD-PARTY-NOTICES.md`,
and `BUILD-INFO.json` under its resources `licenses` directory. The source
manifest identifies the exact commit and its GitHub archive URL. Test packages
are unsigned and not notarized.

## Self-update

Desktop clients default to the built-in Ossia update feed and can switch to
GitHub. Ossia checks and downloads are anonymous. A fine-grained GitHub token
with read-only **Contents** access to `micooz/ossia` is used only when GitHub is
selected.
Custom installations can host a compatible mirror as described in
[`update-mirror.md`](update-mirror.md).

The package script always sets `OSSIA_BUILD_STAMP` to the canonical tag derived
from the package version. An explicitly supplied value must match that version.
The release workflow supplies the annotated tag, while local packaging of the
committed version needs no separate environment variable. `BUILD-INFO.json` records
the resulting `buildStamp`. Stable releases are offered on the stable and beta
channels; pre-releases are offered only on beta.

The updater consumes the ZIP, NSIS and `SHA256SUMS` assets. DMGs are
manual-install artifacts.

For each target, include the generated release notes in the update manifest.
The About dialog reads release content from the manifest body.
The update feed contains installers. Corresponding source is available from the
GitHub release, and third-party materials are included in each installation.

Feed publication runs serially across workflow runs. Each target is updated
individually. The uploader reads the current manifest from COS before merging,
then uploads assets before the manifest using the COS global acceleration
endpoint. If publication fails, rerun `Ossia update feed` or start it manually
with `gh workflow run update-feed.yml --ref main -f tag=<tag>`. GitHub Release
remains available independently of the feed. An existing manifest entry cannot
be replaced with a different installer digest under the same version.

Run `pnpm dev:website` for local development, `pnpm build:website` to build into
`dist/website/`, and `pnpm preview:website` to preview the build. The website
workflow builds once and publishes the same output to GitHub Pages and the Ossia
object store. COS uploads use the existing `OSSIA_RELEASES_*` secrets and retain
old objects. HTML and fixed-path assets are cached for 60 seconds; hashed assets
are cached for one year with `immutable`. GitHub Pages controls its own cache
headers. Run `Deploy website` manually to retry either deployment.
