# Working on this code

Notes for anyone — human or LLM — picking this up cold. Read the invariants section
before changing anything; several of them fail *silently* rather than loudly.

## Getting to a compiled binary

Needs Rust via [rustup](https://rustup.rs). **Zero external crates**, so there is nothing
else to install and no lockfile churn.

```bash
git clone https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies
cd SitefinityScanControllerContainerAssemblies
cargo build --release
cargo test --release
```

Output: `target\release\SitefinitySteveScanControllerAssemblies.exe`

To try it against a real Sitefinity site, just point it at the bin folder:

```bash
target\release\SitefinitySteveScanControllerAssemblies.exe C:\YourSite\bin --stdout
```

The no-argument `..\bin` resolution is covered automatically by
`resolves_sibling_bin_when_given_no_argument` in `tests/cli.rs`, which builds a throwaway
`Build\` + `bin\` layout in the temp directory, so there is no need to stage the binary by
hand to check it.

## The pre-commit hook

A ready-to-pack binary is kept in `packaging/tools/`. A git hook rebuilds it whenever you
commit a change under `src/` or to `Cargo.toml`. Enable it once per clone:

```bash
git config core.hooksPath .githooks
```

It only fires when Rust sources are staged (editing markdown does not trigger a build),
it runs the tests and aborts the commit if they fail, and it **skips with a warning if
cargo is not installed** so the repo stays committable after `rustup self uninstall`.

Note that the committed binary is a convenience, not the artifact users get. Releases are
always rebuilt from source in CI for `x86_64-pc-windows-msvc`, so if you develop with the
GNU toolchain the committed exe will differ from the published one. If you skip the hook
(`--no-verify`, or never enabling it), the committed binary silently goes stale relative
to `src/` — CI is the source of truth.

## Linker gotchas on Windows

Rust needs a linker, and this is where setup usually goes wrong:

- The default `x86_64-pc-windows-msvc` target needs **Visual Studio Build Tools with the
  "Desktop development with C++" workload** plus the Windows SDK. A Visual Studio install
  with only .NET workloads has **no linker**. Check with
  `vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64`.
- **No MSVC?** Use the GNU toolchain, which bundles its own linker and import libraries:

  ```bash
  rustup toolchain install stable-x86_64-pc-windows-gnu
  rustup default stable-x86_64-pc-windows-gnu
  ```

- **On ARM64 Windows**, both commands need `--force-non-host`. The x86_64 toolchain is
  not the host architecture but runs fine under emulation:

  ```bash
  rustup toolchain install stable-x86_64-pc-windows-gnu --force-non-host
  rustup default stable-x86_64-pc-windows-gnu --force-non-host
  ```

  `rustup target add x86_64-pc-windows-gnu` alone is **not** enough — that installs the
  standard library but neither the linker nor the MinGW import libraries.
- `link: extra operand ...` / `Try 'link --help'` means Git Bash's `/usr/bin/link` (a
  coreutils tool) is shadowing MSVC's `link.exe`. It is not a real linker error.

Release binaries are built in CI, so a local toolchain is only needed for development and
can be removed afterwards with `rustup self uninstall`.

## Tests

```bash
cargo test --release
```

Unit tests live beside the code; end-to-end tests are in `tests/cli.rs` and drive the
real executable. They run against small pre-built .NET assemblies committed under
`test/fixtures/prebuilt`, so **no .NET SDK and no Sitefinity install are required**.

The fixtures cover both ways an attribute's type can be reached in ECMA-335 metadata,
which is the part most likely to break:

| Fixture | Path exercised | Expected |
|---|---|---|
| `HasContainer` | `MemberRef → TypeRef` — attribute defined in another assembly | found |
| `SelfDefined` | `MethodDef → TypeDef` — attribute defined in the same assembly, needs the `TypeDef.MethodList` range walk | found |
| `NoAttributes` | references the attribute assembly, applies nothing | **not** found |
| `FakeSitefinity` | defines both attributes, applies neither | **not** found |

The C# sources that produced them are in `test/fixtures/`. To regenerate:

```bash
for p in FakeSitefinity HasContainer SelfDefined NoAttributes; do
  dotnet build "test/fixtures/$p" -c Release -o test/fixtures/prebuilt --nologo -v q
done
```

CI additionally runs `cargo clippy -- -D warnings` and an MSBuild check that our target
definition really does beat Feather's when imported after it. That last one cannot be a
Rust test because it needs MSBuild.

### Checking against a real site

The strongest test is a real Sitefinity `bin` folder. Point the tool at one and diff
against the JSON the stock script produced — it should be byte-identical.

If you suspect the metadata parser specifically, cross-check against Microsoft's
`System.Reflection.Metadata`, which is the reference implementation:

```csharp
using var pe = new PEReader(File.OpenRead(path));
var r = pe.GetMetadataReader();
foreach (var h in r.GetAssemblyDefinition().GetCustomAttributes()) { /* ... */ }
```

## Code map

| File | Responsibility |
|---|---|
| `src/main.rs` | CLI, directory walk, worker threads, verdict logic, JSON output |
| `src/pe.rs` | PE headers, RVA→file-offset mapping, locating the CLI metadata region |
| `src/meta.rs` | ECMA-335 streams, table row sizing, coded indexes, custom attributes |
| `src/bytes.rs` | Bounds-checked little-endian readers (every accessor returns `Option`) |
| `packaging/` | nuspec and the MSBuild targets that override Feather's scan |
| `tests/cli.rs` | End-to-end tests driving the built executable |

The parser only computes row *sizes* for metadata tables `0x00`–`0x0C`, because that is
all that is needed to locate `CustomAttribute` (`0x0C`). Row *counts* are still read for
every table, since coded-index widths depend on them.

## Invariants — do not break these

1. **The output filename typo is load-bearing.**
   `ControllerContainerAsembliesLocation.json` is missing an `s`. Feather reads that exact
   name. Correcting it breaks widget discovery silently.
2. **CRLF line endings**, four-space indent, no BOM, trailing newline — matching
   PowerShell `ConvertTo-Json | Set-Content`. Anything else diffs on every build.
3. **Case-insensitive sort**, matching `Get-ChildItem`.
4. **Fail safe.** Anything genuinely unreadable must be *included*, never skipped. A
   missing entry makes a widget disappear; an extra one only costs a little startup time.
   Note the distinction the code draws: a native DLL or netmodule is a *definitive*
   negative and is correctly excluded. Only truly indeterminate cases are included.
5. **Zero external crates.** This ships to other people's build agents; keep the supply
   chain empty.
6. **Never panic on malformed input.** One bad DLL in `bin` must not fail a build.

Items 1–4 all have tests. If you find yourself changing one of those tests, stop and
re-read this list first.

---

By **Steve McNiven-Scott** — **[sitefinitysteve.com](https://www.sitefinitysteve.com/)**
[github.com/sitefinitysteve](https://github.com/sitefinitysteve/SitefinityScanControllerContainerAssemblies)
