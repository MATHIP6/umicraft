# Glossary

The domain language of Umicraft. Code, docs, ADRs and reviews use these
terms with exactly these meanings. The spec glossary (`UMICRAFT_SPEC.md` §2)
defines the general Minecraft terms; this file adds the terms the codebase
introduced. Keep entries short; link the ADR that fixed a term.

## Network

**Play session** — the play-state half of a connection, after login: it turns
every clientbound play packet into a **Reaction**. `PlaySession` performs no
I/O; it only keeps the state the packets need (whether the entry packets were
sent, the last **placement**). ADR-0010.

**Reaction** — what one clientbound packet produces: at most one event for the
game thread, at most two serverbound replies, and possibly the reason the
server kicked the player. A reaction is a value; nothing has been sent yet
when it exists. ADR-0010.

**Entry packets** — the `Client Settings` and `MC|Brand` packets a client sends
in answer to the first `Join Game`. Sending them earlier makes a 1.8 server
reject them. ADR-0010.

**drive** — the async loop that owns the socket during play: it reads packets,
hands them to the **play session**, writes the replies and the game thread's
commands, and ends with a `SessionEnd`. It is the only code that awaits I/O in
play state. ADR-0010.

**Placement** — the absolute player position and look the server imposes with
`Player Position and Look` (0x08), after resolving the 1.8 relative flags. The
game thread answers it with a position acknowledgement. ADR-0005 §A.5.

## Rendering

**Terrain textures** — the texture array the GPU samples for blocks together
with the mesher that writes layer indices into it. Two sets exist, the
**procedural** one (original art) and the **official** one (the user's 1.8.8
atlas); they are always installed as a whole, never mixed (`TerrainTextures`,
`InstalledTerrain`). ADR-0012.

**World view** — the render thread's copy of the game: the interpolated
camera and clock, the chunks waiting for a mesh worker and the meshes handed
to the renderer (`WorldView`). The window and the screenshot path drive the
same one. ADR-0013.

**Generation** (of the world view) — a counter a world reset increments;
every mesh job carries it, and a mesh of an older generation is dropped.
ADR-0013.

## World

**Chunk** (column) — a 16×256×16 column of blocks addressed by `ChunkPos`;
spec §2.

**Section** — one 16×16×16 cube of a chunk column; spec §2.

**Chunk payload** — the raw 1.8 bytes of one column as the server sends them
(block states, light nibbles, biomes), with the section mask and the
ground-up flag that describe them (`ChunkPayload`). The world parses it; the
protocol only carries it. ADR-0011.
