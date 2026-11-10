# Native engine resources

`ENGINE.lock` is a versioned JSON lock shared by the Rust engine and the Node development installer. Runtime code does not download, unpack or repair resources. LilyPond and its converters are fixed at 2.26.0.

## Five target recipes

| Target | Preparation | Validation in P02 |
|---|---|---|
| darwin-arm64 | Official tar.gz; `pnpm engine:install` on Apple Silicon | Integrity, converters, SVG/PDF/MIDI, Chinese/space relocation, cache and diagnostics |
| darwin-x86_64 | Official tar.gz; `node scripts/engine.mjs install --target=darwin-x86_64 --download` | Archive download/hash, extraction and full resource inventory only |
| mingw-x86_64 | Official ZIP; `node scripts/engine.mjs install --target=mingw-x86_64 --download` | Archive download/hash, extraction and inventory; Rust Windows target static check |
| linux-x86_64 | Official tar.gz; `node scripts/engine.mjs install --target=linux-x86_64 --download` | Archive download/hash, extraction and inventory only |
| linux-arm64 | Native source recipe below | Inputs and recipe preparation only; no Linux environment started |

Run each preparation command from the repository, or call the script by absolute path from another cwd. The target flag selects a resource bundle, not emulation. Foreign target preparation never executes its tools. `node scripts/engine.mjs verify --target=<target>` checks the selected tree; only the current host target runs the smoke test.

The installer verifies the archive before extraction, prepares in a sibling staging directory, inventories and verifies the result, then replaces an existing development tree. A failed download/preparation preserves the previous tree. An explicit `OSSIA_LILYPOND_ROOT` is authoritative for `verify`/`path`, with no fallback. Rust receives its root explicitly.

The complete upstream tree is retained: Python 3.14.3, Guile modules, Ghostscript, standard includes, music/text fonts, libraries and licenses. Converters run as `libexec/python3.14 libexec/musicxml2ly` (or `midi2ly`) on Unix; Windows uses `bin/python.exe bin/musicxml2ly.py` (or `midi2ly.py`). No shell wrapper or user Python is required. The archive checksums were calculated from the actual downloaded official archives, not guessed from filenames.

`resources.json` records the build/target/archive identity and every immutable regular file's size, digest and executable flag. Installer preparation preserves archive permissions and repairs Guile `.go` mtimes in the writable staging tree. Bytecode caches are excluded and runtime bytecode generation is disabled. Finish preparation before signing; verification does not change installed files. Resource inventories are local integrity metadata, not a substitute for package signatures.

## Linux ARM64 source recipe

Use native Linux ARM64 with Python 3.12+, a C/C++ toolchain, GNU make, Autoconf, pkg-config, Bison, Flex, FontForge, gettext tools, gperf, Meson >=1.2, Ninja, archive tools and the upstream font-build prerequisites. The v2.26.0 upstream packaging scripts define build order, patches, static dependency options and font packaging. A known build image/toolchain must be recorded during real validation; no base image digest or successful build is claimed here.

```sh
# Read/download/verify inputs only; this also works on macOS and never starts Linux.
python3 scripts/engine-linux-arm64.py --work-dir /absolute/build/work --fetch-only

# On native Linux ARM64 only; no automatic VM/container startup.
python3 scripts/engine-linux-arm64.py --work-dir /absolute/build/work --jobs 8
```

`ENGINE.dependencies.json` pins the 20 enabled Linux dependency/font source archives and the full upstream packaging-script tree by SHA-256. The source tarball itself is pinned in `ENGINE.lock`. The recipe checks inputs, extracts trusted source with the data filter, fills the upstream downloads cache with verified archives, runs `build-dependencies` then `build-lilypond`, and keeps build logs. No upstream dependency download should be needed after preparation.

The native recipe produces `lilypond-2.26.0-linux-arm64.tar.gz` and a `built-not-validated` report in the chosen work directory. The lock deliberately has `archive: null` and an unbuilt identity for ARM64. A real build must be followed by these checks before adding its archive hash/build identity:

1. Audit ELF dependencies of `bin/lilypond`, `libexec/python3.14` and `libexec/gs` with `readelf`/`ldd`; include any non-fundamental shared libraries and licenses. Check that no build-directory paths are needed.
2. Prepare/inventory a relocated copy; run MusicXML/MIDI conversion and SVG/PDF/MIDI fixtures, including Chinese/space paths, with no external Python, Guile, fonts or LilyPond installation.
3. Record the native OS/architecture, build tools, dependency audit, logs and results; update the ARM64 lock entry with the actual archive hash and a unique build identity. Rebuild the Rust core so its embedded lock matches.
4. Re-run P00's native ARM64 gate. P02's macOS results do not close it.

P07 owns installation-package assembly, signatures and final five-target acceptance. Build dependencies used by developers are separate from runtime dependencies shipped to users.

Sources: [official downloads](https://lilypond.org/download.html), [v2.26.0 native packaging](https://github.com/lilypond/lilypond/tree/v2.26.0/release/binaries), [upstream build requirements](https://lilypond.org/doc/v2.26/Documentation/contributor/requirements-for-compiling-lilypond).
