# UMICRAFT — RAPPORT DE CORRECTION DE LA PHASE B : PARITÉ DE L'ORDRE DES ÉVÉNEMENTS RÉSEAU

**Date :** 12 Novembre 2026  
**Référence comportementale :** Minecraft Java 1.8.9 MCP officiel (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Projet audité :** Umicraft (Client Rust 1.8.9 / Protocole 47)  
**Auteur / Rôle :** Antigravity Engine Auditor  

---

## 1. Contexte et Objectifs de la Phase B

Dans le contre-audit comportemental vanilla, une divergence critique a été identifiée concernant le moment où les événements réseau entrants sont traités dans la boucle principale du client.

Cette divergence entraînait un retard systématique d'un tick (50 ms) sur l'application des paquets serveur (vélocité S12, corrections de position S08, modifications de blocs S23/S22, etc.) par rapport à la simulation physique locale et à la génération des paquets sortants `C03PacketPlayer`.

Le mandat strict de la Phase B imposait :
1. L'étude intégrale des sources Java 1.8.9 de référence (`Minecraft.runGameLoop`, `Minecraft.runTick`, `PacketThreadUtil`, `NetworkManager`, `NetHandlerPlayClient`).
2. L'identification du cycle de vie complet des paquets entrants : thread de réception Netty, mise en file `scheduledTasks`, vidage de la file, traitement des entrées utilisateur, ticks de simulation physique et émission réseau.
3. La documentation de 13 paquets prioritaires avec leurs gestionnaires Java et Rust.
4. La correction de la boucle principale de jeu dans `crates/umicraft-client/src/game.rs` sans régression sur les protections de congestion, l'ordre FIFO ou la fluidité de rendu.
5. La création de 12 tests déterministes couvrant l'ensemble des scénarios critiques.
6. La préservation stricte des 113 fichiers locaux non commités préexistants.

---

## 2. Audit Préalable : Références Java et Comparaison avec Umicraft

### 2.1 Analyse détaillée des sources Java 1.8.9

#### 1. Réception et Enfilage Réseau (Thread Netty Worker)
- **Fichier :** `src/net/minecraft/network/NetworkManager.java` (lignes 149–162)
  ```java
  protected void channelRead0(ChannelHandlerContext p_channelRead0_1_, Packet p_channelRead0_2_) throws Exception {
      if (this.channel.isOpen()) {
          try {
              p_channelRead0_2_.processPacket(this.packetListener);
          } catch (ThreadQuickExitException var4) { ; }
      }
  }
  ```
- **Fichier :** `src/net/minecraft/network/PacketThreadUtil.java` (lignes 7–20)
  ```java
  public static <T extends INetHandler> void checkThreadAndEnqueue(final Packet<T> p_180031_0_, final T p_180031_1_, IThreadListener p_180031_2_) throws ThreadQuickExitException {
      if (!p_180031_2_.isCallingFromMinecraftThread()) {
          p_180031_2_.addScheduledTask(new Runnable() {
              public void run() {
                  p_180031_0_.processPacket(p_180031_1_);
              }
          });
          throw ThreadQuickExitException.field_179886_a;
      }
  }
  ```
  Sur le thread Netty, presque tous les paquets du protocole Play (à l'exception de `S00PacketKeepAlive`, `S40PacketDisconnect` et `S46PacketSetCompressionLevel`) appellent `checkThreadAndEnqueue`.
  Ne s'exécutant pas sur le thread principal de Minecraft, ils sont immédiatement transformés en `FutureTask` ajoutée à `Minecraft.scheduledTasks` (une `Queue<FutureTask<?>>` synchronisée) et lèvent `ThreadQuickExitException` pour stopper l'exécution Netty sans toucher à l'état du jeu.

#### 2. Dépilement sur le Thread Principal (`Minecraft.runGameLoop`)
- **Fichier :** `src/net/minecraft/client/Minecraft.java` (lignes 1061–1100)
  ```java
  private void runGameLoop() throws IOException {
      ...
      this.timer.updateTimer();
      this.mcProfiler.startSection("scheduledExecutables");

      synchronized (this.scheduledTasks) {
          while (!this.scheduledTasks.isEmpty()) {
              Util.func_181617_a((FutureTask)this.scheduledTasks.poll(), logger);
          }
      }

      this.mcProfiler.endSection();
      this.mcProfiler.startSection("tick");

      for (int j = 0; j < this.timer.elapsedTicks; ++j) {
          this.runTick();
      }
      ...
  }
  ```
  **Constat fondamental :**
  Dans `Minecraft.runGameLoop()`, la file `scheduledTasks` est **intégralement vidée au tout début de la boucle**, dans la section `scheduledExecutables`, **STRICTEMENT AVANT** la boucle de simulation `for (int j = 0; j < this.timer.elapsedTicks; ++j) this.runTick();`.

#### 3. Séquence interne de `Minecraft.runTick()`
- **Fichier :** `src/net/minecraft/client/Minecraft.java` (lignes 1716–2246)
  1. `this.ingameGUI.updateTick()`
  2. `this.entityRenderer.getMouseOver(1.0F)` (calcul du raycast)
  3. `this.playerController.updateController()` $\rightarrow$ `networkManager.processReceivedPackets()` (flush de la file d'envoi sortante et du canal TCP)
  4. Traitement des entrées utilisateur : `currentScreen.handleInput()` ou `Mouse.next()`, `Keyboard.next()`
  5. Clics et actions joueur (attaque, minage, utilisation d'item, changement de slot)
  6. `this.theWorld.updateEntities()` :
     - Tick de toutes les entités du monde
     - Tick du joueur local `EntityPlayerSP.onUpdate()` $\rightarrow$ `onLivingUpdate()` : simulation physique du mouvement prenant en compte la vitesse actuelle (`motionX/Y/Z`) et les collisions avec les blocs modifiés par `scheduledExecutables`.
     - Génération et émission du paquet de mouvement sortant : `sendQueue.addToSendQueue(new C03PacketPlayer...))`
  7. `this.theWorld.tick()` : mise à jour des blocs, météo et ticks programmés.

---

### 2.2 Ancien Ordre d'Exécution Rust (Divergence Confirmée)

Dans `crates/umicraft-client/src/game.rs:run()` :
```rust
loop {
    // 1. Drain inputs
    while let Ok(update) = inputs.try_recv() { ... }

    // 2. Prioritize game ticking IF a tick is due
    let mut now = Instant::now();
    if now >= next_tick {
        while now >= next_tick && catches < MAX_CATCH_UP_TICKS && !queue.congested() {
            tick(&mut game, ...); // <-- TICK SIMULE ICI AVEC L'ANCIEN ETAT !
            next_tick += TICK;
        }
        queue.flush();
    }

    // 3. Process network events
    while events_processed < MAX_EVENTS_PER_PASS {
        let Ok(event) = events.try_recv() else { break };
        handle_event(&mut game, event, ...); // <-- EVENEMENTS TRAITES APRES LE TICK !
    }
}
```

#### Conséquence de cette divergence
Lorsqu'un paquet S12 (recul / knockback), S08 (téléportation / correction serveur) ou S23 (changement de bloc) arrivait avant l'échéance d'un tick :
- Umicraft exécutait d'abord l'étape 2 (`tick()`), simulant le mouvement avec **l'ancienne vélocité** et **l'ancienne position**, et émettait un paquet sortant `C03PacketPlayer` erroné (ex: chute normale au lieu d'une impulsion verticale).
- Ce n'est qu'à l'étape 3 qu'Umicraft dépilait `events` et appliquait la vélocité ou la position... **avec un tick de retard systématique (50 ms)**.

---

### 2.3 Tableau Comparatif Ordre d'Exécution Java vs Rust

| Étape | Minecraft Java 1.8.9 Official (`Minecraft.java`) | Ancien Umicraft (`game.rs:run`) | Umicraft Corrigé (`game.rs:run`) |
| :---: | :--- | :--- | :--- |
| **1** | Netty Worker empile dans `scheduledTasks` | Thread réseau empile dans `events` | Thread réseau empile dans `events` |
| **2** | `runGameLoop`: vide `scheduledTasks` (`scheduledExecutables`) | Dépile `inputs` utilisateur | Dépile `inputs` utilisateur |
| **3** | `runGameLoop`: vérifie si tick(s) dû(s) | **Exécute `tick()` (ANCIEN ÉTAT)** | **Dépile `events` (S12, S08, S23...)** |
| **4** | `runTick`: entrées utilisateur (`Mouse/Keyboard`) | Flush `queue` commandes | Vérifie si tick(s) dû(s) |
| **5** | `runTick`: simulation physique joueur + émission C03 | Dépile `events` (Réseau en retard) | **Exécute `tick()` (ÉTAT À JOUR)** |
| **6** | `runTick`: tick du monde et des entités | Envoie frames de rendu | Flush `queue` commandes |
| **7** | Rendu graphique (`entityRenderer`) | `select!` attend prochain tick/event | `select!` attend prochain tick/event |

---

## 3. Matrice des 13 Paquets Prioritaires

| Paquet | Gestionnaire Java (`NetHandlerPlayClient`) | Effet sur l'état local | Moment d'application Java | Gestionnaire Rust (`game.rs`) | Moment d'application Umicraft Corrigé |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **S08** Position & Look | `handlePlayerPosLook` | Réinitialise motion, téléporte `posX/Y/Z`, envoie `C06` ack | `scheduledExecutables` (avant tick) | `apply_placement` | Étape 2 (avant `tick()`) |
| **S12** Entity Velocity | `handleEntityVelocity` | Applique `motionX/Y/Z` (`velocity / 8000.0`) sur joueur/entité | `scheduledExecutables` (avant tick) | `PlayEvent::EntityVelocity` | Étape 2 (avant `tick()`) |
| **S14** Entity Movement | `handleEntityMovement` | Met à jour `serverPosX/Y/Z` et position relative de l'entité | `scheduledExecutables` (avant tick) | `PlayEvent::EntityMovement` | Étape 2 (avant `tick()`) |
| **S18** Entity Teleport | `handleEntityTeleport` | Téléporte l'entité tierce aux coordonnées absolues | `scheduledExecutables` (avant tick) | `PlayEvent::EntityTeleport` | Étape 2 (avant `tick()`) |
| **S19** Entity Status | `handleEntityStatus` | Hurt camera shake, fin d'utilisation d'item, animations | `scheduledExecutables` (avant tick) | `PlayEvent::EntityStatus` | Étape 2 (avant `tick()`) |
| **S1B** Entity Attach | `handleEntityAttach` | Attache/détache monture (bateau, cheval) ou laisse | `scheduledExecutables` (avant tick) | `PlayEvent::AttachEntity` | Étape 2 (avant `tick()`) |
| **S1C** Entity Metadata | `handleEntityMetadata` | Met à jour DataWatcher (score, absorption, vie, poses) | `scheduledExecutables` (avant tick) | `PlayEvent::EntityMetadata` | Étape 2 (avant `tick()`) |
| **S1F** Set Experience | `handleSetExperience` | Met à jour barre d'XP, niveau et total | `scheduledExecutables` (avant tick) | `PlayEvent::SetExperience` | Étape 2 (avant `tick()`) |
| **S23** Block Change | `handleBlockChange` | Met à jour le bloc dans le monde (`WorldClient`) | `scheduledExecutables` (avant tick) | `PlayEvent::BlockChange` | Étape 2 (avant `tick()`) |
| **S22** Multi Block Change | `handleMultiBlockChange` | Met à jour un lot de blocs dans un chunk | `scheduledExecutables` (avant tick) | `PlayEvent::MultiBlockChange` | Étape 2 (avant `tick()`) |
| **S2B** Change Game State | `handleChangeGameState` | Change gamemode, pluie/soleil, messages d'état | `scheduledExecutables` (avant tick) | `PlayEvent::ChangeGameState` | Étape 2 (avant `tick()`) |
| **S07** Respawn | `handleRespawn` | Réinitialise santé (20), joueur, et monde si dimension change | `scheduledExecutables` (avant tick) | `PlayEvent::Respawn` | Étape 2 (avant `tick()`) |
| **S01** Join Game | `handleJoinGame` | Crée `WorldClient`, `playerController`, configure joueur | `scheduledExecutables` (avant tick) | `PlayEvent::JoinGame` | Étape 2 (avant `tick()`) |

---

## 4. Architecture Corrigée dans la Boucle Principale Rust

Dans `crates/umicraft-client/src/game.rs` (`fn run()`) :
1. **Étape 1 : Dépilement des entrées utilisateur (`inputs.try_recv()`)**
   - Les intentions de contrôle (clavier/souris) sont mises à jour pour le tick à venir.
2. **Étape 2 : Dépilement des événements réseau (`events.try_recv()`) AVANT le tick**
   - Reproduit fidèlement la section `scheduledExecutables` de `Minecraft.runGameLoop()`.
   - Traite jusqu'à `MAX_EVENTS_PER_PASS = 256` événements en séquence FIFO stricte.
   - Applique les impulsions de vitesse S12, téléportations S08, modifications de blocs S23/S22, mises à jour d'entités, etc.
   - En cas de déconnexion (`game.end`), sort proprement de la boucle.
   - Flashe la file de commandes sortantes (`queue.flush()`).
3. **Étape 3 : Ticks de simulation (`now >= next_tick`)**
   - Exécute jusqu'à `MAX_CATCH_UP_TICKS = 3` ticks de simulation.
   - Chaque tick opère sur un état **entièrement synchronisé avec les derniers paquets réseau reçus**.
   - Si la file de commandes est congestionnée (`queue.congested()`), la suspension des ticks est maintenue afin d'éviter d'aggraver la saturation réseau.
   - Flashe la file de commandes sortantes.
4. **Étape 4 : Attente active/passive (`select!`)**
   - Attend le prochain tick ou l'arrivée d'une entrée/événement sans gaspillage CPU.

---

## 5. Tests Déterministes Implémentés

12 tests unitaires et d'intégration ciblés ont été ajoutés à la suite de tests de `crates/umicraft-client/src/game.rs` :

1. **`test_event_order_1_s12_velocity_received_before_tick`** :
   Vérifie qu'un paquet S12 (impulsion verticale de $+0.5$ bloc/tick) reçu dans la boucle multithreadée via `LoopHarness` est appliqué immédiatement dans le tout premier tick simulé (inversion de l'accélération de chute dès la première position émise).
2. **`test_event_order_2_s12_velocity_received_after_tick_start`** :
   Vérifie qu'une vélocité S12 reçue après le début d'un tick est appliquée avec rigueur sur le tick immédiatement consécutif.
3. **`test_event_order_3_s08_pos_look_received_before_tick`** :
   Vérifie qu'un S08 repositionne immédiatement le joueur à $(100.0, 150.0, 200.0)$, émet l'`AckPosition` et que le tick consécutif simule la physique depuis ces nouvelles coordonnées.
4. **`test_event_order_4_multiple_s08_successive_in_same_pass`** :
   Vérifie que 3 paquets S08 successifs dans la même passe sont appliqués dans l'ordre FIFO exact et que les 3 acquittements sont placés en file sans perte.
5. **`test_event_order_5_s08_and_s12_in_same_batch`** :
   Vérifie qu'un lot combinant S08 (repositionnement) et S12 (vélocité) applique les deux opérations dans l'ordre séquentiel avant le calcul physique du tick.
6. **`test_event_order_6_multiple_block_changes_before_tick`** :
   Vérifie qu'un bloc placé par le serveur via S23 juste avant un tick est immédiatement présent dans le monde lors de la résolution des collisions de ce tick.
7. **`test_event_order_7_join_game_followed_by_state_packets`** :
   Vérifie la séquence d'initialisation propre : `JoinGame` crée le monde et le joueur, suivi des capacités, du spawn et enfin de la `Placement`, sans collision avec un état inexistant.
8. **`test_event_order_8_respawn_followed_by_placement`** :
   Vérifie que le `Respawn` réinitialise la santé à 20.0 et prépare l'entité joueur, puis que la `Placement` restaure la simulation physique au bon endroit.
9. **`test_event_order_9_dimension_change_with_pending_events`** :
   Vérifie qu'un changement de dimension vers le Nether (dimension -1) vide les anciens chunks et isole le nouveau monde avant d'appliquer les blocs et la position.
10. **`test_event_order_10_multiple_catch_up_ticks`** :
    Vérifie que lors de ticks de rattrapage (catch-up ticks consécutifs), les paquets réseau sont dépilés avant le lot et que chaque tick émet son rapport de mouvement distinct dans l'ordre.
11. **`test_event_order_11_heavy_network_load_burst`** :
    Vérifie qu'une rafale de 150 paquets réseau est absorbée sans perte et que la boucle de jeu continue de ticker sans instabilité.
12. **`test_event_order_12_fifo_order_preserved`** :
    Vérifie la préservation absolue de l'ordre FIFO sur une séquence d'événements alternés.

---

## 6. Résultats des Compilations et Tests

1. **Compilation des bibliothèques :**
   ```text
   cargo check -p umicraft-game   -> OK (0 erreur)
   cargo check -p umicraft-client -> OK (0 erreur)
   ```
2. **Suite de tests physiques et généraux (`umicraft-game`) :**
   ```text
   cargo test -p umicraft-game --lib
   test result: ok. 1392 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
   ```
3. **Tests de parité réseau et de la boucle (`umicraft-client`) :**
   ```text
   cargo test -p umicraft-client --lib test_event_order
   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 1165 filtered out; finished in 0.23s

   cargo test -p umicraft-client --lib game::tests
   test result: ok. 159 passed; 0 failed; 0 ignored; 0 measured; 1018 filtered out; finished in 4.86s
   ```
4. **Intégrité Git :**
   - Aucune commande destructrice (`git reset`, `checkout`, `clean`, `stash`) n'a été exécutée.
   - Les 113 modifications locales préexistantes sont intégralement préservées.
   - Fichiers modifiés dans le cadre de la Phase B : exclusivement `crates/umicraft-client/src/game.rs` et ce rapport d'audit.

---

## 7. Limitations et Tests Non Réalisés

- **Serveurs réels distants :** Conformément aux règles de déontologie et aux contraintes d'audit, aucun test sur un serveur de production en ligne ou sous anti-cheat propriétaire (ex. Hypixel / GrimAC en production) n'a été mené. Les validations ont été exécutées via des simulations déterministes in-memory (`LoopHarness`) reproduisant fidèlement les flux de paquets TCP du protocole 47.
- **Rendu graphique WGPU / DirectX 12 :** L'audit a porté sur la parité logique, physique et temporelle de la boucle d'événements réseau. La boucle de présentation swapchain / vsync s'exécute sur le thread de rendu séparé et communique de façon asynchrone via `RenderEvent`.

---

## 8. Statut Final

La **Phase B est terminée avec succès**.
La parité temporelle vanilla sur le traitement des événements réseau avant la simulation physique des ticks est désormais atteinte et validée par 12 tests différentiels déterministes.

**ARRÊT STRICT :** Aucune modification de la Phase C n'a été entamée. Nous attendons votre validation avant toute étape ultérieure.
