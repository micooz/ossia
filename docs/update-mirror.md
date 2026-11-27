# Self-hosted update server

This document defines the HTTP interface for a compatible Ossia update server.
The backing storage, web server, framework and CDN are implementation details;
any HTTPS server that implements these requests and responses is compatible.

## Endpoints

The client requests one manifest for its current target:

```http
GET <base-url>/<target>/index.json
```

It then downloads the selected installer from the URL advertised by the
manifest. The installer URL may be relative to the manifest URL or absolute.
Both the manifest and installer must be reachable over HTTPS without
authorization.

Release targets and canonical installer names:

| Target          | Asset                     |
| --------------- | ------------------------- |
| `darwin-arm64`  | `ossia-darwin-arm64.zip`  |
| `darwin-amd64`  | `ossia-darwin-amd64.zip`  |
| `windows-amd64` | `ossia-windows-amd64.exe` |

## Manifest request

Request headers:

```http
Accept: application/json
```

A successful response must use an HTTP 2xx status and
`Content-Type: application/json`. A missing target manifest should return 404.

The response body is:

```json
{
  "schemaVersion": 1,
  "target": "darwin-arm64",
  "releases": [
    {
      "version": "0.2.0",
      "prerelease": false,
      "body": "<!-- lang:zh -->\n...",
      "assets": [
        {
          "name": "ossia-darwin-arm64.zip",
          "url": "0.2.0/ossia-darwin-arm64.zip",
          "size": 51085793,
          "digest": "sha256:..."
        }
      ]
    }
  ]
}
```

Manifest fields:

- `schemaVersion` must be `1`. Unknown values are rejected.
- `target` must equal the target requested in the URL.
- `releases` is the candidate list for that target. Newest-first order is
  recommended; the client also sorts by SemVer.
- `releases[].version` is the version without a leading `v`.
- `releases[].prerelease` controls stable-channel visibility. The stable channel
  excludes prereleases and versions with a pre-release suffix.
- `releases[].body` contains optional bilingual release notes. Use
  `<!-- lang:zh -->` to separate English and Chinese.
- `releases[].assets` lists downloadable files for that release.
- `assets[].name` is the complete file name selected by the client. It must
  match the platform table above.
- `assets[].url` is resolved relative to the manifest URL unless absolute.
- `assets[].size` is the exact byte length.
- `assets[].digest` is mandatory and uses `sha256:<64 lowercase hex>`.

The client filters releases by channel, selects the highest compatible version,
downloads the matching asset, verifies its size and SHA-256, and only then stages
it for installation.

## Installer response

The installer endpoint must return:

- an HTTP 2xx status;
- the exact bytes described by the manifest;
- a `Content-Length` matching `assets[].size`;
- HTTPS for the final response, including after redirects.

`HEAD`, byte-range requests and redirects are supported; the final response must
still use HTTPS. Clients may cache immutable versioned installers. A short cache
lifetime is appropriate for `index.json` because it changes when releases are
added.

## Hosting behavior

The server only needs to provide the manifest and installer bytes. It does not
need a release API, authentication, database or specific storage product.

Keep historical installers at stable URLs. Return 404 for missing objects and do
not rewrite an existing version to different bytes; the client treats versions
as immutable and verifies every download against the manifest digest.

## Verification

```sh
curl -fsSI https://<your-host>/<target>/index.json
curl -fsS https://<your-host>/<target>/index.json
curl -fsSI https://<your-host>/<target>/<version>/<installer>
curl -fsS -o /tmp/ossia.zip \
  https://<your-host>/<target>/<version>/<installer>
shasum -a 256 /tmp/ossia.zip
```

Compare the downloaded SHA-256 with `assets[].digest`.
