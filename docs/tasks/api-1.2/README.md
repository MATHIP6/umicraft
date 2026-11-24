# API 1.2 — categories, modules and buttons in the mods menu

Status: **done** (2026-11-23). Design: ADR-0095. Base: API 1.1
(`docs/tasks/api-1.1/README.md`, ADR-0094).

| # | Phase | Acceptance | State |
|---|---|---|---|
| 1.2.0 | ADR-0095 and this plan | ADR accepted | done |
| 1.2.1 | `umicraft-modapi-sys`: types below, `UMI_API_VERSION = 1.2`, host functions appended | layout tests; 1.0/1.1 mods load, a 1.2 mod is refused by a 1.1 host | done |
| 1.2.2 | Host: categories, modules, module settings, buttons, `module_attach`, `module_enabled`, `[modules]` config, the two events | unit tests of every refusal and of the config round trip | done |
| 1.2.3 | UI: button row, tab row that fits | UI tests | done |
| 1.2.4 | Client: module cards and tabs, routing, events | menu tests | done |
| 1.2.5 | `umicraft-modapi`: `Category`, `Module`, buttons, events; example `suite`; docs; template on 1.2 | fmt, clippy `-D warnings`, workspace tests; `include/umicraft.h` regenerated | done |

## Frozen interface (spec §14 rule 11)

### Version

`UMI_API_VERSION = UmiVersion { major: 1, minor: 2 }`.

### Events (`events.rs`)

- `UmiEventId::MODULE_TOGGLED = 14`, payload
  `UmiModuleToggled { header, module: UmiStr, enabled: u8 }`, render thread,
  targeted at the owning mod, not cancellable, not mutable.
- `UmiEventId::BUTTON_PRESSED = 15`, payload
  `UmiButtonPressed { header, module: UmiStr, key: UmiStr }` (`module` empty for a
  button on the mod's own card), render thread, targeted, not cancellable, not
  mutable.

### Menu types (`ui.rs`)

```rust
#[repr(C)]
pub struct UmiModuleDef {
    pub size: u32,
    pub flags: u32,          // UMI_MODULE_DEFAULT_ON = 1
    pub key: UmiStr,
    pub label: UmiStr,
    pub description: UmiStr, // may be empty
    pub category: UmiStr,    // a key given to register_category, or empty
}
pub const UMI_MODULE_DEFAULT_ON: u32 = 1;
pub const UMI_ATTACH_HUD_WIDGET: u32 = 1;
pub const UMI_ATTACH_POST_PASS: u32 = 2;
```

### Host functions (appended to `UmiHost`)

```rust
register_category: extern "C" fn(me, key: UmiStr, label: UmiStr) -> UmiStatus,
register_module: extern "C" fn(me, def: *const UmiModuleDef) -> UmiStatus,
register_setting_in: extern "C" fn(me, module: UmiStr, def: *const UmiSettingDef) -> UmiStatus,
register_button: extern "C" fn(me, module: UmiStr, key: UmiStr, label: UmiStr, text: UmiStr) -> UmiStatus,
module_attach: extern "C" fn(me, module: UmiStr, kind: u32, key: UmiStr) -> UmiStatus,
module_enabled: extern "C" fn(me, module: UmiStr, out: *mut u8) -> UmiStatus,
```

The `register_*` and `module_attach` calls are init-only (`DENIED` after, and
`DENIED` past a limit, as in 1.0). `INVALID_ARGUMENT` for a bad key or text, an
unknown category, module, widget or pass, or an unknown attach kind; `CONFLICT` for a
duplicate key or a second attachment of the same widget or pass. `module_enabled`
works from any thread at any time (`INVALID_ARGUMENT` for an unknown module or a null
`out`).

### Limits

| What | Limit |
|---|---|
| Categories per mod | 2 |
| Category label | 32 bytes |
| Modules per mod | 16 |
| Module description | 256 bytes |
| Button text | 32 bytes |
