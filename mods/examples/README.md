# Example mods

| Example | Shows |
|---|---|
| `hello` | The template to copy: logging, an `observe` handler on `PreTick`, an `intercept` handler on `PostTick` at priority 10, an `intercept` handler on `ChatSend` that replaces `:shrug:` with a shrug through `set_text`, and `shutdown`. |
| `panicking` | A mod whose `init` panics. The host reports `PANICKED` and keeps running. A test fixture, not a template. |

## Build and install

```sh
cargo build -p umicraft-example-hello --release
```

Create the mod folder inside the client's mods directory (`<data dir>/mods`),
named after the mod id, and copy the manifest and the library:

```
<data dir>/mods/hello/mod.toml
<data dir>/mods/hello/bin/linux-x86_64/libhello.so
```

The platform folder is `<os>-<arch>` (`linux-x86_64`, `windows-x86_64`,
`macos-aarch64`, ...); the file is `libhello.so`, `hello.dll` or
`libhello.dylib`, from `target/release/`.

## Trust confirmation

Native mods run with the client's full rights. The first time a library loads,
the client shows its SHA-256 and asks you to confirm; the hash is then kept in
the trust list. A rebuilt library has a new hash and asks again.

## Reference

- [ADR-0086](../../docs/decisions/0086-modding-api-architecture.md): architecture of the modding API.
- [ADR-0087](../../docs/decisions/0087-modding-abi-contract.md): the C ABI contract.

`crates/umicraft-modhost/tests/real_mods.rs` builds both examples and loads them
for real.
