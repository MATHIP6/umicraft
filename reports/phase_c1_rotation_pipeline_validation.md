# UMICRAFT — RAPPORT DE PHASE C.1 : VALIDATION DU PIPELINE RÉEL DES ROTATIONS

Date : 12 novembre 2026  
Projet : Umicraft (Client Minecraft Java 1.8.9 en Rust)  
Référence décompilée MCP : `C:\Users\cbynt\Desktop\umicraft\src`

---

## 1. Objectif de la Phase C.1

Cette phase avait pour but de vérifier et démontrer de façon irréfutable que les corrections de parité apportées en Phase C (sensibilité exacte de $0.15^\circ/\text{px}$ à $S=0.5$ et pitch vertical étendu à $\pm 90.0^\circ$) sont effectivement actives et appliquées sur **l'intégralité du chemin d'exécution réel** du client, depuis les événements bruts du matériel souris jusqu'à la sérialisation des paquets réseau C05 et C06 envoyés au serveur.

---

## 2. Traçage complet du pipeline réel d'exécution

Le tableau ci-dessous décrit chaque étape du traitement d'une rotation joueur, en comparant directement le chemin Java MCP 1.8.9 officiel au chemin Rust Umicraft :

| Étape | Minecraft Java 1.8.9 (MCP) | Umicraft Rust (Production) | Types et conversions |
| :--- | :--- | :--- | :--- |
| **1. Entrée matérielle** | Matériel souris $\to$ OS $\to$ LWJGL `Mouse.getDX()`, `Mouse.getDY()` | Matériel souris $\to$ OS $\to$ winit `DeviceEvent::MouseMotion { delta }` | `delta: (f64, f64)` brut en pixels |
| **2. Réception fenêtre** | `MouseHelper.mouseXYChange()` dans `EntityRenderer.updateCameraAndRender` | `App::device_event` dans `crates/umicraft-client/src/window/mod.rs` | `delta.0 as f32`, `delta.1 as f32` |
| **3. Sensibilité & Inversion** | $f = S \times 0.6\text{F} + 0.2\text{F}$<br>$f_1 = f^3 \times 8.0\text{F}$<br>$f_2 = \Delta X \times f_1$<br>$f_3 = \Delta Y \times f_1 \times (\text{invert} ? -1 : 1)$ | `compute_mouse_look_delta(delta, &self.options.controls, zoom_factor)`<br>`speed = MOUSE_SENSITIVITY * controls.look_factor()`<br>`vertical = if invert { -1.0 } else { 1.0 }`<br>`(delta.0 * speed, delta.1 * speed * vertical)` | `MOUSE_SENSITIVITY: f32 = 0.15_f32.to_radians()`<br>`look_factor(): (S*0.6+0.2)^3 * 8.0`<br>Angles calculés en radians (`f32`) |
| **4. Accumulation inter-trame** | Accumulé dans les buffers LWJGL jusqu'au frame tick | `self.look.0 += dyaw;`<br>`self.look.1 += dpitch;` | Accumulation vectorielle `(f32, f32)` dans `App` |
| **5. Transmission au jeu** | Appel direct sur le thread principal : `thePlayer.setAngles(f2, f3 * i)` | `App::send_input()` $\to$ `Session::send_input(InputUpdate)` $\to$ `game.inputs.try_send(update)` | `InputUpdate { look_yaw: self.look.0, look_pitch: self.look.1 }` |
| **6. Application état joueur** | `Entity.setAngles` :<br>`rotationYaw += (double)yaw * 0.15D`<br>`rotationPitch -= (double)pitch * 0.15D`<br>`clamp(rotationPitch, -90.0F, 90.0F)` | `game.rs::handle_input` :<br>`game.yaw += update.look_yaw;`<br>`game.pitch = (game.pitch + update.look_pitch).clamp(-MAX_PITCH, MAX_PITCH);` | `MAX_PITCH = 90.0_f32.to_radians()` ($1.5707964$ rad)<br>Yaw continu sans modulo<br>Pitch clampé $[-90^\circ, +90^\circ]$ |
| **7. Simulation & Tick** | `EntityPlayerSP.onUpdateWalkingPlayer()` :<br>Calcule $\Delta \text{pos}$ et $\Delta \text{rot}$ | `game.rs::tick()` $\to$ `game.server_view.tick(&snapshot)` dans `traffic.rs` | `TickSnapshot { yaw: game.yaw.to_degrees(), pitch: game.pitch.to_degrees() }` |
| **8. Génération de commande** | `sendQueue.addToSendQueue(new C05PacketPlayerLook(...) ou C06PacketPlayerPosLook(...))` | `movement_command(*packet)` $\to$ `ClientCommand::PlayerLook` (C05) ou `ClientCommand::PositionLook` (C06) | Commande typée poussée dans `CommandQueue` |
| **9. Sérialisation Wire** | `C03PacketPlayer.writePacketData` :<br>C05 = `yaw (f32), pitch (f32), on_ground (u8)`<br>C06 = `x,y,z (f64), yaw,pitch (f32), on_ground (u8)` | `umicraft-protocol::packets::play::serverbound::PlayerLook::encode_body` (9 o)<br>`PositionLook::encode_body` (33 o) | `BigEndian` IEEE 754 float / double |

---

## 3. Vérification de la sensibilité et des conventions de signe

### 3.1 Formule de production validée
Dans `crates/umicraft-client/src/window/mod.rs` :
```rust
pub(crate) fn compute_mouse_look_delta(
    delta: (f64, f64),
    controls: &crate::options::ControlOptions,
    zoom_look_factor: f32,
) -> (f32, f32) {
    let speed = MOUSE_SENSITIVITY * controls.look_factor() * zoom_look_factor;
    let vertical = if controls.invert_mouse {
        -1.0
    } else {
        1.0
    };
    (delta.0 as f32 * speed, delta.1 as f32 * speed * vertical)
}
```
Cette fonction est **la fonction de production réelle** appelée par `App::device_event` lors de chaque mouvement physique de la souris.

### 3.2 Vérification des signes et axes
1. **Axe horizontal ($\Delta X$)** :
   - Mouvement vers la droite : `delta.0 > 0`.
   - `dyaw = delta.0 * speed > 0`.
   - `game.yaw += dyaw` $\implies$ le yaw augmente (rotation sens horaire vue de dessus).
   - En vanilla : `rotationYaw += (deltaX * f1) * 0.15D`. Le déplacement vers la droite augmente `rotationYaw` (0 = Sud, 90 = Ouest, 180 = Nord, 270 = Est). **Parité 100% respectée**.
2. **Axe vertical ($\Delta Y$)** :
   - Mouvement vers le bas (souris tirée vers l'utilisateur) : `delta.1 > 0` dans winit (Y orienté vers le bas de l'écran).
   - Sans inversion (`invert_mouse: false`) : `vertical = 1.0` $\implies$ `dpitch > 0`.
   - `game.pitch += dpitch` $\implies$ le pitch augmente vers $+90.0^\circ$ (regard vers les pieds).
   - En vanilla : LWJGL fournit un `deltaY < 0` lors d'un déplacement vers le bas. `Entity.setAngles` effectue `rotationPitch -= deltaY * 0.15D` $\implies$ `rotationPitch - (-val) = rotationPitch + val` $\implies$ le pitch augmente vers $+90.0\text{F}$. **Parité 100% respectée**.
   - Avec inversion (`invert_mouse: true`) : `vertical = -1.0` $\implies$ tirer la souris vers le bas diminue le pitch vers $-90.0^\circ$ (ciel). **Parité 100% respectée**.

### 3.3 Accumulation multi-événements
Si plusieurs événements `DeviceEvent::MouseMotion` surviennent avant la prochaine trame (souris haute fréquence 1000 Hz) :
- `self.look.0 += dyaw; self.look.1 += dpitch;` accumule les deltas successifs.
- Lors du tick d'envoi (`send_input`), la somme cumulée est expédiée en une seule mise à jour `InputUpdate`.
- Le thread de jeu dépile et cumule tous les `InputUpdate` en attente avant le tick physique :
  ```rust
  while let Ok(update) = inputs.try_recv() {
      handle_input(&mut game, update, &mut queue);
  }
  ```
- Les rotations cumulées sont rigoureusement transmises sans perte d'événements.

---

## 4. Analyse et isolation de `world_view.rs`

### 4.1 Examen de `self.player_yaw.rem_euclid(TAU)`
L'instruction de `crates/umicraft-client/src/world_view.rs:1212` :
```rust
self.player_yaw = (self.player_yaw + yaw_delta).rem_euclid(std::f32::consts::TAU);
```
a fait l'objet d'un audit de flux complet.

### 4.2 Traçage de toutes les lectures et écritures de `player_yaw`
1. **Écritures** :
   - `world_view.rs:1142, 1153` : initialisé à partir du corps local reçu du jeu (`frame.local_body.yaw.to_radians()`).
   - `world_view.rs:1185, 1188` : mis à jour lors de paquets de regard distants.
   - `world_view.rs:1212` : incrémenté avec `rem_euclid(TAU)` dans `apply_look_delta` lors des mouvements locaux de souris pour le rendu immédiat.
   - `world_view.rs:1231` : assigné lors d'un `teleport`.
2. **Lectures** :
   - `world_view.rs:746, 780` : `let live_yaw = self.view.player_yaw.to_degrees();` utilisé exclusivement pour orienter le modèle 3D du joueur (tête et corps) lors du rendu.
   - `world_view.rs:1102, 1104, 1108` : utilisé dans `camera_pose_from_player` pour calculer la position de la caméra en première personne et l'orbite F5 (troisième personne arrière et avant).

### 4.3 Conclusion formelle sur l'isolation protocolaire
- `WorldView` est un récepteur pur côté rendu. Il ne possède **aucun canal émetteur** (`Sender`) vers le thread de jeu (`Game`) ou vers le thread réseau (`umicraft-net`).
- Les paquets C05 et C06 sont construits exclusivement sur le thread de jeu à partir de `game.yaw` (`game.rs:3839` : `yaw: game.yaw.to_degrees()`).
- `game.yaw` est mis à jour dans `handle_input` par simple addition continue (`game.yaw += update.look_yaw;`) sans jamais subir de normalisation ou de wrapping `rem_euclid`.
- **Conclusion** : La normalisation `rem_euclid(TAU)` dans `world_view.rs` est **strictement visuelle**, protège les shaders contre les imprécisions trigonométriques sur les très grands angles ($> 10^6$ rad), et **ne peut en aucun cas contaminer les paquets réseau**. Elle a été conservée telle quelle conformément aux consignes.

---

## 5. Tests du pipeline de production

9 tests d'intégration complets ont été ajoutés dans `crates/umicraft-client/src/game.rs` (`test_pipeline_*`). Ils injectent des mouvements souris bruts `(f64, f64)` à travers la fonction de production `compute_mouse_look_delta`, traversent `handle_input`, `tick`, `server_view.tick`, la file de commandes et l'encodage binaire des paquets réseau.

| Test | Scénario testé | Résultat |
| :--- | :--- | :---: |
| `test_pipeline_sensitivity_0_0_min` | Sensibilité $S = 0.0$ ($f_1 = 0.064$) avec $\Delta = (100, 50)$ px $\implies$ Wire : yaw $0.96^\circ$, pitch $0.48^\circ$ | **OK** |
| `test_pipeline_sensitivity_0_5_default` | Sensibilité $S = 0.5$ ($f_1 = 1.0$) avec $\Delta = (100, -50)$ px $\implies$ Wire : yaw $15.0^\circ$, pitch $-7.5^\circ$ | **OK** |
| `test_pipeline_sensitivity_1_0_max` | Sensibilité $S = 1.0$ ($f_1 = 4.096$) avec $\Delta = (-20, 30)$ px $\implies$ Wire : yaw $-12.288^\circ$, pitch $18.432^\circ$ | **OK** |
| `test_pipeline_positive_and_negative_deltas` | Mouvements combinés $(+50, +40)$ puis $(-80, -100)$ $\implies$ Net $(-30, -60)$ px $\implies$ Wire $(-4.5^\circ, -9.0^\circ)$ | **OK** |
| `test_pipeline_multiple_successive_mouse_events_in_frame` | 5 événements souris successifs dans la même trame (somme $48$ px X, $11$ px Y) $\implies$ Wire ($7.2^\circ, 1.65^\circ$) | **OK** |
| `test_pipeline_pitch_vertical_limits_plus_minus_90` | Poussée extrême vers le haut (-5000 px) et vers le bas (+10000 px) $\implies$ Clamping exact à $-90.0^\circ$ et $+90.0^\circ$ sur le wire | **OK** |
| `test_pipeline_yaw_exceeding_360_continuous` | Mouvement de 3000 px vers la droite $\implies$ Wire $450.0^\circ$ continu (pas de saut de $-360^\circ$) | **OK** |
| `test_pipeline_c05_c06_serialization_after_multiple_rotations` | Rotation seule $\to$ C05 `PlayerLook` (9 octets) ; Déplacement+rotation $\to$ C06 `PositionLook` (33 octets) | **OK** |
| `test_pipeline_consistency_player_state_and_wire` | Concordance exacte entre `game.yaw/pitch.to_degrees()` et les octets décodés des paquets filaires | **OK** |

### Résultats des suites de tests
- `cargo test -p umicraft-client --lib test_pipeline_` : **9 passed, 0 failed**
- `cargo test -p umicraft-client --lib test_rot_` : **11 passed, 0 failed**
- `cargo check -p umicraft-client -p umicraft-game` : **Passed** (0 erreur)
- `cargo test -p umicraft-game --lib` : **1392 passed, 0 failed**

---

## 6. Fichiers modifiés

1. `crates/umicraft-client/src/window/mod.rs` :
   - Factorisation de la fonction de production `pub(crate) fn compute_mouse_look_delta`.
   - Utilisation directe dans `App::device_event`.
2. `crates/umicraft-client/src/game.rs` :
   - Ajout des 9 tests d'intégration du pipeline réel (`test_pipeline_*`).
3. `reports/phase_c1_rotation_pipeline_validation.md` :
   - Rédaction du présent rapport.

Les **113 modifications locales préexistantes** ont été intégralement préservées sans aucun `git reset`, `checkout`, `clean` ou `stash`.

---

## 7. Limitations restantes

1. **Caméra cinématique (F8)** : Comme documenté en Phase C, le lissage cinématique (`smoothCamera` dans `EntityRenderer.java:1106-1115`) n'est pas utilisé dans le pipeline compétitif d'Umicraft.
2. **Validation serveur réel** : Tous les tests ont été exécutés en environnement local déterministe via la simulation complète du protocole 47 et la comparaison avec les sources officielles MCP 1.8.9. Aucun serveur réel n'a été contacté.

---

**Arrêt de mission : La Phase C.1 est terminée. En attente de vos instructions pour la phase suivante.**
