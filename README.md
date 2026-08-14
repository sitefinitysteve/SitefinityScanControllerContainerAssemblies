<a href="https://www.sitefinitysteve.com/">
  <img src="sfslogo.png" alt="SitefinitySteve" width="420">
</a>

# SitefinitySteveScanControllerAssemblies

**Makes every Sitefinity build about 5 seconds faster.**

By **Steve McNiven-Scott** — [sitefinitysteve.com](https://www.sitefinitysteve.com/)

## Overview

Every time you build a Sitefinity site, Feather spends around 5 seconds working out which
assemblies contain MVC widgets. It does this by loading every single DLL in `bin` into a
CLR — hundreds of assemblies, several hundred megabytes — to read two attributes.

This replaces that step with a native executable that reads the same information straight
out of the assembly metadata, without loading anything.

| | Time |
|---|---|
| Sitefinity's built-in scan | ~5,000 ms |
| This | **~40 ms** |
| | **99.2% faster — about 125×** |

The output is **byte-identical**, so nothing downstream can tell the difference. Install
it and your builds are just faster.

- One self-contained executable, ~360 KB
- No PowerShell, no .NET runtime, nothing to install on build agents
- Zero configuration

## Installation

```
Install-Package SitefinityAssemblyScanner
```

**That's it. Build your project. You're done.**

There is nothing to configure. The package adds the executable and points Sitefinity's
build step at it instead of the PowerShell script.

`ScanControllerContainerAssemblies.ps1` stays exactly where it is, untouched — it just
isn't the thing being run any more.

### Do I need to add anything to my .csproj?

**No.** Nothing. The package ships an MSBuild targets file, NuGet imports it into your
project automatically, and that file does all the wiring for you.

If you've come across a manual snippet that defines a `ScanControllerContainerAssemblies`
target — **that is what this package does for you.** You don't need to paste it in.

<details>
<summary><b>Manual setup</b> — only if you can't use the NuGet package</summary>

<br>

Some people vendor the exe into their repo instead of installing the package. If that's
you, drop `SitefinitySteveScanControllerAssemblies.exe` into your site's `Build\` folder
and add this to your `.csproj`:

```xml
<!-- Replaces the ScanControllerContainerAssemblies target from Telerik.Sitefinity.Feather.targets,
     which CLR-loads every assembly in bin on every build just to read two attributes.
     ORDER IS LOAD-BEARING: this must sit BELOW that Import - MSBuild keeps the LAST
     definition of a target name. -->
<PropertyGroup>
  <ScanControllerContainerAssembliesExe>$(MsBuildProjectDirectory)\Build\SitefinitySteveScanControllerAssemblies.exe</ScanControllerContainerAssembliesExe>
</PropertyGroup>

<Target Name="ScanControllerContainerAssemblies" AfterTargets="AfterBuild">
  <Exec Command="&quot;$(ScanControllerContainerAssembliesExe)&quot; &quot;$(MsBuildProjectDirectory)\bin&quot;"
        Condition="Exists('$(ScanControllerContainerAssembliesExe)')" />

  <!-- Exe absent (partial checkout): fall back to the vendor script so a build never
       ships without the JSON. -->
  <Exec Command="powershell.exe -NonInteractive -ExecutionPolicy Unrestricted -command &quot;. '$(ScanControllerContainerAssembliesScript)' -binariesDirectory '$(MsBuildProjectDirectory)\bin'&quot;"
        Condition="!Exists('$(ScanControllerContainerAssembliesExe)')" />
</Target>
```

This must go **after** the `Telerik.Sitefinity.Feather.targets` import, which is normally
the last line before `</Project>`.

The package does exactly this, plus it keeps the exe up to date with the installed
version, so prefer the package if you can.

</details>

### The exe lands in your Build folder

The package copies the executable (and a short README explaining what it is) into your
project's `Build\` folder on each build, so it sits beside the script it replaces and
stays in step with the installed package version.

`Build\` is usually under source control, so add these to `.gitignore` — both are
recreated by any build that restores packages:

```
Build/SitefinitySteveScanControllerAssemblies.exe
Build/SitefinitySteveScanControllerAssemblies.README.md
```

To put nothing in your tree at all and run straight from the package folder, set
`<SitefinitySteveScanCopyToBuildFolder>false</SitefinitySteveScanCopyToBuildFolder>`.

### Did it work?

Build, and look at the Output window (or build log) for a line like this:

```
SitefinitySteveScanControllerAssemblies: 312 dlls, 26 containers, 41 ms
```

If that line is there, you're done.

If it's **not** there and the build still stalls for ~5 seconds after compiling, the old
script ran — see [Troubleshooting](#troubleshooting).

### Uninstalling

```
Uninstall-Package SitefinityAssemblyScanner
```

Sitefinity's original scan is live again immediately. There is no leftover state to clean
up and nothing to undo.

## What this fixes

Feather needs to know which assemblies contain MVC widgets. Those assemblies mark
themselves with one of two assembly-level attributes:

- `ControllerContainerAttribute`
- `ResourcePackageAttribute`

Working that out at application startup would be slow, so Feather does it at **build**
time and caches the answer in `bin\ControllerContainerAsembliesLocation.json`. Sitefinity
reads that list on startup to know which assemblies to search for controllers.

The build step that produces it is `ScanControllerContainerAssemblies.ps1`, shipped inside
the **Telerik.Sitefinity.Feather** NuGet package. It answers the question by calling
`Assembly.LoadFrom` on **every DLL in `bin`** — fully loading each assembly into a CLR,
resolving its dependencies and JIT-ing along the way — to read something that lives in a
few hundred bytes of metadata. On a typical site that's 250–350 assemblies.

This tool reads that metadata directly and never loads an assembly.

### Is the output really identical?

Yes. Verified byte-identical against three production Sitefinity sites, and cross-checked
against Microsoft's own `System.Reflection.Metadata`:

| Corpus | Result |
|---|---|
| Site A, 269 DLLs | byte-identical to the built-in scan's JSON |
| Site B, 335 DLLs | byte-identical to the built-in scan's JSON |
| Site C, 313 DLLs | identical set of containers |
| `System.Reflection.Metadata` | identical across 327 DLLs |

**The fail-safe rule is preserved:** a missing entry makes a widget silently disappear,
while an extra entry only costs a little startup time. So anything genuinely unreadable is
*included*, never skipped.

Reading metadata shrinks "unreadable" to almost nothing. `LoadFrom` can't tell *"has no
such attribute"* from *"couldn't resolve dependencies"*, which is why the built-in script
includes everything that fails to bind. This resolves no dependencies, so a native DLL or
netmodule is a **definitive** negative rather than a guess.

A side benefit: because assemblies are never loaded, a DLL that crashes or hangs
`LoadFrom` can't break the scan. Sites running the built-in script sometimes end up
maintaining a hardcoded skip-list of problem assemblies. No such workaround is needed here.

### Timing caveat

Both figures above are with the bin folder already in the OS file cache, which is the real
build case — MSBuild wrote those assemblies seconds earlier, so they're still in RAM.
Scanning a folder untouched since boot costs ~2.5 s on the first run only, and that's disk
I/O rather than scanner work. The PowerShell version pays that too, worse, since
`LoadFrom` reads whole assemblies while this reads only each file's metadata region.

## Options

You shouldn't need any of these, but they're there.

### MSBuild properties

Set in your `.csproj`:

| Property | Default | Purpose |
|---|---|---|
| `SitefinitySteveScanEnabled` | `true` | Set `false` to fall back to Feather's PowerShell scan |
| `SitefinitySteveScanBinDir` | `$(MSBuildProjectDirectory)\bin` | Folder to scan |
| `SitefinitySteveScanExe` | package `tools\` folder | Path to the executable |
| `SitefinitySteveScanCopyToBuildFolder` | `true` | Copy the exe into your `Build\` folder |

### Command line

The executable also runs standalone. With no arguments it scans the `bin` folder beside
its own parent directory, which is how it behaves when sitting in your site's `Build\`
folder:

```
SitefinitySteveScanControllerAssemblies [binariesDirectory] [OPTIONS]

  -o, --output <path>     Output JSON path
  -a, --attribute <fqn>   Attribute to match, fully qualified. Repeatable.
      --stdout            Write to stdout instead of a file
      --list              Print every assembly-level attribute found
  -j, --threads <n>       Worker threads (note `-j 1`, not `-j1`)
  -q, --quiet             Suppress the summary line
```

`--list` is what to reach for when a widget goes missing — it shows every assembly-level
attribute on every DLL, so you can see exactly why something matched or didn't.

## Other notes

### Troubleshooting

**The summary line doesn't appear and builds are still slow.**

This is almost always import ordering. Open your `.csproj` and look at the very bottom.
You'll see a line for each package that contributes build targets:

```xml
<Import Project="packages\Telerik.Sitefinity.Feather.15.4.8631\build\Telerik.Sitefinity.Feather.targets" ... />
<Import Project="packages\SitefinityAssemblyScanner.1.0.0\build\SitefinityAssemblyScanner.targets" ... />
```

**Ours must come after Feather's.** If it doesn't, move that line so it's last and
rebuild. This can happen if you reinstall or upgrade the Feather package after installing
this one, because NuGet re-adds Feather's import at the end.

Nothing breaks when the order is wrong — Feather's original scan runs and produces correct
output, just at the original speed.

**A widget went missing.** Run the exe with `--list` against your `bin` folder to see
every assembly-level attribute on every DLL, and why an assembly was or wasn't matched.

### How it works

*You don't need any of this to use the package.*

Sitefinity's `.ps1` isn't referenced in your csproj at all. Your csproj imports the Feather
package's targets, and *that* file locates the script by convention and runs it:

```xml
<Target Name="ScanControllerContainerAssemblies" AfterTargets="AfterBuild">
  <Exec Command="powershell.exe ... ScanControllerContainerAssemblies.ps1 ..." />
</Target>
```

So there's no path in your project to repoint, and editing the vendor file is pointless —
NuGet restore overwrites it.

This package instead defines a target with the **same name**. In MSBuild, when two targets
share a name, the last one evaluated wins. NuGet places package imports at the end of the
csproj, after Feather's, so ours replaces theirs and the PowerShell call never happens.

Both import orderings are covered by tests in CI.

### Why is `Assemblies` misspelled in the JSON output?

Because Telerik misspelled it, and it's now part of the contract. The output file is:

```
ControllerContainerAsembliesLocation.json
```

`Asemblies` is missing an `s`. That name is baked into Feather's targets file *and* the
Sitefinity runtime code that reads it at application start, so this tool reproduces it
exactly.

**Don't "fix" it.** Nothing will error. Feather will just look for a file that no longer
exists, silently fall back to scanning every assembly at startup, and any widget relying
on that cache can quietly vanish — a slow, confusing failure rather than an obvious one.

Same reasoning for the file's contents: four-space indent, CRLF, no BOM. That's what
PowerShell's `ConvertTo-Json | Set-Content` produced, so anything else turns every build
into a spurious diff.

### Building from source

Needs Rust via [rustup](https://rustup.rs), zero external crates:

```bash
cargo build --release
cargo test --release
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the linker gotchas on Windows (they trip most
people up), how the tests work, a code map, and the invariants that must not be broken.

Publishing is documented in [NUGET_PUBLISHING.md](NUGET_PUBLISHING.md) — this repo uses
NuGet **Trusted Publishing** (OIDC), so there is no API key to create or store.

### Also worth a look: Sitefinity MCP Server

**[SitefinityCommunity.Mcp](https://github.com/sitefinitysteve/SitefinityCommunity.Mcp)** — a
Model Context Protocol server for Sitefinity CMS, so Claude Code and other AI agents can
work with your site directly.

It exposes 40+ tools across:

- **Logs & diagnostics** — read error and trace logs, regex-search across them, grab the last error
- **Site info** — Sitefinity version, .NET version, project metadata, multisite config
- **Pages & content** — list routes, inspect a page's widgets and their property values, browse templates and taxonomies
- **Modules** — installed modules and Module Builder dynamic types with field definitions
- **APIs** — ServiceStack REST routes and OData entity sets
- **Forms** — forms, field definitions and entries, with sensitive data redacted
- **Permissions & audit** — effective role permissions, and reverse lookups for where a resource is used
- **Maintenance** — clear caches, recycle the app (gated write operations)

If you've watched Laravel or Rails developers get AI tooling that actually understands
their framework and wondered where Sitefinity's was — that's the project.

### Who made this

**Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**

Found a bug, or a site where the output doesn't match?
[Open an issue](https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies/issues).

If this saved you some build time:

<a href="https://www.buymeacoffee.com/stevewgw" target="_blank">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me a Coffee" height="60" width="217">
</a>

### Licence

MIT
