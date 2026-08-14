# Publishing to NuGet.org

Maintainer instructions. For the text shown on the nuget.org package page, see
[`packaging/PACKAGE.md`](packaging/PACKAGE.md).

This repo publishes with **Trusted Publishing**, so there is **no API key to create,
store, or rotate**. That is why nuget.org now labels API Keys "Not recommended": a
long-lived key is a secret that can leak, and Trusted Publishing replaces it with a
short-lived credential issued per workflow run.

## How Trusted Publishing works

1. The release workflow asks GitHub for a signed OIDC token describing this repo and
   workflow.
2. It sends that token to nuget.org.
3. nuget.org validates it against a policy you registered and returns a **temporary API
   key, valid for one hour and usable once**.
4. The workflow pushes with that key.

Nothing long-lived is stored. If the repo is compromised, there is no key to steal.

## One-time setup

**1. Check the package id is free.** Visit
`https://www.nuget.org/packages/SitefinityAssemblyScanner`.

If you change `<id>` in `packaging/SitefinityAssemblyScanner.nuspec`, you **must** rename
`packaging/build/<id>.targets` to match. NuGet only auto-imports a targets file whose
filename equals the package id — get this wrong and the package installs cleanly but
silently does nothing.

**2. Register the Trusted Publishing policy.** On nuget.org → your username →
**Trusted Publishing** → Create:

| Field | Value |
|---|---|
| Policy Name | `SitefinityAssemblyScanner release` (any label — it is just for you) |
| Package Owner | `stevescotthome` |
| CI/CD Provider | GitHub Actions |
| Repository Owner | `sitefinitysteve` |
| Repository | `SitefinityScanControllerContainerAssemblies` |
| Workflow File | `release.yml` |
| Environment | `nuget-publish` |

Three things that catch people out:

- **Package Owner is your nuget.org account, not your GitHub one.** The policy applies to
  every package owned by whichever account you pick here, so choose deliberately if you
  belong to organisations.
- **Workflow File is the filename only.** Enter `release.yml`, *not*
  `.github/workflows/release.yml`.
- **Environment must match** what the workflow declares. `release.yml` uses
  `environment: nuget-publish`. Enter that, or leave the field blank to leave it
  unrestricted. A *different* value makes the token exchange fail.

**3. Add one repo secret.** Settings → Secrets and variables → Actions →

| Secret | Value |
|---|---|
| `NUGET_USER` | `stevescotthome` |

This must be your nuget.org **profile name** and must match the Package Owner above. It
is easy to get wrong, because it is none of the other identifiers you use:

| Not this | |
|---|---|
| `sitefinitysteve` | that is the GitHub account |
| `steve@sitefinitysteve.com` | that is the email address |

It isn't a credential — it only tells nuget.org which account to match the policy
against. It lives in a secret so the username isn't baked into a public workflow file.

**4. Optional approval gate.** Settings → Environments → `nuget-publish` → add yourself
as a required reviewer. The release job then pauses for approval before publishing.

### If the policy shows as pending

A new policy on a **private** repo starts "temporarily active" for 7 days. nuget.org needs
the GitHub repository and owner IDs — which only arrive with the first successful publish
— to pin the policy and prevent someone deleting and recreating a repo under the same
name. Publish once inside that window and it becomes permanent. If it lapses, you can
restart the window.

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
- `Build\` now contains the exe and its README
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

The workflow then:

1. builds `x86_64-pc-windows-msvc` on `windows-latest`
2. runs the full test suite, and refuses to publish if anything fails
3. packs the `.nupkg`
4. exchanges its OIDC token for a one-hour nuget.org key
5. pushes, and attaches the binary and `.nupkg` to a GitHub release

The published binary is always the MSVC build from CI, regardless of which toolchain you
use locally.

## If you ever need to publish by hand

Trusted Publishing only works from CI, since it depends on the OIDC token. For a local
push you would need a classic API key from nuget.org → API Keys, scoped to
*Push new packages and package versions* with glob `SitefinityAssemblyScanner*`:

```bash
dotnet nuget push artifacts/*.nupkg \
  --api-key "$NUGET_API_KEY" \
  --source https://api.nuget.org/v3/index.json \
  --skip-duplicate
```

Pass it via an environment variable. Never put it on a command line that lands in shell
history, and never in a committed `NuGet.config` — both `nuget setapikey` and
`dotnet nuget add source -p` write **plaintext** credentials into that file, which is why
it is gitignored. Delete the key from nuget.org once you are done.

## Things to know

- **Versions are permanent.** Use a `-local` or `-preview` suffix while experimenting.
- New packages take a few minutes to index before they become installable.
- The temporary key is valid for one hour and single-use, so the workflow requests it
  immediately before pushing rather than earlier in the job.
- The package is marked `developmentDependency`, so it will not flow to consumers of the
  host project.

---

By **Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**
[github.com/sitefinitysteve](https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies)
