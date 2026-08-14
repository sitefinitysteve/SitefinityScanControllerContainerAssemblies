# Publishing to NuGet

Maintainer instructions. If you are looking for the text shown on the nuget.org package
page, that is [`packaging/PACKAGE.md`](packaging/PACKAGE.md).

## One-time setup

**1. Check the package id is free.** Visit
`https://www.nuget.org/packages/SitefinityAssemblyScanner`.

If you change the id in `packaging/SitefinityAssemblyScanner.nuspec`, you **must** rename
`packaging/build/<id>.targets` to match. NuGet only auto-imports a targets file whose
filename equals the package id exactly — get this wrong and the package installs cleanly
but does nothing.

**2. Create an API key** at nuget.org → your account → API Keys → Create:

- Scope: *Push new packages and package versions*
- Glob pattern: `SitefinityAssemblyScanner*`
- Keys expire after at most 365 days and need rotating

**3. Add it to GitHub**, never to a file: repo → Settings → Secrets and variables →
Actions → New repository secret, named `NUGET_API_KEY`.

**4. Optional but recommended.** Repo → Settings → Environments → `nuget-publish` → add
yourself as a required reviewer. The release job then pauses for approval before anything
reaches nuget.org.

## Test the package before publishing

Worth doing at least once, because **published versions are permanent** — they can be
unlisted but never deleted or overwritten.

```bash
cargo build --release
mkdir -p packaging/tools && cp target/release/*.exe packaging/tools/
nuget pack packaging/SitefinityAssemblyScanner.nuspec -Version 0.0.1-local -OutputDirectory artifacts
nuget add artifacts/*.nupkg -Source C:/localfeed
```

Add `C:\localfeed` as a package source in Visual Studio, install into a real Sitefinity
project, build, and confirm:

- the build log shows this tool's summary line, not a PowerShell scan
- `bin\ControllerContainerAsembliesLocation.json` is regenerated and **unchanged** from
  what the stock script produced
- the site still starts and every widget appears

That last point is the one that matters. A missing entry makes a widget disappear
silently rather than failing loudly.

## Releasing

The version in `Cargo.toml` must match the tag, or CI fails the release deliberately.

```bash
# bump version = "x.y.z" in Cargo.toml, then:
git commit -am "Release v0.1.0"
git tag v0.1.0
git push origin main --tags
```

CI then:

1. builds `x86_64-pc-windows-msvc` on `windows-latest`
2. runs the full test suite, and refuses to publish if anything fails
3. packs the `.nupkg`
4. pushes to nuget.org using `NUGET_API_KEY`
5. attaches the binary and the `.nupkg` to a GitHub release

The published binary is always the MSVC build from CI, regardless of which toolchain you
use locally.

## Publishing by hand

Only if CI is unavailable. Pack as above with a real version, then:

```bash
dotnet nuget push artifacts/*.nupkg \
  --api-key "$NUGET_API_KEY" \
  --source https://api.nuget.org/v3/index.json \
  --skip-duplicate
```

Pass the key via an environment variable. Never put it on a command line that lands in
shell history, and never in a committed `NuGet.config` — both `nuget setapikey` and
`dotnet nuget add source -p` write **plaintext** credentials into that file, which is why
it is gitignored.

## Things to know

- **Versions are permanent.** Use a `-local` or `-preview` suffix while experimenting.
- New packages take a few minutes to index before they become installable.
- The package is marked `developmentDependency`, so it will not flow to consumers of the
  host project.

---

By **Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**
[github.com/sitefinitysteve](https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies)
