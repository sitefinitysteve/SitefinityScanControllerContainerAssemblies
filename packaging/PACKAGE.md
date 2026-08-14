# SitefinityAssemblyScanner

Replaces Sitefinity Feather's `ScanControllerContainerAssemblies.ps1` build step with a
single self-contained native executable. Byte-identical output, **99% faster**.

No PowerShell. No .NET runtime dependency. Nothing to install on the build agent.

## Install

```
Install-Package SitefinityAssemblyScanner
```

**That's it. Build your project. You're done.**

There is nothing to configure. The package adds the executable and points Sitefinity's
build step at it instead of the PowerShell script.

`ScanControllerContainerAssemblies.ps1` stays exactly where it is, untouched — it just
isn't the thing being run any more. You don't need to edit your `.csproj` either.

### Did it work?

Build and look for this line in the output:

```
SitefinitySteveScanControllerAssemblies: 312 dlls, 26 containers, 41 ms
```

If it's there, the executable ran and the PowerShell script did not.

If it's missing and the build still stalls for ~5 seconds after compiling, check that the
`SitefinityAssemblyScanner` import at the bottom of your `.csproj` comes **after** the
`Telerik.Sitefinity.Feather` one. If it doesn't, move it last and rebuild. Nothing breaks
when the order is wrong — you just get the original slow scan.

## Why it's faster

The stock script calls `Assembly.LoadFrom` on every DLL in `bin` to read two
assembly-level attributes, fully loading each assembly into a CLR to do it. This reads the
ECMA-335 metadata directly and never loads an assembly.

Measured on a real 269-DLL Sitefinity site: ~5,000 ms → ~40 ms — **99.2% faster, about
125x**, saving roughly 5 seconds on every single build.

## Options

Only if you need them:

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
