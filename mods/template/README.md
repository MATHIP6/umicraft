# Mod template

The smallest useful mod: one setting, one tick observer, a shutdown message.
Every line of `src/lib.rs` and `mod.toml` is commented. The guide is
[docs/modding/README.md](../../docs/modding/README.md).

## Make your own mod

1. Copy this folder, for example to `mods/cool_hud`.
2. Pick an id (`^[a-z][a-z0-9_]{0,63}$`) and rename it everywhere:
   - the folder name (it must equal the id);
   - `id` and `library` in `mod.toml`;
   - `name` in the `[lib]` table of `Cargo.toml` (it must equal `library`),
     and the package name.
3. Outside the Umicraft repository, point the `umicraft-modapi` path in
   `Cargo.toml` at `crates/umicraft-modapi`. Inside it, add the folder to the
   workspace `members`.
4. Build: `cargo build --release -p <package>`. The library appears in
   `target/release/` as `lib<name>.so` (Linux), `<name>.dll` (Windows) or
   `lib<name>.dylib` (macOS).

## Install

The client keeps mods in `<data dir>/mods/`. The data dir is the `umicraft`
folder of the platform data directory: `$XDG_DATA_HOME/umicraft` (default
`~/.local/share/umicraft`) on Linux, `~/Library/Application Support/umicraft`
on macOS, `%APPDATA%\umicraft` on Windows. Create:

```
<data dir>/mods/<id>/mod.toml
<data dir>/mods/<id>/bin/<os>-<arch>/<library file>
```

`<os>-<arch>` is one of `linux-x86_64`, `linux-aarch64`, `windows-x86_64`,
`windows-aarch64`, `macos-x86_64`, `macos-aarch64`.

## Trust it

Native mods run with your rights, so the client loads a library only after
you trusted its SHA-256 hash. Either open the mods menu, find the "Not
trusted" card and switch it on (the mod loads at the next start), or add the
hash to `<data dir>/mods/trusted.toml`:

```toml
[trusted]
my_mod = "<64 hex digits of the library's sha256>"
```

A rebuilt library has a new hash and must be trusted again.

## When something goes wrong

Start the client with `--safe-mode` to load no mod at all. The client does the
same by itself after a crash. A mod that fails is switched off until the next
start; the log says why.
