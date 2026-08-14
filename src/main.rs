//! SitefinitySteveScanControllerAssemblies — find Sitefinity Feather
//! controller-container assemblies.
//!
//! Drop-in replacement for Sitefinity's `ScanControllerContainerAssemblies.ps1`,
//! which calls `Assembly.LoadFrom` on every DLL in the bin folder. That fully
//! loads each assembly into a runtime just to read two assembly-level
//! attributes, paying dependency-resolution and JIT costs to do it.
//!
//! This reads the CLI metadata directly instead. Same answer, no runtime.
//!
//! SAFETY RULE (inherited from the script this replaces, do not weaken):
//! a missing entry makes a widget silently disappear, while an over-inclusive
//! entry only costs a little app-start time. Every uncertain path therefore
//! includes the assembly rather than skipping it.
//!
//! Reading metadata rather than loading assemblies shrinks "uncertain"
//! dramatically: `LoadFrom` cannot tell "has no such attribute" apart from
//! "could not resolve this assembly's dependencies", so the script had to
//! include anything that failed to bind. We resolve no dependencies at all, so
//! that ambiguity does not arise. A native DLL is a *definitive* negative here.

mod bytes;
mod meta;
mod pe;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Sitefinity's own filename, reproduced exactly.
///
/// NOTE THE TYPO: "Asemblies" is missing an `s`. That misspelling is Telerik's,
/// it is baked into Feather's targets file and into the runtime code that reads
/// this JSON at application start, and it is therefore part of the contract.
/// Correcting the spelling here would leave Feather looking for a file that no
/// longer exists — it would not error, it would silently fall back to scanning
/// every assembly at startup, and any widget that depended on the cache could
/// disappear. Do not "fix" it.
const OUTPUT_NAME: &str = "ControllerContainerAsembliesLocation.json";

const DEFAULT_ATTRIBUTES: &[&str] = &[
    "Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes.ControllerContainerAttribute",
    "Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes.ResourcePackageAttribute",
];

/// Program name for diagnostics, taken from the binary target so it stays in
/// step with any rename in Cargo.toml.
const PROG: &str = env!("CARGO_BIN_NAME");

const USAGE: &str = concat!(
    env!("CARGO_BIN_NAME"),
    " — scan a Sitefinity bin folder for controller-container assemblies\n",
    "\nUSAGE:\n    ",
    env!("CARGO_BIN_NAME"),
    " [binariesDirectory] [OPTIONS]\n",
    "\nIf binariesDirectory is omitted, the tool looks for a `bin` folder beside its\n",
    "own parent directory (`<exe dir>\\..\\bin`), matching the layout where it sits\n",
    "in the site's `Build` folder.\n",
    "\nOPTIONS:\n",
    "    -o, --output <path>     Output JSON path\n",
    "                            [default: <binariesDirectory>\\ControllerContainerAsembliesLocation.json]\n",
    "    -a, --attribute <fqn>   Attribute to match, fully qualified. Repeatable.\n",
    "                            Replaces the built-in Sitefinity defaults.\n",
    "        --stdout            Write JSON to stdout instead of a file\n",
    "        --list              Diagnostic: print every assembly-level attribute found\n",
    "    -j, --threads <n>       Worker threads [default: available parallelism]\n",
    "    -q, --quiet             Suppress the summary line on stderr\n",
    "    -h, --help              Print help\n",
    "    -V, --version           Print version\n",
);

struct Args {
    dir: Option<PathBuf>,
    output: Option<PathBuf>,
    attributes: Vec<String>,
    to_stdout: bool,
    list: bool,
    threads: usize,
    quiet: bool,
}

/// Where the tool looks when given no directory: `<exe dir>\..\bin`.
/// The exe ships in the site's `Build` folder, alongside the script it
/// replaces, and the bin folder is its sibling.
fn default_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.parent()?.join("bin"))
}

fn parse_args() -> Result<Args, String> {
    let mut dir: Option<PathBuf> = None;
    let mut output = None;
    let mut attributes: Vec<String> = Vec::new();
    let mut to_stdout = false;
    let mut list = false;
    let mut threads = 0usize;
    let mut quiet = false;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("{PROG} {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-o" | "--output" => {
                output = Some(PathBuf::from(it.next().ok_or("--output requires a path")?))
            }
            "-a" | "--attribute" => {
                attributes.push(it.next().ok_or("--attribute requires a name")?)
            }
            "--stdout" => to_stdout = true,
            "--list" => list = true,
            "-q" | "--quiet" => quiet = true,
            "-j" | "--threads" => {
                threads = it
                    .next()
                    .ok_or("--threads requires a number")?
                    .parse()
                    .map_err(|_| "--threads must be a number".to_string())?
            }
            // Accept the PowerShell parameter name so existing call sites can
            // switch over without editing their arguments.
            "-binariesDirectory" | "--binariesDirectory" => {
                dir = Some(PathBuf::from(
                    it.next().ok_or("-binariesDirectory requires a path")?,
                ))
            }
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option: {s}")),
            s => {
                if dir.is_some() {
                    return Err(format!("unexpected argument: {s}"));
                }
                dir = Some(PathBuf::from(s));
            }
        }
    }

    if attributes.is_empty() {
        attributes = DEFAULT_ATTRIBUTES.iter().map(|s| s.to_string()).collect();
    }
    if threads == 0 {
        threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
    }
    Ok(Args { dir, output, attributes, to_stdout, list, threads, quiet })
}

/// .NET lets you write `[ControllerContainer]` for `ControllerContainerAttribute`,
/// and Feather's own targets file spells one of these without the suffix.
/// Compare on the stripped form so either spelling matches.
fn strip_attribute_suffix(s: &str) -> &str {
    s.strip_suffix("Attribute").unwrap_or(s)
}

enum Verdict {
    /// Carries one of the attributes.
    Container,
    /// Definitively does not, and we are confident.
    NotContainer,
    /// We could not determine it. Included, per the safety rule.
    Uncertain,
}

fn scan_one(path: &Path, targets: &[String], list: bool) -> (Verdict, Vec<String>) {
    let md = match pe::read_metadata(path) {
        pe::Outcome::Metadata(m) => m,
        // Native / resource-only DLL. It has no CLI metadata whatsoever, so it
        // cannot carry a managed attribute. This is a certainty, not a guess.
        pe::Outcome::NotManaged => return (Verdict::NotContainer, Vec::new()),
        pe::Outcome::Unreadable => return (Verdict::Uncertain, Vec::new()),
    };

    let Some(meta) = meta::Meta::parse(&md) else {
        // Managed image whose metadata we failed to parse — genuinely unknown.
        return (Verdict::Uncertain, Vec::new());
    };

    // A netmodule has no assembly manifest, so it cannot carry assembly-level
    // attributes at all. Another definitive negative.
    if !meta.is_assembly() {
        return (Verdict::NotContainer, Vec::new());
    }

    let mut found = Vec::new();
    let mut buf = String::new();

    let matched = meta.for_each_assembly_attribute(|ns, name| {
        buf.clear();
        if !ns.is_empty() {
            buf.push_str(ns);
            buf.push('.');
        }
        buf.push_str(name);
        if list {
            found.push(buf.clone());
            return false; // keep going; we want the full list
        }
        let lhs = strip_attribute_suffix(&buf);
        targets.iter().any(|t| strip_attribute_suffix(t) == lhs)
    });

    let verdict = if matched { Verdict::Container } else { Verdict::NotContainer };
    (verdict, found)
}

/// Reproduce `Get-ChildItem` ordering: case-insensitive, with an ordinal
/// comparison as a tiebreak so the result is stable rather than merely
/// unspecified when two names differ only by case.
fn sort_like_get_childitem(items: &mut [String]) {
    items.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
}

fn json_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

/// Reproduce PowerShell `ConvertTo-Json | Set-Content` output byte for byte:
/// four-space indent, one entry per line, CRLF endings, trailing newline, no
/// BOM. The CRLF is not cosmetic — `Set-Content` emits CRLF on Windows, and
/// existing checked-in files were produced that way, so anything else shows up
/// as a diff on every build.
fn render_json(items: &[String]) -> String {
    const NL: &str = "\r\n";
    let mut s = String::from("[");
    s.push_str(NL);
    for (i, item) in items.iter().enumerate() {
        s.push_str("    \"");
        json_escape(item, &mut s);
        s.push('"');
        if i + 1 < items.len() {
            s.push(',');
        }
        s.push_str(NL);
    }
    s.push(']');
    s.push_str(NL);
    s
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{PROG}: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let dir = match args.dir.clone().or_else(default_bin_dir) {
        Some(d) => d,
        None => {
            eprintln!("{PROG}: no <binariesDirectory> given and could not locate a default\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if !dir.is_dir() {
        eprintln!("{PROG}: not a directory: {}", dir.display());
        std::process::exit(2);
    }

    let started = Instant::now();

    let mut files: Vec<PathBuf> = Vec::new();
    match std::fs::read_dir(&dir) {
        Ok(rd) => {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                    && p.is_file()
                {
                    files.push(p);
                }
            }
        }
        Err(e) => {
            eprintln!("{PROG}: cannot read {}: {e}", dir.display());
            std::process::exit(2);
        }
    }

    let next = AtomicUsize::new(0);
    let hits: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let listing: Mutex<Vec<(String, Vec<String>)>> = Mutex::new(Vec::new());
    let uncertain = AtomicUsize::new(0);

    let nthreads = args.threads.min(files.len()).max(1);
    std::thread::scope(|scope| {
        for _ in 0..nthreads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                let (verdict, found) = scan_one(path, &args.attributes, args.list);
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();

                if args.list {
                    listing.lock().unwrap().push((name, found));
                    continue;
                }
                match verdict {
                    Verdict::Container => hits.lock().unwrap().push(name),
                    Verdict::Uncertain => {
                        // Fail safe: we could not read it, so assume it might
                        // be a container rather than risk dropping a widget.
                        uncertain.fetch_add(1, Ordering::Relaxed);
                        hits.lock().unwrap().push(name);
                    }
                    Verdict::NotContainer => {}
                }
            });
        }
    });

    if args.list {
        let mut all = listing.into_inner().unwrap();
        all.sort_by_key(|a| a.0.to_lowercase());
        for (name, attrs) in all {
            println!("{name}");
            for a in attrs {
                println!("    {a}");
            }
        }
        return;
    }

    let mut hits = hits.into_inner().unwrap();
    sort_like_get_childitem(&mut hits);

    let json = render_json(&hits);

    if args.to_stdout {
        print!("{json}");
    } else {
        let out = args.output.unwrap_or_else(|| dir.join(OUTPUT_NAME));
        if let Err(e) = std::fs::write(&out, json.as_bytes()) {
            eprintln!("{PROG}: cannot write {}: {e}", out.display());
            std::process::exit(1);
        }
    }

    if !args.quiet {
        let n = uncertain.load(Ordering::Relaxed);
        let note = if n > 0 {
            format!(", {n} unreadable (included to be safe)")
        } else {
            String::new()
        };
        eprintln!(
            "{PROG}: {} dlls, {} containers{note}, {} ms",
            files.len(),
            hits.len(),
            started.elapsed().as_millis()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    /// The output must match `ConvertTo-Json | Set-Content` byte for byte.
    /// CRLF is the part that is easy to get wrong and shows up as a diff on
    /// every build if you do.
    #[test]
    fn render_json_uses_crlf_and_four_space_indent() {
        let out = render_json(&s(&["A.dll", "B.dll"]));
        assert_eq!(out, "[\r\n    \"A.dll\",\r\n    \"B.dll\"\r\n]\r\n");
        assert!(!out.contains("\n\n"));
        // Every LF must be preceded by a CR: no bare newlines anywhere.
        assert_eq!(out.matches('\n').count(), out.matches("\r\n").count());
    }

    #[test]
    fn render_json_single_item_has_no_trailing_comma() {
        assert_eq!(render_json(&s(&["Only.dll"])), "[\r\n    \"Only.dll\"\r\n]\r\n");
    }

    /// PowerShell would emit `null` here, but an empty array is what consumers
    /// can actually parse. This is a deliberate divergence.
    #[test]
    fn render_json_empty_is_an_empty_array() {
        assert_eq!(render_json(&[]), "[\r\n]\r\n");
    }

    #[test]
    fn json_escape_handles_quotes_and_backslashes() {
        let mut out = String::new();
        json_escape(r#"we"ird\name"#, &mut out);
        assert_eq!(out, r#"we\"ird\\name"#);
    }

    #[test]
    fn json_escape_encodes_control_characters() {
        let mut out = String::new();
        json_escape("a\u{1}b", &mut out);
        assert_eq!(out, "a\\u0001b");
    }

    /// `[ControllerContainer]` and `[ControllerContainerAttribute]` name the
    /// same type, and Feather's own targets file spells one of these without
    /// the suffix, so matching has to be suffix-insensitive.
    #[test]
    fn attribute_suffix_is_optional_when_matching() {
        assert_eq!(strip_attribute_suffix("Foo.BarAttribute"), "Foo.Bar");
        assert_eq!(strip_attribute_suffix("Foo.Bar"), "Foo.Bar");
        // Only a trailing occurrence is stripped.
        assert_eq!(strip_attribute_suffix("Foo.AttributeBar"), "Foo.AttributeBar");
    }

    /// Regression guard for real output: a lowercase-initial name must sort
    /// before an uppercase-initial one, because `Get-ChildItem` is
    /// case-insensitive. Ordinal sorting would put "SitefinityWebApp" first
    /// and produce a spurious diff against existing checked-in files.
    #[test]
    fn sort_is_case_insensitive_like_get_childitem() {
        let mut v = s(&["SitefinityWebApp.dll", "pavliks.Connector.dll", "Telerik.A.dll"]);
        sort_like_get_childitem(&mut v);
        assert_eq!(
            v,
            s(&["pavliks.Connector.dll", "SitefinityWebApp.dll", "Telerik.A.dll"])
        );
    }

    #[test]
    fn sort_is_stable_for_names_differing_only_by_case() {
        let mut v = s(&["b.dll", "B.dll", "a.dll"]);
        sort_like_get_childitem(&mut v);
        assert_eq!(v, s(&["a.dll", "B.dll", "b.dll"]));
    }
}
