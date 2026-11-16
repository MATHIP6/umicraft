# Phase E.10 — Validation réelle de la boucle d'événements réseau

Date : 2026-11-13. Suite de E.9 (cadence par image des événements réseau, plus d'oubli de position après S08, mort muette après 20 ticks).

**Décision : validée pour ce qui a été mesuré, avec des limites explicites** (§8) : le déclencheur de la boucle de
téléportation n'est pas reproductible à volonté (2 runs sur 6 l'ont montrée, aucun à 144 FPS), le recul (S12) n'a pas
été mesuré, et les runs fenêtrés ont été faits avec un binaire `debug`. Aucun correctif n'a été nécessaire :
**aucun changement de production dans cette phase** (l'instrumentation temporaire a été retirée, §7).

## 1. Ce qui est « fenêtre réelle » et ce qui ne l'est pas

| Niveau | Contenu |
|---|---|
| **Fenêtre réelle + serveur réel** (client `umicraft-client` avec sa fenêtre winit/wgpu, serveur PandaSpigot 225, proxy d'enregistrement) | §3 à §5 |
| Sans fenêtre (`--headless`), serveur réel | non-régression `e1`, `e2`, `e3` (§6) |
| Unitaire | suites `umicraft-client`, `umicraft-game`, `umicraft-testkit` (§7) |

La voie `window/mod.rs` → `Session::note_frame` à chaque image est **exécutée et mesurée** (6 runs X3 propres, 2 runs de
latence, 3 runs de respawn). Les durées d'image sont relevées à l'appel de `note_frame` ; la cadence des fenêtres de
traitement est relevée dans le thread de jeu ; les paquets sont relevés par le proxy (horloge unique).

## 2. FPS configurés et FPS réels

`video.max_fps` écrit dans `options.toml` (sauvegardé avant, restauré après chaque run, comparaison binaire), fenêtre non plein écran.

| FPS configuré | Durée d'image médiane | FPS réel (médiane) | p99 | Arrêts > 100 ms |
|---|---|---|---|---|
| 60 | 18,8 ms | **53** | 35–36 ms | 1 (démarrage, ~3,6 s) |
| 120 | 10,1 ms | **99** | 21 ms | 1 (démarrage) |
| 144 | 8,6 ms | **116** | 19 ms | 1 (démarrage) |

Le FPS réel est **inférieur** au FPS configuré à chaque configuration (limiteur à granularité système, binaire `debug`).
Les tableaux donnent donc le FPS réel, pas la consigne.

## 3. X3 en fenêtre réelle (joueur mort à terre, puis `tp`)

Données : `reports/phase_e10_data/windowed_runs.txt`. Chaque run : joueur neuf, proxy, 35 s. « Boucle » = le serveur renvoie
un S08 à chaque C06 (E.8). Le run `fps60-X3-1` est **exclu** : son dossier mélange la session fermée à la main avec la
suivante ; remplacé par `fps60-X3-3`.

| Run | Boucle serveur | S08 | C06 | S08/s max | Écart médian entre fenêtres | S08 reçu → traité (méd. / p95) | S08 → C06 (méd. / p95) | Paquets de mouvement après la mort | Expulsé |
|---|---|---|---|---|---|---|---|---|---|
| 60-2 | non | 3 | 4 | 2 | 50 ms (attente de tick) | 33 / 83 ms | 33 / 86 ms | 20, dernier à +1 000 ms | non |
| 60-3 | **oui** | 1 060 | 1 060 | **48** | **20,2 ms** | 0 / 24 ms | 0 / 20 ms | 20, dernier à +1 000 ms | non |
| 120-1 | non | 3 | 4 | 2 | 50 ms | 14 / 80 ms | 14 / 83 ms | 20, +999 ms | non |
| 120-2 | **oui** | 1 891 | 1 891 | **85** | **11,1 ms** | 0 / 14 ms | 0 / 11 ms | 20, +1 000 ms | non |
| 144-1 | non | 3 | 4 | 2 | 50 ms | 27 / 68 ms | 27 / 71 ms | 20, +999 ms | non |
| 144-2 | non | 3 | 4 | 1 | 50 ms | 0 / 69 ms | 0 / 72 ms | 20, +999 ms | non |

Lecture :

- **Aucune expulsion** (`Sent too many packets` : 0 sur tous les runs de la phase), y compris dans les deux runs où le
  serveur boucle pendant 35 s.
- **Cadence de la boucle = cadence réelle des images** : 20,2 ms d'écart médian pour 18,8 ms d'image (60 → 53 FPS),
  11,1 ms pour 10,1 ms (120 → 99 FPS). En E.9 sans fenêtre l'écart était de 26 ms ; avec les durées d'image réelles il
  suit l'image, comme vanilla (17 ms à 60 FPS, 61 S08/s). S08/s max : 48 et 85, très en dessous du seuil de 500/s.
- **Ordre S08 / C06** : exactement un C06 par S08 dans les deux runs en boucle (1 060/1 060, 1 891/1 891) ; dans les autres,
  un C06 de plus (le C06 de connexion).
- **Pas de croissance de file non maîtrisée** : S08 traités = C06 envoyés, latence S08 → C06 ≤ 1 fenêtre (p95 ≤ 24 ms)
  pendant 35 s de boucle.
- **Après 20 ticks de mort** : 20 paquets de mouvement exactement, le dernier à +999/+1 000 ms, dans tous les runs
  (vanilla : 20, dernier à +1 049 ms). Le comportement E.9 est confirmé en fenêtre réelle.
- Les latences « reçu → traité » plus élevées des runs sans boucle (p95 ~70–83 ms) portent sur 3 événements dont des
  téléportations de connexion ; elles sont mesurées sur un échantillon plus grand en §4.

## 4. Latence des événements entrants (S08)

Joueur vivant au sol, 40 `tp` espacés de 0,35 s (`lat144-L-1`, `lat60-L-1` dans `windowed_runs.txt`).

| FPS configuré (réel) | S08 | C06 | S08 → traité (méd. / p95) | S08 → C06 (méd. / p95 / max) | Perdus / réordonnés |
|---|---|---|---|---|---|
| 144 (116) | 41 | 42 | 0 / 9 ms | 0 / 10 / 65 ms | 0 / 0 |
| 60 (53) | 41 | 42 | 0 / 31 ms | 0 / 31 / 76 ms | 0 / 0 |

Le maximum (65–76 ms) correspond aux téléportations de connexion ; en régime, le délai reste de l'ordre d'une durée
d'image. Aucun événement perdu ni réordonné.

**Recul (S12) : non mesuré.** Il n'y a pas de serveur de test qui inflige un coup de façon reproductible ; je n'invente
pas de mesure de combat. S12 emprunte la même file et les mêmes fenêtres que S08 (test unitaire de cadence), mais son
délai en combat réel reste à mesurer.

## 5. Respawn sur serveur réel

Joueur vivant, `kill` console, puis, 4 s après la mort, `GameEvent::Respawn` (équivalent du clic ; le clic n'est pas
simulé, son chemin est couvert par `the_respawn_button_sends_the_request_only_to_a_dead_player`).
Données : `reports/phase_e10_data/respawn.txt`.

| Run | Paquets de mouvement avant respawn | Dernier | C16 → S07 | Reprise des paquets de mouvement | Chunks reçus après S07 | Expulsé |
|---|---|---|---|---|---|---|
| 60-1 | 20 | +999 ms | 4 ms | 242 paquets, premier à +96 ms | 46 | non |
| 60-2 | 20 | +1 001 ms | 2 ms | 239 paquets, premier à +150 ms | 90 | non |
| 144-1 | 20 | +1 001 ms | 3 ms | 240 paquets, premier à +96 ms | 17 | non |

Silence entre le 20ᵉ paquet et le respawn (3 s), reprise ensuite, chunks renvoyés. Pas de comparaison vanilla de la
reprise (non capturée).

## 6. Non-régression (rejeu contre les références vanilla, strict, serveur réel, sans fenêtre)

| Scénario | Référence | Écart maximum | Verdict |
|---|---|---|---|
| `e1` (sol, sprint, sauts, strafe, sneak ; 520 ticks) | vanilla E.6 | `dz` ≤ 3,05e-6, `dy` ≤ 6,7e-8, `dm` ≤ 6,9e-8, `dx` ≤ 1,1e-6, drapeaux 0 écart | identique aux tableaux E.4/E.6/E.9 (bruit `float`) |
| `e2` (sprint arrêté par sneak ; 80 ticks) | vanilla E.5 | `dz` ≤ 2,09e-6, `dm` ≤ 1,7e-8 | identique à E.5 |
| `e3` (Depth Strider III ; 115 ticks) | vanilla E.6 | `dy` ≤ 7,3e-8, `dz` ≤ 1,04e-6, `dm` ≤ 1,9e-8 | identique à `state-diff-e3-after.txt` d'E.6 |

Fichiers : `reports/phase_e10_data/state-diff-e1.txt`, `-e2.txt`, `-e3.txt`. Deux essais invalides ont été écartés : un
joueur déjà mort à la connexion, et un monde dont la couche y=1 de la piscine avait été remplacée par des phases
précédentes (premier `e3` : écart d'un bloc en `y`, sans rapport avec le client). Le monde a été restauré et le résultat
ci-dessus est celui du run valide.

## 7. Tests, code et dépôt

| Suite | Résultat |
|---|---|
| `umicraft-client --lib` | **1 178** ok, 0 échec (27 ignorés), après retrait de l'instrumentation |
| `umicraft-game` | 1 397 + 15 + 30 ok, 0 échec |
| `umicraft-testkit` | 48 + 2 + 2 + 11 + 4 ok, 0 échec |

- **Instrumentation temporaire** (`e10_probe.rs`, appels dans `window/mod.rs` et `game.rs`, hook de respawn automatique)
  **retirée** ; copie du module en `reports/phase_e10_data/e10_probe.rs.txt` (texte, non compilé), scripts en `*.txt`.
- `physics.rs` non touché ; corrections E.4/E.6 préservées.
- Dépôt : **158** chemins modifiés ou non suivis, comme au début ; aucun `reset`, `checkout`, `clean`, `stash`.
- `options.toml` de l'utilisateur identique à la sauvegarde ; serveur de test arrêté.

## 8. Limites restantes

1. **Déclencheur de la boucle non reproductible** : 2 runs fenêtrés sur 6 l'ont montrée (60 et 120 FPS), aucun à 144 FPS ;
   le comportement à 144 FPS en boucle n'est pas mesuré (débit attendu ~100 S08/s, loin de 500/s).
2. **S12 (recul) non mesuré** en combat réel.
3. **Binaire `debug`** : FPS absolus meilleurs en `release` ; la cadence suit la durée d'image mesurée, donc la conclusion
   de cadence tient.
4. **Clic sur « Respawn » non exercé** en fenêtre (même événement déclenché par l'instrumentation).
5. **Vanilla non plafonné** (E.8 addendum) : borne de 1 000 images/s non testée avec une fenêtre aux images de ~1 ms.
6. **Reprise après respawn non comparée à vanilla** ; les 96–150 ms avant le premier paquet ne sont pas expliqués.

## 9. Critères de validation

| Critère | Résultat |
|---|---|
| Voie fenêtre réelle exécutée | oui |
| Cadences mesurées | oui (FPS réels, écart des fenêtres, S08/s) |
| Aucune expulsion dans les scénarios testés | oui |
| Événements ordonnés | oui (S08 = C06, 0 perdu / 0 réordonné sur 41) |
| Latence des événements entrants documentée | S08 oui ; S12 non mesuré |
| Respawn fonctionnel | oui (3 runs) |
| Pas de régression | oui (e1, e2, e3, suites) |
