<a href="https://www.sitefinitysteve.com/">
  <img src="sfslogo.png" alt="SitefinitySteve" width="420">
</a>

# SitefinitySteveScanControllerAssemblies

A drop-in replacement for Sitefinity Feather's `ScanControllerContainerAssemblies.ps1`
build step. Byte-identical output, **99% faster** (roughly 125x).

One self-contained native executable. No PowerShell, no .NET runtime, nothing to install
on the build agent.

By **Steve McNiven-Scott** — [sitefinitysteve.com](https://www.sitefinitysteve.com/)

## What is ScanControllerContainerAssemblies.ps1?

It's a build step that ships inside the **Telerik.Sitefinity.Feather** NuGet package, at
`tools\ScanControllerContainerAssemblies.ps1`.

Feather needs to know which assemblies contain MVC widgets. Those assemblies mark
themselves with one of two assembly-level attributes:

- `ControllerContainerAttribute`
- `ResourcePackageAttribute`

Finding them by scanning everything at application startup would be slow, so Feather
works it out at **build** time instead and caches the answer in
`bin\ControllerContainerAsembliesLocation.json`. Sitefinity reads that list on startup to
know which assemblies to search for controllers.

The package's `build\Telerik.Sitefinity.Feather.targets` runs the script after every
build. It prefers a copy at `<YourProject>\Build\ScanControllerContainerAssemblies.ps1`
if one exists, falling back to its own packaged copy — which is why plenty of Sitefinity
sites have a copy of this script checked in.

The cache is an optimisation, not a requirement: if the JSON is missing, Feather falls
back to scanning at startup. That's why an over-inclusive list is cheap and a
*under*-inclusive one is dangerous — see [Correctness](#correctness).

## Why it's slow

The script answers that question by calling `Assembly.LoadFrom` on **every DLL in `bin`**
— fully loading each assembly into a CLR, resolving its dependencies and JIT-ing along
the way, to read something that lives in a few hundred bytes of metadata. On a typical
site that's 250–350 assemblies and several hundred MB.

This tool reads that metadata directly and never loads an assembly.

## The numbers

Measured on a production Sitefinity bin folder, 269 DLLs, 311 MB:

| | Time |
|---|---|
| Stock `ScanControllerContainerAssemblies.ps1` | ~5,000 ms |
| This tool | **~40 ms** |
| **Improvement** | **99.2% faster — about 125× — saving ~4.96 s per build** |

If you build 20 times a day that is around **100 seconds a day**, or roughly **7 hours a
year**, per developer.

Both with the bin folder already in the OS file cache, which is the real build case —
MSBuild wrote those assemblies seconds earlier, so they're still in RAM. Scanning a
folder untouched since boot costs ~2.5 s on the first run only, and that's disk I/O, not
scanner work. The PowerShell version pays that too, worse, since `LoadFrom` reads whole
assemblies while this reads only each file's metadata region.

## Correctness

Verified byte-identical against three production Sitefinity sites, and cross-checked
against Microsoft's own `System.Reflection.Metadata`:

| Corpus | Result |
|---|---|
| Site A, 269 DLLs | byte-identical to the stock script's JSON |
| Site B, 335 DLLs | byte-identical to the stock script's JSON |
| Site C, 313 DLLs | identical set of containers |
| `System.Reflection.Metadata` | identical across 327 DLLs |

**The fail-safe rule is preserved:** a missing entry makes a widget silently disappear,
while an extra entry only costs a little startup time. So anything genuinely unreadable
is *included*, never skipped.

Reading metadata shrinks "unreadable" to almost nothing. `LoadFrom` can't tell *"has no
such attribute"* from *"couldn't resolve dependencies"*, which is why the stock script
includes everything that fails to bind. This resolves no dependencies, so a native DLL
or netmodule is a **definitive** negative rather than a guess. A side benefit: a DLL that
crashes `LoadFrom` can't break the scan, so no skip-lists are needed.

## Install

Via NuGet:

```
Install-Package SitefinityAssemblyScanner
```

Or manually — drop the exe in your site's `Build` folder, beside the `.ps1` it replaces.
With no arguments it scans `..\bin`:

```
YourSite\
  Build\SitefinitySteveScanControllerAssemblies.exe
  bin\        <-- scanned, and where the JSON is written
```

## How it replaces the .ps1

**The `.ps1` isn't referenced in your csproj.** Your csproj imports the Feather package's
targets, and *that* file finds the script by convention:

```xml
<ScanControllerContainerAssembliesScript>$(MsBuildProjectDirectory)\Build\ScanControllerContainerAssemblies.ps1</ScanControllerContainerAssembliesScript>

<Target Name="ScanControllerContainerAssemblies" AfterTargets="AfterBuild">
  <Exec Command="powershell.exe ... '$(ScanControllerContainerAssembliesScript)' ..." />
</Target>
```

So there's no path in your project to repoint, and editing the vendor file is pointless —
NuGet restore overwrites it.

Instead, this package **doesn't touch the `.ps1` at all**. It overrides the MSBuild
*target* that invokes PowerShell. When two targets share a name the last one evaluated
wins, and NuGet places package imports after Feather's, so ours replaces theirs. The
script stays on disk, unused. Uninstall the package and Feather's original target is live
again, with no leftover state.

If the import ever lands *before* Feather's, Feather wins and you get correct output at
the original speed — a safe failure. Both orderings are tested in CI. The build log shows
which one ran.

To opt out without uninstalling: `<SitefinitySteveScanEnabled>false</SitefinitySteveScanEnabled>`.

## Usage

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

## Building and contributing

Needs Rust via [rustup](https://rustup.rs), zero external crates:

```bash
cargo build --release
cargo test --release
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the linker gotchas on Windows (they trip most
people up), how the tests work, a code map, and the invariants that must not be broken.

## Publishing

See [NUGET_PUBLISHING.md](NUGET_PUBLISHING.md) — this repo uses NuGet **Trusted Publishing**
(OIDC), so there is no API key to create or store.

## Why is `Assemblies` misspelled in the JSON output?

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

## Who made this

**Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**

If this saved you some build time:

<a href="https://www.buymeacoffee.com/stevewgw" target="_blank">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me a Coffee" height="60" width="217">
</a>

Found a bug or have a site where the output doesn't match?
[Open an issue](https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies/issues).

## Licence

MIT
