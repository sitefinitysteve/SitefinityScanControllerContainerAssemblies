# SitefinityAssemblyScanner

Replaces Sitefinity Feather's `ScanControllerContainerAssemblies.ps1` build step with a
single self-contained native executable. Byte-identical output, **99% faster**.

No PowerShell. No .NET runtime dependency. Nothing to install on the build agent.

## What happens when you install it

An MSBuild targets file is imported automatically and overrides Feather's
`ScanControllerContainerAssemblies` target. Nothing is copied into your project and no
vendor package is patched.

If the import ordering ever ends up wrong, Feather's original target wins and the build
still produces correct output — just at the original speed. The failure mode is safe.

## Why it is faster

The stock script calls `Assembly.LoadFrom` on every DLL in `bin` to read two
assembly-level attributes, fully loading each assembly into a CLR to do it. This reads
the ECMA-335 metadata directly and never loads an assembly.

Measured on a real 269-DLL Sitefinity site: ~5,000 ms → ~40 ms — **99.2% faster,
about 125x**, saving roughly 5 seconds on every single build.

## Options

Set these in your `.csproj` if you need to:

| Property | Default | Purpose |
|---|---|---|
| `SitefinitySteveScanEnabled` | `true` | Set `false` to fall back to Feather's PowerShell scan |
| `SitefinitySteveScanBinDir` | `$(MSBuildProjectDirectory)\bin` | Folder to scan |
| `SitefinitySteveScanExe` | package `tools\` folder | Path to the executable |

## Note on the output filename

The file written is `ControllerContainerAsembliesLocation.json`. The misspelling
(`Asemblies`, missing an `s`) is Telerik's and is part of the Sitefinity contract —
Feather reads that exact name. Do not correct it.

## Links

Source, full documentation, and issues:
https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies

## Who made this

**Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**

If this saved you some build time:
[Buy Me a Coffee](https://www.buymeacoffee.com/stevewgw)
