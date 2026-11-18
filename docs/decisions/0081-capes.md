# 0081. Capes built-in mod

Status: accepted

## Context

The owner wants a built-in mod (ADR-0065) that shows the capes players own on
the third-party cape services, the way the Fabric mod "Capes" (LGPL-2.1) does
on modern clients. The Fabric mod was read for its behaviour only; no code is
taken from it (the licences differ, spec §3 rule 7). The services were probed
on 2026-11-18 (`docs/sources.md`, section "Capes").

What the Fabric mod does, and what applies to a 1.8 client:

| Fabric feature | Here |
|---|---|
| Providers OptiFine, LabyMod, MinecraftCapes, Cosmetica | Ported. |
| Provider Cloaks+ (a bare IP over plain HTTP) | Not ported (owner, 2026-11-18). |
| Own cape: one chosen provider; others: first provider with a cape | Ported, plus an `all` choice (the default) that gives the local player the order of the others. The order differs: OptiFine comes last (owner, 2026-11-18). |
| HD capes padded into `64·2ⁿ x 32·2ⁿ` | Ported. The cape box UVs are normalised (`CapeModel`), so a larger texture needs no model change. |
| Animated capes: vertical strip of `w x w/2` frames, 100 ms each | Ported. |
| LabyMod "no cape" placeholder image ignored | Ported. |
| Cape render type with alpha cutout | Already the case (`entity.wgsl` discards texels below the cutoff). |
| Cape texture on the elytra | Nothing to do: 1.8 has no elytra. |
| Cape selector screen with a turning player preview | Left for later (needs a menu player preview). |
| OptiFine cape editor button (joins a session with the access token) | Not ported (owner, 2026-11-18). |
| `/capes debug` client command | Not ported (owner, 2026-11-18). |

## Decision

A visual mod `capes` ("Capes"), **on by default with every provider on**
(owner decision, 2026-11-18, taken knowing the privacy cost below).

Settings (`umicraft-builtin-mods/src/mods/capes.rs`):

| Key | Kind | Default |
|---|---|---|
| `own_source` | Choice `all`, `labymod`, `minecraftcapes`, `cosmetica`, `optifine`, `minecraft` | `all` (the local player is looked up like everybody else; owner, 2026-11-18) |
| `labymod` | Toggle | on |
| `minecraftcapes` | Toggle | on |
| `cosmetica` | Toggle | on |
| `optifine` | Toggle | on |

A provider whose toggle is off is never asked, not even for the local player.

### Behaviour

- Only players with a version-4 UUID (online-mode accounts) are looked up.
  NPCs (version 2) and offline-mode players (version 3) are not sent to any
  service.
- Lookup order, for every player: LabyMod, MinecraftCapes, Cosmetica,
  OptiFine, then the Mojang cape of the profile. The first cape found wins
  and the later providers are not asked. OptiFine comes last, just before
  Mojang, by owner decision (2026-11-18); the Fabric mod asks it first.
- The local player follows the same order on `all` (the default). A single
  provider asks only that one (the Mojang cape if it has none); `minecraft`
  means no lookup.
- A third-party cape replaces the Mojang cape of the profile; without one the
  Mojang cape shows as before. The skin part "Cape" and invisibility still
  hide it (`cape_layer`).
- Each player is resolved once per session. A missing cape is remembered too
  (no retry storm). Changing a setting of the mod clears every result, so the
  new choice applies at once.
- An animated cape shows frame `(now_ms / 100) % frames`, so no per-player
  timer is needed.
- Nothing is sent to the game server; anti-cheat parity (ADR-0076) is not
  affected.

### Network safety

- A dedicated worker thread (`cape-fetch`, current-thread tokio runtime, at
  most 4 downloads at a time, bounded queue), the same shape as
  `TextureFetcher`. The render thread only asks and polls.
- Host allowlist by domain suffix: `optifine.net`, `labymod.net`, `laby.net`,
  `minecraftcapes.net`, `cosmetica.cc`. Redirects are followed only to an
  allowed host (LabyMod redirects to `items.laby.net`). Plain HTTP is allowed
  only for `optifine.net`, which does not answer on HTTPS.
  `profile_textures::is_trusted_texture_url` stays limited to Mojang.
- A response body is at most 1 MiB, a decoded cape at most 2048 x 2048 and 64
  frames. Every response is untrusted: no `unwrap`, typed `CapeError`.
- No disk cache in this version.

### Frozen interfaces

`umicraft-assets/src/capes/` (pure parts unit tested without network):

```rust
pub enum CapeProvider { OptiFine, LabyMod, MinecraftCapes, Cosmetica }
impl CapeProvider {
    pub const ALL: [Self; 4];                     // lookup order: LabyMod, MinecraftCapes, Cosmetica, OptiFine
    pub fn key(self) -> &'static str;             // "optifine", ...: the setting keys
    pub fn from_key(key: &str) -> Option<Self>;
    pub fn request(self, owner: &CapeOwner) -> CapeRequest;
}
pub struct CapeOwner { pub uuid: u128, pub name: String }
impl CapeOwner { pub fn is_lookup_allowed(&self) -> bool } // UUID version 4
pub struct CapeRequest { pub url: String, pub kind: ResponseKind }
pub enum ResponseKind { Png, CosmeticaJson, MinecraftCapesJson }
pub fn is_allowed_cape_url(url: &str) -> bool;

pub struct CapeImage { pub width: u32, pub height: u32, pub frames: Vec<Vec<u8>> } // RGBA8 per frame
pub fn decode_cape(png: &[u8], animated: bool, provider: CapeProvider)
    -> Result<Option<CapeImage>, CapeError>;      // None: LabyMod placeholder

pub struct CapeQuery { pub owner: CapeOwner, pub providers: Vec<CapeProvider> }
pub struct FoundCape { pub provider: CapeProvider, pub image: CapeImage }
pub struct CapeResult { pub uuid: u128, pub cape: Option<FoundCape> }
pub struct CapeResolver;
impl CapeResolver {
    pub fn spawn() -> Self;
    pub fn request(&self, query: CapeQuery) -> bool; // false: queue full, ask again later
    pub fn poll(&self) -> impl Iterator<Item = CapeResult> + '_;
}
pub enum CapeError { /* thiserror: Http, TooLarge, Decode, BadSize, BadJson, HostNotAllowed */ }
```

Added while building it: `CapeOwner::has_valid_name` (OptiFine is skipped
for a name outside `[A-Za-z0-9_]{1,16}`, as the name goes into its URL),
and the re-exported `CapeSource` / `ImageSource` of the JSON answers.

Client side:

- `ModsRuntime::capes() -> Option<CapeSettings>` with
  `CapeSettings { own: OwnCape, enabled: [bool; 4] }`, where
  `OwnCape` is `All`, `Provider(CapeProvider)` or `Mojang` (`enabled` in the
  order of `CapeProvider::ALL`, `Copy` so the per-frame read never
  allocates), `None` while the mod is off. `EntityTextures::set_capes` takes
  it with the local player's UUID every frame; a change of either drops the
  worker (and the answers still on their way), the results and the uploaded
  frames.
- `entity_render/capes.rs`: `CapeSet` keyed by UUID (`Pending`, `Absent`,
  `Ready { first: u32, frames: u32 }`). Its worker starts with the first
  lookup, so with the mod off no thread runs and no request leaves. It
  uploads with
  `Renderer::create_entity_texture` into the new
  `TextureSlot::CustomCape(u32)`.
- `SkinLookup::custom_cape(uuid, name) -> Option<u32>` asks on first use, as
  `SkinLookup::skin` does; `cape_layer` prefers it over the profile cape.

## Consequences

- **Privacy:** while the mod is on, the name or UUID of every online-mode
  player met is sent to up to four third-party services, and to OptiFine over
  plain HTTP (readable on the network path). The mod description in the menu
  says so; turning the mod or a provider off stops it.
- The MinecraftCapes JSON changed (base64 textures removed). The response
  type is tolerant (every field optional) and must be checked against a real
  profile that owns a cape before the merge.
- Left for later: a cape selector page with a player preview, a disk cache,
  version-2 UUIDs whose name matches the profile (Fabric accepts them).
