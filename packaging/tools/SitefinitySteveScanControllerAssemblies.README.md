# SitefinitySteveScanControllerAssemblies.exe

**This file is placed here automatically by a NuGet package. You don't need to edit,
commit, or manage it.**

## What is it?

It replaces Sitefinity Feather's `ScanControllerContainerAssemblies.ps1` build step.

Feather needs to know which assemblies in `bin` carry `[ControllerContainer]` or
`[ResourcePackage]`, and it caches that list in
`bin\ControllerContainerAsembliesLocation.json`. The stock script works this out by
calling `Assembly.LoadFrom` on **every DLL in `bin`**, which takes about 5 seconds on a
typical site. This reads the assembly metadata directly instead and does the same job in
around 40 milliseconds - 99% faster - producing byte-identical output.

## Where did it come from?

The `SitefinityAssemblyScanner` NuGet package. It is copied here from the package's
`tools\` folder on every build, so it always matches the installed package version — if
you upgrade the package, this file updates itself on the next build.

## Should I commit it to source control?

**No.** Add this to your `.gitignore`:

```
Build/SitefinitySteveScanControllerAssemblies.exe
Build/SitefinitySteveScanControllerAssemblies.README.md
```

Both files are recreated automatically by any build that restores packages.

## Can I delete it?

Yes. The next build puts it back. If you uninstall the NuGet package it stops being
copied, and Sitefinity reverts to its original PowerShell scan with no further action
needed.

## Running it by hand

With no arguments it scans the `bin` folder next door:

```
SitefinitySteveScanControllerAssemblies.exe
```

`--list` is useful when a widget has gone missing — it prints every assembly-level
attribute on every DLL, so you can see exactly why something was or wasn't matched.

## More information

**Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**

Source, issues and full documentation:
https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies
