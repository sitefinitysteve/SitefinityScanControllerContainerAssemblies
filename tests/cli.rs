//! End-to-end tests driving the real executable.
//!
//! These run against small pre-built .NET assemblies committed under
//! `test/fixtures/prebuilt`, so `cargo test` needs no .NET SDK and no
//! Sitefinity install. The C# sources that produced them live beside them in
//! `test/fixtures/` if they ever need regenerating.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_SitefinitySteveScanControllerAssemblies");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test/fixtures/prebuilt")
}

/// Copy the fixtures somewhere writable, so tests that exercise file output do
/// not litter the committed fixture folder.
fn temp_copy_of_fixtures(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sfscan-test-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    for entry in fs::read_dir(fixtures()).expect("read fixtures") {
        let p = entry.expect("entry").path();
        if p.extension().is_some_and(|e| e == "dll") {
            fs::copy(&p, dir.join(p.file_name().unwrap())).expect("copy fixture");
        }
    }
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(EXE).args(args).output().expect("run scanner")
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf8 stdout")
}

/// The headline behaviour: exactly the two assemblies carrying the attributes,
/// and neither of the two that do not.
#[test]
fn finds_exactly_the_container_assemblies() {
    let out = run(&[fixtures().to_str().unwrap(), "--stdout", "-q"]);
    assert!(out.status.success(), "exit: {:?}", out.status);
    assert_eq!(
        stdout_of(&out),
        "[\r\n    \"HasContainer.dll\",\r\n    \"SelfDefined.dll\"\r\n]\r\n"
    );
}

/// `HasContainer` reaches its attribute through MemberRef -> TypeRef (defined
/// in another assembly) and `SelfDefined` through MethodDef -> TypeDef (defined
/// in the same assembly). The second requires walking TypeDef.MethodList ranges
/// to recover the declaring type, which is the subtlest part of the reader.
#[test]
fn covers_both_attribute_resolution_paths() {
    let text = stdout_of(&run(&[fixtures().to_str().unwrap(), "--stdout", "-q"]));
    assert!(text.contains("HasContainer.dll"), "MemberRef path failed: {text}");
    assert!(text.contains("SelfDefined.dll"), "MethodDef path failed: {text}");
}

/// Guards against matching a TypeRef that merely appears in the metadata rather
/// than one that is actually applied to the assembly.
#[test]
fn ignores_assemblies_that_only_reference_the_attributes() {
    let text = stdout_of(&run(&[fixtures().to_str().unwrap(), "--stdout", "-q"]));
    assert!(!text.contains("NoAttributes.dll"), "{text}");
    // FakeSitefinity *defines* both attributes but applies neither.
    assert!(!text.contains("FakeSitefinity.dll"), "{text}");
}

/// The filename typo is Telerik's and is part of the contract. Feather reads
/// this exact name; "correcting" it breaks widget discovery silently.
#[test]
fn writes_the_misspelled_filename_sitefinity_expects() {
    let dir = temp_copy_of_fixtures("filename");
    let out = run(&[dir.to_str().unwrap(), "-q"]);
    assert!(out.status.success());

    let expected = dir.join("ControllerContainerAsembliesLocation.json");
    assert!(expected.is_file(), "expected file was not written");
    assert!(
        !dir.join("ControllerContainerAssembliesLocation.json").exists(),
        "wrote the corrected spelling; Feather will not find it"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// PowerShell's Set-Content writes CRLF. Emitting LF turns every build into a
/// spurious diff against files already checked in.
#[test]
fn written_file_uses_crlf_throughout() {
    let dir = temp_copy_of_fixtures("crlf");
    run(&[dir.to_str().unwrap(), "-q"]);
    let bytes = fs::read(dir.join("ControllerContainerAsembliesLocation.json")).expect("read json");

    let lf = bytes.iter().filter(|&&b| b == b'\n').count();
    let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
    assert_eq!(lf, crlf, "found bare LF line endings");
    assert!(lf > 0);
    assert_ne!(&bytes[..3], b"\xEF\xBB\xBF", "unexpected UTF-8 BOM");
    let _ = fs::remove_dir_all(&dir);
}

/// With no directory argument the tool scans `<exe dir>\..\bin`, which is how
/// it behaves when dropped into a site's `Build` folder beside the .ps1.
#[test]
fn resolves_sibling_bin_when_given_no_argument() {
    let root = std::env::temp_dir().join(format!("sfscan-site-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let build = root.join("Build");
    let bin = root.join("bin");
    fs::create_dir_all(&build).expect("mkdir Build");
    fs::create_dir_all(&bin).expect("mkdir bin");

    for entry in fs::read_dir(fixtures()).expect("read fixtures") {
        let p = entry.expect("entry").path();
        if p.extension().is_some_and(|e| e == "dll") {
            fs::copy(&p, bin.join(p.file_name().unwrap())).expect("copy");
        }
    }
    let staged = build.join("scanner.exe");
    fs::copy(EXE, &staged).expect("stage exe");

    let out = Command::new(&staged).arg("--stdout").arg("-q").output().expect("run");
    assert!(out.status.success());
    assert_eq!(
        stdout_of(&out),
        "[\r\n    \"HasContainer.dll\",\r\n    \"SelfDefined.dll\"\r\n]\r\n"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Callers can override which attributes count, which is what makes this
/// reusable outside the two Feather defaults.
#[test]
fn custom_attribute_filter_replaces_the_defaults() {
    let out = run(&[
        fixtures().to_str().unwrap(),
        "--stdout",
        "-q",
        "-a",
        "Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes.ResourcePackageAttribute",
    ]);
    let text = stdout_of(&out);
    // Only the ResourcePackage one now qualifies.
    assert!(text.contains("SelfDefined.dll"), "{text}");
    assert!(!text.contains("HasContainer.dll"), "{text}");
}

#[test]
fn list_mode_reports_attributes_per_assembly() {
    let out = run(&[fixtures().to_str().unwrap(), "--list"]);
    let text = stdout_of(&out);
    assert!(text.contains("HasContainer.dll"));
    assert!(text.contains("ControllerContainerAttribute"), "{text}");
}

#[test]
fn missing_directory_fails_with_usage_exit_code() {
    let out = run(&["definitely-not-a-real-directory"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn unknown_option_is_rejected() {
    let out = run(&[fixtures().to_str().unwrap(), "--nope"]);
    assert_eq!(out.status.code(), Some(2));
}

/// A folder with no managed assemblies must still produce a valid, empty JSON
/// array rather than nothing at all.
#[test]
fn empty_directory_produces_empty_array() {
    let dir = std::env::temp_dir().join(format!("sfscan-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    let out = run(&[dir.to_str().unwrap(), "--stdout", "-q"]);
    assert!(out.status.success());
    assert_eq!(stdout_of(&out), "[\r\n]\r\n");
    let _ = fs::remove_dir_all(&dir);
}

/// One unreadable file must not abort the scan, and must be included rather
/// than dropped: a missing entry makes a widget disappear, an extra one only
/// costs a little startup time.
#[test]
fn truncated_dll_is_included_rather_than_skipped() {
    let dir = temp_copy_of_fixtures("truncated");
    fs::write(dir.join("Corrupt.dll"), b"MZ\x90\x00this is not a real PE image")
        .expect("write corrupt dll");

    let out = run(&[dir.to_str().unwrap(), "--stdout", "-q"]);
    assert!(out.status.success(), "a bad DLL must not fail the scan");
    let text = stdout_of(&out);
    assert!(text.contains("HasContainer.dll"), "{text}");
    assert!(text.contains("SelfDefined.dll"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}
