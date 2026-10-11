fn main() {
    // A dependency reached through `tauri-plugin-dialog` (rfd's message dialogs)
    // calls `TaskDialogIndirect`, which only comctl32 **v6** exports. The loader
    // only binds v6 when the executable's manifest declares the
    // `Microsoft.Windows.Common-Controls` v6 dependency; otherwise it binds the
    // v5 DLL in System32, which has no such export, and the process is killed at
    // startup with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before `main` — or a
    // single test — ever runs.
    //
    // Binary targets get that manifest from tauri's own link args. Test harnesses
    // do not, so they have to be told explicitly. Keep this on one line: a `\`
    // line continuation in Rust eats the leading space and produces
    // `type='win32'name='…'`, which the linker rejects.
    // Not `rustc-link-arg-tests`: that bucket is for files under `tests/`, and
    // this package has none — the unit-test harness is the lib target, which is
    // only reachable through the unfiltered instruction. Bins get the same
    // dependency from tauri's own link args and the linker merges duplicates.
    #[cfg(all(windows, target_env = "msvc"))]
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );

    tauri_build::build()
}
